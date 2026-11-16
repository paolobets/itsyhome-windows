//! Home Assistant finto per le prove dal vivo dell'assistente (mai la casa vera).
//!
//! ```text
//! cargo run -p casa-core --example finto_ha -- prepara <cartella> <porta> [--preferiti]
//!     scrive <cartella>/assistente.json con l'ambiente "prova-locale" su ws://127.0.0.1:<porta>
//!     e mette un token FINTO nel portachiavi sotto "prova-locale"
//! cargo run -p casa-core --example finto_ha -- <porta> [--con-notifica] [--notifica-dopo <s>] [--rimuovi-dopo <s>] [--raffica <n>] [--casa-grande <n>] [--lenta <ms>]
//!     avvia il server (Ctrl+C o chiusura del processo per fermarlo).
//!     --con-notifica: una notifica persistente c'è già all'avvio ("aggiornamento");
//!     --notifica-dopo: dopo <s> secondi compare la notifica "lavatrice";
//!     --rimuovi-dopo: dopo <s> secondi HA stesso archivia la notifica "lavatrice".
//!     `persistent_notification.dismiss` la toglie e manda "removed", come l'HA vero.
//!     --raffica <n>: casa che si muove sempre, <n> state_changed al secondo su entità non mostrate
//!         più qualche cambio su entità mostrate (vedi `raffica`);
//!     --casa-grande <n>: <n> entità in più con attributi da casa vera (vedi `casa_grande`).
//!     --lenta <ms>: ogni comando risponde (e manda il suo state_changed) dopo <ms> millisecondi.
//!     --registro <s>: ogni <s> secondi un evento del registro (a chi vi è iscritto), come
//!         un'integrazione che riscrive il registro; serve a provare il registro diagnostico.
//!     --registro-tipo <t>: quale evento (Task 14h): «nome» (di serie: il nome di una luce
//!         mostrata, fa rileggere i soli registri), «inutile» (la versione di un dispositivo e le
//!         opzioni di un'entità: si scartano), «nuova» (un'entità creata: rilettura completa).
//! cargo run -p casa-core --example finto_ha -- pulisci
//!     cancella il token finto dal portachiavi
//! ```
//! Poi: `ITSYHOME_ASSISTENTE_DIR=<cartella>` per l'assistente (niente import dalla v2).
//!
//! Meteo: `weather.casa` (`cloudy`, 9 °C); `weather.imposta` ne cambia condizione e temperatura.
//!
//! Entità: 2 luci (una accesa), 1 interruttore, 1 scena, 1 sensore di temperatura, una luce non
//! disponibile senza stanza (→ "Altro") e un interruttore che rifiuta ogni comando; in più sei
//! stanze (camera, cameretta, bagno, studio, ingresso, terrazzo) con luci, prese, sensori, una
//! tapparella, un termostato e una serratura, per provare stanze, ordine, nascoste e icone.
//! `call_service` cambia lo stato finto e manda il cambio come l'HA vero: `state_changed` a chi
//! è iscritto a quell'evento, e in forma compressa (`subscribe_entities`, `a`/`c`/`r`) a chi segue
//! quell'entità. Stessa logica del server finto dei test d'integrazione (`tests/finto_ha.rs`), con
//! uno stato che cambia. Notifiche persistenti come `persistent_notification/subscribe` di HA:
//! "current" all'iscrizione, poi "added" e "removed" con l'id della richiesta d'iscrizione.
use std::collections::{BTreeMap, BTreeSet};
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
    nome: String,
    area: Option<String>,
    /// `entity_category` del registro («diagnostic»/«config»: entità che l'app non mostra).
    categoria: Option<&'static str>,
    attributi: Value,
    /// `last_updated`, in secondi dalle 10:00 del 13/11/2026: a ogni scrittura prende `adesso()`.
    aggiornata: u64,
}

impl Entita {
    fn new(stato: &str, nome: &str, area: Option<&str>, attributi: Value) -> Entita {
        Entita {
            stato: stato.into(),
            nome: nome.into(),
            area: area.map(str::to_owned),
            categoria: None,
            attributi,
            aggiornata: 0,
        }
    }
}

type Stati = Arc<Mutex<BTreeMap<String, Entita>>>;

