//! Claude Code per l'assistente «isola» (M3).
//!
//! Il canale protetto (spec M3 §10): isola serve un tubo con nome riservato all'utente
//! (`tubo`), l'aiutante `isola-hook.exe` (`aiutante`, `barra`) lo usa dopo aver verificato che il
//! server sia davvero isola (`verifica`). Per il tubo passano solo i campi già ridotti
//! (`messaggio`).

pub mod aiutante;
pub mod barra;
pub mod evento;
pub mod installa;
pub mod messaggio;
pub mod sessioni;
pub mod tubo;
pub mod verifica;

pub use barra::{DatiBarra, Limite};
pub use tubo::{
    avvia, nome_tubo, Arrivo, AttesaPermesso, ConfigTubo, Contenuto, Decisione, ErroreAvvio,
    Gestore, Servitore, ATTESA_MASSIMA,
};
