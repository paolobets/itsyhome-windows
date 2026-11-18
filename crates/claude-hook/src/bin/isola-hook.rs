//! `isola-hook.exe`: l'aiutante che Claude Code lancia come hook `command` (spec M3 §10).
//!
//! - `isola-hook evento`: legge lo stdin dell'hook, lo riduce, lo manda a isola per il tubo
//!   protetto **dopo** aver verificato che il server sia `assistente.exe` nella stessa cartella e
//!   dello stesso utente. Stampa qualcosa solo per un `PermissionRequest` deciso da isola.
//! - `isola-hook barra`: la riga della barra di stato, poi i soli `DatiBarra` a isola.
//!
//! Ogni errore: uscita 0, niente su stdout, niente su stderr. La destinazione è sempre quella vera
//! (`Destinazione::reale`): nessuna variabile d'ambiente, nessun argomento la cambia.

use std::io::Write;
use std::time::Duration;

use claude_hook::tubo::Destinazione;

/// Budget totale della barra di stato.
const TEMPO_BARRA: Duration = Duration::from_millis(40);

fn main() {
    // Un panico non deve scrivere su stderr (Claude Code potrebbe mostrarlo): si esce con 0.
    std::panic::set_hook(Box::new(|_| {}));
    let _ = std::panic::catch_unwind(esegui);
}

fn esegui() {
    let comando = std::env::args().nth(1);
    match comando.as_deref() {
        Some("evento") => {
            let uscita = claude_hook::aiutante::evento(
                std::io::stdin().lock(),
                Destinazione::reale().as_ref(),
            );
            // L'unico punto in cui `evento` scrive su stdout: il testo viene solo da `decisione_json`.
            if let Some(testo) = uscita {
                let mut out = std::io::stdout().lock();
                let _ = out.write_all(testo.as_bytes());
                let _ = out.flush();
            }
        }
        Some("barra") => {
            let _ = claude_hook::barra::esegui(
                std::io::stdin().lock(),
                std::io::stdout().lock(),
                Destinazione::reale().as_ref(),
                TEMPO_BARRA,
            );
        }
        _ => {}
    }
}
