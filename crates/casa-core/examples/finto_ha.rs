//! Home Assistant finto per le prove dal vivo dell'assistente (mai la casa vera).
//!
//! ```text
//! cargo run -p casa-core --example finto_ha -- prepara <cartella> <porta> [--preferiti]
//!     scrive <cartella>/assistente.json con l'ambiente "prova-locale" su ws://127.0.0.1:<porta>
//!     e mette un token FINTO nel portachiavi sotto "prova-locale"
//! cargo run -p casa-core --example finto_ha -- <porta> [--notifica-dopo <secondi>]
//!     avvia il server (Ctrl+C o chiusura del processo per fermarlo)
//! cargo run -p casa-core --example finto_ha -- pulisci
//!     cancella il token finto dal portachiavi
//! ```
//! Poi: `ITSYHOME_ASSISTENTE_DIR=<cartella>` per l'assistente (niente import dalla v2).
//!
//! Entità: 2 luci (una accesa), 1 interruttore, 1 scena, 1 sensore di temperatura, una luce non
//! disponibile senza stanza (→ "Altro") e un interruttore che rifiuta ogni comando. `call_service`
//! cambia lo stato finto e manda `state_changed`, come l'HA vero. Stessa logica del server finto
//! dei test d'integrazione (`tests/finto_ha.rs`), con uno stato che cambia.
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use casa_core::config::{Config, ConfigStore};
use casa_core::segreti::{KeyringSegreti, Segreti};
use casa_core::types::HaEnvironment;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message;

/// Ambiente e token di prova: mai quelli veri.
const AMBIENTE: &str = "prova-locale";
const TOKEN: &str = "token-finto-di-prova";

struct Entita {
    stato: String,
    nome: &'static str,
    area: Option<&'static str>,
    attributi: Value,
}

type Stati = Arc<Mutex<BTreeMap<&'static str, Entita>>>;

fn entita_iniziali() -> BTreeMap<&'static str, Entita> {
    let mut m = BTreeMap::new();
    let mut e = |id, stato: &str, nome, area, attributi: Value| {
        m.insert(id, Entita { stato: stato.into(), nome, area, attributi });
    };
    let luce = json!({ "supported_color_modes": ["onoff"], "color_mode": "onoff" });
    e("light.soggiorno", "on", "Luce soggiorno", Some("soggiorno"), luce.clone());
    e("light.cucina", "off", "Luce cucina", Some("cucina"), luce.clone());
    e("light.giardino", "unavailable", "Luce giardino", None, luce);
    e("switch.presa", "off", "Presa TV", Some("soggiorno"), json!({}));
    e("switch.guasto", "off", "Presa guasta", Some("cucina"), json!({}));
    e("scene.cinema", "2026-11-13T10:00:00+00:00", "Cinema", None, json!({}));
    e(
        "sensor.temperatura",
        "21.5",
        "Temperatura soggiorno",
        Some("soggiorno"),
        json!({ "device_class": "temperature", "unit_of_measurement": "°C" }),
    );
    m
}

fn stato_json(id: &str, e: &Entita) -> Value {
    let mut attributi = e.attributi.clone();
    attributi["friendly_name"] = json!(e.nome);
    json!({
        "entity_id": id, "state": e.stato, "attributes": attributi,
        "last_changed": "2026-11-13T10:00:00+00:00", "last_updated": "2026-11-13T10:00:00+00:00"
    })
}

fn errore(id: &Value, codice: &str, messaggio: &str) -> Value {
    json!({ "id": id, "type": "result", "success": false, "error": { "code": codice, "message": messaggio } })
}

