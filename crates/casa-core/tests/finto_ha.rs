//! `CasaService` contro un Home Assistant finto (WebSocket locale su 127.0.0.1:0).
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use casa_core::config::{Config, ConfigStore};
use casa_core::eventi::{CasaEvento, StatoConnessione};
use casa_core::segreti::{MemoriaSegreti, Segreti};
use casa_core::servizio::CasaService;
use casa_core::types::{AppEntity, HaEnvironment, MenuData, TestConnectionResult};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc};
use tokio::time::{sleep, sleep_until, timeout, timeout_at, Instant};
use tokio_tungstenite::tungstenite::Message;

const TOKEN: &str = "token-finto-di-prova";
/// Messaggio interno del finto HA: chiude le connessioni aperte.
const CHIUDI: &str = "__chiudi__";
const RITARDO_TEST: Duration = Duration::from_millis(50);

// ─── Server finto ─────────────────────────────────────────────────────────────

#[derive(Default)]
struct Contatori {
    connessioni: AtomicUsize,
    get_states: AtomicUsize,
    /// Id della richiesta `persistent_notification/subscribe` (0 = nessuna iscrizione).
    iscrizione_notifiche: AtomicU64,
    /// `notification_id` ricevuti con `persistent_notification.dismiss`.
    archiviate: Mutex<Vec<String>>,
    /// Le notifiche presenti, mandate come "current" a ogni iscrizione.
    presenti: Mutex<Value>,
    /// Un cambio di stato (`data` di `state_changed`) che avviene subito dopo la risposta a
    /// `get_states` (una volta).
    cambio_dopo_stati: Mutex<Option<Value>>,
    /// Gli stati di adesso: `get_states` e il primo evento di `subscribe_entities` li leggono,
    /// `state_changed` li cambia.
    stati: Mutex<BTreeMap<String, Value>>,
    /// Gli `entity_ids` di ogni `subscribe_entities` ricevuta, in ordine.
    iscrizioni_entita: Mutex<Vec<Vec<String>>>,
    /// Quante `unsubscribe_events` ricevute.
    disiscrizioni: AtomicUsize,
    /// Qualcuno si è iscritto a `state_changed` (non deve più succedere).
    iscritto_state_changed: AtomicBool,
    /// Ritardo della risposta al registro delle stanze (le altre letture restano in volo).
    ritardo_aree_ms: AtomicU64,
    letture_registri: AtomicUsize,
    /// Quante risposte a `get_states` rifiutare ancora (errore di HA).
    stati_da_rifiutare: AtomicUsize,
    /// La presa nascosta dall'utente nel registro delle entità (`hidden_by: "user"`).
    presa_nascosta: AtomicBool,
    /// Voci in più nel registro delle entità (anche senza stato: integrazione non ancora caricata).
    voci_extra: Mutex<Vec<Value>>,
    /// Lo stato del core in `get_config` («RUNNING» se vuoto).
    stato_core: Mutex<String>,
    /// Quante `get_config` ricevute.
    letture_config: AtomicUsize,
    /// Nomi scelti nel registro delle entità (`name`), per entity_id.
    nomi_registro: Mutex<BTreeMap<String, String>>,
    /// Il registro dei dispositivi (vuoto di serie) e stanze in più.
    dispositivi: Mutex<Vec<Value>>,
    aree_extra: Mutex<Vec<Value>>,
    /// Voci del registro delle entità che sostituiscono quelle di serie, per entity_id.
    voci_sostituite: Mutex<BTreeMap<String, Value>>,
    /// Subito dopo la risposta a `config/device_registry/list` (una volta): il registro dei
    /// dispositivi diventa questo e parte questo evento (testo intero del messaggio).
    dopo_dispositivi: Mutex<Option<(Vec<Value>, String)>>,
    /// Ritardo della risposta al registro delle entità.
    ritardo_voci_ms: AtomicU64,
    /// Quante risposte a `config/entity_registry/list` rifiutare ancora.
    voci_da_rifiutare: AtomicUsize,
    /// Senza le voci di serie (luce e presa) nel registro delle entità.
    senza_voci_di_serie: AtomicBool,
    /// Alla prossima `get_states` la connessione cade senza risposta (una volta).
    cadi_su_get_states: AtomicBool,
}

struct FintoHa {
    url: String,
    contatori: Arc<Contatori>,
    eventi: broadcast::Sender<String>,
}

impl FintoHa {
    async fn avvia() -> FintoHa {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let contatori = Arc::new(Contatori::default());
        *contatori.stati.lock().unwrap() = [stato("light.cucina", "on", "Luce cucina"), stato("switch.presa", "off", "Presa")]
            .into_iter()
            .map(|s| (s["entity_id"].as_str().unwrap().to_owned(), s))
            .collect();
        let (eventi, _) = broadcast::channel(512);
        let (c, e) = (contatori.clone(), eventi.clone());
        tokio::spawn(async move {
            while let Ok((tcp, _)) = listener.accept().await {
                c.connessioni.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(servi(tcp, c.clone(), e.subscribe()));
            }
        });
        FintoHa { url, contatori, eventi }
    }

    /// Un evento del bus di HA. `state_changed` cambia anche gli stati di adesso (`old_state` lo
    /// prende da lì) e ogni client lo riceve come lo chiede: intero se iscritto a `state_changed`,
    /// compresso se l'entità è fra quelle del suo `subscribe_entities`.
    fn manda_evento(&self, tipo: &str, mut data: Value) {
        if tipo == "state_changed" {
            data = cambia_stato(&self.contatori, data);
        }
        let v = json!({ "id": 1, "type": "event", "event": { "event_type": tipo, "data": data } });
        let _ = self.eventi.send(v.to_string());
    }

    /// Chiude tutte le connessioni aperte (il client si riconnette da solo).
    fn chiudi_connessioni(&self) {
        let _ = self.eventi.send(CHIUDI.to_owned());
    }

    fn letture_config(&self) -> usize {
        self.contatori.letture_config.load(Ordering::SeqCst)
    }

    fn iscrizioni_entita(&self) -> Vec<Vec<String>> {
        self.contatori.iscrizioni_entita.lock().unwrap().clone()
    }

    /// Evento della iscrizione `persistent_notification/subscribe`, come lo manda HA
    /// (`websocket_api.event_message(msg_id, {"type": ..., "notifications": {...}})`).
    fn manda_notifiche(&self, tipo: &str, notifiche: Value) {
        let id = self.contatori.iscrizione_notifiche.load(Ordering::SeqCst);
        let v = json!({ "id": id, "type": "event", "event": { "type": tipo, "notifications": notifiche } });
        let _ = self.eventi.send(v.to_string());
    }

    async fn aspetta_iscrizione_notifiche(&self) {
        let inizio = Instant::now();
        while self.contatori.iscrizione_notifiche.load(Ordering::SeqCst) == 0 {
            assert!(inizio.elapsed() < Duration::from_secs(5), "nessuna iscrizione alle notifiche");
            sleep(Duration::from_millis(10)).await;
        }
    }

    fn metti_presenti(&self, notifiche: Value) {
        *self.contatori.presenti.lock().unwrap() = notifiche;
    }

    fn azzera_iscrizione(&self) {
        self.contatori.iscrizione_notifiche.store(0, Ordering::SeqCst);
    }

    fn archiviate(&self) -> Vec<String> {
        self.contatori.archiviate.lock().unwrap().clone()
    }

    fn get_states(&self) -> usize {
        self.contatori.get_states.load(Ordering::SeqCst)
    }

    fn connessioni(&self) -> usize {
        self.contatori.connessioni.load(Ordering::SeqCst)
    }

    fn letture_registri(&self) -> usize {
        self.contatori.letture_registri.load(Ordering::SeqCst)
    }
}

fn entry(entity_id: &str, area: &str) -> Value {
    json!({
        "entity_id": entity_id, "name": null, "platform": "finto", "device_id": null,
        "area_id": area, "disabled_by": null, "hidden_by": null, "icon": null, "entity_category": null
    })
}

fn stato(entity_id: &str, state: &str, nome: &str) -> Value {
    json!({
        "entity_id": entity_id, "state": state, "attributes": { "friendly_name": nome },
        "last_changed": "2026-11-13T10:00:00+00:00", "last_updated": "2026-11-13T10:00:00+00:00"
    })
}

/// Mette `data.new_state` negli stati di adesso (lo toglie se null) e restituisce `data` con
/// l'`old_state` vero. Uno stato senza `state` testuale (illeggibile) non entra negli stati.
fn cambia_stato(contatori: &Contatori, mut data: Value) -> Value {
    let id = data["entity_id"].as_str().unwrap_or_default().to_owned();
    let mut stati = contatori.stati.lock().unwrap();
    let vecchio = stati.get(&id).cloned().unwrap_or(Value::Null);
    let nuovo = &data["new_state"];
    if nuovo.is_null() {
        stati.remove(&id);
    } else if nuovo["state"].is_string() {
        stati.insert(id, nuovo.clone());
    }
    data["old_state"] = vecchio;
    data
}

/// Uno stato in forma compressa (`s`, `a`, `c`, `lc`, `lu`), come lo manda `subscribe_entities`.
fn compresso(s: &Value) -> Value {
    json!({ "s": s["state"], "a": s["attributes"], "c": "contesto", "lc": 1763028000.0, "lu": 1763028005.25 })
}

/// Il corpo dell'evento compresso di `subscribe_entities` per un `state_changed` (come
/// `_state_diff_event` di HA): `r` se lo stato è tolto, `a` se è nuovo, altrimenti `c` con sotto
/// `+` lo stato e gli attributi cambiati e sotto `-` quelli tolti.
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
        piu.insert("lc".into(), json!(1763028010.0));
    } else {
        piu.insert("lu".into(), json!(1763028010.5));
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
    let tolti: Vec<&String> = va.into_iter().flatten().map(|(k, _)| k).filter(|k| na.is_none_or(|na| !na.contains_key(*k))).collect();
    let mut diff = json!({ "+": piu });
    if !tolti.is_empty() {
        diff["-"] = json!({ "a": tolti });
    }
    json!({ "c": { id: diff } })
}

