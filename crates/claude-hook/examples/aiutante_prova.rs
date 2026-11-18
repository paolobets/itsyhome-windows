//! L'aiutante **di prova**: lo stesso corpo di `isola-hook.exe` (`aiutante::principale`), ma la
//! destinazione viene dall'ambiente. NON è codice di prodotto e non si distribuisce: esiste perché
//! l'eseguibile vero non ha (e non deve avere) alcun modo di cambiare tubo o server atteso.
//!
//! - `ISOLA_PROVA_TUBO`: il nome del tubo (`\.\pipe\…`), per esempio quello di un'isola di prova
//!   (`isola-claude-<SID>-prova`) o uno unico dei test;
//! - `ISOLA_PROVA_SERVER`: il percorso dell'`assistente.exe` atteso come server.
//!
//! Senza tutte e due, nessuna destinazione: si comporta come con isola spenta.

use claude_hook::tubo::Destinazione;

fn main() {
    std::panic::set_hook(Box::new(|_| {}));
    let _ = std::panic::catch_unwind(|| {
        let destinazione = match (
            std::env::var("ISOLA_PROVA_TUBO"),
            std::env::var_os("ISOLA_PROVA_SERVER"),
        ) {
            (Ok(nome), Some(server)) => Some(Destinazione {
                nome,
                server_atteso: server.into(),
            }),
            _ => None,
        };
        let comando = std::env::args().nth(1);
        claude_hook::aiutante::principale(
            comando.as_deref(),
            std::io::stdin().lock(),
            std::io::stdout().lock(),
            destinazione.as_ref(),
        );
    });
}
