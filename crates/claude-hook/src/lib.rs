//! Hook HTTP di Claude Code per l'assistente «isola» (M3).
//!
//! Per ora solo i percorsi su cui l'ascoltatore risponde (spec M3 §4).

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
