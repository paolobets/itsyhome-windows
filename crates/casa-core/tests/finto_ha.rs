//! `CasaService` contro un Home Assistant finto (WebSocket locale su 127.0.0.1:0).
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use casa_core::config::{Config, ConfigStore};
use casa_core::eventi::{CasaEvento, StatoConnessione};
use casa_core::segreti::{MemoriaSegreti, Segreti};
use casa_core::servizio::CasaService;
use casa_core::types::{HaEnvironment, MenuData, TestConnectionResult};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc};
use tokio::time::{sleep, sleep_until, timeout, Instant};
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
            ok(json!([stato("light.cucina", "on", "Luce cucina"), stato("switch.presa", "off", "Presa")]))
        }
        "config/area_registry/list" => ok(json!([{ "area_id": "cucina", "name": "Cucina" }])),
        "config/device_registry/list" => ok(json!([])),
        "config/entity_registry/list" => {
            ok(json!([entry("light.cucina", "cucina"), entry("switch.presa", "cucina")]))
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
                if tx.send(testo(risposta(&v, &contatori))).await.is_err() { return; }
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

#[tokio::test]
async fn raffica_di_state_changed_una_sola_rilettura() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    aspetta_dati(&mut p).await;
    assert_eq!(ha.get_states(), 1);

    let inizio = Instant::now();
    for i in 0..100 {
        sleep_until(inizio + Duration::from_millis(i)).await;
        ha.manda_evento("state_changed", json!({ "entity_id": "light.cucina" }));
    }
    aspetta_dati(&mut p).await;
    sleep(Duration::from_millis(1500)).await;
    assert_eq!(ha.get_states(), 2);
}

#[tokio::test]
async fn flusso_continuo_rilegge_almeno_ogni_secondo() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.connetti_attivo();
    aspetta_dati(&mut p).await;

    // un evento ogni 100 ms per 2,5 s: il trailing di 300 ms da solo non scatterebbe mai
    let inizio = Instant::now();
    for i in 0..25 {
        sleep_until(inizio + Duration::from_millis(i * 100)).await;
        ha.manda_evento("state_changed", json!({ "entity_id": "light.cucina" }));
    }
    let durante = ha.get_states() - 1;
    assert!((2..=3).contains(&durante), "riletture durante il flusso: {durante}");
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

    p.servizio.comando("light.cucina", "light", "toggle");
    let esito = loop {
        if let e @ CasaEvento::Esito { .. } = prossimo(&mut p).await {
            break e;
        }
    };
    assert!(matches!(esito, CasaEvento::Esito { ref entity_id, ok: true, errore: None } if entity_id == "light.cucina"));

    p.servizio.comando("light.fantasma", "light", "toggle");
    let esito = loop {
        if let e @ CasaEvento::Esito { .. } = prossimo(&mut p).await {
            break e;
        }
    };
    assert!(matches!(esito, CasaEvento::Esito { ref entity_id, ok: false, errore: Some(_) } if entity_id == "light.fantasma"));
}

#[tokio::test]
async fn comando_senza_connessione_fallisce() {
    let ha = FintoHa::avvia().await;
    let mut p = prepara(&ha.url, Some(TOKEN));
    p.servizio.comando("light.cucina", "light", "toggle");
    assert!(matches!(prossimo(&mut p).await, CasaEvento::Esito { ok: false, errore: Some(_), .. }));
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
