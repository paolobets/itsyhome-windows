//! Connessione WebSocket a Home Assistant (port di `src-tauri/src/ha/client.rs`).
//!
//! Un attore tokio per ambiente: si autentica, si iscrive agli eventi dei registri (stanze,
//! dispositivi, entità) e della configurazione e alle notifiche persistenti
//! (`persistent_notification/subscribe`), risponde alle richieste e si riconnette con i ritardi
//! di v2. Gli stati arrivano da `subscribe_entities`, chiesta dal servizio con le sole entità che
//! l'app mostra (`Client::iscrivi_entita`). Gli eventi escono da una callback (`DalClient`), mai
//! dal token.
use std::collections::HashMap;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::value::RawValue;
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::{sleep_until, timeout, timeout_at, Instant, MissedTickBehavior};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

use super::eventi::{interpreta, interpreta_entita, interpreta_notifiche, EventoWs, EVENTI_RILETTURA};
use crate::diagnosi::{Contatore, Diagnosi};
use crate::eventi::StatoConnessione;
use crate::types::TestConnectionResult;

/// `reconnect_delay` di v2: 1, 2, 4, 8, 16, poi sempre 30 secondi.
pub(crate) const RITARDI_RICONNESSIONE: [Duration; 6] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
    Duration::from_secs(8),
    Duration::from_secs(16),
    Duration::from_secs(30),
];
const PING_OGNI: Duration = Duration::from_secs(30);
const PING_TIMEOUT: Duration = Duration::from_secs(10);
/// Connessione + autenticazione: oltre questo tempo si considera fallita
/// (v2 non lo aveva e poteva restare appesa su un host irraggiungibile).
const TIMEOUT_APERTURA: Duration = Duration::from_secs(10);

pub(crate) const NON_CONNESSO: &str = "non connesso";

/// Callback con cui l'attore parla al servizio.
pub(crate) type Uscita = Box<dyn Fn(DalClient) + Send + Sync>;

/// Ciò che l'attore comunica al servizio.
pub(crate) enum DalClient {
    Stato(StatoConnessione),
    Evento(EventoWs),
}

struct Richiesta {
    tipo: String,
    extra: Value,
    risposta: oneshot::Sender<Result<Box<RawValue>, String>>,
}

/// Ciò che il servizio chiede all'attore.
enum Verso {
    Richiesta(Richiesta),
    /// Sostituisce l'iscrizione agli stati con una sulle sole `entity_ids` (vedi `iscrivi_entita`).
    IscriviEntita { numero: u64, entity_ids: Vec<String> },
}

/// Maniglia economica da clonare verso l'attore.
#[derive(Clone)]
pub(crate) struct Client {
    tx: mpsc::UnboundedSender<Verso>,
}

impl Client {
    /// Comando WS generico; `extra` (oggetto) viene fuso nel messaggio.
    /// Il `result` della risposta così com'è nel testo (chi chiama lo legge nel suo tipo).
    pub(crate) async fn richiesta(&self, tipo: &str, extra: Value) -> Result<Box<RawValue>, String> {
        let (risposta, rx) = oneshot::channel();
        self.tx
            .send(Verso::Richiesta(Richiesta { tipo: tipo.to_owned(), extra, risposta }))
            .map_err(|_| NON_CONNESSO.to_owned())?;
        rx.await.map_err(|_| NON_CONNESSO.to_owned())?
    }

    /// Segue gli stati delle sole `entity_ids` (`subscribe_entities`), al posto dell'iscrizione
    /// di prima, che si chiude (`unsubscribe_events`). I suoi eventi escono come
    /// `EventoWs::Entita` col `numero` scelto dal servizio; quelli di un'iscrizione superata non
    /// escono più. Con `entity_ids` vuoto si chiude soltanto: HA leggerebbe un elenco vuoto come
    /// «tutte le entità» (`set(msg.get("entity_ids", [])) or None`).
    /// Senza connessione non fa nulla: alla prossima connessione il servizio rilegge e si
    /// riscrive.
    pub(crate) fn iscrivi_entita(&self, numero: u64, entity_ids: Vec<String>) {
        let _ = self.tx.send(Verso::IscriviEntita { numero, entity_ids });
    }

    pub(crate) async fn call_service(&self, dominio: &str, servizio: &str, entity_id: &str) -> Result<(), String> {
        let extra = json!({
            "domain": dominio,
            "service": servizio,
            "target": { "entity_id": entity_id },
            "service_data": {}
        });
        self.richiesta("call_service", extra).await.map(|_| ())
    }

