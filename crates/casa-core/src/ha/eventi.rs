//! Interpretazione dei messaggi `event` del WebSocket di Home Assistant.
//!
//! Due sorgenti:
//! - `subscribe_events` per `state_changed`: `{"type":"event","event":{"event_type":...,"data":...}}`;
//! - `persistent_notification/subscribe` (homeassistant/components/persistent_notification/
//!   __init__.py, `websocket_subscribe_notifications`): `{"id": <id dell'iscrizione>,
//!   "type":"event","event":{"type":"current"|"added"|"updated"|"removed",
//!   "notifications":{<notification_id>: {"notification_id","title","message","created_at"}}}}`.
//!   Subito dopo l'iscrizione arriva "current" con tutte le notifiche presenti. Il vecchio
//!   evento del bus `persistent_notification_created` HA non lo emette più.
use serde_json::Value;

use super::models::HaState;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventoWs {
    /// `state_changed` con lo stato nuovo dell'entità (`data.new_state`); `None` se l'entità è
    /// stata tolta (`new_state` null).
    StatoCambiato { entity_id: String, nuovo: Option<HaState> },
    /// `state_changed` con uno stato nuovo che non si riesce a leggere: si rilegge tutto.
    StatoIlleggibile,
    /// Un registro (`area_registry_updated`, `device_registry_updated`, `entity_registry_updated`)
    /// o la configurazione (`core_config_updated`: unità di temperatura) sono cambiati.
    DaRileggere,
    Notifiche { tipo: Aggiornamento, notifiche: Vec<NotificaHa> },
}

/// Gli eventi che fanno rileggere registri e configurazione, a cui ci si iscrive oltre a
/// `state_changed`. Sono tutti nella lista degli eventi che anche un utente non amministratore
/// può seguire (`SUBSCRIBE_ALLOWLIST` di HA, `homeassistant/auth/permissions/events.py`:
/// `EVENT_*_REGISTRY_UPDATED` ed `EVENT_CORE_CONFIG_UPDATE`).
pub const EVENTI_RILETTURA: [&str; 4] =
    ["area_registry_updated", "device_registry_updated", "entity_registry_updated", "core_config_updated"];

/// Titolo di una notifica senza titolo (in HA `title` è facoltativo e può essere null).
pub const TITOLO_PREDEFINITO: &str = "Home Assistant";

