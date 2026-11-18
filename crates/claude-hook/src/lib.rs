//! Hook HTTP di Claude Code per l'assistente «isola» (M3).
//!
//! L'ascoltatore locale blindato (spec M3 §6.2): solo 127.0.0.1, codice segreto, nessuna
//! intestazione CORS, limiti stretti su intestazioni e corpo.

pub mod ascoltatore;
pub mod codice;
pub mod evento;
pub mod mittente;
pub mod sessioni;

pub use ascoltatore::{
    avvia, Arrivo, Ascoltatore, AttesaPermesso, ConfigAscolto, Decisione, ErroreAvvio, Gestore,
    Percorso, ATTESA_MASSIMA,
};
pub use codice::CodiceHook;

/// Percorso a cui Claude Code manda gli eventi degli hook.
pub const PERCORSO_EVENTO: &str = "/isola/v1/evento";
/// Percorso a cui il comando della barra di stato manda i suoi dati.
pub const PERCORSO_BARRA: &str = "/isola/v1/barra";

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn percorsi_distinti_sotto_lo_stesso_prefisso() {
        assert_ne!(PERCORSO_EVENTO, PERCORSO_BARRA);
        assert!(PERCORSO_EVENTO.starts_with("/isola/v1/"));
        assert!(PERCORSO_BARRA.starts_with("/isola/v1/"));
    }
}