/// L'orologio finto di `last_updated`: un contatore unico per tutto il server, sempre in avanti
/// (raffica e comandi scrivono le stesse entità, e il servizio scarta gli eventi «più vecchi»).
static OROLOGIO: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn adesso() -> u64 {
    OROLOGIO.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1
}
/// Notifiche persistenti: id → (titolo, messaggio).
type Notifiche = Arc<Mutex<BTreeMap<String, (String, String)>>>;

/// Il corpo di un evento d'iscrizione alle notifiche (`{"type": ..., "notifications": {...}}`).
fn corpo_notifiche(tipo: &str, n: &BTreeMap<String, (String, String)>) -> Value {
    let notifiche: serde_json::Map<String, Value> = n
        .iter()
        .map(|(id, (titolo, messaggio))| {
            let v = json!({ "notification_id": id, "title": titolo, "message": messaggio,
                            "created_at": "2026-11-13T10:00:00+00:00" });
            (id.clone(), v)
        })
        .collect();
    json!({ "type": tipo, "notifications": notifiche })
}

fn aggiungi_notifica(n: &Notifiche, tx: &broadcast::Sender<Value>, id: &str, titolo: &str, messaggio: &str) {
    let mut n = n.lock().unwrap();
    let tipo = if n.contains_key(id) { "updated" } else { "added" };
    n.insert(id.into(), (titolo.into(), messaggio.into()));
    let una: BTreeMap<_, _> = [(id.to_owned(), (titolo.to_owned(), messaggio.to_owned()))].into();
    let _ = tx.send(corpo_notifiche(tipo, &una));
    println!("notifica {id} {tipo}");
}

fn togli_notifica(n: &Notifiche, tx: &broadcast::Sender<Value>, id: &str) -> bool {
    let Some(v) = n.lock().unwrap().remove(id) else { return false };
    let una: BTreeMap<_, _> = [(id.to_owned(), v)].into();
    let _ = tx.send(corpo_notifiche("removed", &una));
    println!("notifica {id} removed");
    true
}