fn risposta(v: &Value, contatori: &Contatori) -> Value {
    let id = v["id"].clone();
    let ok = |r: Value| json!({ "id": id, "type": "result", "success": true, "result": r });
    match v["type"].as_str().unwrap_or("") {
        "get_states" => {
            contatori.get_states.fetch_add(1, Ordering::SeqCst);
            let da_rifiutare = &contatori.stati_da_rifiutare;
            if da_rifiutare.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1)).is_ok() {
                return json!({ "id": id, "type": "result", "success": false,
                               "error": { "code": "unknown_error", "message": "Errore finto" } });
            }
            ok(Value::Array(contatori.stati.lock().unwrap().values().cloned().collect()))
        }
        "config/area_registry/list" => {
            let mut aree = vec![json!({ "area_id": "cucina", "name": "Cucina" })];
            aree.extend(contatori.aree_extra.lock().unwrap().iter().cloned());
            ok(Value::Array(aree))
        }
        "config/device_registry/list" => ok(Value::Array(contatori.dispositivi.lock().unwrap().clone())),
        "config/entity_registry/list" => {
            contatori.letture_registri.fetch_add(1, Ordering::SeqCst);
            let da_rifiutare = &contatori.voci_da_rifiutare;
            if da_rifiutare.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1)).is_ok() {
                return json!({ "id": id, "type": "result", "success": false,
                               "error": { "code": "unknown_error", "message": "Errore finto" } });
            }
            let mut presa = entry("switch.presa", "cucina");
            if contatori.presa_nascosta.load(Ordering::SeqCst) {
                presa["hidden_by"] = json!("user");
            }
            let mut voci = if contatori.senza_voci_di_serie.load(Ordering::SeqCst) {
                vec![]
            } else {
                vec![entry("light.cucina", "cucina"), presa]
            };
            voci.extend(contatori.voci_extra.lock().unwrap().iter().cloned());
            for (id, nome) in contatori.nomi_registro.lock().unwrap().iter() {
                if let Some(v) = voci.iter_mut().find(|v| v["entity_id"] == id.as_str()) {
                    v["name"] = json!(nome);
                }
            }
            for (id, sostituta) in contatori.voci_sostituite.lock().unwrap().iter() {
                if let Some(v) = voci.iter_mut().find(|v| v["entity_id"] == id.as_str()) {
                    *v = sostituta.clone();
                }
            }
            ok(Value::Array(voci))
        }
        "get_config" => {
            contatori.letture_config.fetch_add(1, Ordering::SeqCst);
            let stato = contatori.stato_core.lock().unwrap().clone();
            let stato = if stato.is_empty() { "RUNNING".to_owned() } else { stato };
            ok(json!({ "unit_system": { "temperature": "°C" }, "location_name": "Casa", "state": stato }))
        }
        "persistent_notification/get" => ok(json!([])),
        "subscribe_events" => {
            if v["event_type"] == "state_changed" {
                contatori.iscritto_state_changed.store(true, Ordering::SeqCst);
            }
            ok(Value::Null)
        }
        "unsubscribe_events" => {
            contatori.disiscrizioni.fetch_add(1, Ordering::SeqCst);
            ok(Value::Null)
        }
        "ping" => json!({ "id": id, "type": "pong" }),
        "call_service" if v["domain"] == "persistent_notification" && v["service"] == "dismiss" => {
            let id = v["service_data"]["notification_id"].as_str().unwrap_or_default().to_owned();
            contatori.archiviate.lock().unwrap().push(id);
            ok(Value::Null)
        }
        "call_service" => {
            if v["target"]["entity_id"] == "light.cucina" {
                ok(json!({ "context": {} }))
            } else {
                json!({ "id": id, "type": "result", "success": false, "error": { "code": "not_found" } })
            }
        }
        _ => json!({ "id": id, "type": "result", "success": false,
                     "error": { "code": "unknown_command", "message": "Unknown command." } }),
    }
}

async fn servi(tcp: tokio::net::TcpStream, contatori: Arc<Contatori>, mut eventi: broadcast::Receiver<String>) {
    let Ok(ws) = tokio_tungstenite::accept_async(tcp).await else { return };
    let (mut tx, mut rx) = ws.split();
    let testo = |v: Value| Message::Text(v.to_string().into());
    if tx.send(testo(json!({ "type": "auth_required", "ha_version": "2026.11.0" }))).await.is_err() {
        return;
    }
    // autenticazione
    loop {
        let Some(Ok(m)) = rx.next().await else { return };
        let Message::Text(t) = m else { continue };
        let v: Value = serde_json::from_str(&t).unwrap();
        if v["type"] == "auth" {
            if v["access_token"] == TOKEN {
                let _ = tx.send(testo(json!({ "type": "auth_ok", "ha_version": "2026.11.0" }))).await;
                break;
            }
            let _ = tx.send(testo(json!({ "type": "auth_invalid", "message": "Invalid access" }))).await;
            let _ = tx.close().await;
            return;
        }
    }
    // Iscrizioni di questo client: `state_changed` intero, `subscribe_entities` (id, entità).
    let mut con_state_changed = false;
    let mut entita: Option<(u64, BTreeSet<String>)> = None;
    loop {
        tokio::select! {
            m = rx.next() => {
                let Some(Ok(m)) = m else { return };
                let Message::Text(t) = m else { continue };
                let v: Value = serde_json::from_str(&t).unwrap();
                if v["type"] == "subscribe_entities" {
                    // Come HA: prima il risultato, poi un evento con gli stati interi delle entità chieste.
                    let id = v["id"].as_u64().unwrap();
                    let ids: Vec<String> =
                        v["entity_ids"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_owned()).collect();
                    contatori.iscrizioni_entita.lock().unwrap().push(ids.clone());
                    let ids: BTreeSet<String> = ids.into_iter().collect();
                    let a: serde_json::Map<String, Value> = contatori
                        .stati
                        .lock()
                        .unwrap()
                        .iter()
                        .filter(|(k, _)| ids.contains(*k))
                        .map(|(k, s)| (k.clone(), compresso(s)))
                        .collect();
                    let risultato = json!({ "id": id, "type": "result", "success": true, "result": null });
                    let primo = json!({ "id": id, "type": "event", "event": { "a": a } });
                    if tx.send(testo(risultato)).await.is_err() || tx.send(testo(primo)).await.is_err() {
                        return;
                    }
                    entita = Some((id, ids));
                    continue;
                }
                if v["type"] == "unsubscribe_events" && entita.as_ref().map(|(id, _)| *id) == v["subscription"].as_u64() {
                    entita = None;
                }
                if v["type"] == "subscribe_events" && v["event_type"] == "state_changed" {
                    con_state_changed = true;
                }
                if v["type"] == "persistent_notification/subscribe" {
                    // Come HA: prima il risultato, poi subito l'elenco "current" (qui vuoto).
                    let id = v["id"].as_u64().unwrap();
                    contatori.iscrizione_notifiche.store(id, Ordering::SeqCst);
                    let risultato = json!({ "id": id, "type": "result", "success": true, "result": null });
                    let presenti = contatori.presenti.lock().unwrap().clone();
                    let presenti = if presenti.is_object() { presenti } else { json!({}) };
                    let attuali = json!({ "id": id, "type": "event", "event": { "type": "current", "notifications": presenti } });
                    if tx.send(testo(risultato)).await.is_err() || tx.send(testo(attuali)).await.is_err() {
                        return;
                    }
                    continue;
                }
                if v["type"] == "get_states" && contatori.cadi_su_get_states.swap(false, Ordering::SeqCst) {
                    // HA che cade a metà lettura: nessuna risposta, connessione chiusa.
                    return;
                }
                if v["type"] == "config/area_registry/list" {
                    let ms = contatori.ritardo_aree_ms.load(Ordering::SeqCst);
                    if ms > 0 { sleep(Duration::from_millis(ms)).await; }
                }
                if v["type"] == "config/entity_registry/list" {
                    let ms = contatori.ritardo_voci_ms.load(Ordering::SeqCst);
                    if ms > 0 { sleep(Duration::from_millis(ms)).await; }
                }
                if tx.send(testo(risposta(&v, &contatori))).await.is_err() { return; }
                if v["type"] == "config/device_registry/list" {
                    let dopo = contatori.dopo_dispositivi.lock().unwrap().take();
                    if let Some((nuovi, evento)) = dopo {
                        *contatori.dispositivi.lock().unwrap() = nuovi;
                        if tx.send(Message::Text(evento.into())).await.is_err() { return; }
                    }
                }
                if v["type"] == "get_states" {
                    let cambio = contatori.cambio_dopo_stati.lock().unwrap().take();
                    if let Some(data) = cambio {
                        cambia_stato(&contatori, data);
                    }
                }
            }
            e = eventi.recv() => {
                let Ok(e) = e else { return };
                // La connessione cade (come un riavvio di HA).
                if e == CHIUDI {
                    let _ = tx.close().await;
                    return;
                }
                let v: Value = serde_json::from_str(&e).unwrap();
                if v["event"]["event_type"] != "state_changed" {
                    if tx.send(Message::Text(e.into())).await.is_err() { return; }
                    continue;
                }
                if con_state_changed && tx.send(Message::Text(e.clone().into())).await.is_err() {
                    return;
                }
                let data = &v["event"]["data"];
                if let Some((id, ids)) = &entita {
                    if ids.contains(data["entity_id"].as_str().unwrap_or_default()) {
                        let compresso = json!({ "id": id, "type": "event", "event": diff_compresso(data) });
                        if tx.send(testo(compresso)).await.is_err() { return; }
                    }
                }
            }
        }
    }
}

