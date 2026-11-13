//! Eventi che `CasaService` consegna all'applicazione.
use std::sync::Arc;

use crate::types::MenuData;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatoConnessione {
    Disconnesso,
    InConnessione,
    Connesso,
    Errore(String),
}

#[derive(Debug, Clone)]
pub enum CasaEvento {
    Stato(StatoConnessione),
    Dati(Arc<MenuData>),
    Notifica { titolo: String, messaggio: String },
    Esito { entity_id: String, ok: bool, errore: Option<String> },
}

/// Callback dell'applicazione: viene chiamata dal thread del servizio.
pub type Ascoltatore = Arc<dyn Fn(CasaEvento) + Send + Sync>;
