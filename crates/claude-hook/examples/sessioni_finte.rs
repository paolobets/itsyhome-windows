//! Sessioni finte di Claude Code verso l'ascoltatore dell'assistente, per prove e misure (M3). NON è
//! codice di prodotto.
//!
//! Uso: `cargo run -p claude-hook --example sessioni_finte -- --porta <n> [--sessioni 3] [--secondi 60]`
//!
//! Il codice segreto viene dalla variabile `ISOLA_HOOK_TOKEN` del processo (64 cifre esadecimali):
//! non si stampa mai e non si legge nessun file. Per ogni sessione: un prompt all'inizio, un
//! `PreToolUse` ogni 2 s, uno `Stop` ogni 30 s (seguito da 10 s di pausa e da un nuovo prompt), una barra di stato
//! ogni 5 s, un `SessionEnd` alla fine. Le cartelle sono finte («progetto-a»…); nessun dato vero.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use claude_hook::{PERCORSO_BARRA, PERCORSO_EVENTO};

const STRUMENTI: [&str; 4] = ["Edit", "Bash", "Read", "Grep"];
const CARTELLE: [&str; 6] = ["progetto-a", "progetto-b", "progetto-c", "progetto-d", "progetto-e", "progetto-f"];

/// Una richiesta; il codice di stato della risposta (0 se la connessione è fallita).
fn invia(porta: u16, codice: &str, percorso: &str, corpo: &str) -> u16 {
    let indirizzo = SocketAddr::from((Ipv4Addr::LOCALHOST, porta));
    let Ok(mut s) = TcpStream::connect_timeout(&indirizzo, Duration::from_secs(2)) else { return 0 };
    let _ = s.set_read_timeout(Some(Duration::from_secs(3)));
    let _ = s.set_write_timeout(Some(Duration::from_secs(3)));
    let richiesta = format!(
        "POST {percorso} HTTP/1.1\r\nHost: 127.0.0.1:{porta}\r\nAuthorization: Bearer {codice}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{corpo}",
        corpo.len()
    );
    if s.write_all(richiesta.as_bytes()).is_err() {
        return 0;
    }
    let mut risposta = Vec::new();
    let _ = s.read_to_end(&mut risposta);
    String::from_utf8_lossy(&risposta).split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0)
}

fn json_testo(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn evento(sessione: &str, cartella: &str, nome: &str, strumento: Option<&str>) -> String {
    let cwd = json_testo(&format!("C:\\finto\\{cartella}"));
    let tool = strumento.map(|t| format!(",\"tool_name\":{}", json_testo(t))).unwrap_or_default();
    format!("{{\"session_id\":{},\"cwd\":{cwd},\"hook_event_name\":\"{nome}\"{tool}}}", json_testo(sessione))
}

fn barra(sessione: &str, cartella: &str, t: u64) -> String {
    let cwd = json_testo(&format!("C:\\finto\\{cartella}"));
    format!(
        "{{\"session_id\":{},\"cwd\":{cwd},\"model\":{{\"display_name\":\"Modello finto\"}},\"context_window\":{{\"used_percentage\":{}}},\"rate_limits\":{{\"five_hour\":{{\"used_percentage\":{},\"resets_at\":4102444800}},\"seven_day\":{{\"used_percentage\":{},\"resets_at\":4102444800}}}}}}",
        json_testo(sessione),
        (t * 3) % 100,
        (t / 2) % 100,
        (t / 10) % 100
    )
}

fn main() {
    let mut porta: u16 = 0;
    let mut sessioni: usize = 3;
    let mut secondi: u64 = 60;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let valore = args.next();
        match (a.as_str(), valore.and_then(|v| v.parse::<u64>().ok())) {
            ("--porta", Some(v)) => porta = u16::try_from(v).unwrap_or(0),
            ("--sessioni", Some(v)) => sessioni = (v as usize).clamp(1, CARTELLE.len()),
            ("--secondi", Some(v)) => secondi = v,
            _ => {
                eprintln!("uso: sessioni_finte --porta <n> [--sessioni 3] [--secondi 60]");
                std::process::exit(2);
            }
        }
    }
    let Ok(codice) = std::env::var("ISOLA_HOOK_TOKEN") else {
        eprintln!("manca la variabile ISOLA_HOOK_TOKEN");
        std::process::exit(2);
    };
    if porta == 0 {
        eprintln!("manca --porta");
        std::process::exit(2);
    }
    // Id finti, distinti a ogni esecuzione.
    let avvio = Instant::now();
    let prefisso = format!("finta-{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()));
    let id = |i: usize| format!("{prefisso}-{i}");
    let mut rifiutate = 0u32;
    let mut manda = |percorso: &str, corpo: String| {
        let stato = invia(porta, &codice, percorso, &corpo);
        if stato != 200 {
            rifiutate += 1;
            eprintln!("risposta {stato} a {percorso}");
        }
    };
    println!("{sessioni} sessioni finte sulla porta {porta} per {secondi} s");
    for (i, cartella) in CARTELLE.iter().enumerate().take(sessioni) {
        manda(PERCORSO_EVENTO, evento(&id(i), cartella, "UserPromptSubmit", None));
    }
    for t in 1..=secondi {
        // Un passo al secondo, con i secondi contati dall'inizio (nessuna deriva).
        let atteso = Duration::from_secs(t);
        if let Some(manca) = atteso.checked_sub(avvio.elapsed()) {
            std::thread::sleep(manca);
        }
        for (i, cartella) in CARTELLE.iter().enumerate().take(sessioni) {
            // Le sessioni non partono tutte insieme: ognuna con il suo sfasamento di 7 s.
            let s = t.saturating_sub(7 * i as u64);
            if s == 0 {
                continue;
            }
            let sessione = id(i);
            // Ogni 30 s: Stop, 10 s di «fatto», un nuovo prompt, poi uno strumento ogni 2 s.
            if s % 30 == 0 {
                manda(PERCORSO_EVENTO, evento(&sessione, cartella, "Stop", None));
            } else if s % 30 == 10 {
                manda(PERCORSO_EVENTO, evento(&sessione, cartella, "UserPromptSubmit", None));
            } else if s % 30 > 10 && s % 2 == 0 {
                let strumento = STRUMENTI[(s / 2) as usize % STRUMENTI.len()];
                manda(PERCORSO_EVENTO, evento(&sessione, cartella, "PreToolUse", Some(strumento)));
            }
            if s % 5 == 0 {
                manda(PERCORSO_BARRA, barra(&sessione, cartella, s));
            }
        }
    }
    for (i, cartella) in CARTELLE.iter().enumerate().take(sessioni) {
        manda(PERCORSO_EVENTO, evento(&id(i), cartella, "SessionEnd", None));
    }
    println!("finito; richieste non accettate: {rifiutate}");
}