// ─── Servizio sotto test ──────────────────────────────────────────────────────

struct Prova {
    servizio: CasaService,
    eventi: mpsc::UnboundedReceiver<CasaEvento>,
    _dir: tempfile::TempDir,
}

fn prepara(url: &str, token: Option<&str>) -> Prova {
    let config = Config {
        environments: vec![HaEnvironment { id: "casa".into(), name: "Casa".into(), ha_url: url.into() }],
        active_env_id: Some("casa".into()),
        ..Config::default()
    };
    let token: Vec<(&str, &str)> = token.map(|t| ("casa", t)).into_iter().collect();
    prepara_con(tempfile::tempdir().unwrap(), Some(config), &token)
}

/// `config: None` lascia la cartella com'è (per preparare file illeggibili/non scrivibili).
fn prepara_con(dir: tempfile::TempDir, config: Option<Config>, token: &[(&str, &str)]) -> Prova {
    let store = Arc::new(ConfigStore::apri(dir.path()));
    if let Some(config) = config {
        store.salva(&config).unwrap();
    }
    let segreti = Arc::new(MemoriaSegreti::new());
    for (env, t) in token {
        segreti.salva(env, t).unwrap();
    }
    let (tx, eventi) = mpsc::unbounded_channel();
    let servizio = CasaService::avvia_per_prova(
        store,
        segreti,
        Arc::new(move |e| {
            let _ = tx.send(e);
        }),
        vec![RITARDO_TEST],
        Arc::new(casa_core::diagnosi::Diagnosi::default()),
    );
    Prova { servizio, eventi, _dir: dir }
}

async fn prossimo(p: &mut Prova) -> CasaEvento {
    timeout(Duration::from_secs(5), p.eventi.recv())
        .await
        .expect("nessun evento entro 5 s")
        .expect("canale chiuso")
}

async fn aspetta_dati(p: &mut Prova) -> Arc<MenuData> {
    loop {
        if let CasaEvento::Dati(d) = prossimo(p).await {
            return d;
        }
    }
}

fn conta_entita(d: &MenuData) -> usize {
    d.rooms.iter().map(|r| r.entities.len()).sum()
}

fn stato_di(e: &CasaEvento) -> Option<&StatoConnessione> {
    match e {
        CasaEvento::Stato(s) => Some(s),
        _ => None,
    }
}

// ─── Test ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn connessione_porta_a_connesso_e_dati() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();

    assert_eq!(stato_di(&prossimo(&mut p).await), Some(&StatoConnessione::InConnessione));
    assert_eq!(stato_di(&prossimo(&mut p).await), Some(&StatoConnessione::Connesso));
    match prossimo(&mut p).await {
        CasaEvento::Dati(d) => assert_eq!(conta_entita(&d), 2),
        altro => panic!("atteso Dati, arrivato {altro:?}"),
    }
}

#[tokio::test]
async fn token_sbagliato_da_errore_senza_riprovare() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some("token-sbagliato"));
    p.servizio.connetti_attivo();

    assert_eq!(stato_di(&prossimo(&mut p).await), Some(&StatoConnessione::InConnessione));
    assert_eq!(stato_di(&prossimo(&mut p).await), Some(&StatoConnessione::TokenNonValido));
    sleep(Duration::from_secs(5)).await;
    assert_eq!(ha.connessioni(), 1);
    assert!(p.eventi.try_recv().is_err(), "nessun altro evento dopo l'errore");
}

#[tokio::test]
async fn token_mancante_da_errore_senza_connettersi() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, None);
    p.servizio.connetti_attivo();

    assert_eq!(stato_di(&prossimo(&mut p).await), Some(&StatoConnessione::TokenMancante));
    sleep(Duration::from_millis(500)).await;
    assert_eq!(ha.connessioni(), 0);
    assert!(p.eventi.try_recv().is_err());
}

#[tokio::test]
async fn senza_ambiente_attivo_non_configurato() {
    let mut p = prepara_con(tempfile::tempdir().unwrap(), Some(Config::default()), &[]);
    p.servizio.connetti_attivo();
    assert_eq!(stato_di(&prossimo(&mut p).await), Some(&StatoConnessione::NonConfigurato));
    // Un ambiente che non esiste (rimosso) è lo stesso caso.
    p.servizio.connetti("sparito");
    assert_eq!(stato_di(&prossimo(&mut p).await), Some(&StatoConnessione::NonConfigurato));
}

#[tokio::test]
async fn connessione_caduta_non_e_disconnesso() {
    // Un server che autentica e poi chiude: la caduta è un errore con nuovi tentativi, mai il
    // `Disconnesso` voluto di «Disconnetti».
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            tokio::spawn(async move {
                let Ok(ws) = tokio_tungstenite::accept_async(tcp).await else { return };
                let (mut tx, mut rx) = ws.split();
                let testo = |v: Value| Message::Text(v.to_string().into());
                let _ = tx.send(testo(json!({ "type": "auth_required" }))).await;
                let _ = rx.next().await;
                let _ = tx.send(testo(json!({ "type": "auth_ok" }))).await;
                let _ = tx.close().await;
            });
        }
    });
    let mut p = prepara(&url, Some(TOKEN));
    p.servizio.connetti_attivo();
    let mut stati = Vec::new();
    while stati.len() < 4 {
        if let CasaEvento::Stato(s) = prossimo(&mut p).await {
            stati.push(s);
        }
    }
    assert_eq!(stati[0], StatoConnessione::InConnessione);
    assert_eq!(stati[1], StatoConnessione::Connesso);
    assert!(matches!(stati[2], StatoConnessione::Errore(_)), "{:?}", stati[2]);
    assert_eq!(stati[3], StatoConnessione::InConnessione);
}

#[tokio::test]
async fn server_spento_riprova() {
    // porta libera e chiusa: nessuno ascolta
    let porta = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let mut p = prepara(&format!("http://127.0.0.1:{porta}"), Some(TOKEN));
    p.servizio.connetti_attivo();

    assert_eq!(stato_di(&prossimo(&mut p).await), Some(&StatoConnessione::InConnessione));
    assert!(matches!(stato_di(&prossimo(&mut p).await), Some(StatoConnessione::Errore(_))));
    assert_eq!(stato_di(&prossimo(&mut p).await), Some(&StatoConnessione::InConnessione));
    assert!(matches!(stato_di(&prossimo(&mut p).await), Some(StatoConnessione::Errore(_))));
}


/// Uno stato nuovo come lo porta `state_changed` (con `last_updated` dopo quello di `get_states`).
fn stato_nuovo(entity_id: &str, state: &str, nome: &str) -> Value {
    let mut s = stato(entity_id, state, nome);
    s["last_updated"] = json!("2026-11-13T10:00:05.250000+00:00");
    s
}

fn cambio(ha: &FintoHa, entity_id: &str, nuovo: Value) {
    ha.manda_evento("state_changed", json!({ "entity_id": entity_id, "old_state": null, "new_state": nuovo }));
}

fn acceso(d: &MenuData, entity_id: &str) -> Option<bool> {
    d.rooms.iter().flat_map(|r| &r.entities).find(|e| e.entity_id() == entity_id).map(|e| match e {
        AppEntity::Light(l) => l.is_on,
        AppEntity::Switch(s) => s.is_on,
        _ => false,
    })
}

/// I `Dati` arrivati entro `per`.
async fn dati_entro(p: &mut Prova, per: Duration) -> Vec<Arc<MenuData>> {
    let fine = Instant::now() + per;
    let mut dati = Vec::new();
    while let Ok(Some(e)) = timeout_at(fine, p.eventi.recv()).await {
        if let CasaEvento::Dati(d) = e {
            dati.push(d);
        }
    }
    dati
}

/// Aspetta la prima iscrizione alle entità (l'iscrizione parte dopo la prima lettura).
async fn aspetta_iscrizione(ha: &FintoHa, quante: usize) {
    aspetta(|| ha.iscrizioni_entita().len() >= quante, "iscrizione alle entità").await;
    // Il primo evento dell'iscrizione è già partito col risultato: lascia che arrivi.
    sleep(Duration::from_millis(100)).await;
}

/// Task 14e: gli stati arrivano da `subscribe_entities` con le sole entità mostrate, non più da
/// `state_changed` di tutta la casa. Giro di correzioni 1 del 14h: anche quelle nascoste
/// dall'utente restano seguite (sono nel catalogo delle Impostazioni, col nome dello stato):
/// nasconderle o rimostrarle non rifà l'iscrizione.
#[tokio::test]
async fn si_seguono_solo_le_entita_mostrate() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    aspetta_dati(&mut p).await;
    aspetta_iscrizione(&ha, 1).await;
    assert_eq!(ha.iscrizioni_entita(), [vec!["light.cucina".to_string(), "switch.presa".to_string()]]);
    assert!(!ha.contatori.iscritto_state_changed.load(Ordering::SeqCst), "niente più state_changed di tutta la casa");

    // Preferiti: stesse entità, nessuna nuova iscrizione.
    p.servizio.aggiorna_config(|c| c.favorites = vec!["light.cucina".into()]);
    aspetta_dati(&mut p).await;
    // Presa nascosta: esce dalle stanze ma resta seguita, e il suo stato resta di adesso.
    p.servizio.aggiorna_config(|c| c.hidden_entities = vec!["switch.presa".into()]);
    let d = aspetta_dati(&mut p).await;
    assert_eq!(conta_entita(&d), 1);
    cambio(&ha, "switch.presa", stato_nuovo("switch.presa", "on", "Presa"));
    sleep(Duration::from_millis(300)).await;
    p.servizio.aggiorna_config(|c| c.hidden_entities.clear());
    let d = aspetta_dati(&mut p).await;
    assert_eq!(acceso(&d, "switch.presa"), Some(true), "mostrata di nuovo, con lo stato di adesso");
    assert_eq!(ha.iscrizioni_entita().len(), 1, "nessuna iscrizione rifatta");
    assert_eq!(ha.get_states(), 1, "nessuna rilettura per seguire altre entità");
}

