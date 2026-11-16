//! Interpretazione dei messaggi `event` del WebSocket di Home Assistant.
//!
//! Tre sorgenti:
//! - `subscribe_entities` per gli stati delle entità mostrate (vedi `interpreta_entita`);
//! - `subscribe_events` per gli eventi dei registri e della configurazione:
//!   `{"type":"event","event":{"event_type":...,"data":...}}`;
//! - `persistent_notification/subscribe` (homeassistant/components/persistent_notification/
//!   __init__.py, `websocket_subscribe_notifications`): `{"id": <id dell'iscrizione>,
//!   "type":"event","event":{"type":"current"|"added"|"updated"|"removed",
//!   "notifications":{<notification_id>: {"notification_id","title","message","created_at"}}}}`.
//!   Subito dopo l'iscrizione arriva "current" con tutte le notifiche presenti. Il vecchio
//!   evento del bus `persistent_notification_created` HA non lo emette più.
use serde_json::Value;

use super::models::HaState;
use super::registri::{self, CambioRegistro};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificaHa {
    pub id: String,
    pub titolo: String,
    pub messaggio: String,
}

/// `UpdateType` di persistent_notification/const.py.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aggiornamento {
    /// L'elenco completo, mandato all'iscrizione.
    Attuali,
    Aggiunte,
    Aggiornate,
    Rimosse,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EventoWs {
    /// Un evento dell'iscrizione `subscribe_entities` numero `iscrizione` (il numero lo sceglie
    /// il servizio: gli eventi di un'iscrizione superata si scartano). `aggiunte`: stati interi
    /// (il primo evento dell'iscrizione li porta tutti); `cambiate`: per entità, il diff
    /// compresso (`{"+": {...}, "-": {...}}`, vedi `applica_diff`); `tolte`: entità rimosse.
    Entita { iscrizione: u64, aggiunte: Vec<HaState>, cambiate: Vec<(String, Value)>, tolte: Vec<String> },
    /// Un evento delle entità che non si riesce a leggere: si rilegge tutto.
    StatoIlleggibile,
    /// La configurazione (`core_config_updated`: unità di temperatura, stato del core) è cambiata,
    /// o un evento di un registro non si legge: si rilegge tutto.
    DaRileggere,
    /// Un evento di un registro, letto (vedi `registri`): il servizio decide cosa rileggere.
    Registro(CambioRegistro),
    Notifiche { tipo: Aggiornamento, notifiche: Vec<NotificaHa> },
}

/// Gli eventi che fanno rileggere registri e configurazione, a cui ci si iscrive (gli stati
/// arrivano da `subscribe_entities`). Sono tutti nella lista degli eventi che anche un utente non amministratore
/// può seguire (`SUBSCRIBE_ALLOWLIST` di HA, `homeassistant/auth/permissions/events.py`:
/// `EVENT_*_REGISTRY_UPDATED` ed `EVENT_CORE_CONFIG_UPDATE`).
pub const EVENTI_RILETTURA: [&str; 4] =
    ["area_registry_updated", "device_registry_updated", "entity_registry_updated", "core_config_updated"];

/// Titolo di una notifica senza titolo (in HA `title` è facoltativo e può essere null).
pub const TITOLO_PREDEFINITO: &str = "Home Assistant";

/// Un evento di `subscribe_events`: solo quelli che fanno rileggere (`EVENTI_RILETTURA`).
///
/// Forma (documentazione di HA, «Events» e «WebSocket API»): `{"type": "event", "event":
/// {"event_type": ..., "data": {...}}}`.
pub fn interpreta(v: &Value) -> Option<EventoWs> {
    let evento = v.get("event")?;
    match evento.get("event_type")?.as_str()? {
        "core_config_updated" => Some(EventoWs::DaRileggere),
        t if EVENTI_RILETTURA.contains(&t) => Some(match registri::leggi(t, &evento["data"]) {
            Some(c) => EventoWs::Registro(c),
            None => EventoWs::DaRileggere,
        }),
        _ => None,
    }
}