fn entita_iniziali() -> BTreeMap<String, Entita> {
    let mut m = BTreeMap::new();
    let mut e = |id: &str, stato: &str, nome: &str, area: Option<&str>, attributi: Value| {
        m.insert(id.to_owned(), Entita::new(stato, nome, area, attributi));
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
    // Le altre stanze (Task 14e: stanze nella vista Casa e gestione nelle Impostazioni).
    let luce = json!({ "supported_color_modes": ["brightness"], "color_mode": "brightness", "brightness": 150 });
    let temperatura = |v: &str| (v.to_owned(), json!({ "device_class": "temperature", "unit_of_measurement": "°C" }));
    e("light.camera", "off", "Luce camera", Some("camera"), luce.clone());
    e("light.abat_jour", "on", "Abat-jour", Some("camera"), luce.clone());
    e("switch.coperta", "off", "Coperta elettrica", Some("camera"), json!({}));
    e(
        "cover.tapparella_camera",
        "open",
        "Tapparella camera",
        Some("camera"),
        json!({ "current_position": 100, "supported_features": 15, "device_class": "shutter" }),
    );
    e("light.cameretta", "off", "Luce cameretta", Some("cameretta"), luce.clone());
    e("switch.lucina", "on", "Lucina notturna", Some("cameretta"), json!({}));
    let (v, a) = temperatura("20.1");
    e("sensor.temperatura_cameretta", &v, "Temperatura cameretta", Some("cameretta"), a);
    e("light.bagno", "off", "Luce bagno", Some("bagno"), luce.clone());
    e("switch.scaldasalviette", "on", "Scaldasalviette", Some("bagno"), json!({}));
    e(
        "sensor.umidita_bagno",
        "64",
        "Umidità bagno",
        Some("bagno"),
        json!({ "device_class": "humidity", "unit_of_measurement": "%" }),
    );
    e("light.studio", "on", "Luce studio", Some("studio"), luce.clone());
    e("switch.monitor", "on", "Monitor", Some("studio"), json!({}));
    e(
        "climate.studio",
        "heat",
        "Termostato studio",
        Some("studio"),
        json!({ "hvac_modes": ["off", "heat"], "current_temperature": 20.5, "temperature": 21, "hvac_action": "heating" }),
    );
    e("light.ingresso", "off", "Luce ingresso", Some("ingresso"), luce.clone());
    e("lock.porta", "locked", "Porta d'ingresso", Some("ingresso"), json!({}));
    e("light.terrazzo", "off", "Luce terrazzo", Some("terrazzo"), luce);
    let (v, a) = temperatura("9.4");
    e("sensor.temperatura_esterna", &v, "Temperatura esterna", Some("terrazzo"), a);
    e("scene.notte", "2026-11-13T10:00:00+00:00", "Notte", None, json!({}));
    // Il meteo (Task 2 della M2): non entra in stanze né preferiti. Per cambiarlo:
    // `weather.imposta` con `service_data` `{ "state": "rainy", "temperature": 12 }`.
    e("weather.casa", "cloudy", "Meteo casa", None, json!({ "temperature": 9, "temperature_unit": "°C" }));
    m
}

/// Le stanze della casa finta (`config/area_registry/list`).
const STANZE: [(&str, &str); 8] = [
    ("soggiorno", "Soggiorno"),
    ("cucina", "Cucina"),
    ("camera", "Camera da letto"),
    ("cameretta", "Cameretta"),
    ("bagno", "Bagno"),
    ("studio", "Studio"),
    ("ingresso", "Ingresso"),
    ("terrazzo", "Terrazzo"),
];

fn stato_json(id: &str, e: &Entita) -> Value {
    let mut attributi = e.attributi.clone();
    attributi["friendly_name"] = json!(e.nome);
    // Dal 13/11/2026 alle 10:00 in avanti, giorno compreso (il mese basta per 17 giorni di prova).
    let secondi = 10 * 3600 + e.aggiornata;
    let (giorno, resto) = (13 + secondi / 86_400, secondi % 86_400);
    let quando = format!(
        "2026-11-{giorno:02}T{:02}:{:02}:{:02}+00:00",
        resto / 3600,
        resto % 3600 / 60,
        resto % 60
    );
    json!({
        "entity_id": id, "state": e.stato, "attributes": attributi,
        "last_changed": "2026-11-13T10:00:00+00:00", "last_updated": quando
    })
}

/// L'evento `state_changed` di un'entità appena cambiata, come lo manda HA
/// (`{"event_type": "state_changed", "data": {"entity_id", "old_state", "new_state"}}`).
fn evento_cambio(id: &str, vecchio: Value, e: &Entita) -> Value {
    json!({
        "id": 1, "type": "event",
        "event": { "event_type": "state_changed",
                   "data": { "entity_id": id, "old_state": vecchio, "new_state": stato_json(id, e) } }
    })
}

/// I secondi dall'epoca di un `last_updated` finto (`2026-11-DDTHH:MM:SS+00:00`), per gli stati
/// compressi. Non serve esatto: solo crescente.
fn epoca(quando: &Value) -> f64 {
    let t = quando.as_str().unwrap_or_default();
    let n = |a: usize, b: usize| t.get(a..b).and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
    1_762_992_000.0 + (n(8, 10) - 13.0) * 86_400.0 + n(11, 13) * 3600.0 + n(14, 16) * 60.0 + n(17, 19)
}

/// Uno stato in forma compressa di `subscribe_entities` (`s`, `a`, `c`, `lc`, `lu`).
fn compresso(s: &Value) -> Value {
    json!({ "s": s["state"], "a": s["attributes"], "c": "contesto-finto",
            "lc": epoca(&s["last_changed"]), "lu": epoca(&s["last_updated"]) })
}

/// Il corpo compresso di `subscribe_entities` per un `state_changed`, come `_state_diff_event` di
/// HA (`websocket_api/messages.py`): `r` se lo stato è tolto, `a` se è nuovo, altrimenti `c` con
/// sotto `+` lo stato (se cambia), gli attributi cambiati e il tempo, e sotto `-` gli attributi tolti.
fn diff_compresso(data: &Value) -> Value {
    let id = data["entity_id"].as_str().unwrap_or_default();
    let (vecchio, nuovo) = (&data["old_state"], &data["new_state"]);
    if nuovo.is_null() {
        return json!({ "r": [id] });
    }
    if vecchio.is_null() {
        return json!({ "a": { id: compresso(nuovo) } });
    }
    let mut piu = serde_json::Map::new();
    if nuovo["state"] != vecchio["state"] {
        piu.insert("s".into(), nuovo["state"].clone());
        piu.insert("lc".into(), json!(epoca(&nuovo["last_updated"])));
    } else {
        piu.insert("lu".into(), json!(epoca(&nuovo["last_updated"])));
    }
    let (va, na) = (vecchio["attributes"].as_object(), nuovo["attributes"].as_object());
    let cambiati: serde_json::Map<String, Value> = na
        .into_iter()
        .flatten()
        .filter(|(k, v)| va.and_then(|va| va.get(*k)) != Some(*v))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if !cambiati.is_empty() {
        piu.insert("a".into(), Value::Object(cambiati));
    }
    let tolti: Vec<&String> =
        va.into_iter().flatten().map(|(k, _)| k).filter(|k| na.is_none_or(|na| !na.contains_key(*k))).collect();
    let mut diff = json!({ "+": piu });
    if !tolti.is_empty() {
        diff["-"] = json!({ "a": tolti });
    }
    json!({ "c": { id: diff } })
}

/// `--casa-grande <n>`: `n` entità in più con attributi come quelli di una casa vera (luci a
/// colori, prese, consumi, temperature, movimento, lettori multimediali, automazioni, segnali in
/// diagnostica), divise in dieci stanze in più. Serve a misurare la memoria con una casa pesante.
fn casa_grande(m: &mut BTreeMap<String, Entita>, n: usize) {
    for i in 0..n {
        let area = format!("stanza_{}", i % 10);
        let area = Some(area.as_str());
        let (id, e) = match i % 8 {
            0 => {
                let attributi = json!({
                    "supported_color_modes": ["color_temp", "xy"], "color_mode": "xy", "brightness": 180,
                    "hs_color": [30.0, 60.0], "rgb_color": [255, 180, 100], "xy_color": [0.5, 0.4],
                    "color_temp_kelvin": 3000, "min_color_temp_kelvin": 2000, "max_color_temp_kelvin": 6500,
                    "min_mireds": 153, "max_mireds": 500,
                    "effect_list": ["colorloop", "random", "candle", "fireplace"], "supported_features": 44
                });
                let stato = if i % 3 == 0 { "on" } else { "off" };
                (format!("light.grande_{i}"), Entita::new(stato, &format!("Luce {i}"), area, attributi))
            }
            1 => (
                format!("switch.grande_{i}"),
                Entita::new("off", &format!("Presa {i}"), area, json!({ "device_class": "outlet" })),
            ),
            2 => {
                let attributi = json!({ "state_class": "measurement", "unit_of_measurement": "W", "device_class": "power" });
                (format!("sensor.potenza_{i}"), Entita::new("42.7", &format!("Potenza {i}"), area, attributi))
            }
            3 => {
                let attributi =
                    json!({ "state_class": "measurement", "unit_of_measurement": "°C", "device_class": "temperature" });
                (format!("sensor.temperatura_{i}"), Entita::new("20.4", &format!("Temperatura {i}"), area, attributi))
            }
            4 => (
                format!("binary_sensor.movimento_{i}"),
                Entita::new("off", &format!("Movimento {i}"), area, json!({ "device_class": "motion" })),
            ),
            5 => {
                let attributi = json!({
                    "source_list": (0..40).map(|k| format!("Sorgente {k}")).collect::<Vec<_>>(),
                    "sound_mode_list": ["Film", "Musica", "Notte", "Diretta"], "volume_level": 0.3,
                    "is_volume_muted": false,
                    "entity_picture": format!("/api/media_player_proxy/media_player.grande_{i}?token=0123456789abcdef&cache=1"),
                    "supported_features": 152463
                });
                (format!("media_player.grande_{i}"), Entita::new("idle", &format!("Lettore {i}"), area, attributi))
            }
            6 => {
                let attributi = json!({
                    "id": format!("{i:013}"), "last_triggered": "2026-11-13T09:00:00+00:00", "mode": "single", "current": 0
                });
                (format!("automation.grande_{i}"), Entita::new("on", &format!("Automazione {i}"), None, attributi))
            }
            _ => {
                let attributi =
                    json!({ "state_class": "measurement", "unit_of_measurement": "dBm", "device_class": "signal_strength" });
                let mut e = Entita::new("-61", &format!("Segnale {i}"), area, attributi);
                e.categoria = Some("diagnostic");
                (format!("sensor.segnale_{i}"), e)
            }
        };
        m.insert(id, e);
    }
}

/// Le entità che l'app non mostra e che la raffica fa cambiare: due segnali in diagnostica, un
/// sensore di movimento (dominio escluso) e una potenza (sensore che il mapper non tiene).
const NASCOSTE: [&str; 4] =
    ["sensor.segnale_wifi", "sensor.segnale_zigbee", "binary_sensor.movimento_corridoio", "sensor.potenza_frigo"];

fn entita_della_raffica(m: &mut BTreeMap<String, Entita>) {
    for id in NASCOSTE {
        let (nome, attributi) = match id {
            "binary_sensor.movimento_corridoio" => ("Movimento corridoio", json!({ "device_class": "motion" })),
            "sensor.potenza_frigo" => ("Potenza frigo", json!({ "unit_of_measurement": "W", "device_class": "power" })),
            _ => ("Segnale", json!({ "unit_of_measurement": "dBm", "device_class": "signal_strength" })),
        };
        let mut e = Entita::new("-50", nome, Some("soggiorno"), attributi);
        if id.starts_with("sensor.segnale") {
            e.categoria = Some("diagnostic");
        }
        m.insert(id.to_owned(), e);
    }
}

/// `--raffica <n>`: una casa che si muove sempre. `n` `state_changed` al secondo su entità che
/// l'app non mostra, più qualche cambio su entità mostrate: ogni 2 s la temperatura del soggiorno
/// (si vede), ogni secondo un attributo che l'app non usa della luce del soggiorno (non si vede:
/// cambiano solo quell'attributo e `last_updated`).
async fn raffica(stati: Stati, tx: broadcast::Sender<String>, al_secondo: u32) {
    let al_secondo = u64::from(al_secondo.max(1));
    let mut passo = tokio::time::interval(Duration::from_secs_f64(1.0 / al_secondo as f64));
    let mut giro: u64 = 0;
    loop {
        passo.tick().await;
        giro += 1;
        let mut cambi: Vec<&str> = vec![NASCOSTE[(giro % 4) as usize]];
        if giro.is_multiple_of(2 * al_secondo) {
            cambi.push("sensor.temperatura");
        }
        if giro % al_secondo == al_secondo / 2 {
            cambi.push("light.soggiorno");
        }
        let mut stati = stati.lock().unwrap();
        for id in cambi {
            let Some(e) = stati.get_mut(id) else { continue };
            let vecchio = stato_json(id, e);
            e.aggiornata = adesso();
            match id {
                "sensor.temperatura" => e.stato = if e.stato == "21.5" { "21.6" } else { "21.5" }.into(),
                "light.soggiorno" => e.attributi["linkquality"] = json!(giro % 255),
                "binary_sensor.movimento_corridoio" => e.stato = if e.stato == "on" { "off" } else { "on" }.into(),
                _ => e.stato = format!("-{}", 40 + giro % 30),
            }
            let _ = tx.send(evento_cambio(id, vecchio, e).to_string());
        }
    }
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
        "config/area_registry/list" => {
            let mut aree: Vec<Value> = STANZE.iter().map(|(id, nome)| json!({ "area_id": id, "name": nome })).collect();
            // Le stanze della casa grande (vedi `casa_grande`).
            if stati_ora.keys().any(|k| k.contains("_grande_") || k.contains(".potenza_") || k.contains(".segnale_")) {
                aree.extend((0..10).map(|i| json!({ "area_id": format!("stanza_{i}"), "name": format!("Stanza {i}") })));
            }
            (ok(json!(aree)), None)
        }
        "config/device_registry/list" => (ok(json!([])), None),
        "config/entity_registry/list" => (
            ok(stati_ora
                .iter()
                .map(|(id, e)| {
                    json!({
                        "entity_id": id, "name": null, "platform": "finto", "device_id": null,
                        "area_id": e.area, "disabled_by": null, "hidden_by": null, "icon": null,
                        "entity_category": e.categoria
                    })
                })
                .collect()),
            None,
        ),
        "get_config" => (ok(json!({ "unit_system": { "temperature": "°C" }, "location_name": "Casa finta" })), None),
        "persistent_notification/get" => (ok(json!([])), None),
        "subscribe_events" | "unsubscribe_events" => (ok(Value::Null), None),
        "ping" => (json!({ "id": id, "type": "pong" }), None),
        "call_service" => {
            drop(stati_ora);
            servizio(v, &id, stati)
        }
        _ => (errore(&id, "unknown_command", "Unknown command."), None),
    }
}

