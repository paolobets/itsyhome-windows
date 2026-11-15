//! Eventi che `CasaService` consegna all'applicazione.
use std::sync::Arc;

use crate::types::MenuData;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatoConnessione {
    /// Nessuna connessione perché non la si vuole: «Disconnetti» (o prima del primo tentativo).
    /// Una connessione caduta non è `Disconnesso`: è un `Errore` seguito da nuovi tentativi.
    Disconnesso,
    InConnessione,
    Connesso,
    /// Nessun ambiente da cui connettersi (nessuno attivo, o l'attivo non esiste più).
    NonConfigurato,
    /// Home Assistant ha rifiutato il token (`auth_invalid`): definitivo, nessun nuovo tentativo.
    TokenNonValido,
    /// L'ambiente attivo non ha un token nel portachiavi: nessun tentativo.
    TokenMancante,
    /// Connessione non riuscita o caduta (si riprova da soli); il testo è il dettaglio tecnico.
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
    /// Esito di `CasaService::comando`: `numero` è quello dato dal chiamante con il comando, così
    /// l'esito torna al comando giusto anche con due comandi quasi simultanei sulla stessa entità
    /// (ogni comando gira in un suo task e gli esiti possono arrivare in un altro ordine).
    Esito { entity_id: String, numero: u64, ok: bool, errore: Option<String> },
    /// La configurazione non è stata scritta su disco (resta valida in memoria).
    ConfigNonSalvata(String),
}

/// Callback dell'applicazione: viene chiamata dal thread del servizio.
pub type Ascoltatore = Arc<dyn Fn(CasaEvento) + Send + Sync>;