#[tokio::test]
async fn un_cambio_compresso_aggiorna_solo_quell_entita_senza_rileggere() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    let d = aspetta_dati(&mut p).await;
    assert_eq!(acceso(&d, "light.cucina"), Some(true));
    aspetta_iscrizione(&ha, 1).await;

    cambio(&ha, "light.cucina", stato_nuovo("light.cucina", "off", "Luce cucina"));
    let d = aspetta_dati(&mut p).await;
    assert_eq!(acceso(&d, "light.cucina"), Some(false));
    assert_eq!(acceso(&d, "switch.presa"), Some(false));
    sleep(Duration::from_millis(200)).await;
    assert_eq!(ha.get_states(), 1, "nessuna rilettura per un cambio di stato");
    assert_eq!(ha.letture_registri(), 1);
}

#[tokio::test]
async fn cambi_che_non_si_vedono_non_pubblicano() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    aspetta_dati(&mut p).await;
    aspetta_iscrizione(&ha, 1).await;

    let inizio = Instant::now();
    for i in 0..60u64 {
        sleep_until(inizio + Duration::from_millis(i * 5)).await;
        // Sensori che l'app non mostra, un dominio escluso, un attributo che non usiamo.
        let mut potenza = stato_nuovo("sensor.frigo", &format!("{i}"), "Frigo");
        potenza["attributes"]["device_class"] = json!("power");
        cambio(&ha, "sensor.frigo", potenza);
        cambio(&ha, "binary_sensor.porta", stato_nuovo("binary_sensor.porta", if i % 2 == 0 { "on" } else { "off" }, "Porta"));
        let mut luce = stato_nuovo("light.cucina", "on", "Luce cucina");
        luce["attributes"]["linkquality"] = json!(i);
        cambio(&ha, "light.cucina", luce);
    }
    let dati = dati_entro(&mut p, Duration::from_millis(600)).await;
    assert!(dati.is_empty(), "pubblicati {} Dati per cambi invisibili", dati.len());
    assert_eq!(ha.get_states(), 1);
}

#[tokio::test]
async fn una_raffica_visibile_si_pubblica_una_volta() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    aspetta_dati(&mut p).await;
    aspetta_iscrizione(&ha, 1).await;

    // 10 cambi in 18 ms (una scena): luce e presa accese/spente, alla fine luce spenta e presa accesa.
    for i in 0..10 {
        let s = if i % 2 == 0 { "off" } else { "on" };
        cambio(&ha, "light.cucina", stato_nuovo("light.cucina", s, "Luce cucina"));
        cambio(&ha, "switch.presa", stato_nuovo("switch.presa", s, "Presa"));
        sleep(Duration::from_millis(2)).await;
    }
    cambio(&ha, "light.cucina", stato_nuovo("light.cucina", "off", "Luce cucina"));
    let dati = dati_entro(&mut p, Duration::from_millis(500)).await;
    assert_eq!(dati.len(), 1, "una pubblicazione per la raffica");
    assert_eq!(acceso(&dati[0], "light.cucina"), Some(false));
    assert_eq!(acceso(&dati[0], "switch.presa"), Some(true));
}

#[tokio::test]
async fn un_flusso_visibile_continuo_si_pubblica_almeno_ogni_300_ms() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    aspetta_dati(&mut p).await;
    aspetta_iscrizione(&ha, 1).await;

    // Un cambio visibile ogni 30 ms per 1,5 s: il riposo di 50 ms da solo non scatterebbe mai.
    let raccolta = tokio::spawn(async move {
        let dati = dati_entro(&mut p, Duration::from_millis(1600)).await;
        (p, dati)
    });
    let inizio = Instant::now();
    for i in 0..50u64 {
        sleep_until(inizio + Duration::from_millis(i * 30)).await;
        // Tre stati in giro: ogni 300 ms (10 cambi) lo stato è diverso dall'ultimo pubblicato.
        let s = ["on", "off", "unavailable"][(i % 3) as usize];
        cambio(&ha, "switch.presa", stato_nuovo("switch.presa", s, "Presa"));
    }
    let (_p, dati) = raccolta.await.unwrap();
    assert!((4..=7).contains(&dati.len()), "pubblicazioni durante il flusso: {}", dati.len());
}

/// Un'entità seguita che esce arriva con `r`; un'entità nuova non è fra quelle seguite: arriva
/// con l'evento del registro che la crea (rilettura) e da lì è seguita.
#[tokio::test]
async fn entita_tolta_esce_e_nuova_entra_col_registro() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    assert_eq!(conta_entita(&*aspetta_dati(&mut p).await), 2);
    aspetta_iscrizione(&ha, 1).await;

    ha.manda_evento("state_changed", json!({ "entity_id": "switch.presa", "new_state": null }));
    assert_eq!(conta_entita(&*aspetta_dati(&mut p).await), 1);
    cambio(&ha, "switch.nuova", stato_nuovo("switch.nuova", "on", "Nuova"));
    ha.manda_evento("entity_registry_updated", json!({ "action": "create", "entity_id": "switch.nuova" }));
    let d = aspetta_dati(&mut p).await;
    assert_eq!(conta_entita(&d), 2);
    assert_eq!(acceso(&d, "switch.nuova"), Some(true));
    assert_eq!(ha.get_states(), 2);
    // La presa tolta resta seguita (ha la sua voce nel registro: può tornare); dopo la rilettura
    // l'iscrizione rifatta segue anche la nuova.
    let attesa = vec!["light.cucina".to_string(), "switch.nuova".to_string(), "switch.presa".to_string()];
    aspetta(|| ha.iscrizioni_entita().last() == Some(&attesa), "riscritta con la nuova").await;
    assert!(ha.iscrizioni_entita().iter().all(|i| i.contains(&"switch.presa".to_string())), "mai senza la presa");
}

#[tokio::test]
async fn eventi_dei_registri_e_della_configurazione_rileggono_una_volta() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    aspetta_dati(&mut p).await;

    // Quattro eventi in un colpo: una sola rilettura. I tempi esatti dell'attesa (1 s dall'ultimo,
    // tetto 5 s) li prova `servizio::test::attesa_con_tetto` senza orologio vero; qui si aspetta
    // la rilettura senza margini stretti e si controlla che non ne arrivi una seconda.
    for tipo in ["entity_registry_updated", "device_registry_updated", "area_registry_updated", "core_config_updated"] {
        ha.manda_evento(tipo, json!({ "action": "update" }));
    }
    aspetta(|| ha.letture_registri() == 2, "rilettura dopo gli eventi").await;
    sleep(Duration::from_millis(1500)).await;
    assert_eq!(ha.letture_registri(), 2, "una sola rilettura per la raffica");
    assert_eq!(ha.get_states(), 2);
}

/// Aspetta (al più 10 s) che `fatto` diventi vero.
async fn aspetta(fatto: impl Fn() -> bool, cosa: &str) {
    let fine = Instant::now() + Duration::from_secs(10);
    while !fatto() {
        assert!(Instant::now() < fine, "{cosa}: non è successo entro 10 s");
        sleep(Duration::from_millis(20)).await;
    }
}

/// Revisione del 14d, rilievo 2: se la prima lettura fallisce si riprova da soli, senza
/// aspettare un evento o una riconnessione.
#[tokio::test]
async fn prima_lettura_fallita_si_riprova() {
    let ha = FintoHa::avvia().await;
    ha.contatori.stati_da_rifiutare.store(1, Ordering::SeqCst);
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    let d = aspetta_dati(&mut p).await;
    assert_eq!(conta_entita(&d), 2);
    assert_eq!(ha.get_states(), 2);
    assert_eq!(ha.connessioni(), 1, "riprova sulla stessa connessione");
}

/// Rilievo 8: un'entità nascosta nel registro ha lo stato alleggerito; quando torna visibile
/// (evento del registro) ricompare con gli attributi interi (qui il nome, da `friendly_name`).
#[tokio::test]
async fn entita_nascosta_nel_registro_torna_con_gli_attributi() {
    let ha = FintoHa::avvia().await;
    ha.contatori.presa_nascosta.store(true, Ordering::SeqCst);
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    assert_eq!(conta_entita(&*aspetta_dati(&mut p).await), 1, "presa nascosta");
    ha.contatori.presa_nascosta.store(false, Ordering::SeqCst);
    // Come HA: `changes` coi valori di prima.
    ha.manda_evento("entity_registry_updated", json!({ "action": "update", "entity_id": "switch.presa", "changes": { "hidden_by": "user" } }));
    let d = aspetta_dati(&mut p).await;
    let presa = d.rooms.iter().flat_map(|r| &r.entities).find(|e| e.entity_id() == "switch.presa").expect("presa");
    assert_eq!(presa.name(), "Presa", "attributi interi, non lo stato alleggerito");
    // Task 14h: il suo stato in memoria era alleggerito: la lettura dei registri lo scopre e
    // rilegge tutto.
    assert_eq!(ha.get_states(), 2);
}