/// Risposta a una richiesta; per `call_service` riuscito anche l'evento `state_changed`.
fn risposta(v: &Value, stati: &Stati) -> (Value, Option<Value>) {
    let id = v["id"].clone();
    let ok = |r: Value| json!({ "id": id, "type": "result", "success": true, "result": r });
    let stati_ora = stati.lock().unwrap();
    match v["type"].as_str().unwrap_or("") {
        "get_states" => (ok(stati_ora.iter().map(|(id, e)| stato_json(id, e)).collect()), None),
        "config/area_registry/list" => (
            ok(json!([{ "area_id": "soggiorno", "name": "Soggiorno" }, { "area_id": "cucina", "name": "Cucina" }])),
            None,
        ),
        "config/device_registry/list" => (ok(json!([])), None),
        "config/entity_registry/list" => (
            ok(stati_ora
                .iter()
                .map(|(id, e)| {
                    json!({
                        "entity_id": id, "name": null, "platform": "finto", "device_id": null,
                        "area_id": e.area, "disabled_by": null, "hidden_by": null, "icon": null,
                        "entity_category": null
                    })
                })
                .collect()),
            None,
        ),
        "get_config" => (ok(json!({ "unit_system": { "temperature": "°C" }, "location_name": "Casa finta" })), None),
        "persistent_notification/get" => (ok(json!([])), None),
        "subscribe_events" => (ok(Value::Null), None),
        "ping" => (json!({ "id": id, "type": "pong" }), None),
        "call_service" => {
            drop(stati_ora);
            servizio(v, &id, stati)
        }
        _ => (errore(&id, "unknown_command", "Unknown command."), None),
    }
}

fn servizio(v: &Value, id: &Value, stati: &Stati) -> (Value, Option<Value>) {
    let entity_id = v["target"]["entity_id"].as_str().unwrap_or_default();
    let servizio = v["service"].as_str().unwrap_or_default();
    let mut stati = stati.lock().unwrap();
    let Some((&chiave, e)) = stati.iter_mut().find(|(k, _)| **k == entity_id) else {
        return (errore(id, "not_found", "Entità sconosciuta"), None);
    };
    if chiave == "switch.guasto" {
        return (errore(id, "home_assistant_error", "Dispositivo non raggiungibile"), None);
    }
    if e.stato == "unavailable" {
        return (errore(id, "home_assistant_error", "Entità non disponibile"), None);
    }
    let vecchio = stato_json(chiave, e);
    e.stato = match (chiave.split('.').next(), servizio) {
        (Some("scene"), "turn_on") => "2026-11-13T10:05:00+00:00".into(),
        (Some("light" | "switch"), "toggle") => if e.stato == "on" { "off" } else { "on" }.into(),
        (Some("light" | "switch"), "turn_on") => "on".into(),
        (Some("light" | "switch"), "turn_off") => "off".into(),
        _ => return (errore(id, "not_supported", "Servizio non supportato"), None),
    };
    println!("call_service {entity_id} {servizio} → {}", e.stato);
    let evento = json!({
        "id": 1, "type": "event",
        "event": { "event_type": "state_changed",
                   "data": { "entity_id": chiave, "old_state": vecchio, "new_state": stato_json(chiave, e) } }
    });
    (json!({ "id": id, "type": "result", "success": true, "result": { "context": {} } }), Some(evento))
}

