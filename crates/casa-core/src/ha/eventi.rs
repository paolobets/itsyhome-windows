//! Interpretazione dei messaggi `event` del WebSocket di Home Assistant.
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventoWs {
    StatoCambiato,
    Notifica { titolo: String, messaggio: String },
}

/// `v` è il messaggio completo (`{"type":"event","event":{...}}`).
pub fn interpreta(v: &Value) -> Option<EventoWs> {
    let evento = &v["event"];
    match evento["event_type"].as_str()? {
        "state_changed" => Some(EventoWs::StatoCambiato),
        "persistent_notification_created" => {
            let data = &evento["data"];
            let messaggio = data["message"].as_str().unwrap_or("");
            if messaggio.is_empty() {
                return None;
            }
            let titolo = data["title"].as_str().unwrap_or("ItsyHome");
            Some(EventoWs::Notifica { titolo: titolo.to_owned(), messaggio: messaggio.to_owned() })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn evento(tipo: &str, data: Value) -> Value {
        json!({ "id": 2, "type": "event", "event": { "event_type": tipo, "data": data } })
    }

    #[test]
    fn state_changed_e_stato_cambiato() {
        let v = evento("state_changed", json!({ "entity_id": "light.cucina" }));
        assert_eq!(interpreta(&v), Some(EventoWs::StatoCambiato));
    }

    #[test]
    fn notifica_con_titolo_e_messaggio() {
        let v = evento(
            "persistent_notification_created",
            json!({ "title": "Lavatrice", "message": "Ciclo finito" }),
        );
        assert_eq!(
            interpreta(&v),
            Some(EventoWs::Notifica { titolo: "Lavatrice".into(), messaggio: "Ciclo finito".into() })
        );
    }

    #[test]
    fn notifica_senza_titolo_usa_itsyhome() {
        let v = evento("persistent_notification_created", json!({ "message": "Ciao" }));
        assert_eq!(
            interpreta(&v),
            Some(EventoWs::Notifica { titolo: "ItsyHome".into(), messaggio: "Ciao".into() })
        );
        // titolo null come in HA quando non è impostato
        let v = evento("persistent_notification_created", json!({ "title": null, "message": "Ciao" }));
        assert_eq!(
            interpreta(&v),
            Some(EventoWs::Notifica { titolo: "ItsyHome".into(), messaggio: "Ciao".into() })
        );
    }

    #[test]
    fn notifica_vuota_ignorata() {
        let v = evento("persistent_notification_created", json!({ "title": "T", "message": "" }));
        assert_eq!(interpreta(&v), None);
        let v = evento("persistent_notification_created", json!({ "title": "T" }));
        assert_eq!(interpreta(&v), None);
    }

    #[test]
    fn evento_sconosciuto_ignorato() {
        assert_eq!(interpreta(&evento("call_service", json!({}))), None);
        assert_eq!(interpreta(&json!({ "type": "event" })), None);
        assert_eq!(interpreta(&json!({ "type": "result", "id": 1 })), None);
    }
}
