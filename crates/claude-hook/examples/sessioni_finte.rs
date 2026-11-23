//! Sessioni finte di Claude Code verso l'assistente, per prove e misure (M3). NON è codice di
//! prodotto.
//!
//! Uso: `cargo run -p claude-hook --example sessioni_finte -- --assistente <percorso di assistente.exe> [--sessioni 3] [--secondi 60] [--permesso]`
//!
//! Con `--permesso` la sessione 0 chiede anche un permesso finto (`PermissionRequest`, strumento
//! `Bash`, comando «npm test -- checkout --watch=false», cartella «progetto-a»), in un suo thread
//! che aspetta la decisione come l'aiutante vero e la stampa; senza risposta rinuncia da sé
//! (`ATTESA_MASSIMA`), e a fine prova il processo esce comunque. Serve per le foto e le prove del
//! permesso nella Coda e nel dettaglio della pagina Agenti.
//!
//! Parla per il tubo protetto di un'isola **di prova** (`isola-claude-<SID>-prova`) come
//! l'aiutante (`aiutante::evento`, `barra::esegui`): verifica che il server sia l'`assistente.exe`
//! indicato. Isola, dal canto suo, accetta solo il cliente atteso:
//! avviarla in prova (`ITSYHOME_ASSISTENTE_DIR`) con `ITSYHOME_AIUTANTE_ATTESO` = il percorso di
//! questo esempio. Per ogni sessione: un prompt all'inizio, un `PreToolUse` ogni 2 s, uno `Stop` ogni
//! 30 s (seguito da 10 s di pausa e da un nuovo prompt), una barra di stato ogni 5 s, un
//! `SessionEnd` alla fine. Le cartelle sono finte («progetto-a»…) e quattro sessioni su sei hanno un
//! `session_name` finto (`TITOLI`) nella barra; nessun dato vero.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use claude_hook::tubo::{nome_tubo, Destinazione, SUFFISSO_PROVA};

const STRUMENTI: [&str; 4] = ["Edit", "Bash", "Read", "Grep"];
const CARTELLE: [&str; 6] = [
    "progetto-a",
    "progetto-b",
    "progetto-c",
    "progetto-d",
    "progetto-e",
    "progetto-f",
];

/// Il `session_name` di ogni sessione, come lo manda la barra di stato di Claude Code: alcune ne
/// hanno uno (anche lungo, per vederlo tagliato), altre no e restano con la cartella.
const TITOLI: [Option<&str>; 6] = [
    Some("Sistemare il restyle"),
    Some("Aggiornare i test"),
    None,
    Some("Un titolo di sessione davvero molto lungo, per vedere come si taglia nella riga"),
    None,
    Some("Indagare sul riavvio"),
];

/// Che cosa mandare.
enum Invio {
    Evento(String),
    Barra(String),
}

fn invia(d: &Destinazione, i: Invio) {
    match i {
        // Fuori dal permesso l'aiutante non stampa mai nulla.
        Invio::Evento(json) => {
            let _ = claude_hook::aiutante::evento(json.as_bytes(), Some(d));
        }
        Invio::Barra(json) => {
            let _ = claude_hook::barra::esegui(
                json.as_bytes(),
                std::io::sink(),
                Some(d),
                Duration::from_millis(200),
            );
        }
    }
}

