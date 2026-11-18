#![allow(dead_code)]
//! Un server HTTP finto su 127.0.0.1 che serve le fixture e REGISTRA ogni richiesta (metodo,
//! percorso, parametri, intestazioni). Lo usano i test del crate e `examples/finta_api.rs`
//! (con `#[path]`): non è compilato nell'assistente.
//!
//! Il registro tiene le intestazioni (anche `x-api-key`) solo in memoria, per i test: il server
//! non le stampa mai.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Richiesta {
    pub metodo: String,
    pub percorso: String,
    pub query: Vec<(String, String)>,
    pub intestazioni: Vec<(String, String)>,
}

impl Richiesta {
    pub fn parametro(&self, nome: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(k, _)| k == nome)
            .map(|(_, v)| v.as_str())
    }
    pub fn intestazione(&self, nome: &str) -> Option<&str> {
        self.intestazioni
            .iter()
            .find(|(k, _)| k == nome)
            .map(|(_, v)| v.as_str())
    }
}

pub type Gestore = dyn Fn(&Richiesta) -> (u16, String) + Send + Sync + 'static;

pub struct ServerFinto {
    pub porta: u16,
    registro: Arc<Mutex<Vec<Richiesta>>>,
    ferma: Arc<AtomicBool>,
}

impl ServerFinto {
    /// In ascolto su una porta libera di 127.0.0.1.
    pub fn avvia(
        gestore: impl Fn(&Richiesta) -> (u16, String) + Send + Sync + 'static,
    ) -> ServerFinto {
        Self::avvia_su(0, gestore)
    }

    pub fn avvia_su(
        porta: u16,
        gestore: impl Fn(&Richiesta) -> (u16, String) + Send + Sync + 'static,
    ) -> ServerFinto {
        let ascolto =
            TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], porta))).expect("porta locale");
        ascolto.set_nonblocking(true).expect("non bloccante");
        let porta = ascolto.local_addr().expect("indirizzo").port();
        let registro = Arc::new(Mutex::new(Vec::new()));
        let ferma = Arc::new(AtomicBool::new(false));
        let gestore: Arc<Gestore> = Arc::new(gestore);
        let (r, f) = (registro.clone(), ferma.clone());
        std::thread::spawn(move || {
            while !f.load(Ordering::Relaxed) {
                match ascolto.accept() {
                    Ok((flusso, _)) => {
                        let (r, g) = (r.clone(), gestore.clone());
                        std::thread::spawn(move || servi(flusso, &r, g.as_ref()));
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(5)),
                }
            }
        });
        ServerFinto {
            porta,
            registro,
            ferma,
        }
    }

    pub fn base(&self) -> String {
        format!("http://127.0.0.1:{}", self.porta)
    }

    pub fn richieste(&self) -> Vec<Richiesta> {
        self.registro.lock().unwrap().clone()
    }
}

impl Drop for ServerFinto {
    fn drop(&mut self) {
        self.ferma.store(true, Ordering::Relaxed);
    }
}

fn decodifica(s: &str) -> String {
    let b = s.as_bytes();
    let mut fuori = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let valore = std::str::from_utf8(&b[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok());
            if let Some(v) = valore {
                fuori.push(v);
                i += 3;
                continue;
            }
        }
        fuori.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&fuori).into_owned()
}

fn servi(mut flusso: TcpStream, registro: &Mutex<Vec<Richiesta>>, gestore: &Gestore) {
    let _ = flusso.set_nonblocking(false);
    let _ = flusso.set_read_timeout(Some(Duration::from_secs(5)));
    let mut testa = Vec::new();
    let mut buf = [0u8; 2048];
    while !testa.windows(4).any(|w| w == b"\r\n\r\n") {
        match flusso.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => testa.extend_from_slice(&buf[..n]),
        }
        if testa.len() > 64 * 1024 {
            return;
        }
    }
    let testo = String::from_utf8_lossy(&testa).into_owned();
    let mut righe = testo.split("\r\n");
    let prima = righe.next().unwrap_or("");
    let mut parti = prima.split(' ');
    let metodo = parti.next().unwrap_or("").to_owned();
    let bersaglio = parti.next().unwrap_or("");
    let (percorso, query) = match bersaglio.split_once('?') {
        Some((p, q)) => (p.to_owned(), q.to_owned()),
        None => (bersaglio.to_owned(), String::new()),
    };
    let query = query
        .split('&')
        .filter(|c| !c.is_empty())
        .map(|c| match c.split_once('=') {
            Some((k, v)) => (decodifica(k), decodifica(v)),
            None => (decodifica(c), String::new()),
        })
        .collect();
    let intestazioni = righe
        .take_while(|r| !r.is_empty())
        .filter_map(|r| {
            r.split_once(':')
                .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_owned()))
        })
        .collect();
    let richiesta = Richiesta {
        metodo,
        percorso,
        query,
        intestazioni,
    };
    registro.lock().unwrap().push(richiesta.clone());
    let (codice, corpo) = gestore(&richiesta);
    let risposta = format!(
        "HTTP/1.1 {codice} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{corpo}",
        corpo.len()
    );
    let _ = flusso.write_all(risposta.as_bytes());
    let _ = flusso.flush();
}

/// La fixture giusta per una richiesta (il suo contenuto è di prova: id finti, «Chiave di prova»).
pub fn fixture(r: &Richiesta) -> Option<&'static str> {
    Some(match r.percorso.as_str() {
        "/v1/organizations/me" => include_str!("../tests/fixture/organizzazione.json"),
        "/v1/organizations/analytics/cost" => {
            if r.parametro("time_bucket_width").is_some() {
                include_str!("../tests/fixture/costo_giorni.json")
            } else {
                match r.parametro("group_by[]") {
                    Some("model") => include_str!("../tests/fixture/costo_modello.json"),
                    Some("api_key_id") => include_str!("../tests/fixture/costo_chiave.json"),
                    Some("workspace_id") => include_str!("../tests/fixture/costo_workspace.json"),
                    _ => return None,
                }
            }
        }
        "/v1/organizations/analytics/usage" => match r.parametro("group_by[]") {
            Some("api_key_id") => include_str!("../tests/fixture/uso_chiave.json"),
            None => include_str!("../tests/fixture/uso_mese.json"),
            _ => return None,
        },
        "/v1/organizations/api_keys" => include_str!("../tests/fixture/chiavi.json"),
        "/v1/organizations/workspaces" => include_str!("../tests/fixture/workspace.json"),
        "/v1/organizations/users" => include_str!("../tests/fixture/membri.json"),
        "/v1/organizations/rate_limits" => include_str!("../tests/fixture/limiti.json"),
        _ => return None,
    })
}

/// Il gestore delle fixture: 200 con il file, 404 per il resto.
pub fn gestore_fixture(r: &Richiesta) -> (u16, String) {
    match (r.metodo.as_str(), fixture(r)) {
        ("GET", Some(corpo)) => (200, corpo.to_owned()),
        _ => (404, r#"{"error":"non trovato"}"#.to_owned()),
    }
}