/// Un evento di `subscribe_events`. `v` è il messaggio completo; lo stato nuovo ne viene tolto
/// (preso, non copiato: su una casa vera arriva di continuo).
///
/// Forma (documentazione di HA, «Events» e «WebSocket API»): `{"type": "event", "event":
/// {"event_type": "state_changed", "data": {"entity_id", "old_state", "new_state"}}}`, con
/// `new_state` null se lo stato è stato tolto e `old_state` null se è nuovo.
pub fn interpreta(v: &mut Value) -> Option<EventoWs> {
    // `get_mut`, mai `[]` mutabile: su un valore che non è un oggetto andrebbe in panico.
    let evento = v.get_mut("event")?;
    match evento.get("event_type")?.as_str()? {
        "state_changed" => {
            let Some(dati) = evento.get_mut("data").filter(|d| d.is_object()) else {
                return Some(EventoWs::StatoIlleggibile);
            };
            // Senza la chiave non si sa nulla: non è una rimozione (quella ha `new_state` null).
            let Some(nuovo) = dati.get_mut("new_state").map(Value::take) else {
                return Some(EventoWs::StatoIlleggibile);
            };
            let entity_id = match dati.get("entity_id").and_then(Value::as_str) {
                Some(id) => id.to_owned(),
                None => return Some(EventoWs::StatoIlleggibile),
            };
            if nuovo.is_null() {
                return Some(EventoWs::StatoCambiato { entity_id, nuovo: None });
            }
            match serde_json::from_value::<HaState>(nuovo) {
                Ok(s) if s.entity_id == entity_id => Some(EventoWs::StatoCambiato { entity_id, nuovo: Some(s) }),
                _ => Some(EventoWs::StatoIlleggibile),
            }
        }
        t if EVENTI_RILETTURA.contains(&t) => Some(EventoWs::DaRileggere),
        _ => None,
    }
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

    fn stato(id: &str, s: &str) -> Value {
        json!({ "entity_id": id, "state": s, "attributes": { "friendly_name": "Cucina" },
                "last_changed": "2026-11-13T10:00:00+00:00", "last_updated": "2026-11-13T10:00:01.5+00:00",
                "context": { "id": "x", "parent_id": null, "user_id": null } })
    }

    #[test]
    fn state_changed_porta_lo_stato_nuovo() {
        let mut v = evento(
            "state_changed",
            json!({ "entity_id": "light.cucina", "old_state": stato("light.cucina", "off"), "new_state": stato("light.cucina", "on") }),
        );
        let Some(EventoWs::StatoCambiato { entity_id, nuovo: Some(s) }) = interpreta(&mut v) else { panic!() };
        assert_eq!(entity_id, "light.cucina");
        assert_eq!(s.state, "on");
        assert_eq!(s.attributes["friendly_name"], "Cucina");
        assert_eq!(s.last_updated, "2026-11-13T10:00:01.5+00:00");
        // Preso, non copiato.
        assert!(v["event"]["data"]["new_state"].is_null());
    }

    #[test]
    fn state_changed_senza_stato_nuovo_e_rimozione() {
        let mut v = evento("state_changed", json!({ "entity_id": "light.cucina", "old_state": stato("light.cucina", "off"), "new_state": null }));
        assert_eq!(interpreta(&mut v), Some(EventoWs::StatoCambiato { entity_id: "light.cucina".into(), nuovo: None }));
    }

    #[test]
    fn state_changed_illeggibile() {
        let mut senza_id = evento("state_changed", json!({ "new_state": stato("light.cucina", "on") }));
        assert_eq!(interpreta(&mut senza_id), Some(EventoWs::StatoIlleggibile));
        let mut stato_rotto = evento("state_changed", json!({ "entity_id": "light.cucina", "new_state": { "state": 3 } }));
        assert_eq!(interpreta(&mut stato_rotto), Some(EventoWs::StatoIlleggibile));
        let mut senza_chiave = evento("state_changed", json!({ "entity_id": "light.cucina" }));
        assert_eq!(interpreta(&mut senza_chiave), Some(EventoWs::StatoIlleggibile));
        let mut evento_strano = json!({ "type": "event", "event": 5 });
        assert_eq!(interpreta(&mut evento_strano), None);
        let mut dati_strani = evento("state_changed", json!([1, 2]));
        assert_eq!(interpreta(&mut dati_strani), Some(EventoWs::StatoIlleggibile));
        let mut altro_id = evento("state_changed", json!({ "entity_id": "light.sala", "new_state": stato("light.cucina", "on") }));
        assert_eq!(interpreta(&mut altro_id), Some(EventoWs::StatoIlleggibile));
    }

    #[test]
    fn eventi_dei_registri_e_della_configurazione() {
        for t in EVENTI_RILETTURA {
            let mut v = evento(t, json!({ "action": "update" }));
            assert_eq!(interpreta(&mut v), Some(EventoWs::DaRileggere), "{t}");
        }
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
        assert_eq!(interpreta(&mut evento("call_service", json!({}))), None);
        assert_eq!(interpreta(&mut evento("persistent_notification_created", json!({ "message": "x" }))), None);
        assert_eq!(interpreta(&mut json!({ "type": "event" })), None);
        assert_eq!(interpreta(&mut json!({ "type": "result", "id": 1 })), None);
        assert_eq!(interpreta_notifiche(&notifiche("strano", json!({}))), None);
        assert_eq!(interpreta_notifiche(&json!({ "type": "event", "event": { "type": "added" } })), None);
    }
}