    /// Servizio senza entità, con i suoi dati (es. `persistent_notification.dismiss`).
    pub(crate) async fn servizio_con_dati(&self, dominio: &str, servizio: &str, dati: Value) -> Result<(), String> {
        let extra = json!({ "domain": dominio, "service": servizio, "service_data": dati });
        self.richiesta("call_service", extra).await.map(|_| ())
    }
}

/// Avvia l'attore sul runtime corrente.
pub(crate) fn avvia(
    url: String,
    token: String,
    ritardi: Vec<Duration>,
    uscita: Uscita,
    diagnosi: std::sync::Arc<Diagnosi>,
) -> (Client, JoinHandle<()>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(attore(url, token, ritardi, rx, uscita, diagnosi));
    (Client { tx }, task)
}

enum Fine {
    TokenNonValido,
    Chiusa,
    Errore(String),
}

async fn attore(
    url: String,
    token: String,
    ritardi: Vec<Duration>,
    mut rx: mpsc::UnboundedReceiver<Verso>,
    uscita: Uscita,
    diagnosi: std::sync::Arc<Diagnosi>,
) {
    let mut tentativo: usize = 0;
    loop {
        uscita(DalClient::Stato(StatoConnessione::InConnessione));
        match sessione(&url, &token, &mut rx, &uscita, &mut tentativo, &diagnosi).await {
            Fine::TokenNonValido => {
                uscita(DalClient::Stato(StatoConnessione::TokenNonValido));
                return; // definitivo, come in v2
            }
            // Una caduta non è il `Disconnesso` voluto: è un errore, e si riprova.
            Fine::Chiusa => uscita(DalClient::Stato(StatoConnessione::Errore(
                "connessione chiusa da Home Assistant".into(),
            ))),
            Fine::Errore(e) => uscita(DalClient::Stato(StatoConnessione::Errore(e))),
        }
        let attesa = Instant::now() + ritardo(&ritardi, tentativo);
        tentativo += 1;
        loop {
            tokio::select! {
                _ = sleep_until(attesa) => break,
                r = rx.recv() => match r {
                    Some(Verso::Richiesta(r)) => { let _ = r.risposta.send(Err(NON_CONNESSO.to_owned())); }
                    Some(Verso::IscriviEntita { .. }) => {}
                    None => return,
                },
            }
        }
    }
}