/// Task 14h: eventi dei registri che non toccano nulla di ciò che l'app mostra o segue (versioni
/// e connessioni di un dispositivo, opzioni di un'entità, entità che non si mostrano, stanze
/// riordinate in HA): nessuna lettura, nessuna iscrizione rifatta, nessun dato.
#[tokio::test]
async fn eventi_dei_registri_che_non_cambiano_nulla_non_rileggono() {
    let ha = FintoHa::avvia().await;
    // Un sensore nel registro che l'app non mostra (senza classe di temperatura né stato).
    ha.contatori.voci_extra.lock().unwrap().push(entry("sensor.frigo", "cucina"));
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    aspetta_dati(&mut p).await;
    aspetta_iscrizione(&ha, 1).await;
    ha.manda_evento("device_registry_updated", json!({ "action": "update", "device_id": "d1", "changes": { "sw_version": "1.2" } }));
    ha.manda_evento("device_registry_updated", json!({ "action": "update", "device_id": "d1", "changes": { "connections": [] } }));
    ha.manda_evento("entity_registry_updated", json!({ "action": "update", "entity_id": "light.cucina", "changes": { "options": {} } }));
    ha.manda_evento("entity_registry_updated", json!({ "action": "update", "entity_id": "sensor.frigo", "changes": { "name": null } }));
    ha.manda_evento("entity_registry_updated", json!({ "action": "create", "entity_id": "automation.sera" }));
    ha.manda_evento("area_registry_updated", json!({ "action": "reorder", "area_id": null }));
    let dati = dati_entro(&mut p, Duration::from_millis(1800)).await;
    assert!(dati.is_empty(), "{} Dati", dati.len());
    assert_eq!((ha.get_states(), ha.letture_registri(), ha.letture_config()), (1, 1, 1), "nessuna lettura");
    assert_eq!(ha.iscrizioni_entita().len(), 1, "nessuna iscrizione rifatta");
}

/// Task 14h: il nome di un'entità mostrata cambiato nel registro: si rileggono i soli registri
/// (non gli stati, non la configurazione) e l'iscrizione resta quella (le entità seguite sono le
/// stesse).
#[tokio::test]
async fn nome_cambiato_nel_registro_rilegge_solo_i_registri() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    aspetta_dati(&mut p).await;
    aspetta_iscrizione(&ha, 1).await;
    ha.contatori.nomi_registro.lock().unwrap().insert("light.cucina".into(), "Lampadario".into());
    ha.manda_evento("entity_registry_updated", json!({ "action": "update", "entity_id": "light.cucina", "changes": { "name": null } }));
    let d = aspetta_dati(&mut p).await;
    let luce = d.rooms.iter().flat_map(|r| &r.entities).find(|e| e.entity_id() == "light.cucina").expect("luce");
    assert_eq!(luce.name(), "Lampadario");
    assert_eq!(acceso(&d, "light.cucina"), Some(true), "lo stato è quello seguito");
    sleep(Duration::from_millis(200)).await;
    assert_eq!((ha.get_states(), ha.letture_registri(), ha.letture_config()), (1, 2, 1));
    assert_eq!(ha.iscrizioni_entita().len(), 1, "stesse entità seguite: nessuna iscrizione rifatta");
}

/// Task 14h: un'entità mostrata nascosta nel registro esce rileggendo i soli registri; l'iscrizione
/// si rifà perché le entità seguite cambiano.
#[tokio::test]
async fn entita_nascosta_nel_registro_esce_senza_rileggere_gli_stati() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    assert_eq!(conta_entita(&*aspetta_dati(&mut p).await), 2);
    aspetta_iscrizione(&ha, 1).await;
    ha.contatori.presa_nascosta.store(true, Ordering::SeqCst);
    ha.manda_evento("entity_registry_updated", json!({ "action": "update", "entity_id": "switch.presa", "changes": { "hidden_by": null } }));
    assert_eq!(conta_entita(&*aspetta_dati(&mut p).await), 1);
    aspetta(|| ha.iscrizioni_entita().len() == 2, "iscrizione rifatta").await;
    assert_eq!(ha.iscrizioni_entita()[1], ["light.cucina"]);
    assert_eq!((ha.get_states(), ha.letture_registri()), (1, 2));
}

/// Il nome di un'entità nel catalogo delle Impostazioni (dove ci sono anche le nascoste).
fn nome_nel_catalogo(d: &MenuData, entity_id: &str) -> Option<String> {
    d.catalogo.iter().flat_map(|s| &s.voci).find(|v| v.entity_id == entity_id).map(|v| v.name.clone())
}

/// Aspetta dati in cui il catalogo dà a `entity_id` il nome `nome` (al più 5 s).
async fn aspetta_nome_nel_catalogo(p: &mut Prova, entity_id: &str, nome: &str) {
    let fine = Instant::now() + Duration::from_secs(5);
    loop {
        let d = timeout_at(fine, aspetta_dati(p)).await.unwrap_or_else(|_| panic!("«{nome}» mai arrivato nel catalogo"));
        if nome_nel_catalogo(&d, entity_id).as_deref() == Some(nome) {
            return;
        }
    }
}

/// Giro di correzioni 1 del 14h, I1: un'entità nascosta dall'utente resta nel catalogo delle
/// Impostazioni: se l'integrazione la rinomina (`original_name`, HA riscrive il `friendly_name`)
/// il nome nuovo deve arrivare. Prima l'evento si scartava e lo stato delle nascoste non era seguito.
#[tokio::test]
async fn entita_nascosta_dall_utente_rinominata_dall_integrazione() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.aggiorna_config(|c| c.hidden_entities = vec!["switch.presa".into()]);
    p.servizio.connetti_attivo();
    let d = aspetta_dati(&mut p).await;
    assert_eq!(nome_nel_catalogo(&d, "switch.presa").as_deref(), Some("Presa"));
    aspetta_iscrizione(&ha, 1).await;
    cambio(&ha, "switch.presa", stato_nuovo("switch.presa", "off", "Presa asciugatrice"));
    ha.manda_evento("entity_registry_updated", json!({ "action": "update", "entity_id": "switch.presa", "changes": { "original_name": "Presa" } }));
    aspetta_nome_nel_catalogo(&mut p, "switch.presa", "Presa asciugatrice").await;
    assert_eq!(ha.get_states(), 1, "senza lettura completa");
}

/// I1, secondo caso: il dispositivo di un'entità con `has_entity_name` rinominato in HA. L'evento
/// del dispositivo porta `name`, quello dell'entità `changes: {}` (HA toglie `compat_name`).
#[tokio::test]
async fn dispositivo_rinominato_con_has_entity_name() {
    let ha = FintoHa::avvia().await;
    let mut presa = entry("switch.presa", "cucina");
    presa["device_id"] = json!("shelly");
    ha.contatori.voci_sostituite.lock().unwrap().insert("switch.presa".into(), presa);
    *ha.contatori.dispositivi.lock().unwrap() = vec![json!({ "id": "shelly", "name": "Presa lavatrice", "name_by_user": null, "area_id": "cucina", "disabled_by": null })];
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.aggiorna_config(|c| c.hidden_entities = vec!["switch.presa".into()]);
    p.servizio.connetti_attivo();
    aspetta_dati(&mut p).await;
    aspetta_iscrizione(&ha, 1).await;
    cambio(&ha, "switch.presa", stato_nuovo("switch.presa", "off", "Presa asciugatrice Interruttore"));
    ha.manda_evento("device_registry_updated", json!({ "action": "update", "device_id": "shelly", "changes": { "name_by_user": null } }));
    ha.manda_evento("entity_registry_updated", json!({ "action": "update", "entity_id": "switch.presa", "changes": {} }));
    aspetta_nome_nel_catalogo(&mut p, "switch.presa", "Presa asciugatrice Interruttore").await;
    assert_eq!(ha.get_states(), 1, "senza lettura completa");
}

/// Giro di correzioni 1 del 14h, I2: un evento che arriva mentre i registri si stanno leggendo
/// (qui dopo la risposta a `device_registry/list` e prima di quella, ritardata, del registro delle
/// entità) non si decide sui registri vecchi: si rileggono i registri dopo.
#[tokio::test]
async fn evento_durante_la_lettura_dei_registri_non_si_perde() {
    let ha = FintoHa::avvia().await;
    let dispositivo = |area: &str| json!({ "id": "lampada", "name": "Lampada", "name_by_user": null, "area_id": area, "disabled_by": null });
    let mut luce = entry("light.cucina", "cucina");
    luce["device_id"] = json!("lampada");
    ha.contatori.voci_sostituite.lock().unwrap().insert("light.cucina".into(), luce.clone());
    *ha.contatori.dispositivi.lock().unwrap() = vec![dispositivo("cucina")];
    ha.contatori.aree_extra.lock().unwrap().push(json!({ "area_id": "salotto", "name": "Salotto" }));
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    aspetta_dati(&mut p).await;
    aspetta_iscrizione(&ha, 1).await;
    // 1. La luce perde la stanza propria (prende quella del dispositivo): lettura dei registri.
    luce["area_id"] = Value::Null;
    ha.contatori.voci_sostituite.lock().unwrap().insert("light.cucina".into(), luce);
    ha.contatori.ritardo_voci_ms.store(400, Ordering::SeqCst);
    // 2. Durante quella lettura il dispositivo va in salotto.
    let evento = json!({ "id": 1, "type": "event", "event": { "event_type": "device_registry_updated",
        "data": { "action": "update", "device_id": "lampada", "changes": { "area_id": "cucina" } } } });
    *ha.contatori.dopo_dispositivi.lock().unwrap() = Some((vec![dispositivo("salotto")], evento.to_string()));
    ha.manda_evento("entity_registry_updated", json!({ "action": "update", "entity_id": "light.cucina", "changes": { "area_id": "cucina" } }));
    let fine = Instant::now() + Duration::from_secs(6);
    loop {
        let d = timeout_at(fine, aspetta_dati(&mut p)).await.expect("la luce non è mai arrivata in salotto");
        if d.rooms.iter().any(|r| r.area_id == "salotto" && r.entities.iter().any(|e| e.entity_id() == "light.cucina")) {
            break;
        }
    }
    assert_eq!(ha.get_states(), 1, "solo registri");
}