/// `persistent_notification.dismiss`: toglie la notifica (e manda "removed"), come HA.
fn archivia(v: &Value, n: &Notifiche, tx: &broadcast::Sender<Value>) -> Value {
    let nid = v["service_data"]["notification_id"].as_str().unwrap_or_default();
    println!("call_service persistent_notification dismiss {nid}");
    // Come HA: un id sconosciuto non è un errore.
    togli_notifica(n, tx, nid);
    json!({ "id": v["id"], "type": "result", "success": true, "result": { "context": {} } })
}

fn servizio(v: &Value, id: &Value, stati: &Stati) -> (Value, Option<Value>) {
    let entity_id = v["target"]["entity_id"].as_str().unwrap_or_default();
    let servizio = v["service"].as_str().unwrap_or_default();
    let mut stati = stati.lock().unwrap();
    let Some((chiave, e)) = stati.iter_mut().find(|(k, _)| k.as_str() == entity_id) else {
        return (errore(id, "not_found", "Entità sconosciuta"), None);
    };
    let chiave = chiave.as_str();
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
        // Solo del finto HA (in HA le `weather.*` non hanno servizi per questo): cambia condizione
        // e temperatura del meteo.
        (Some("weather"), "imposta") => {
            if let Some(t) = v["service_data"]["temperature"].as_f64() {
                e.attributi["temperature"] = json!(t);
            }
            v["service_data"]["state"].as_str().map_or_else(|| e.stato.clone(), str::to_owned)
        }
        _ => return (errore(id, "not_supported", "Servizio non supportato"), None),
    };
    e.aggiornata = adesso();
    println!("call_service {entity_id} {servizio} → {}", e.stato);
    let evento = evento_cambio(chiave, vecchio, e);
    (json!({ "id": id, "type": "result", "success": true, "result": { "context": {} } }), Some(evento))
}

