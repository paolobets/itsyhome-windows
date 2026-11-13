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
    /// Notifica persistente di Home Assistant comparsa o cambiata (stesso `id` = aggiornata).
    /// All'iscrizione arrivano anche quelle già presenti.
    NotificaAggiunta { id: String, titolo: String, messaggio: String },
    /// Notifica persistente archiviata (in Home Assistant o dall'app) o non più raggiungibile
    /// perché la connessione all'ambiente è stata chiusa.
    NotificaRimossa { id: String },
    /// `archivia_notifica` non è riuscita (la notifica resta finché HA non la rimuove).
    NotificaNonArchiviata { id: String, errore: String },
    Esito { entity_id: String, ok: bool, errore: Option<String> },
    /// La configurazione non è stata scritta su disco (resta valida in memoria).
    ConfigNonSalvata(String),
}

/// Callback dell'applicazione: viene chiamata dal thread del servizio.
pub type Ascoltatore = Arc<dyn Fn(CasaEvento) + Send + Sync>;