/// Giro di correzioni 3 del 14h, P1: la connessione cade mentre una lettura completa è in volo
/// (chiesta da un evento); il client si riconnette da solo dopo 3 s (come i ritardi veri 2, 4 …
/// 30 s, più lunghi del primo tentativo da 2 s). L'errore della lettura interrotta non arma un
/// tentativo a connessione caduta (nessuna lettura fallita in più mentre si è disconnessi), la
/// riconnessione fa una sola lettura e una sola iscrizione, e la prima lettura fallita dopo la
/// riconnessione si riprova dopo 2 s, non con l'attesa gonfiata durante la caduta.
#[tokio::test]
async fn lettura_in_volo_alla_caduta_non_lascia_tentativi() {
    use casa_core::diagnosi::{Contatore, Diagnosi};
    let ha = FintoHa::avvia().await;
    let diagnosi = Arc::new(Diagnosi::new(true));
    let mut p = prepara_con_ritardi(&ha.url, vec![Duration::from_secs(3)], diagnosi.clone());
    p.servizio.connetti_attivo();
    aspetta_dati(&mut p).await;
    aspetta_iscrizione(&ha, 1).await;
    ha.contatori.cadi_su_get_states.store(true, Ordering::SeqCst);
    ha.manda_evento("entity_registry_updated", json!({ "action": "create", "entity_id": "switch.nuova" }));
    aspetta(|| ha.connessioni() == 2, "riconnessione dopo la caduta").await;
    aspetta(|| ha.iscrizioni_entita().len() == 2, "iscrizione della connessione nuova").await;
    assert_eq!(diagnosi.valore(Contatore::letture_fallite), 1, "solo la lettura interrotta: nessun tentativo da disconnessi");
    // La lettura caduta non ha avuto risposta e non si conta.
    let (stati, iscrizioni) = (ha.get_states(), ha.iscrizioni_entita().len());
    assert_eq!(stati, 2, "prima lettura e quella della riconnessione");
    sleep(Duration::from_millis(2500)).await;
    assert_eq!((ha.get_states(), ha.iscrizioni_entita().len()), (stati, iscrizioni), "nessuna lettura in più");
    // Una lettura che fallisce adesso si riprova dopo 2 s.
    ha.contatori.stati_da_rifiutare.store(1, Ordering::SeqCst);
    ha.manda_evento("entity_registry_updated", json!({ "action": "create", "entity_id": "switch.nuova" }));
    aspetta(|| ha.get_states() == stati + 1, "lettura rifiutata").await;
    let rifiutata = Instant::now();
    aspetta(|| ha.get_states() == stati + 2, "nuovo tentativo").await;
    assert!(rifiutata.elapsed() < Duration::from_millis(3500), "tentativo dopo {:?}, non dopo 2 s", rifiutata.elapsed());
}

/// Giro di correzioni 3 del 14h, P2: un tentativo armato si annulla quando parte un'altra lettura
/// completa (qui chiesta da un evento); se scadesse mentre quella è in volo ne accoderebbe una
/// seconda, inutile.
#[tokio::test]
async fn una_lettura_completa_annulla_il_tentativo_armato() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    aspetta_dati(&mut p).await;
    aspetta_iscrizione(&ha, 1).await;
    // Una lettura fallita: tentativo armato fra 2 s.
    ha.contatori.stati_da_rifiutare.store(1, Ordering::SeqCst);
    ha.manda_evento("entity_registry_updated", json!({ "action": "create", "entity_id": "switch.nuova" }));
    aspetta(|| ha.get_states() == 2, "lettura rifiutata").await;
    // Subito un altro evento: la sua lettura parte dopo 1 s e resta in volo oltre i 2 s.
    ha.contatori.ritardo_aree_ms.store(1500, Ordering::SeqCst);
    ha.manda_evento("entity_registry_updated", json!({ "action": "create", "entity_id": "switch.altra" }));
    aspetta(|| ha.get_states() == 3, "lettura dell'evento").await;
    ha.contatori.ritardo_aree_ms.store(0, Ordering::SeqCst);
    sleep(Duration::from_millis(3500)).await;
    assert_eq!(ha.get_states(), 3, "nessuna seconda lettura accodata dal tentativo");
}

/// Giro di correzioni 2 del 14h (N2): una casa senza nulla da mostrare né da seguire non manda
/// mai `subscribe_entities` con l'elenco vuoto (HA lo leggerebbe come «tutte le entità»).
#[tokio::test]
async fn casa_senza_entita_mostrabili_non_si_iscrive_a_tutte() {
    let ha = FintoHa::avvia().await;
    ha.contatori.senza_voci_di_serie.store(true, Ordering::SeqCst);
    *ha.contatori.stati.lock().unwrap() =
        [("binary_sensor.porta".to_string(), stato("binary_sensor.porta", "off", "Porta"))].into_iter().collect();
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    let d = aspetta_dati(&mut p).await;
    assert_eq!(conta_entita(&d), 0);
    sleep(Duration::from_millis(300)).await;
    assert!(ha.iscrizioni_entita().is_empty(), "iscrizioni: {:?}", ha.iscrizioni_entita());
}

/// Giro di correzioni 2 del 14h: una lettura completa chiesta da un evento (entità nuova) che
/// fallisce dopo la prima lettura riuscita si riprova da sola, senza aspettare un altro evento.
#[tokio::test]
async fn lettura_completa_fallita_dopo_la_prima_si_riprova() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    aspetta_dati(&mut p).await;
    aspetta_iscrizione(&ha, 1).await;
    ha.contatori.stati_da_rifiutare.store(1, Ordering::SeqCst);
    cambio(&ha, "switch.nuova", stato_nuovo("switch.nuova", "on", "Nuova"));
    ha.manda_evento("entity_registry_updated", json!({ "action": "create", "entity_id": "switch.nuova" }));
    aspetta(|| ha.get_states() == 2, "lettura chiesta dall'evento (rifiutata)").await;
    let fine = Instant::now() + Duration::from_secs(8);
    loop {
        let d = timeout_at(fine, aspetta_dati(&mut p)).await.expect("l'entità nuova non è mai arrivata");
        if acceso(&d, "switch.nuova") == Some(true) {
            break;
        }
    }
    assert_eq!(ha.get_states(), 3, "un nuovo tentativo");
    assert_eq!(ha.connessioni(), 1, "sulla stessa connessione");
}

/// Giro di correzioni 1 del 14h, M4: se la lettura dei soli registri fallisce si rilegge tutto
/// (giro 2: col ritentativo crescente, dopo 2 s).
#[tokio::test]
async fn lettura_dei_registri_fallita_rilegge_tutto() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    aspetta_dati(&mut p).await;
    aspetta_iscrizione(&ha, 1).await;
    ha.contatori.voci_da_rifiutare.store(1, Ordering::SeqCst);
    ha.manda_evento("entity_registry_updated", json!({ "action": "update", "entity_id": "light.cucina", "changes": { "name": null } }));
    aspetta(|| ha.get_states() == 2, "lettura completa dopo il rifiuto").await;
    assert_eq!(ha.letture_registri(), 3, "prima, rifiutata, completa");
}

/// Un cambio avvenuto mentre la lettura completa è in volo non si perde: l'iscrizione parte dopo
/// la lettura e il suo primo evento porta gli stati di adesso (prima lo garantivano un buffer
/// degli `state_changed` e il confronto di `last_updated`).
#[tokio::test]
async fn cambio_durante_la_lettura_arriva_con_l_iscrizione() {
    let ha = FintoHa::avvia().await;
    // Subito dopo la risposta di `get_states` la luce si spegne; il registro delle stanze arriva
    // 300 ms dopo.
    *ha.contatori.cambio_dopo_stati.lock().unwrap() =
        Some(json!({ "entity_id": "light.cucina", "new_state": stato_nuovo("light.cucina", "off", "Luce cucina") }));
    ha.contatori.ritardo_aree_ms.store(300, Ordering::SeqCst);
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    let fine = Instant::now() + Duration::from_secs(5);
    loop {
        let d = timeout_at(fine, aspetta_dati(&mut p)).await.expect("la luce spenta non è mai arrivata");
        if acceso(&d, "light.cucina") == Some(false) {
            break;
        }
    }
    assert_eq!(ha.get_states(), 1);
}

#[tokio::test]
async fn stato_illeggibile_rilegge_tutto() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    aspetta_dati(&mut p).await;
    aspetta_iscrizione(&ha, 1).await;
    ha.manda_evento("state_changed", json!({ "entity_id": "light.cucina", "new_state": { "state": 3 } }));
    sleep(Duration::from_millis(300)).await;
    assert_eq!(ha.get_states(), 2);
}
#[tokio::test]
async fn notifica_aggiunta_e_rimossa_come_eventi() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    aspetta_dati(&mut p).await;
    ha.aspetta_iscrizione_notifiche().await;

    let notifica = json!({ "lavatrice": {
        "notification_id": "lavatrice", "title": "Lavatrice", "message": "Ciclo finito",
        "created_at": "2026-11-13T10:00:00+00:00" } });
    ha.manda_notifiche("added", notifica.clone());
    loop {
        if let CasaEvento::NotificaAggiunta { id, titolo, messaggio } = prossimo(&mut p).await {
            assert_eq!((id.as_str(), titolo.as_str(), messaggio.as_str()), ("lavatrice", "Lavatrice", "Ciclo finito"));
            break;
        }
    }
    ha.manda_notifiche("removed", notifica);
    loop {
        if let CasaEvento::NotificaRimossa { id } = prossimo(&mut p).await {
            assert_eq!(id, "lavatrice");
            break;
        }
    }
}

