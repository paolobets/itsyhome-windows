//! `isola-hook.exe`: l'aiutante che Claude Code lancia come hook `command` (spec M3 §10).
//!
//! - `isola-hook evento`: legge lo stdin dell'hook, lo riduce, lo manda a isola per il tubo
//!   protetto **dopo** aver verificato che il server sia isola (proprietario del tubo, utente,
//!   integrità, `assistente.exe` nella stessa cartella). Stampa qualcosa solo per un
//!   `PermissionRequest` deciso da isola.
//! - `isola-hook barra`: la riga della barra di stato, poi i soli `DatiBarra` a isola.
//!
//! Ogni errore: uscita 0, niente su stdout, niente su stderr. La destinazione è sempre quella vera
//! (`Destinazione::reale`): nessuna variabile d'ambiente, nessun argomento la cambia.

use claude_hook::tubo::Destinazione;

fn main() {
    // Un panico non deve scrivere su stderr (Claude Code potrebbe mostrarlo): si esce con 0.
    std::panic::set_hook(Box::new(|_| {}));
    let _ = std::panic::catch_unwind(|| {
        let comando = std::env::args().nth(1);
        claude_hook::aiutante::principale(
            comando.as_deref(),
            std::io::stdin().lock(),
            std::io::stdout().lock(),
            Destinazione::reale().as_ref(),
        );
    });
}