/// Un evento dell'iscrizione `subscribe_entities` (`v` è il messaggio completo; gli stati ne
/// vengono presi, non copiati).
///
/// Forma (documentazione di HA, «WebSocket API», `subscribe_entities`, e
/// `homeassistant/components/websocket_api/messages.py`): `"event"` è una mappa con al più tre
/// chiavi: `a` (entità aggiunte, `{entity_id: stato compresso}`; il primo evento dopo
/// l'iscrizione porta tutte le entità chieste), `c` (entità cambiate, `{entity_id: {"+": {...},
/// "-": {"a": [attributi tolti]}}}`) e `r` (id delle entità rimosse). In uno stato compresso `s`
/// è lo stato, `a` gli attributi, `c` il contesto, `lc` e `lu` i tempi (numeri, `lu` solo se
/// diverso da `lc`).
pub fn interpreta_entita(v: &mut Value, iscrizione: u64) -> EventoWs {
    let Some(evento) = v.get_mut("event").and_then(Value::as_object_mut) else {
        return EventoWs::StatoIlleggibile;
    };
    let mut aggiunte = Vec::new();
    if let Some(a) = evento.get_mut("a") {
        let Some(a) = a.as_object_mut() else { return EventoWs::StatoIlleggibile };
        for (id, compresso) in a.iter_mut() {
            match stato_compresso(id, compresso.take()) {
                Some(s) => aggiunte.push(s),
                None => return EventoWs::StatoIlleggibile,
            }
        }
    }
    let mut cambiate = Vec::new();
    if let Some(c) = evento.get_mut("c") {
        let Some(c) = c.as_object_mut() else { return EventoWs::StatoIlleggibile };
        for (id, diff) in c.iter_mut() {
            if !diff.is_object() {
                return EventoWs::StatoIlleggibile;
            }
            cambiate.push((id.clone(), diff.take()));
        }
    }
    let mut tolte = Vec::new();
    if let Some(r) = evento.get("r") {
        let Some(r) = r.as_array() else { return EventoWs::StatoIlleggibile };
        for id in r {
            let Some(id) = id.as_str() else { return EventoWs::StatoIlleggibile };
            tolte.push(id.to_owned());
        }
    }
    EventoWs::Entita { iscrizione, aggiunte, cambiate, tolte }
}

/// Un tempo compresso (secondi dall'epoca, con decimali) come testo: l'app non lo legge, lo
/// tiene solo per completezza dello stato.
fn tempo(v: &Value) -> Option<String> {
    v.as_f64().map(|t| t.to_string())
}

/// Uno stato compresso di `subscribe_entities` come `HaState`. `None` se non si legge.
fn stato_compresso(id: &str, mut v: Value) -> Option<HaState> {
    let o = v.as_object_mut()?;
    let state = o.get("s")?.as_str()?.to_owned();
    let attributes = match o.get_mut("a").map(Value::take) {
        None | Some(Value::Null) => Value::Object(Default::default()),
        Some(a) if a.is_object() => a,
        Some(_) => return None,
    };
    let last_changed = o.get("lc").and_then(tempo).unwrap_or_default();
    let last_updated = o.get("lu").and_then(tempo).unwrap_or_else(|| last_changed.clone());
    Some(HaState { entity_id: id.to_owned(), state, attributes, last_changed, last_updated })
}

/// Applica a uno stato il diff compresso di `subscribe_entities`: sotto `+` lo stato nuovo
/// (`s`), gli attributi cambiati o nuovi (`a`, fusi con quelli che c'erano), i tempi (`lc`,
/// `lu`); sotto `-` gli attributi tolti (`{"a": [nomi]}`). `None` se il diff non si legge (chi
/// chiama rilegge tutto).
pub fn applica_diff(stato: &HaState, diff: &Value) -> Option<HaState> {
    let diff = diff.as_object()?;
    let mut nuovo = stato.clone();
    if !nuovo.attributes.is_object() {
        // Uno stato alleggerito (attributi tolti): si riparte da un oggetto vuoto.
        nuovo.attributes = Value::Object(Default::default());
    }
    if let Some(piu) = diff.get("+") {
        let piu = piu.as_object()?;
        if let Some(s) = piu.get("s") {
            nuovo.state = s.as_str()?.to_owned();
        }
        if let Some(a) = piu.get("a") {
            let a = a.as_object()?;
            let attributi = nuovo.attributes.as_object_mut()?;
            for (k, v) in a {
                attributi.insert(k.clone(), v.clone());
            }
        }
        if let Some(lc) = piu.get("lc") {
            nuovo.last_changed = tempo(lc)?;
            nuovo.last_updated = nuovo.last_changed.clone();
        }
        if let Some(lu) = piu.get("lu") {
            nuovo.last_updated = tempo(lu)?;
        }
    }
    if let Some(meno) = diff.get("-") {
        let meno = meno.as_object()?;
        if let Some(a) = meno.get("a") {
            let attributi = nuovo.attributes.as_object_mut()?;
            for k in a.as_array()? {
                attributi.remove(k.as_str()?);
            }
        }
    }
    Some(nuovo)
}