#[tokio::test]
async fn riconnessione_allo_stesso_ambiente_non_toglie_le_notifiche() {
    // «Riprova» = connetti_attivo() sullo stesso ambiente: una notifica ancora presente in HA
    // non deve uscire (al più torna come aggiornamento dall'elenco "current").
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    aspetta_dati(&mut p).await;
    ha.aspetta_iscrizione_notifiche().await;
    let notifica = json!({ "lav": { "notification_id": "lav", "title": "Lavatrice", "message": "Fatto" } });
    ha.metti_presenti(notifica.clone());
    ha.manda_notifiche("added", notifica);
    loop {
        if let CasaEvento::NotificaAggiunta { .. } = prossimo(&mut p).await {
            break;
        }
    }

    ha.azzera_iscrizione();
    p.servizio.connetti_attivo();
    let mut eventi = Vec::new();
    loop {
        let e = prossimo(&mut p).await;
        let dati = matches!(e, CasaEvento::Dati(_));
        eventi.push(e);
        if dati {
            break;
        }
    }
    ha.aspetta_iscrizione_notifiche().await;
    sleep(Duration::from_millis(300)).await;
    eventi.extend(std::iter::from_fn(|| p.eventi.try_recv().ok()));
    assert!(
        !eventi.iter().any(|e| matches!(e, CasaEvento::NotificaRimossa { .. })),
        "nessuna rimozione per una notifica ancora presente: {eventi:?}"
    );
}

#[tokio::test]
async fn archivia_chiama_il_servizio() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    aspetta_dati(&mut p).await;

    p.servizio.archivia_notifica("lavatrice");
    let inizio = Instant::now();
    while ha.archiviate().is_empty() {
        assert!(inizio.elapsed() < Duration::from_secs(5), "nessun dismiss arrivato");
        sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(ha.archiviate(), ["lavatrice"]);
    sleep(Duration::from_millis(200)).await;
    assert!(
        !std::iter::from_fn(|| p.eventi.try_recv().ok()).any(|e| matches!(e, CasaEvento::NotificaNonArchiviata { .. })),
        "archiviazione riuscita: nessun errore"
    );
}

#[tokio::test]
async fn comando_ok_e_comando_rifiutato() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    aspetta_dati(&mut p).await;

    p.servizio.comando("light.cucina", "light", "toggle", 7);
    let esito = loop {
        if let e @ CasaEvento::Esito { .. } = prossimo(&mut p).await {
            break e;
        }
    };
    assert!(matches!(esito, CasaEvento::Esito { ref entity_id, numero: 7, ok: true, errore: None } if entity_id == "light.cucina"));

    p.servizio.comando("light.fantasma", "light", "toggle", 8);
    let esito = loop {
        if let e @ CasaEvento::Esito { .. } = prossimo(&mut p).await {
            break e;
        }
    };
    assert!(matches!(esito, CasaEvento::Esito { ref entity_id, numero: 8, ok: false, errore: Some(_) } if entity_id == "light.fantasma"));
}

#[tokio::test]
async fn comando_senza_connessione_fallisce() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.comando("light.cucina", "light", "toggle", 9);
    assert!(matches!(prossimo(&mut p).await, CasaEvento::Esito { numero: 9, ok: false, errore: Some(_), .. }));
}

#[tokio::test]
async fn preferiti_cambiati_rimappano_senza_rileggere() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    let d = aspetta_dati(&mut p).await;
    assert!(d.favorites.is_empty());

    p.servizio.aggiorna_config(|c| c.favorites = vec!["light.cucina".into()]);
    assert_eq!(p.servizio.config().favorites, vec!["light.cucina".to_string()]);
    let d = aspetta_dati(&mut p).await;
    assert_eq!(d.favorites.len(), 1);
    assert_eq!(ha.get_states(), 1);

    // cambio che non tocca preferiti/nascosti/ordini: nessuna rimappatura
    p.servizio.aggiorna_config(|c| c.ultimo_layout = "compatto".into());
    sleep(Duration::from_millis(300)).await;
    assert!(p.eventi.try_recv().is_err());
}

#[tokio::test]
async fn disconnetti_porta_a_disconnesso() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    aspetta_dati(&mut p).await;

    p.servizio.disconnetti();
    assert_eq!(stato_di(&prossimo(&mut p).await), Some(&StatoConnessione::Disconnesso));
    sleep(Duration::from_millis(300)).await;
    assert_eq!(ha.connessioni(), 1);
    assert!(p.eventi.try_recv().is_err());
}

#[tokio::test]
async fn prova_connessione_conta_le_entita() {
    let ha = FintoHa::avvia().await;
    let p = prepara(&ha.url, Some(TOKEN));
    let (tx, mut rx) = mpsc::unbounded_channel::<TestConnectionResult>();

    let t = tx.clone();
    p.servizio.prova_connessione(ha.url.clone(), TOKEN.into(), move |r| {
        let _ = t.send(r);
    });
    let r = timeout(Duration::from_secs(5), rx.recv()).await.unwrap().unwrap();
    assert!(r.ok);
    assert_eq!(r.entity_count, Some(2));

    p.servizio.prova_connessione(ha.url.clone(), "sbagliato".into(), move |r| {
        let _ = tx.send(r);
    });
    let r = timeout(Duration::from_secs(5), rx.recv()).await.unwrap().unwrap();
    assert!(!r.ok);
    assert_eq!(r.error.as_deref(), Some("token non valido"));
}

#[tokio::test]
async fn config_non_salvata_arriva_come_evento() {
    // al posto del file c'è una cartella: la scrittura (tmp + rename) fallisce
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("assistente.json")).unwrap();
    let mut p = prepara_con(dir, None, &[]);

    p.servizio.aggiorna_config(|c| c.ultimo_layout = "compatto".into());
    match prossimo(&mut p).await {
        CasaEvento::ConfigNonSalvata(msg) => assert!(msg.starts_with("impossibile salvare la configurazione")),
        altro => panic!("atteso ConfigNonSalvata, arrivato {altro:?}"),
    }
    // la modifica resta in memoria
    assert_eq!(p.servizio.config().ultimo_layout, "compatto");

    // nessuna modifica: nessuna scrittura, nessun evento
    p.servizio.aggiorna_config(|c| c.ultimo_layout = "compatto".into());
    sleep(Duration::from_millis(300)).await;
    assert!(p.eventi.try_recv().is_err());
}

#[tokio::test]
async fn connetti_rende_attivo_l_ambiente_e_lo_salva() {
    let ha = FintoHa::avvia().await;
    let env = |id: &str| HaEnvironment { id: id.into(), name: id.into(), ha_url: ha.url.clone() };
    let config = Config {
        environments: vec![env("casa"), env("env2")],
        active_env_id: Some("casa".into()),
        ..Config::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let percorso = dir.path().to_path_buf();
    let mut p = prepara_con(dir, Some(config), &[("casa", TOKEN), ("env2", TOKEN)]);

    p.servizio.connetti("env2");
    loop {
        if stato_di(&prossimo(&mut p).await) == Some(&StatoConnessione::Connesso) {
            break;
        }
    }
    assert_eq!(p.servizio.config().active_env_id.as_deref(), Some("env2"));
    let su_disco = ConfigStore::apri(&percorso).leggi();
    assert_eq!(su_disco.active_env_id.as_deref(), Some("env2"));
}

/// Giro di correzioni 1, B1: un'entità del registro senza stato alla lettura (HA appena riavviato,
/// integrazione che carica dopo) è già seguita; quando nasce compare, senza riconnettersi.
#[tokio::test]
async fn entita_del_registro_che_nasce_dopo_la_lettura_compare() {
    let ha = FintoHa::avvia().await;
    ha.contatori.voci_extra.lock().unwrap().push(entry("light.tardiva", "cucina"));
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    assert_eq!(conta_entita(&*aspetta_dati(&mut p).await), 2);
    aspetta_iscrizione(&ha, 1).await;
    assert!(ha.iscrizioni_entita()[0].contains(&"light.tardiva".to_string()), "seguita anche senza stato");
    cambio(&ha, "light.tardiva", stato_nuovo("light.tardiva", "on", "Tardiva"));
    let d = aspetta_dati(&mut p).await;
    assert_eq!(acceso(&d, "light.tardiva"), Some(true));
    assert_eq!((ha.get_states(), ha.connessioni()), (1, 1), "senza riletture né riconnessioni");
}

/// B1, seconda parte: un'entità seguita senza voce nel registro (template YAML senza
/// `unique_id`) tolta con `r` da un ricaricamento resta seguita fino alla prossima lettura: quando
/// torna, ricompare.
#[tokio::test]
async fn entita_senza_registro_tolta_e_rimessa_ricompare() {
    let ha = FintoHa::avvia().await;
    ha.contatori.stati.lock().unwrap().insert("switch.modello".into(), stato("switch.modello", "off", "Modello"));
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    assert_eq!(conta_entita(&*aspetta_dati(&mut p).await), 3);
    aspetta_iscrizione(&ha, 1).await;
    ha.manda_evento("state_changed", json!({ "entity_id": "switch.modello", "new_state": null }));
    assert_eq!(conta_entita(&*aspetta_dati(&mut p).await), 2);
    // Oltre la raffica (la rimappatura è già pubblicata): il ricaricamento dura di più.
    sleep(Duration::from_millis(500)).await;
    cambio(&ha, "switch.modello", stato_nuovo("switch.modello", "on", "Modello"));
    let d = aspetta_dati(&mut p).await;
    assert_eq!(acceso(&d, "switch.modello"), Some(true), "ricaricata, torna");
    assert_eq!(ha.get_states(), 1);
}

/// B1: lettura mentre HA è ancora in avvio (`get_config` → «STARTING»): si rilegge finché non è
/// «RUNNING», poi basta.
#[tokio::test]
async fn ha_in_avvio_fa_rileggere_finche_non_e_pronto() {
    let ha = FintoHa::avvia().await;
    *ha.contatori.stato_core.lock().unwrap() = "STARTING".into();
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    aspetta_dati(&mut p).await;
    // Il core passa a «RUNNING» solo dopo la seconda `get_config` (fine della seconda lettura):
    // aspettando la seconda `get_states` lo si poteva cambiare a metà lettura, e allora la seconda
    // vedeva già «RUNNING» e la terza non arrivava (test instabile, N3 del 14e).
    aspetta(|| ha.letture_config() >= 2, "rilettura con HA in avvio").await;
    *ha.contatori.stato_core.lock().unwrap() = "RUNNING".into();
    aspetta(|| ha.letture_config() >= 3, "terza lettura").await;
    let fatte = ha.get_states();
    sleep(Duration::from_secs(7)).await;
    assert_eq!(ha.get_states(), fatte, "con HA pronto niente più riletture");
}

/// Giro di correzioni 1, I1 (e M2): dopo una riconnessione la prima lettura fallisce. Il servizio
/// si riscrive comunque agli stati coi dati di prima e riprova la lettura: i cambi arrivano.
#[tokio::test]
async fn riconnessione_con_lettura_fallita_riprova_e_segue_gli_stati() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    aspetta_dati(&mut p).await;
    aspetta_iscrizione(&ha, 1).await;
    ha.contatori.stati_da_rifiutare.store(1, Ordering::SeqCst);
    ha.chiudi_connessioni();
    aspetta(|| ha.connessioni() == 2, "riconnessione").await;
    aspetta(|| ha.iscrizioni_entita().len() >= 2, "iscrizione rifatta sulla connessione nuova").await;
    assert_eq!(ha.iscrizioni_entita()[1], ha.iscrizioni_entita()[0], "stesse entità");
    sleep(Duration::from_millis(200)).await;
    cambio(&ha, "light.cucina", stato_nuovo("light.cucina", "off", "Luce cucina"));
    let fine = Instant::now() + Duration::from_secs(5);
    loop {
        let d = timeout_at(fine, aspetta_dati(&mut p)).await.expect("il cambio non è arrivato");
        if acceso(&d, "light.cucina") == Some(false) {
            break;
        }
    }
    aspetta(|| ha.get_states() >= 3, "nuovo tentativo della lettura").await;
}


