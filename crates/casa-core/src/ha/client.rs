//! Connessione WebSocket a Home Assistant (port di `src-tauri/src/ha/client.rs`).
//!
//! Un attore tokio per ambiente: si autentica, si iscrive a `state_changed` e alle notifiche
//! persistenti (`persistent_notification/subscribe`), risponde alle richieste e si riconnette
//! con i ritardi di v2. Gli eventi escono da una callback (`DalClient`), mai dal token.
use std::collections::HashMap;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::{sleep_until, timeout, timeout_at, Instant, MissedTickBehavior};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

use super::eventi::{interpreta, interpreta_notifiche, EventoWs};
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
    risposta: oneshot::Sender<Result<Value, String>>,
}

/// Maniglia economica da clonare verso l'attore.
#[derive(Clone)]
pub(crate) struct Client {
    tx: mpsc::UnboundedSender<Richiesta>,
}

impl Client {
    /// Comando WS generico; `extra` (oggetto) viene fuso nel messaggio.
    pub(crate) async fn richiesta(&self, tipo: &str, extra: Value) -> Result<Value, String> {
        let (risposta, rx) = oneshot::channel();
        self.tx
            .send(Richiesta { tipo: tipo.to_owned(), extra, risposta })
            .map_err(|_| NON_CONNESSO.to_owned())?;
        rx.await.map_err(|_| NON_CONNESSO.to_owned())?
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
) -> (Client, JoinHandle<()>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(attore(url, token, ritardi, rx, uscita));
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
    mut rx: mpsc::UnboundedReceiver<Richiesta>,
    uscita: Uscita,
) {
    let mut tentativo: usize = 0;
    loop {
        uscita(DalClient::Stato(StatoConnessione::InConnessione));
        match sessione(&url, &token, &mut rx, &uscita, &mut tentativo).await {
            Fine::TokenNonValido => {
                uscita(DalClient::Stato(StatoConnessione::Errore("token non valido".into())));
                return; // definitivo, come in v2
            }
            Fine::Chiusa => uscita(DalClient::Stato(StatoConnessione::Disconnesso)),
            Fine::Errore(e) => uscita(DalClient::Stato(StatoConnessione::Errore(e))),
        }
        let attesa = Instant::now() + ritardo(&ritardi, tentativo);
        tentativo += 1;
        loop {
            tokio::select! {
                _ = sleep_until(attesa) => break,
                r = rx.recv() => match r {
                    Some(r) => { let _ = r.risposta.send(Err(NON_CONNESSO.to_owned())); }
                    None => return,
                },
            }
        }
    }
}

async fn sessione(
    url: &str,
    token: &str,
    rx: &mut mpsc::UnboundedReceiver<Richiesta>,
    uscita: &Uscita,
    tentativo: &mut usize,
) -> Fine {
    let scadenza_apertura = Instant::now() + TIMEOUT_APERTURA;
    let ws = match timeout_at(scadenza_apertura, connect_async(url_ws(url))).await {
        Err(_) => return Fine::Errore("Home Assistant non risponde".into()),
        Ok(Err(e)) => return Fine::Errore(format!("connessione non riuscita: {e}")),
        Ok(Ok((ws, _))) => ws,
    };
    let (mut scrivi, mut leggi) = ws.split();

    let mut pendenti: HashMap<u64, oneshot::Sender<Result<Value, String>>> = HashMap::new();
    let mut prossimo_id: u64 = 1;
    let mut autenticato = false;
    // Id della richiesta `persistent_notification/subscribe`: i suoi eventi portano quell'id.
    let mut iscrizione_notifiche: Option<u64> = None;
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
                let Ok(v) = serde_json::from_str::<Value>(&testo) else { continue };
                match v["type"].as_str() {
                    Some("auth_required") => {
                        uscenti.push(json!({ "type": "auth", "access_token": token }));
                    }
                    Some("auth_ok") => {
                        *tentativo = 0;
                        autenticato = true;
                        uscenti.push(json!({ "type": "subscribe_events", "event_type": "state_changed", "id": prossimo_id }));
                        prossimo_id += 1;
                        uscenti.push(json!({ "type": "persistent_notification/subscribe", "id": prossimo_id }));
                        iscrizione_notifiche = Some(prossimo_id);
                        prossimo_id += 1;
                        uscita(DalClient::Stato(StatoConnessione::Connesso));
                    }
                    Some("auth_invalid") => break 'sessione Fine::TokenNonValido,
                    Some("result") => {
                        let id = v["id"].as_u64().unwrap_or(0);
                        if let Some(tx) = pendenti.remove(&id) {
                            let _ = tx.send(esito(&v));
                        }
                    }
                    Some("pong") => attesa_pong = None,
                    Some("event") if autenticato => {
                        let e = if v["id"].as_u64().is_some() && v["id"].as_u64() == iscrizione_notifiche {
                            interpreta_notifiche(&v)
                        } else {
                            interpreta(&v)
                        };
                        if let Some(e) = e {
                            uscita(DalClient::Evento(e));
                        }
                    }
                    _ => {}
                }
            }
            r = rx.recv() => {
                let Some(r) = r else { break 'sessione Fine::Chiusa };
                if !autenticato {
                    let _ = r.risposta.send(Err(NON_CONNESSO.to_owned()));
                    continue;
                }
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

fn esito(v: &Value) -> Result<Value, String> {
    if v["success"].as_bool().unwrap_or(false) {
        return Ok(v["result"].clone());
    }
    let e = &v["error"];
    Err(e["message"]
        .as_str()
        .or_else(|| e["code"].as_str())
        .unwrap_or("errore di Home Assistant")
        .to_owned())
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
                    return match esito(&v) {
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