fn json_testo(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn evento(sessione: &str, cartella: &str, nome: &str, strumento: Option<&str>) -> String {
    let cwd = json_testo(&format!("C:\\finto\\{cartella}"));
    let tool = strumento
        .map(|t| format!(",\"tool_name\":{}", json_testo(t)))
        .unwrap_or_default();
    format!(
        "{{\"session_id\":{},\"cwd\":{cwd},\"hook_event_name\":\"{nome}\"{tool}}}",
        json_testo(sessione)
    )
}

/// Il permesso finto della sessione 0, come lo manda Claude Code all'hook.
fn permesso(sessione: &str, cartella: &str) -> String {
    let cwd = json_testo(&format!("C:\\finto\\{cartella}"));
    format!(
        "{{\"session_id\":{},\"cwd\":{cwd},\"hook_event_name\":\"PermissionRequest\",\"tool_name\":\"Bash\",\"tool_input\":{{\"command\":{}}}}}",
        json_testo(sessione),
        json_testo("npm test -- checkout --watch=false")
    )
}

fn barra(sessione: &str, cartella: &str, titolo: Option<&str>, t: u64) -> String {
    let cwd = json_testo(&format!("C:\\finto\\{cartella}"));
    // Come Claude Code: `session_name` c'è solo se la sessione ha un nome.
    let nome = titolo
        .map(|n| format!(",\"session_name\":{}", json_testo(n)))
        .unwrap_or_default();
    format!(
        "{{\"session_id\":{},\"cwd\":{cwd}{nome},\"model\":{{\"display_name\":\"Modello finto\"}},\"context_window\":{{\"used_percentage\":{}}},\"rate_limits\":{{\"five_hour\":{{\"used_percentage\":{},\"resets_at\":4102444800}},\"seven_day\":{{\"used_percentage\":{},\"resets_at\":4102444800}}}}}}",
        json_testo(sessione),
        (t * 3) % 100,
        (t / 2) % 100,
        (t / 10) % 100
    )
}

fn main() {
    let mut assistente: Option<PathBuf> = None;
    let mut sessioni: usize = 3;
    let mut secondi: u64 = 60;
    let mut con_permesso = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--permesso" {
            con_permesso = true;
            continue;
        }
        let valore = args.next();
        if a == "--assistente" {
            assistente = valore.map(PathBuf::from);
            continue;
        }
        match (a.as_str(), valore.and_then(|v| v.parse::<u64>().ok())) {
            ("--sessioni", Some(v)) => sessioni = (v as usize).clamp(1, CARTELLE.len()),
            ("--secondi", Some(v)) => secondi = v,
            _ => {
                eprintln!("uso: sessioni_finte --assistente <assistente.exe> [--sessioni 3] [--secondi 60] [--permesso]");
                std::process::exit(2);
            }
        }
    }
    let Some(server_atteso) = assistente else {
        eprintln!("manca --assistente");
        std::process::exit(2);
    };
    let nome = match nome_tubo() {
        Ok(n) => format!("{n}{SUFFISSO_PROVA}"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let d = Destinazione {
        nome,
        server_atteso,
    };
    // Id finti, distinti a ogni esecuzione.
    let avvio = Instant::now();
    let prefisso = format!(
        "finta-{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    );
    let id = |i: usize| format!("{prefisso}-{i}");
    let manda = |i: Invio| invia(&d, i);
    println!("{sessioni} sessioni finte per {secondi} s");
    for (i, cartella) in CARTELLE.iter().enumerate().take(sessioni) {
        manda(Invio::Evento(evento(
            &id(i),
            cartella,
            "UserPromptSubmit",
            None,
        )));
    }
    if con_permesso {
        // Come l'aiutante vero: manda il permesso e aspetta la decisione, in un suo thread (il
        // resto delle sessioni va avanti). Non lo si aspetta a fine prova.
        let d = d.clone();
        let json = permesso(&id(0), CARTELLE[0]);
        std::thread::spawn(move || {
            match claude_hook::aiutante::evento(json.as_bytes(), Some(&d)) {
                Some(decisione) => println!("decisione del permesso: {decisione}"),
                None => println!("permesso senza decisione"),
            }
        });
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
                manda(Invio::Evento(evento(&sessione, cartella, "Stop", None)));
            } else if s % 30 == 10 {
                manda(Invio::Evento(evento(
                    &sessione,
                    cartella,
                    "UserPromptSubmit",
                    None,
                )));
            } else if s % 30 > 10 && s % 2 == 0 {
                let strumento = STRUMENTI[(s / 2) as usize % STRUMENTI.len()];
                manda(Invio::Evento(evento(
                    &sessione,
                    cartella,
                    "PreToolUse",
                    Some(strumento),
                )));
            }
            if s % 5 == 0 {
                manda(Invio::Barra(barra(&sessione, cartella, TITOLI[i], s)));
            }
        }
    }
    for (i, cartella) in CARTELLE.iter().enumerate().take(sessioni) {
        manda(Invio::Evento(evento(&id(i), cartella, "SessionEnd", None)));
    }
    println!("finito");
}
