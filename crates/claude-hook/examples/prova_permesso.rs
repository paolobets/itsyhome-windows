//! Spike del permesso con hook HTTP di Claude Code (spec M3 §6.3). NON è codice di prodotto.
//!
//! Uso: `cargo run -p claude-hook --example prova_permesso -- [--porta N] [--cartella <dir>]`
//!
//! Ascolta solo su 127.0.0.1, scrive `<cartella>/impostazioni-prova.json` (cartella predefinita:
//! `%TEMP%/isola-spike`) e stampa i due comandi da dare a Claude Code. Non legge né scrive mai
//! `~/.claude/settings.json`. Di ogni richiesta stampa solo metadati: mai `tool_input`, `prompt`,
//! `cwd`.
//!
//! Documentazione usata (code.claude.com/docs/en/hooks, letta il 17/11/2026):
//!
//! HTTP hook fields
//! - `type: "http"`, `url` (POST), `headers` (valori con `$VAR` o `${VAR}` solo per le variabili
//!   elencate in `allowedEnvVars`; le non elencate diventano stringa vuota), `timeout` in secondi
//!   (predefinito 600), `onFailure` ("continue" | "block", da v2.1.295; noi NON lo usiamo).
//! - Il corpo è il JSON dell'evento con `Content-Type: application/json`; la risposta usa lo stesso
//!   JSON dei comandi. 2xx a corpo vuoto = successo senza output. 2xx con oggetto JSON = letto come
//!   output dei comandi. 2xx con altro corpo = errore non bloccante. Non 2xx, connessione fallita
//!   o scadenza = errore non bloccante: l'azione prosegue (salvo `onFailure: "block"`).
//!
//! PermissionRequest (ingresso)
//! - Campi: `session_id`, `transcript_path`, `cwd`, `permission_mode`, `hook_event_name`,
//!   `tool_name`, `tool_input` (niente `tool_use_id`; per gli strumenti MCP anche `mcp_server`),
//!   `permission_suggestions` opzionale.
//!
//! PermissionRequest decision control (uscita)
//! - `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}`
//!   (facoltativi: `updatedInput`, `updatedPermissions`: noi non li usiamo mai).
//! - `...{"behavior":"deny","message":"...","interrupt":false}`.
//! - Le regole deny e ask continuano a valere anche con `allow`. Nessuna decisione (corpo vuoto)
//!   = il flusso normale dei permessi prosegue. L'uscita 2 non vale per questo evento.
//! - La documentazione NON dice cosa succede se l'utente risponde nel terminale mentre l'hook è
//!   in attesa: è la domanda P1/P2 di questa prova.

use std::io::{BufRead, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use claude_hook::PERCORSO_EVENTO;
use serde_json::{json, Value};

const MAX_INTESTAZIONI: usize = 64 * 1024;
const MAX_CORPO: usize = 1024 * 1024;
/// Oltre il timeout dell'hook (120 s) la prova smette di aspettare.
const LIMITE_ATTESA: Duration = Duration::from_secs(125);

type Righe = Arc<Mutex<Receiver<String>>>;

fn ora() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

fn main() {
    let mut porta: u16 = 0;
    let mut cartella = std::env::temp_dir().join("isola-spike");
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--porta" => {
                porta = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| uscita("--porta vuole un numero"));
            }
            "--cartella" => {
                cartella = PathBuf::from(
                    args.next()
                        .unwrap_or_else(|| uscita("--cartella vuole un percorso")),
                );
            }
            altro => uscita(&format!("argomento sconosciuto: {altro}")),
        }
    }

    let ascolto = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, porta)))
        .unwrap_or_else(|e| uscita(&format!("non riesco ad ascoltare su 127.0.0.1: {e}")));
    let porta = ascolto.local_addr().map(|a| a.port()).unwrap_or(porta);

    let mut grezzo = [0u8; 16];
    getrandom::fill(&mut grezzo).unwrap_or_else(|e| uscita(&format!("niente casualità: {e}")));
    let codice: String = grezzo.iter().map(|b| format!("{b:02x}")).collect();

    if let Err(e) = std::fs::create_dir_all(&cartella) {
        uscita(&format!("non riesco a creare {}: {e}", cartella.display()));
    }
    let file = cartella.join("impostazioni-prova.json");
    let impostazioni = impostazioni_di_prova(porta);
    let testo = serde_json::to_string_pretty(&impostazioni).unwrap_or_default();
    if let Err(e) = std::fs::write(&file, testo) {
        uscita(&format!("non riesco a scrivere {}: {e}", file.display()));
    }

    println!("Ascolto su 127.0.0.1:{porta}; impostazioni di prova: {}", file.display());
    println!();
    println!("Per Paolo, in un terminale in una cartella già fidata:");
    println!("  $env:ISOLA_HOOK_TOKEN = '{codice}'");
    println!(
        "  claude --permission-mode default --settings {}",
        file.display().to_string().replace('\\', "/")
    );
    println!();
    println!("Sui permessi: c = consenti, n = nega, a = aspetta senza rispondere, x = chiudi senza decisione.");
    println!();

    // Un solo lettore della console; le connessioni in attesa si passano le righe.
    let (tx, rx) = mpsc::channel::<String>();
    thread::spawn(move || {
        for riga in std::io::stdin().lock().lines().map_while(Result::ok) {
            if tx.send(riga).is_err() {
                break;
            }
        }
    });
    let righe: Righe = Arc::new(Mutex::new(rx));
    let codice = Arc::new(codice);

    for flusso in ascolto.incoming().flatten() {
        let righe = Arc::clone(&righe);
        let codice = Arc::clone(&codice);
        thread::spawn(move || gestisci(flusso, porta, &codice, &righe));
    }
}

