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
    StatoCambiato,
    Notifiche { tipo: Aggiornamento, notifiche: Vec<NotificaHa> },
}

/// Titolo di una notifica senza titolo (in HA `title` è facoltativo e può essere null).
pub const TITOLO_PREDEFINITO: &str = "Home Assistant";

/// Un evento di `subscribe_events`. `v` è il messaggio completo.
pub fn interpreta(v: &Value) -> Option<EventoWs> {
    match v["event"]["event_type"].as_str()? {
        "state_changed" => Some(EventoWs::StatoCambiato),
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

    #[test]
    fn state_changed_e_stato_cambiato() {
        let v = evento("state_changed", json!({ "entity_id": "light.cucina" }));
        assert_eq!(interpreta(&v), Some(EventoWs::StatoCambiato));
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
        assert_eq!(interpreta(&json!({ "type": "event" })), None);
        assert_eq!(interpreta(&json!({ "type": "result", "id": 1 })), None);
        assert_eq!(interpreta_notifiche(&notifiche("strano", json!({}))), None);
        assert_eq!(interpreta_notifiche(&json!({ "type": "event", "event": { "type": "added" } })), None);
    }
}
