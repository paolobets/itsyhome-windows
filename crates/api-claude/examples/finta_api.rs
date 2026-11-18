//! Un server finto delle API di Anthropic su 127.0.0.1, per provare l'assistente senza chiave
//! vera: serve le fixture di `tests/fixture`.
//!
//!   cargo run -p api-claude --example finta_api            (porta 18765)
//!   cargo run -p api-claude --example finta_api -- 19000   (porta a scelta)
//!
//! Poi si avvia l'assistente di prova con `ITSYHOME_ASSISTENTE_DIR=<cartella>`,
//! `ITSYHOME_API_CLAUDE_BASE=http://127.0.0.1:<porta>` e `ITSYHOME_API_CLAUDE_CHIAVE=finta-...`.
//! Ogni richiesta arrivata si stampa (metodo e percorso; mai le intestazioni, quindi mai la chiave).

#[path = "../src/server_finto.rs"]
mod server_finto;

use server_finto::{gestore_fixture, ServerFinto};

fn main() {
    let porta: u16 = std::env::args()
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(18765);
    let server = ServerFinto::avvia_su(porta, |r| {
        let esito = gestore_fixture(r);
        println!("{} {} -> {}", r.metodo, r.percorso, esito.0);
        esito
    });
    println!("API finta su {} (Ctrl+C per fermare)", server.base());
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