async fn servi(
    tcp: tokio::net::TcpStream,
    stati: Stati,
    eventi_tx: broadcast::Sender<String>,
    notifiche: Notifiche,
    notifiche_tx: broadcast::Sender<Value>,
    lenta: Duration,
) {
    let mut eventi = eventi_tx.subscribe();
    let mut aggiornamenti = notifiche_tx.subscribe();
    // Id della richiesta `persistent_notification/subscribe` di questo client.
    let mut iscrizione: Option<Value> = None;
    // Le altre iscrizioni: `state_changed` intero e `subscribe_entities` (id, entità seguite).
    let mut con_state_changed = false;
    // Gli altri eventi del bus seguiti (`subscribe_events`): tipo → id dell'iscrizione.
    let mut eventi_seguiti: BTreeMap<String, Value> = BTreeMap::new();
    let mut seguite: Option<(Value, BTreeSet<String>)> = None;
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
    loop {
        tokio::select! {
            m = rx.next() => {
                let Some(Ok(m)) = m else { return };
                let Message::Text(t) = m else { continue };
                let Ok(v) = serde_json::from_str::<Value>(&t) else { continue };
                if v["type"] == "subscribe_entities" {
                    // Come HA: il risultato, poi un evento con gli stati interi delle entità chieste.
                    let id = v["id"].clone();
                    let ids: BTreeSet<String> = v["entity_ids"]
                        .as_array()
                        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect())
                        .unwrap_or_default();
                    let a: serde_json::Map<String, Value> = stati
                        .lock()
                        .unwrap()
                        .iter()
                        .filter(|(k, _)| ids.is_empty() || ids.contains(*k))
                        .map(|(k, e)| (k.clone(), compresso(&stato_json(k, e))))
                        .collect();
                    println!("subscribe_entities: {} entità seguite", a.len());
                    let ok = json!({ "id": id, "type": "result", "success": true, "result": null });
                    let primo = json!({ "id": id, "type": "event", "event": { "a": a } });
                    if tx.send(testo(ok)).await.is_err() || tx.send(testo(primo)).await.is_err() { return; }
                    seguite = Some((id, ids));
                    continue;
                }
                if v["type"] == "unsubscribe_events" && seguite.as_ref().is_some_and(|(id, _)| *id == v["subscription"]) {
                    seguite = None;
                }
                if v["type"] == "subscribe_events" && v["event_type"] == "state_changed" {
                    con_state_changed = true;
                } else if v["type"] == "subscribe_events" {
                    if let Some(t) = v["event_type"].as_str() {
                        eventi_seguiti.insert(t.to_owned(), v["id"].clone());
                    }
                }
                if v["type"] == "persistent_notification/subscribe" {
                    let id = v["id"].clone();
                    let attuali = corpo_notifiche("current", &notifiche.lock().unwrap());
                    let ok = json!({ "id": id, "type": "result", "success": true, "result": null });
                    let evento = json!({ "id": id, "type": "event", "event": attuali });
                    if tx.send(testo(ok)).await.is_err() || tx.send(testo(evento)).await.is_err() { return; }
                    iscrizione = Some(id);
                    continue;
                }
                if v["type"] == "call_service" && v["domain"] == "persistent_notification" && v["service"] == "dismiss" {
                    let r = archivia(&v, &notifiche, &notifiche_tx);
                    if tx.send(testo(r)).await.is_err() { return; }
                    continue;
                }
                if v["type"] == "call_service" && !lenta.is_zero() {
                    // Una casa lenta: risposta e state_changed arrivano dopo `lenta`.
                    tokio::time::sleep(lenta).await;
                }
                let (r, evento) = risposta(&v, &stati);
                if tx.send(testo(r)).await.is_err() { return; }
                if let Some(e) = evento {
                    let _ = eventi_tx.send(e.to_string());
                }
            }
            e = eventi.recv() => {
                let e = match e {
                    Ok(e) => e,
                    // Un client lento perde qualche evento (come con un HA vero sotto carico), non la connessione.
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => return,
                };
                // Come HA: a ognuno solo ciò a cui è iscritto. Le entità che non segue non gli arrivano.
                let Ok(mut v) = serde_json::from_str::<Value>(&e) else { continue };
                // Un evento del bus che non è un cambio di stato: a chi è iscritto a quel tipo.
                if let Some(t) = v["event"]["event_type"].as_str().filter(|t| *t != "state_changed") {
                    if let Some(id) = eventi_seguiti.get(t) {
                        v["id"] = id.clone();
                        if tx.send(testo(v)).await.is_err() { return; }
                    }
                    continue;
                }
                if con_state_changed && tx.send(Message::Text(e.into())).await.is_err() { return; }
                let data = &v["event"]["data"];
                let Some((id, ids)) = &seguite else { continue };
                if ids.is_empty() || ids.contains(data["entity_id"].as_str().unwrap_or_default()) {
                    let compresso = json!({ "id": id, "type": "event", "event": diff_compresso(data) });
                    if tx.send(testo(compresso)).await.is_err() { return; }
                }
            }
            n = aggiornamenti.recv() => {
                let Ok(corpo) = n else { return };
                let Some(id) = &iscrizione else { continue };
                let evento = json!({ "id": id, "type": "event", "event": corpo });
                if tx.send(testo(evento)).await.is_err() { return; }
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
            let secondi = |nome: &str| {
                argomenti
                    .iter()
                    .position(|a| a == nome)
                    .and_then(|i| argomenti.get(i + 1))
                    .and_then(|s| s.parse().ok())
                    .map(Duration::from_secs)
            };
            let (notifica_dopo, rimuovi_dopo) = (secondi("--notifica-dopo"), secondi("--rimuovi-dopo"));
            let con_notifica = argomenti.iter().any(|a| a == "--con-notifica");
            let numero = |nome: &str| -> Option<u32> {
                let i = argomenti.iter().position(|a| a == nome)?;
                argomenti.get(i + 1).and_then(|s| s.parse().ok())
            };
            let raffica_al_secondo = numero("--raffica");
            let lenta = Duration::from_millis(numero("--lenta").unwrap_or(0).into());
            let registro_ogni = secondi("--registro");
            let registro_tipo = argomenti
                .iter()
                .position(|a| a == "--registro-tipo")
                .and_then(|i| argomenti.get(i + 1))
                .cloned()
                .unwrap_or_else(|| "nome".into());
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
            rt.block_on(async move {
                let listener = TcpListener::bind(("127.0.0.1", porta)).await.map_err(|e| e.to_string())?;
                println!("HA finto su ws://127.0.0.1:{porta}/api/websocket");
                let mut iniziali = entita_iniziali();
                if let Some(n) = numero("--casa-grande") {
                    casa_grande(&mut iniziali, n as usize);
                }
                if raffica_al_secondo.is_some() {
                    entita_della_raffica(&mut iniziali);
                }
                println!("{} entità", iniziali.len());
                let stati: Stati = Arc::new(Mutex::new(iniziali));
                let (eventi, _) = broadcast::channel(1024);
                if let Some(n) = raffica_al_secondo {
                    tokio::spawn(raffica(stati.clone(), eventi.clone(), n));
                }
                if let Some(ogni) = registro_ogni {
                    let tx = eventi.clone();
                    tokio::spawn(async move {
                        let mut passo = tokio::time::interval_at(tokio::time::Instant::now() + ogni, ogni);
                        loop {
                            passo.tick().await;
                            // Come HA: `changes` coi valori di prima dei soli campi cambiati.
                            let eventi = match registro_tipo.as_str() {
                                "inutile" => vec![
                                    ("device_registry_updated", json!({ "action": "update", "device_id": "finto", "changes": { "sw_version": "1.0" } })),
                                    ("entity_registry_updated", json!({ "action": "update", "entity_id": "light.soggiorno", "changes": { "options": {} } })),
                                ],
                                "nuova" => vec![("entity_registry_updated", json!({ "action": "create", "entity_id": "light.soggiorno" }))],
                                _ => vec![("entity_registry_updated", json!({ "action": "update", "entity_id": "light.soggiorno", "changes": { "name": null } }))],
                            };
                            for (tipo, data) in eventi {
                                let e = json!({ "type": "event", "event": { "event_type": tipo, "data": data } });
                                let _ = tx.send(e.to_string());
                            }
                        }
                    });
                }
                let notifiche: Notifiche = Arc::default();
                let (notifiche_tx, _) = broadcast::channel(64);
                if con_notifica {
                    let testo = "Disponibile Home Assistant 2026.11.1";
                    notifiche.lock().unwrap().insert("aggiornamento".into(), ("Aggiornamento".into(), testo.into()));
                }
                if let Some(d) = notifica_dopo {
                    let (n, tx) = (notifiche.clone(), notifiche_tx.clone());
                    tokio::spawn(async move {
                        tokio::time::sleep(d).await;
                        aggiungi_notifica(&n, &tx, "lavatrice", "Lavatrice finita", "Il ciclo è terminato: puoi stendere.");
                    });
                }
                if let Some(d) = rimuovi_dopo {
                    let (n, tx) = (notifiche.clone(), notifiche_tx.clone());
                    tokio::spawn(async move {
                        tokio::time::sleep(d).await;
                        togli_notifica(&n, &tx, "lavatrice");
                    });
                }
                loop {
                    let (tcp, _) = listener.accept().await.map_err(|e| e.to_string())?;
                    tokio::spawn(servi(tcp, stati.clone(), eventi.clone(), notifiche.clone(), notifiche_tx.clone(), lenta));
                }
            })
        }
        None => Err("uso: finto_ha <porta> [--con-notifica] [--notifica-dopo <s>] [--rimuovi-dopo <s>] [--raffica <n>] [--casa-grande <n>] [--lenta <ms>] [--registro <s>] [--registro-tipo nome|inutile|nuova] | prepara <cartella> <porta> [--preferiti] | pulisci".into()),
    }
}