/// Un evento dell'iscrizione `persistent_notification/subscribe`. `v` è il messaggio completo.
pub fn interpreta_notifiche(v: &Value) -> Option<EventoWs> {
    let evento = &v["event"];
    let tipo = match evento["type"].as_str()? {
        "current" => Aggiornamento::Attuali,
        "added" => Aggiornamento::Aggiunte,
        "updated" => Aggiornamento::Aggiornate,
        "removed" => Aggiornamento::Rimosse,
        _ => return None,
    };
    let mut notifiche: Vec<NotificaHa> = evento["notifications"]
        .as_object()?
        .iter()
        .map(|(chiave, n)| NotificaHa {
            id: n["notification_id"].as_str().unwrap_or(chiave).to_owned(),
            titolo: n["title"]
                .as_str()
                .filter(|t| !t.trim().is_empty())
                .unwrap_or(TITOLO_PREDEFINITO)
                .to_owned(),
            messaggio: n["message"].as_str().unwrap_or_default().to_owned(),
        })
        .collect();
    notifiche.sort_by(|a, b| a.id.cmp(&b.id));
    Some(EventoWs::Notifiche { tipo, notifiche })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn evento(tipo: &str, data: Value) -> Value {
        json!({ "id": 2, "type": "event", "event": { "event_type": tipo, "data": data } })
    }

    fn notifiche(tipo: &str, n: Value) -> Value {
        json!({ "id": 3, "type": "event", "event": { "type": tipo, "notifications": n } })
    }

    fn entita(event: Value) -> Value {
        json!({ "id": 7, "type": "event", "event": event })
    }

    /// Il primo evento dell'iscrizione: tutti gli stati chiesti, compressi (esempio della
    /// documentazione di HA: `s`, `a`, `c`, `lc`, `lu`).
    #[test]
    fn subscribe_entities_aggiunte_intere() {
        let mut v = entita(json!({ "a": {
            "light.cucina": { "s": "on", "a": { "friendly_name": "Cucina", "brightness": 200 },
                              "c": "01HXYZ", "lc": 1763028000.5, "lu": 1763028001.25 },
            "switch.presa": { "s": "off", "a": {}, "c": { "id": "x", "parent_id": null, "user_id": "u" }, "lc": 1763028000.0 } } }));
        let EventoWs::Entita { iscrizione, aggiunte, cambiate, tolte } = interpreta_entita(&mut v, 3) else { panic!() };
        assert_eq!(iscrizione, 3);
        assert!(cambiate.is_empty() && tolte.is_empty());
        let cucina = aggiunte.iter().find(|s| s.entity_id == "light.cucina").unwrap();
        assert_eq!(cucina.state, "on");
        assert_eq!(cucina.attributes["brightness"], 200);
        assert_eq!(cucina.last_updated, "1763028001.25");
        let presa = aggiunte.iter().find(|s| s.entity_id == "switch.presa").unwrap();
        assert_eq!(presa.last_updated, presa.last_changed, "senza lu vale lc");
        // Presi, non copiati.
        assert!(v["event"]["a"]["light.cucina"].is_null());
    }

    #[test]
    fn subscribe_entities_cambi_e_rimozioni() {
        let mut v = entita(json!({
            "c": { "light.cucina": { "+": { "s": "off", "lc": 1763028100.0, "c": { "parent_id": null } } } },
            "r": ["switch.presa"] }));
        let EventoWs::Entita { aggiunte, cambiate, tolte, .. } = interpreta_entita(&mut v, 1) else { panic!() };
        assert!(aggiunte.is_empty());
        assert_eq!(cambiate.len(), 1);
        assert_eq!(cambiate[0].0, "light.cucina");
        assert_eq!(tolte, ["switch.presa"]);
    }

    #[test]
    fn subscribe_entities_illeggibile() {
        for evento in [
            json!(5),
            json!({ "a": [1] }),
            json!({ "a": { "light.x": { "s": 3 } } }),
            json!({ "a": { "light.x": { "a": {} } } }),
            json!({ "a": { "light.x": { "s": "on", "a": [1] } } }),
            json!({ "c": { "light.x": 3 } }),
            json!({ "r": "light.x" }),
            json!({ "r": [3] }),
        ] {
            let mut v = entita(evento.clone());
            assert_eq!(interpreta_entita(&mut v, 1), EventoWs::StatoIlleggibile, "{evento}");
        }
        let mut senza_evento = json!({ "id": 7, "type": "event" });
        assert_eq!(interpreta_entita(&mut senza_evento, 1), EventoWs::StatoIlleggibile);
    }

    fn base() -> HaState {
        HaState {
            entity_id: "light.cucina".into(),
            state: "on".into(),
            attributes: json!({ "friendly_name": "Cucina", "brightness": 200, "linkquality": 80 }),
            last_changed: "1".into(),
            last_updated: "1".into(),
        }
    }

    #[test]
    fn diff_applicato_stato_attributi_e_tolti() {
        let diff = json!({ "+": { "s": "off", "a": { "brightness": null, "color_mode": "onoff" }, "lc": 5.5 },
                           "-": { "a": ["linkquality"] } });
        let n = applica_diff(&base(), &diff).unwrap();
        assert_eq!(n.state, "off");
        assert_eq!(n.attributes, json!({ "friendly_name": "Cucina", "brightness": null, "color_mode": "onoff" }));
        assert_eq!((n.last_changed.as_str(), n.last_updated.as_str()), ("5.5", "5.5"));
        // Solo l'ultimo aggiornamento: lo stato resta.
        let n = applica_diff(&base(), &json!({ "+": { "lu": 7.0, "a": { "linkquality": 81 } } })).unwrap();
        assert_eq!(n.state, "on");
        assert_eq!(n.attributes["linkquality"], 81);
        assert_eq!((n.last_changed.as_str(), n.last_updated.as_str()), ("1", "7"));
    }

    #[test]
    fn diff_su_stato_alleggerito_e_diff_illeggibili() {
        let mut alleggerito = base();
        alleggerito.attributes = Value::Null;
        let n = applica_diff(&alleggerito, &json!({ "+": { "a": { "x": 1 } } })).unwrap();
        assert_eq!(n.attributes, json!({ "x": 1 }));
        for diff in [json!(3), json!({ "+": 1 }), json!({ "+": { "s": 2 } }), json!({ "+": { "a": [] } }),
                     json!({ "-": { "a": "x" } }), json!({ "-": { "a": [1] } }), json!({ "+": { "lc": "ieri" } })] {
            assert!(applica_diff(&base(), &diff).is_none(), "{diff}");
        }
    }

    #[test]
    fn eventi_dei_registri_e_della_configurazione() {
        for t in EVENTI_RILETTURA {
            let v = evento(t, json!({ "action": "update" }));
            assert_eq!(interpreta(&v), Some(EventoWs::DaRileggere), "{t}");
        }
    }

    /// Task 14h: un evento di un registro che si legge arriva col suo contenuto.
    #[test]
    fn evento_di_un_registro_letto() {
        let v = evento("device_registry_updated", json!({ "action": "update", "device_id": "d1", "changes": { "sw_version": "1" } }));
        let Some(EventoWs::Registro(CambioRegistro::Dispositivo { device_id, campi, .. })) = interpreta(&v) else { panic!() };
        assert_eq!((device_id.as_str(), campi), ("d1", vec!["sw_version".to_owned()]));
    }

    #[test]
    fn notifica_aggiunta_con_titolo_e_messaggio() {
        let v = notifiche(
            "added",
            json!({ "lav": { "notification_id": "lav", "title": "Lavatrice", "message": "Ciclo finito",
                             "created_at": "2026-11-13T10:00:00+00:00" } }),
        );
        assert_eq!(
            interpreta_notifiche(&v),
            Some(EventoWs::Notifiche {
                tipo: Aggiornamento::Aggiunte,
                notifiche: vec![NotificaHa { id: "lav".into(), titolo: "Lavatrice".into(), messaggio: "Ciclo finito".into() }],
            })
        );
    }

    #[test]
    fn notifica_senza_titolo_usa_home_assistant() {
        let v = notifiche("updated", json!({ "x": { "notification_id": "x", "title": null, "message": "Ciao" } }));
        let Some(EventoWs::Notifiche { tipo, notifiche }) = interpreta_notifiche(&v) else { panic!() };
        assert_eq!(tipo, Aggiornamento::Aggiornate);
        assert_eq!(notifiche[0].titolo, "Home Assistant");
    }

    #[test]
    fn attuali_vuote_e_rimosse() {
        assert_eq!(
            interpreta_notifiche(&notifiche("current", json!({}))),
            Some(EventoWs::Notifiche { tipo: Aggiornamento::Attuali, notifiche: vec![] })
        );
        let Some(EventoWs::Notifiche { tipo, notifiche }) =
            interpreta_notifiche(&notifiche("removed", json!({ "a": { "notification_id": "a", "message": "m" } })))
        else {
            panic!()
        };
        assert_eq!(tipo, Aggiornamento::Rimosse);
        assert_eq!(notifiche[0].id, "a");
    }

    #[test]
    fn evento_sconosciuto_ignorato() {
        assert_eq!(interpreta(&evento("call_service", json!({}))), None);
        assert_eq!(interpreta(&evento("persistent_notification_created", json!({ "message": "x" }))), None);
        assert_eq!(interpreta(&evento("state_changed", json!({}))), None, "gli stati arrivano da subscribe_entities");
        assert_eq!(interpreta(&json!({ "type": "event" })), None);
        assert_eq!(interpreta(&json!({ "type": "result", "id": 1 })), None);
        assert_eq!(interpreta(&json!({ "type": "event", "event": 5 })), None);
        assert_eq!(interpreta_notifiche(&notifiche("strano", json!({}))), None);
        assert_eq!(interpreta_notifiche(&json!({ "type": "event", "event": { "type": "added" } })), None);
    }
}
