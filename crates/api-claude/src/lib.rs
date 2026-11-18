//! Le API di Anthropic in sola lettura per l'assistente «isola» (M3, Task 8).
//!
//! - `chiave`: la chiave da amministratore (mai stampabile).
//! - `letture`: l'elenco chiuso delle letture, tutte `GET`.
//! - `client`: l'unico metodo di rete (`leggi`).
//! - `modello`: le risposte, nei soli campi che servono.
//! - `riepilogo`: la sequenza di letture messa insieme.

pub mod chiave;
pub mod client;
pub mod letture;
pub mod modello;
pub mod riepilogo;

pub use chiave::ChiaveAdmin;
pub use client::{anthropic, finto, ClientApi, ErroreApi};
pub use letture::{Lettura, Raggruppa, PERCORSI_AMMESSI};
pub use modello::{importo_da, Importo};
pub use riepilogo::{aggiorna, spesa_insolita, Riepilogo, Scheda};

#[cfg(test)]
mod server_finto;

#[cfg(test)]
mod prove;

#[cfg(test)]
mod test {
    /// Il client non ha nessuna chiamata che scrive. Le parole cercate sono composte qui con
    /// `concat!` così questo file non le contiene.
    #[test]
    fn sorgente_senza_scritture() {
        let sorgente = include_str!("client.rs");
        for vietata in [
            concat!(".po", "st("),
            concat!(".p", "ut("),
            concat!(".pa", "tch("),
            concat!(".del", "ete("),
            concat!("Meth", "od::"),
            concat!(".r", "un("),
            concat!(".req", "uest("),
            concat!(".he", "ad("),
            concat!(".opt", "ions("),
        ] {
            assert!(!sorgente.contains(vietata), "client.rs contiene {vietata}");
        }
        assert!(sorgente.contains(".get("), "la sola chiamata è la lettura");
    }
}
