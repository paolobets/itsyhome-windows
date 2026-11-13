//! Archiviazione dei token di Home Assistant: portachiavi del sistema operativo.
use std::collections::HashMap;
use std::sync::Mutex;

const SERVIZIO: &str = "itsyhome";

pub trait Segreti: Send + Sync {
    fn salva(&self, env_id: &str, token: &str) -> Result<(), String>;
    fn leggi(&self, env_id: &str) -> Result<Option<String>, String>;
    fn cancella(&self, env_id: &str) -> Result<(), String>;
}

/// Portachiavi di sistema (Credential Manager / Keychain / Secret Service).
#[derive(Debug, Default, Clone, Copy)]
pub struct KeyringSegreti;

impl KeyringSegreti {
    fn voce(env_id: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(SERVIZIO, env_id).map_err(|e| format!("portachiavi: {e}"))
    }
}

impl Segreti for KeyringSegreti {
    fn salva(&self, env_id: &str, token: &str) -> Result<(), String> {
        Self::voce(env_id)?
            .set_password(token)
            .map_err(|e| format!("portachiavi: {e}"))
    }

    fn leggi(&self, env_id: &str) -> Result<Option<String>, String> {
        match Self::voce(env_id)?.get_password() {
            Ok(t) => Ok(Some(t)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("portachiavi: {e}")),
        }
    }

    fn cancella(&self, env_id: &str) -> Result<(), String> {
        match Self::voce(env_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("portachiavi: {e}")),
        }
    }
}

/// Implementazione in memoria, per i test.
#[derive(Debug, Default)]
pub struct MemoriaSegreti {
    mappa: Mutex<HashMap<String, String>>,
}

impl MemoriaSegreti {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Segreti for MemoriaSegreti {
    fn salva(&self, env_id: &str, token: &str) -> Result<(), String> {
        self.mappa
            .lock()
            .map_err(|e| e.to_string())?
            .insert(env_id.to_owned(), token.to_owned());
        Ok(())
    }

    fn leggi(&self, env_id: &str) -> Result<Option<String>, String> {
        Ok(self.mappa.lock().map_err(|e| e.to_string())?.get(env_id).cloned())
    }

    fn cancella(&self, env_id: &str) -> Result<(), String> {
        self.mappa.lock().map_err(|e| e.to_string())?.remove(env_id);
        Ok(())
    }
}