async fn servi(
    tcp: tokio::net::TcpStream,
    stati: Stati,
    eventi_tx: broadcast::Sender<String>,
    notifica_dopo: Option<Duration>,
) {
    let mut eventi = eventi_tx.subscribe();
    let Ok(ws) = tokio_tungstenite::accept_async(tcp).await else { return };
    let (mut tx, mut rx) = ws.split();
    let testo = |v: Value| Message::Text(v.to_string().into());
    if tx.send(testo(json!({ "type": "auth_required", "ha_version": "2026.11.0" }))).await.is_err() {
        return;
    }
    loop {
        let Some(Ok(m)) = rx.next().await else { return };
        let Message::Text(t) = m else { continue };
        let Ok(v) = serde_json::from_str::<Value>(&t) else { continue };
        if v["type"] == "auth" {
            if v["access_token"] == TOKEN {
                let _ = tx.send(testo(json!({ "type": "auth_ok", "ha_version": "2026.11.0" }))).await;
                println!("client autenticato");
                break;
            }
            let _ = tx.send(testo(json!({ "type": "auth_invalid", "message": "Invalid access" }))).await;
            let _ = tx.close().await;
            return;
        }
    }
    if let Some(d) = notifica_dopo {
        let e = eventi_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(d).await;
            let v = json!({ "id": 1, "type": "event", "event": {
                "event_type": "persistent_notification_created",
                "data": { "notification_id": "prova", "title": "Lavatrice finita", "message": "Il ciclo è terminato" } } });
            let _ = e.send(v.to_string());
            println!("notifica mandata");
        });
    }
    loop {
        tokio::select! {
            m = rx.next() => {
                let Some(Ok(m)) = m else { return };
                let Message::Text(t) = m else { continue };
                let Ok(v) = serde_json::from_str::<Value>(&t) else { continue };
                let (r, evento) = risposta(&v, &stati);
                if tx.send(testo(r)).await.is_err() { return; }
                if let Some(e) = evento {
                    let _ = eventi_tx.send(e.to_string());
                }
            }
            e = eventi.recv() => {
                let Ok(e) = e else { return };
                if tx.send(Message::Text(e.into())).await.is_err() { return; }
            }
        }
    }
}

fn prepara(cartella: &str, porta: u16, con_preferiti: bool) -> Result<(), String> {
    let config = Config {
        environments: vec![HaEnvironment {
            id: AMBIENTE.into(),
            name: "Casa finta".into(),
            ha_url: format!("http://127.0.0.1:{porta}"),
        }],
        active_env_id: Some(AMBIENTE.into()),
        favorites: if con_preferiti {
            ["light.soggiorno", "light.cucina", "switch.presa", "sensor.temperatura", "scene.cinema", "light.giardino", "switch.guasto"]
                .map(String::from)
                .to_vec()
        } else {
            vec![]
        },
        ..Config::default()
    };
    ConfigStore::apri(cartella.as_ref()).salva(&config).map_err(|e| e.to_string())?;
    KeyringSegreti.salva(AMBIENTE, TOKEN)?;
    println!("configurazione di prova in {cartella}, token finto sotto \"{AMBIENTE}\"");
    Ok(())
}

fn main() -> Result<(), String> {
    let argomenti: Vec<String> = std::env::args().skip(1).collect();
    match argomenti.first().map(String::as_str) {
        Some("prepara") => {
            let cartella = argomenti.get(1).ok_or("manca la cartella")?;
            let porta = argomenti.get(2).and_then(|p| p.parse().ok()).ok_or("manca la porta")?;
            prepara(cartella, porta, argomenti.iter().any(|a| a == "--preferiti"))
        }
        Some("pulisci") => {
            KeyringSegreti.cancella(AMBIENTE)?;
            println!("token finto \"{AMBIENTE}\" cancellato");
            Ok(())
        }
        Some(porta) => {
            let porta: u16 = porta.parse().map_err(|_| "porta non valida".to_owned())?;
            let notifica_dopo = argomenti
                .iter()
                .position(|a| a == "--notifica-dopo")
                .and_then(|i| argomenti.get(i + 1))
                .and_then(|s| s.parse().ok())
                .map(Duration::from_secs);
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
            rt.block_on(async move {
                let listener = TcpListener::bind(("127.0.0.1", porta)).await.map_err(|e| e.to_string())?;
                println!("HA finto su ws://127.0.0.1:{porta}/api/websocket");
                let stati: Stati = Arc::new(Mutex::new(entita_iniziali()));
                let (eventi, _) = broadcast::channel(64);
                loop {
                    let (tcp, _) = listener.accept().await.map_err(|e| e.to_string())?;
                    tokio::spawn(servi(tcp, stati.clone(), eventi.clone(), notifica_dopo));
                }
            })
        }
        None => Err("uso: finto_ha <porta> [--notifica-dopo <s>] | prepara <cartella> <porta> [--preferiti] | pulisci".into()),
    }
}