async fn sessione(
    url: &str,
    token: &str,
    rx: &mut mpsc::UnboundedReceiver<Verso>,
    uscita: &Uscita,
    tentativo: &mut usize,
    diagnosi: &Diagnosi,
) -> Fine {
    let scadenza_apertura = Instant::now() + TIMEOUT_APERTURA;
    let ws = match timeout_at(scadenza_apertura, connect_async(url_ws(url))).await {
        Err(_) => return Fine::Errore("Home Assistant non risponde".into()),
        Ok(Err(e)) => return Fine::Errore(format!("connessione non riuscita: {e}")),
        Ok(Ok((ws, _))) => ws,
    };
    let (mut scrivi, mut leggi) = ws.split();

    let mut pendenti: HashMap<u64, oneshot::Sender<Result<Box<RawValue>, String>>> = HashMap::new();
    let mut prossimo_id: u64 = 1;
    let mut autenticato = false;
    // Id della richiesta `persistent_notification/subscribe`: i suoi eventi portano quell'id.
    let mut iscrizione_notifiche: Option<u64> = None;
    // L'iscrizione `subscribe_entities` in corso: (id della richiesta, numero del servizio).
    let mut iscrizione_entita: Option<(u64, u64)> = None;
    let mut ping = tokio::time::interval_at(Instant::now() + PING_OGNI, PING_OGNI);
    ping.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut attesa_pong: Option<Instant> = None;

    let fine = 'sessione: loop {
        // Prima dell'autenticazione vale la scadenza di apertura, dopo quella del pong.
        let scadenza = if autenticato { attesa_pong } else { Some(scadenza_apertura) };
        let mut uscenti: Vec<Value> = Vec::new();
        tokio::select! {
            m = leggi.next() => {
                let testo = match m {
                    Some(Ok(Message::Text(t))) => t,
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break 'sessione Fine::Chiusa,
                    Some(Ok(_)) => continue,
                };
                diagnosi.conta(Contatore::ws_messaggi);
                diagnosi.conta_n(Contatore::ws_byte, testo.len() as u64);
                // Le risposte (anche di qualche MB: stati e registri di una casa vera) si leggono
                // senza farne un albero `Value`: il `result` passa così com'è a chi l'ha chiesto.
                if let Some((id, esito)) = risultato_diretto(&testo) {
                    diagnosi.conta(Contatore::ws_result);
                    if let Some(tx) = pendenti.remove(&id) {
                        let _ = tx.send(esito);
                    } else if iscrizione_entita.is_some_and(|(ws, _)| ws == id) {
                        if let Err(e) = esito {
                            eprintln!("iscrizione agli stati (subscribe_entities) rifiutata da Home Assistant: {e}");
                            iscrizione_entita = None;
                        }
                    }
                    continue;
                }
                let Ok(mut v) = serde_json::from_str::<Value>(&testo) else { continue };
                // Il tipo copiato: risultati ed eventi si prendono dal messaggio (`esito`, `interpreta`).
                let tipo = v["type"].as_str().map(str::to_owned);
                conta_messaggio(diagnosi, tipo.as_deref());
                match tipo.as_deref() {
                    Some("auth_required") => {
                        uscenti.push(json!({ "type": "auth", "access_token": token }));
                    }
                    Some("auth_ok") => {
                        *tentativo = 0;
                        autenticato = true;
                        // Per rileggere registri e configurazione solo quando cambiano, i loro eventi.
                        // Gli stati no: li porta `subscribe_entities`, solo delle entità mostrate.
                        for tipo in EVENTI_RILETTURA {
                            uscenti.push(json!({ "type": "subscribe_events", "event_type": tipo, "id": prossimo_id }));
                            prossimo_id += 1;
                        }
                        uscenti.push(json!({ "type": "persistent_notification/subscribe", "id": prossimo_id }));
                        iscrizione_notifiche = Some(prossimo_id);
                        prossimo_id += 1;
                        uscita(DalClient::Stato(StatoConnessione::Connesso));
                    }
                    Some("auth_invalid") => break 'sessione Fine::TokenNonValido,
                    Some("result") => {
                        let id = v["id"].as_u64().unwrap_or(0);
                        if let Some(tx) = pendenti.remove(&id) {
                            let esito = esito(v).and_then(|r| serde_json::value::to_raw_value(&r).map_err(|e| e.to_string()));
                            let _ = tx.send(esito);
                        } else if iscrizione_entita.is_some_and(|(ws, _)| ws == id) {
                            // L'iscrizione agli stati rifiutata (HA troppo vecchio, un id che HA
                            // non accetta): senza, gli stati restano fermi. Lo si scrive; rileggere
                            // non servirebbe (la rifiuterebbe di nuovo, a ripetizione).
                            if let Err(e) = esito(v) {
                                eprintln!("iscrizione agli stati (subscribe_entities) rifiutata da Home Assistant: {e}");
                                iscrizione_entita = None;
                            }
                        }
                    }
                    Some("pong") => attesa_pong = None,
                    Some("event") if autenticato => {
                        let id = v["id"].as_u64();
                        let e = match iscrizione_entita {
                            Some((ws, numero)) if id == Some(ws) => {
                                diagnosi.conta(Contatore::ev_entita);
                                Some(interpreta_entita(&mut v, numero))
                            }
                            _ if id.is_some() && id == iscrizione_notifiche => {
                                diagnosi.conta(Contatore::ev_notifiche);
                                interpreta_notifiche(&v)
                            }
                            _ => {
                                diagnosi.conta(contatore_evento(v["event"]["event_type"].as_str()));
                                interpreta(&v)
                            }
                        };
                        if let Some(e) = e {
                            uscita(DalClient::Evento(e));
                        }
                    }
                    _ => {}
                }
            }
            r = rx.recv() => match r {
                None => break 'sessione Fine::Chiusa,
                // Prima dell'autenticazione non c'è nulla da seguire: dopo l'autenticazione il
                // servizio rilegge tutto e si riscrive.
                Some(Verso::IscriviEntita { .. }) if !autenticato => {}
                Some(Verso::IscriviEntita { numero, entity_ids }) => {
                    if let Some((vecchia, _)) = iscrizione_entita.take() {
                        uscenti.push(json!({ "type": "unsubscribe_events", "subscription": vecchia, "id": prossimo_id }));
                        prossimo_id += 1;
                    }
                    if !entity_ids.is_empty() {
                        uscenti.push(json!({ "type": "subscribe_entities", "entity_ids": entity_ids, "id": prossimo_id }));
                        iscrizione_entita = Some((prossimo_id, numero));
                        prossimo_id += 1;
                    }
                }
                Some(Verso::Richiesta(r)) if !autenticato => {
                    let _ = r.risposta.send(Err(NON_CONNESSO.to_owned()));
                }
                Some(Verso::Richiesta(r)) => {
                    let mut msg = json!({ "type": r.tipo, "id": prossimo_id });
                    if let (Some(m), Value::Object(extra)) = (msg.as_object_mut(), r.extra) {
                        for (k, v) in extra {
                            m.entry(k).or_insert(v);
                        }
                    }
                    pendenti.insert(prossimo_id, r.risposta);
                    prossimo_id += 1;
                    uscenti.push(msg);
                }
            },
            _ = ping.tick(), if autenticato => {
                uscenti.push(json!({ "type": "ping", "id": prossimo_id }));
                prossimo_id += 1;
                if attesa_pong.is_none() {
                    attesa_pong = Some(Instant::now() + PING_TIMEOUT);
                }
            }
            _ = sleep_until(scadenza.unwrap_or(scadenza_apertura)), if scadenza.is_some() => {
                // Il server non risponde: non si aspetta oltre nemmeno la chiusura.
                let _ = timeout(Duration::from_secs(1), scrivi.close()).await;
                break 'sessione Fine::Errore("Home Assistant non risponde".into());
            }
        }
        for m in uscenti {
            if scrivi.send(Message::Text(m.to_string().into())).await.is_err() {
                break 'sessione Fine::Chiusa;
            }
        }
    };

    for (_, tx) in pendenti.drain() {
        let _ = tx.send(Err(NON_CONNESSO.to_owned()));
    }
    fine
}