// ─── Registro diagnostico (Task 14g) ──────────────────────────────────────────

fn prepara_con_diagnosi(url: &str, diagnosi: Arc<casa_core::diagnosi::Diagnosi>) -> Prova {
    prepara_con_ritardi(url, vec![RITARDO_TEST], diagnosi)
}

fn prepara_con_ritardi(url: &str, ritardi: Vec<Duration>, diagnosi: Arc<casa_core::diagnosi::Diagnosi>) -> Prova {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(ConfigStore::apri(dir.path()));
    store
        .salva(&Config {
            environments: vec![HaEnvironment { id: "casa".into(), name: "Casa".into(), ha_url: url.into() }],
            active_env_id: Some("casa".into()),
            ..Config::default()
        })
        .unwrap();
    let segreti = Arc::new(MemoriaSegreti::new());
    segreti.salva("casa", TOKEN).unwrap();
    let (tx, eventi) = mpsc::unbounded_channel();
    let servizio = CasaService::avvia_per_prova(
        store,
        segreti,
        Arc::new(move |e| {
            let _ = tx.send(e);
        }),
        ritardi,
        diagnosi,
    );
    Prova { servizio, eventi, _dir: dir }
}

/// I valori di una riga del registro, per nome.
fn valori(riga: &str) -> BTreeMap<String, String> {
    riga.split(' ')
        .map(|p| {
            let (n, v) = p.split_once('=').unwrap_or_else(|| panic!("«{p}» non è nome=valore"));
            (n.to_owned(), v.to_owned())
        })
        .collect()
}

fn numero(v: &BTreeMap<String, String>, nome: &str) -> u64 {
    v.get(nome).unwrap_or_else(|| panic!("manca {nome}")).parse().unwrap()
}

/// Task 14g: con il registro acceso i contatori dicono cosa è arrivato da HA e cosa ne è
/// seguito: connessione, riposo, cambi invisibili, un cambio visibile, eventi del registro. E
/// nelle righe non c'è nulla della casa: né entity_id, né nomi, né token, né indirizzi.
#[tokio::test]
async fn diagnosi_conta_messaggi_cambi_e_riletture_senza_dati_della_casa() {
    use casa_core::diagnosi::{Diagnosi, NOMI};
    let ha = FintoHa::avvia().await;
    let diagnosi = Arc::new(Diagnosi::new(true));
    let mut p = prepara_con_diagnosi(&ha.url, diagnosi.clone());
    p.servizio.connetti_attivo();
    aspetta_dati(&mut p).await;
    aspetta_iscrizione(&ha, 1).await;
    let mut righe = Vec::new();

    // Connessione: una lettura completa, un'iscrizione con le due entità, una consegna.
    righe.push(diagnosi.riga(&[]));
    let v = valori(righe.last().unwrap());
    assert_eq!(
        [
            numero(&v, "rilettura_connessione"),
            numero(&v, "riletture"),
            numero(&v, "iscrizioni"),
            numero(&v, "consegne"),
            numero(&v, "entita_a"),
            numero(&v, "seguite"),
        ],
        [1, 1, 1, 1, 2, 2],
        "{}",
        righe.last().unwrap()
    );
    assert_eq!(v["ha"], "RUNNING");
    // Giro di correzioni 1 del 14h: il tempo della prima lettura; nessuna interfaccia, nessuna consegna.
    assert!(numero(&v, "avvio_lettura_ms") > 0, "{}", righe.last().unwrap());
    assert_eq!(numero(&v, "avvio_consegna_ms"), 0);
    assert!(numero(&v, "ws_messaggi") >= 8 && numero(&v, "ws_byte") > 0, "{}", righe.last().unwrap());
    assert_eq!(numero(&v, "ws_auth"), 2, "auth_required e auth_ok");

    // Riposo: nulla.
    sleep(Duration::from_millis(300)).await;
    righe.push(diagnosi.riga(&[]));
    let v = valori(righe.last().unwrap());
    assert_eq!(numero(&v, "ws_messaggi"), 0, "{}", righe.last().unwrap());
    assert_eq!(numero(&v, "consegne") + numero(&v, "riletture"), 0);

    // Cinque cambi che non si vedono (un attributo che l'app non usa): nessuna pubblicazione.
    for i in 0..5 {
        let mut luce = stato_nuovo("light.cucina", "on", "Luce cucina");
        luce["attributes"]["linkquality"] = json!(i);
        cambio(&ha, "light.cucina", luce);
    }
    aspetta(|| diagnosi.valore(casa_core::diagnosi::Contatore::effetto_niente) == 5, "cinque cambi invisibili").await;
    sleep(Duration::from_millis(150)).await;
    righe.push(diagnosi.riga(&[]));
    let v = valori(righe.last().unwrap());
    assert_eq!(
        [numero(&v, "ev_entita"), numero(&v, "entita_c"), numero(&v, "effetto_niente"), numero(&v, "pubblicazioni"), numero(&v, "consegne")],
        [5, 5, 5, 0, 0],
        "{}",
        righe.last().unwrap()
    );

    // Un cambio visibile: sostituito, pubblicato e consegnato una volta.
    cambio(&ha, "light.cucina", stato_nuovo("light.cucina", "off", "Luce cucina"));
    aspetta_dati(&mut p).await;
    righe.push(diagnosi.riga(&[]));
    let v = valori(righe.last().unwrap());
    assert_eq!(
        [numero(&v, "entita_c"), numero(&v, "effetto_sostituita"), numero(&v, "pubblicazioni"), numero(&v, "consegne")],
        [1, 1, 1, 1],
        "{}",
        righe.last().unwrap()
    );

    // Tre eventi del registro di fila sul nome di un'entità mostrata: tre richieste, una sola
    // lettura dei soli registri (dopo 1 s di calma), nessuna lettura completa né iscrizione. Un
    // quarto evento (versione di un dispositivo) si scarta.
    for _ in 0..3 {
        ha.manda_evento("entity_registry_updated", json!({ "action": "update", "entity_id": "light.cucina", "changes": { "name": null } }));
    }
    ha.manda_evento("device_registry_updated", json!({ "action": "update", "device_id": "d1", "changes": { "sw_version": "1" } }));
    aspetta(|| ha.letture_registri() == 2, "lettura dei registri").await;
    aspetta(|| diagnosi.valore(casa_core::diagnosi::Contatore::consegne_uguali) == 1, "mappatura dopo i registri").await;
    sleep(Duration::from_millis(100)).await;
    righe.push(diagnosi.riga(&[]));
    let v = valori(righe.last().unwrap());
    assert_eq!(
        [
            numero(&v, "ev_entity_registry_updated"),
            numero(&v, "ev_device_registry_updated"),
            numero(&v, "rilettura_registri"),
            numero(&v, "rilettura_solo_registri"),
            numero(&v, "registro_scartati"),
            numero(&v, "letture_registri"),
            numero(&v, "riletture"),
            numero(&v, "iscrizioni"),
        ],
        [3, 1, 3, 3, 1, 1, 0, 0],
        "{}",
        righe.last().unwrap()
    );
    // I campi di `changes`, da un elenco fisso.
    assert_eq!(
        [numero(&v, "registro_campo_name"), numero(&v, "registro_campo_sw_version"), numero(&v, "registro_campo_altro")],
        [3, 1, 0],
        "{}",
        righe.last().unwrap()
    );
    assert_eq!(ha.get_states(), 1);

    // Nessun dato della casa: solo nomi fissi e numeri.
    let tutto = righe.join("\n");
    for vietato in ["light", "switch", "cucina", "Luce", "Presa", TOKEN, "127.0.0.1", "http", &ha.url] {
        assert!(!tutto.contains(vietato), "«{vietato}» nel registro:\n{tutto}");
    }
    for riga in &righe {
        for (n, v) in valori(riga) {
            match n.as_str() {
                "ha" => assert!(["nessuna", "assente", "altro", "RUNNING", "STARTING"].contains(&v.as_str()), "ha={v}"),
                "seguite" => assert!(v.parse::<u64>().is_ok()),
                n if casa_core::diagnosi::AVVIO.contains(&n) => assert!(v.parse::<u64>().is_ok()),
                _ => assert!(NOMI.contains(&n.as_str()) && v.parse::<u64>().is_ok(), "{n}={v}"),
            }
        }
    }
}
