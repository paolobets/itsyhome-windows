//! Finestra di gestione FINTA, solo per i test di `assistente::gestione` (nessuna finestra).
//!
//! Si comporta secondo il nome dell'ultima cartella di `--dati <cartella>` e scrive il proprio
//! PID in `<cartella>/../pid.txt`:
//! - `educato`: a ogni `apri` o `apri_schede` risponde `pronta`; alla `chiudi` scrive `chiusa` DUE volte ed esce
//!   (l'assistente deve consegnare `Chiusa` una volta sola);
//! - `muto`: esce subito senza dire niente (stdout chiuso = finestra chiusa);
//! - `appeso`: legge una riga e poi si blocca per un'ora con stdout aperto, senza più leggere stdin
//!   (l'assistente deve ucciderlo entro la sua scadenza).
use std::io::{BufRead, Write};
use std::path::PathBuf;

fn main() {
    let argomenti: Vec<String> = std::env::args().collect();
    let dati = argomenti
        .iter()
        .position(|a| a == "--dati")
        .and_then(|i| argomenti.get(i + 1))
        .map(PathBuf::from)
        .expect("--dati");
    if let Some(su) = dati.parent() {
        // File temporaneo poi rinominato: chi legge vede o nessun file o il PID intero.
        let provvisorio = su.join("pid.txt.tmp");
        if std::fs::write(&provvisorio, std::process::id().to_string()).is_ok() {
            let _ = std::fs::rename(&provvisorio, su.join("pid.txt"));
        }
    }
    let modo = dati.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_owned();
    let mut uscita = std::io::stdout();
    match modo.as_str() {
        "muto" => {}
        "appeso" => {
            let mut riga = String::new();
            let _ = std::io::stdin().lock().read_line(&mut riga);
            std::thread::sleep(std::time::Duration::from_secs(3600));
        }
        _ => {
            for riga in std::io::stdin().lock().lines() {
                let Ok(riga) = riga else { break };
                if riga.contains(r#""tipo":"apri""#) || riga.contains(r#""tipo":"apri_schede""#) {
                    // Le schede arrivate si salvano accanto al PID (`ultima.json`): chi prova vede
                    // cosa ha davvero ricevuto il figlio.
                    if riga.contains(r#""tipo":"apri_schede""#) {
                        if let Some(su) = dati.parent() {
                            let _ = std::fs::write(su.join("ultima.json"), &riga);
                        }
                    }
                    let _ = writeln!(uscita, r#"{{"tipo":"pronta"}}"#);
                } else if riga.contains(r#""tipo":"chiudi""#) {
                    let _ = writeln!(uscita, r#"{{"tipo":"chiusa"}}"#);
                    let _ = writeln!(uscita, r#"{{"tipo":"chiusa"}}"#);
                    break;
                }
                let _ = uscita.flush();
            }
        }
    }
}