/// Il risultato si prende dal messaggio, non si copia: quello di `get_states` di una casa vera è grosso.
/// Il contatore del registro diagnostico per il tipo di un messaggio (gli `event` si contano a
/// parte, per sorgente).
fn conta_messaggio(diagnosi: &Diagnosi, tipo: Option<&str>) {
    let c = match tipo {
        Some("result") => Contatore::ws_result,
        Some("pong") => Contatore::ws_pong,
        Some("auth_required" | "auth_ok" | "auth_invalid") => Contatore::ws_auth,
        Some("event") => return,
        _ => Contatore::ws_altro,
    };
    diagnosi.conta(c);
}

/// Il contatore di un evento del bus: solo quelli a cui ci si iscrive hanno un nome loro.
fn contatore_evento(tipo: Option<&str>) -> Contatore {
    match tipo {
        Some("area_registry_updated") => Contatore::ev_area_registry_updated,
        Some("device_registry_updated") => Contatore::ev_device_registry_updated,
        Some("entity_registry_updated") => Contatore::ev_entity_registry_updated,
        Some("core_config_updated") => Contatore::ev_core_config_updated,
        _ => Contatore::ev_altro,
    }
}

fn esito(mut v: Value) -> Result<Value, String> {
    if v["success"].as_bool().unwrap_or(false) {
        return Ok(v.get_mut("result").map(Value::take).unwrap_or(Value::Null));
    }
    Err(messaggio_errore(&v["error"]))
}

fn messaggio_errore(e: &Value) -> String {
    e["message"].as_str().or_else(|| e["code"].as_str()).unwrap_or("errore di Home Assistant").to_owned()
}

/// Una risposta (`"type": "result"`) letta senza l'albero `Value`: (id, `result` così com'è nel
/// testo, o il messaggio d'errore). `None` per ogni altro messaggio, che segue la strada solita.
/// Il controllo del testo è solo un filtro veloce: decide il campo `type` letto davvero.
fn risultato_diretto(testo: &str) -> Option<(u64, Result<Box<RawValue>, String>)> {
    #[derive(serde::Deserialize)]
    struct Risultato<'a> {
        id: u64,
        #[serde(rename = "type", borrow)]
        tipo: std::borrow::Cow<'a, str>,
        #[serde(default)]
        success: bool,
        #[serde(borrow, default)]
        result: Option<&'a RawValue>,
        #[serde(default)]
        error: Value,
    }
    if !testo.contains("\"type\":\"result\"") {
        return None;
    }
    let r: Risultato = serde_json::from_str(testo).ok()?;
    if r.tipo != "result" {
        return None;
    }
    let esito = if r.success {
        Ok(match r.result {
            Some(raw) => raw.to_owned(),
            None => RawValue::from_string("null".into()).ok()?,
        })
    } else {
        Err(messaggio_errore(&r.error))
    };
    Some((r.id, esito))
}