fn uscita(messaggio: &str) -> ! {
    eprintln!("{messaggio}");
    std::process::exit(2);
}

/// Le impostazioni di prova: hook HTTP, il codice solo come variabile d'ambiente.
fn impostazioni_di_prova(porta: u16) -> Value {
    let gancio = |timeout: u32| {
        json!([{ "hooks": [{
            "type": "http",
            "url": format!("http://127.0.0.1:{porta}{PERCORSO_EVENTO}"),
            "timeout": timeout,
            "headers": { "Authorization": "Bearer $ISOLA_HOOK_TOKEN" },
            "allowedEnvVars": ["ISOLA_HOOK_TOKEN"],
        }] }])
    };
    json!({ "hooks": {
        "PermissionRequest": gancio(120),
        "PreToolUse": gancio(5),
        "Notification": gancio(5),
        "Stop": gancio(5),
    } })
}

struct Richiesta {
    intestazioni: Vec<(String, String)>,
    corpo: Vec<u8>,
}

fn leggi_richiesta(flusso: &mut TcpStream) -> Option<Richiesta> {
    flusso.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let mut buf: Vec<u8> = Vec::new();
    let mut pezzo = [0u8; 4096];
    let fine = loop {
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p;
        }
        if buf.len() > MAX_INTESTAZIONI {
            return None;
        }
        let n = flusso.read(&mut pezzo).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&pezzo[..n]);
    };
    let testa = String::from_utf8_lossy(&buf[..fine]).into_owned();
    let intestazioni: Vec<(String, String)> = testa
        .lines()
        .skip(1)
        .filter_map(|r| r.split_once(':'))
        .map(|(n, v)| (n.trim().to_string(), v.trim().to_string()))
        .collect();
    let lunghezza = intestazioni
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse::<usize>().ok())
        .unwrap_or(0)
        .min(MAX_CORPO);
    let mut corpo = buf[fine + 4..].to_vec();
    while corpo.len() < lunghezza {
        let n = flusso.read(&mut pezzo).ok()?;
        if n == 0 {
            break;
        }
        corpo.extend_from_slice(&pezzo[..n]);
    }
    Some(Richiesta { intestazioni, corpo })
}

fn valore<'a>(r: &'a Richiesta, nome: &str) -> Option<&'a str> {
    r.intestazioni
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(nome))
        .map(|(_, v)| v.as_str())
}

fn gestisci(mut flusso: TcpStream, porta: u16, codice: &str, righe: &Righe) {
    let mittente = flusso.peer_addr().ok();
    let Some(richiesta) = leggi_richiesta(&mut flusso) else {
        println!("[{}] richiesta illeggibile o incompleta", ora());
        return;
    };
    // Solo i due campi che servono: il resto del corpo non si stampa e non si tiene.
    let corpo: Value = serde_json::from_slice(&richiesta.corpo).unwrap_or(Value::Null);
    let evento = corpo["hook_event_name"].as_str().unwrap_or("?").to_string();
    let strumento = corpo["tool_name"].as_str().unwrap_or("-").to_string();
    drop(corpo);

    let nomi: Vec<&str> = richiesta.intestazioni.iter().map(|(n, _)| n.as_str()).collect();
    let autorizzazione = match valore(&richiesta, "authorization") {
        None => "assente".to_string(),
        Some(v) => {
            let t = v.strip_prefix("Bearer").unwrap_or(v).trim();
            if t.is_empty() {
                "vuota".to_string()
            } else if t == codice {
                "corretta".to_string()
            } else {
                "diversa dal codice".to_string()
            }
        }
    };
    println!("[{}] evento={evento} strumento={strumento}", ora());
    println!("    intestazioni: {}", nomi.join(", "));
    println!(
        "    Host={} Content-Type={} Origin={} Authorization={autorizzazione}",
        valore(&richiesta, "host").unwrap_or("(assente)"),
        valore(&richiesta, "content-type").unwrap_or("(assente)"),
        if valore(&richiesta, "origin").is_some() { "PRESENTE" } else { "assente" },
    );
    match mittente {
        Some(m) => {
            println!("    porta del mittente: {}", m.port());
            stampa_mittente(m.port(), porta);
        }
        None => println!("    mittente sconosciuto"),
    }

    if evento == "PermissionRequest" {
        permesso(flusso, righe);
    } else {
        rispondi(&mut flusso, None);
    }
}

