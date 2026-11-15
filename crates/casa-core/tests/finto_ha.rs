//! `CasaService` contro un Home Assistant finto (WebSocket locale su 127.0.0.1:0).
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
    /// Un evento da mandare subito dopo la risposta a `get_states` (una volta).
    evento_dopo_stati: Mutex<Option<String>>,
    /// Ritardo della risposta al registro delle stanze (le altre letture restano in volo).
    ritardo_aree_ms: AtomicU64,
    letture_registri: AtomicUsize,
    /// Quante risposte a `get_states` rifiutare ancora (errore di HA).
    stati_da_rifiutare: AtomicUsize,
    /// La presa nascosta dall'utente nel registro delle entità (`hidden_by: "user"`).
    presa_nascosta: AtomicBool,
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

    fn manda_evento(&self, tipo: &str, data: Value) {
        let v = json!({ "id": 1, "type": "event", "event": { "event_type": tipo, "data": data } });
        let _ = self.eventi.send(v.to_string());
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
            ok(json!([stato("light.cucina", "on", "Luce cucina"), stato("switch.presa", "off", "Presa")]))
        }
        "config/area_registry/list" => ok(json!([{ "area_id": "cucina", "name": "Cucina" }])),
        "config/device_registry/list" => ok(json!([])),
        "config/entity_registry/list" => {
            contatori.letture_registri.fetch_add(1, Ordering::SeqCst);
            let mut presa = entry("switch.presa", "cucina");
            if contatori.presa_nascosta.load(Ordering::SeqCst) {
                presa["hidden_by"] = json!("user");
            }
            ok(json!([entry("light.cucina", "cucina"), presa]))
        }
        "get_config" => ok(json!({ "unit_system": { "temperature": "°C" }, "location_name": "Casa" })),
        "persistent_notification/get" => ok(json!([])),
        "subscribe_events" => ok(Value::Null),
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
    loop {
        tokio::select! {
            m = rx.next() => {
                let Some(Ok(m)) = m else { return };
                let Message::Text(t) = m else { continue };
                let v: Value = serde_json::from_str(&t).unwrap();
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
                if v["type"] == "config/area_registry/list" {
                    let ms = contatori.ritardo_aree_ms.load(Ordering::SeqCst);
                    if ms > 0 { sleep(Duration::from_millis(ms)).await; }
                }
                if tx.send(testo(risposta(&v, &contatori))).await.is_err() { return; }
                if v["type"] == "get_states" {
                    let evento = contatori.evento_dopo_stati.lock().unwrap().take();
                    if let Some(e) = evento {
                        if tx.send(Message::Text(e.into())).await.is_err() { return; }
                    }
                }
            }
            e = eventi.recv() => {
                let Ok(e) = e else { return };
                if tx.send(Message::Text(e.into())).await.is_err() { return; }
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
    let servizio = CasaService::avvia_con_ritardi(
        store,
        segreti,
        Arc::new(move |e| {
            let _ = tx.send(e);
        }),
        vec![RITARDO_TEST],
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


/// Uno stato come lo porta `state_changed` (con `last_updated` dopo quello di `get_states`).
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

#[tokio::test]
async fn state_changed_aggiorna_solo_quell_entita_senza_rileggere() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    let d = aspetta_dati(&mut p).await;
    assert_eq!(acceso(&d, "light.cucina"), Some(true));

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

#[tokio::test]
async fn entita_tolta_esce_e_nuova_entra() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    assert_eq!(conta_entita(&*aspetta_dati(&mut p).await), 2);

    ha.manda_evento("state_changed", json!({ "entity_id": "switch.presa", "old_state": stato("switch.presa", "off", "Presa"), "new_state": null }));
    assert_eq!(conta_entita(&*aspetta_dati(&mut p).await), 1);
    cambio(&ha, "switch.nuova", stato_nuovo("switch.nuova", "on", "Nuova"));
    let d = aspetta_dati(&mut p).await;
    assert_eq!(conta_entita(&d), 2);
    assert_eq!(acceso(&d, "switch.nuova"), Some(true));
    assert_eq!(ha.get_states(), 1);
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
    ha.manda_evento("entity_registry_updated", json!({ "action": "update", "entity_id": "switch.presa" }));
    let d = aspetta_dati(&mut p).await;
    let presa = d.rooms.iter().flat_map(|r| &r.entities).find(|e| e.entity_id() == "switch.presa").expect("presa");
    assert_eq!(presa.name(), "Presa", "attributi interi, non lo stato alleggerito");
}

#[tokio::test]
async fn evento_arrivato_durante_la_lettura_non_si_perde() {
    let ha = FintoHa::avvia().await;
    // Subito dopo la risposta di `get_states` la luce si spegne; il registro delle stanze arriva
    // 300 ms dopo, quindi l'evento raggiunge il servizio prima che la lettura sia completa.
    let evento = json!({ "id": 1, "type": "event", "event": { "event_type": "state_changed",
        "data": { "entity_id": "light.cucina", "old_state": stato("light.cucina", "on", "Luce cucina"),
                  "new_state": stato_nuovo("light.cucina", "off", "Luce cucina") } } });
    *ha.contatori.evento_dopo_stati.lock().unwrap() = Some(evento.to_string());
    ha.contatori.ritardo_aree_ms.store(300, Ordering::SeqCst);
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    let d = aspetta_dati(&mut p).await;
    assert_eq!(acceso(&d, "light.cucina"), Some(false), "l'evento dopo get_states vale");
}

#[tokio::test]
async fn evento_piu_vecchio_della_lettura_non_la_sovrascrive() {
    let ha = FintoHa::avvia().await;
    let mut vecchio = stato("light.cucina", "off", "Luce cucina");
    vecchio["last_updated"] = json!("2026-11-13T09:59:00+00:00");
    let evento = json!({ "id": 1, "type": "event", "event": { "event_type": "state_changed",
        "data": { "entity_id": "light.cucina", "old_state": null, "new_state": vecchio } } });
    *ha.contatori.evento_dopo_stati.lock().unwrap() = Some(evento.to_string());
    ha.contatori.ritardo_aree_ms.store(300, Ordering::SeqCst);
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    let d = aspetta_dati(&mut p).await;
    assert_eq!(acceso(&d, "light.cucina"), Some(true));
}

#[tokio::test]
async fn stato_illeggibile_rilegge_tutto() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    aspetta_dati(&mut p).await;
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