/// Connessione di prova: apre, autentica, legge gli stati, chiude. 10 s al massimo.
pub(crate) async fn prova_connessione(url: &str, token: &str) -> TestConnectionResult {
    let fallita = |e: String| TestConnectionResult { ok: false, entity_count: None, error: Some(e) };
    let prova = async {
        let ws = match connect_async(url_ws(url)).await {
            Ok((ws, _)) => ws,
            Err(e) => return fallita(format!("connessione non riuscita: {e}")),
        };
        let (mut scrivi, mut leggi) = ws.split();
        let mut autenticato = false;
        while let Some(Ok(m)) = leggi.next().await {
            let Message::Text(testo) = m else { continue };
            let Ok(v) = serde_json::from_str::<Value>(&testo) else { continue };
            let uscente = match v["type"].as_str() {
                Some("auth_required") => json!({ "type": "auth", "access_token": token }),
                Some("auth_ok") => {
                    autenticato = true;
                    json!({ "type": "get_states", "id": 1 })
                }
                Some("auth_invalid") => return fallita("token non valido".into()),
                Some("result") if autenticato => {
                    let _ = scrivi.close().await;
                    return match esito(v) {
                        Ok(r) => TestConnectionResult {
                            ok: true,
                            entity_count: r.as_array().map(|a| a.len() as u32),
                            error: None,
                        },
                        Err(e) => fallita(e),
                    };
                }
                _ => continue,
            };
            if scrivi.send(Message::Text(uscente.to_string().into())).await.is_err() {
                break;
            }
        }
        fallita("connessione chiusa inaspettatamente".into())
    };
    timeout(TIMEOUT_APERTURA, prova)
        .await
        .unwrap_or_else(|_| fallita("Home Assistant non risponde".into()))
}

/// `http(s)://host` → `ws(s)://host/api/websocket`.
fn url_ws(url_http: &str) -> String {
    let base = url_http.trim_end_matches('/');
    let ws = if let Some(resto) = base.strip_prefix("https://") {
        format!("wss://{resto}")
    } else if let Some(resto) = base.strip_prefix("http://") {
        format!("ws://{resto}")
    } else {
        base.to_owned()
    };
    format!("{ws}/api/websocket")
}

fn ritardo(ritardi: &[Duration], tentativo: usize) -> Duration {
    let ritardi = if ritardi.is_empty() { &RITARDI_RICONNESSIONE[..] } else { ritardi };
    ritardi[tentativo.min(ritardi.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Task 14g: le risposte si leggono senza albero `Value`, col `result` così com'è nel testo;
    /// gli altri messaggi (anche un evento che contiene quel testo) seguono la strada solita.
    #[test]
    fn risposte_lette_dal_testo() {
        let stati = r#"[{"entity_id":"light.a","state":"on","attributes":{"x":[1,2]}}]"#;
        let ok = format!(r#"{{"id":12,"type":"result","success":true,"result":{stati}}}"#);
        let (id, esito) = risultato_diretto(&ok).expect("una risposta");
        assert_eq!((id, esito.unwrap().get()), (12, stati));
        // Chiavi in un altro ordine (come le scrive serde_json) e `result` nullo.
        let (_, nullo) = risultato_diretto(r#"{"id":3,"result":null,"success":true,"type":"result"}"#).unwrap();
        assert_eq!(nullo.unwrap().get(), "null");
        let errore = r#"{"id":4,"type":"result","success":false,"error":{"code":"unknown_command","message":"Unknown command."}}"#;
        assert_eq!(risultato_diretto(errore).unwrap().1.unwrap_err(), "Unknown command.");
        let evento = r#"{"id":5,"type":"event","event":{"a":{"sensor.x":{"s":"on","a":{"type":"result"}}}}}"#;
        assert!(risultato_diretto(evento).is_none());
        assert!(risultato_diretto(r#"{"id":6,"type":"pong"}"#).is_none());
    }

    #[test]
    fn url_websocket() {
        assert_eq!(url_ws("http://casa.local:8123/"), "ws://casa.local:8123/api/websocket");
        assert_eq!(url_ws("https://casa.example.org"), "wss://casa.example.org/api/websocket");
    }

    #[test]
    fn ritardi_come_v2() {
        let attesi = [1, 2, 4, 8, 16, 30, 30, 30];
        for (i, s) in attesi.iter().enumerate() {
            assert_eq!(ritardo(&[], i), Duration::from_secs(*s));
        }
        assert_eq!(ritardo(&[Duration::from_millis(5)], 9), Duration::from_millis(5));
    }

    /// Senza un provider crittografico di rustls, `wss://` va in panico invece di dare errore.
    #[tokio::test]
    async fn wss_da_errore_e_non_panico() {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let porta = l.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((tcp, _)) = l.accept().await {
                drop(tcp); // chiude subito: l'handshake TLS fallisce
            }
        });
        let r = prova_connessione(&format!("https://127.0.0.1:{porta}"), "finto").await;
        assert!(!r.ok);
    }
}