/// Scrive la risposta e chiude. `None` = 200 a corpo vuoto (nessuna decisione).
fn rispondi(flusso: &mut TcpStream, corpo: Option<&str>) {
    let corpo = corpo.unwrap_or("");
    let tipo = if corpo.is_empty() { "" } else { "Content-Type: application/json\r\n" };
    let risposta = format!(
        "HTTP/1.1 200 OK\r\n{tipo}Content-Length: {}\r\nConnection: close\r\n\r\n{corpo}",
        corpo.len()
    );
    let _ = flusso.write_all(risposta.as_bytes());
    let _ = flusso.flush();
}

fn decisione(comportamento: &str) -> String {
    let mut d = json!({ "behavior": comportamento });
    if comportamento == "deny" {
        d["message"] = json!("Negato dalla prova (spike)");
        d["interrupt"] = json!(false);
    }
    json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": d } })
        .to_string()
}

/// `true` se il mittente ha chiuso la connessione (peek non bloccante).
fn chiusa(flusso: &TcpStream) -> bool {
    let mut b = [0u8; 1];
    match flusso.peek(&mut b) {
        Ok(0) => true,
        Ok(_) => false,
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => false,
        Err(_) => true,
    }
}

fn permesso(mut flusso: TcpStream, righe: &Righe) {
    let inizio = Instant::now();
    println!("    PERMESSO in attesa. [c]onsenti / [n]ega / [a]spetta / [x] chiudi:");
    if flusso.set_nonblocking(true).is_err() {
        println!("    non riesco a controllare la connessione");
    }
    let mut muto = false;
    loop {
        if chiusa(&flusso) {
            println!(
                "    connessione chiusa dal terminale dopo {} s",
                inizio.elapsed().as_secs()
            );
            return;
        }
        if inizio.elapsed() > LIMITE_ATTESA {
            println!(
                "    ancora aperta dopo {} s: chiudo senza decisione",
                inizio.elapsed().as_secs()
            );
            return;
        }
        if muto {
            thread::sleep(Duration::from_millis(250));
            continue;
        }
        let riga = match righe.lock() {
            Ok(rx) => rx.recv_timeout(Duration::from_millis(250)),
            Err(_) => return,
        };
        match riga {
            Ok(r) => match r.trim() {
                "c" => {
                    let _ = flusso.set_nonblocking(false);
                    rispondi(&mut flusso, Some(&decisione("allow")));
                    println!("    risposto: consenti (dopo {} s)", inizio.elapsed().as_secs());
                    return;
                }
                "n" => {
                    let _ = flusso.set_nonblocking(false);
                    rispondi(&mut flusso, Some(&decisione("deny")));
                    println!("    risposto: nega (dopo {} s)", inizio.elapsed().as_secs());
                    return;
                }
                "a" => {
                    muto = true;
                    println!("    aspetto senza rispondere (fino a {} s)", LIMITE_ATTESA.as_secs());
                }
                "x" => {
                    println!("    chiusa senza decisione");
                    return;
                }
                _ => println!("    scelta non valida: c, n, a oppure x"),
            },
            Err(RecvTimeoutError::Timeout) => {}
            // Console chiusa: niente più scelte, si resta in attesa come con «a».
            Err(RecvTimeoutError::Disconnected) => muto = true,
        }
    }
}

#[cfg(not(windows))]
fn stampa_mittente(_porta_mittente: u16, _porta_nostra: u16) {
    println!("    PID del mittente: non disponibile fuori da Windows");
}

#[cfg(windows)]
fn stampa_mittente(porta_mittente: u16, porta_nostra: u16) {
    match claude_hook::mittente::pid_mittente(porta_mittente, porta_nostra) {
        Some(pid) => println!(
            "    PID del mittente: {pid}; catena: {}",
            claude_hook::mittente::catena(pid).join(" <- ")
        ),
        None => println!("    PID del mittente: non trovato nella tabella TCP"),
    }
}
