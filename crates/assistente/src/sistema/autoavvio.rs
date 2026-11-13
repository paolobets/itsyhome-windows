//! Avvio con il sistema.
//!
//! Windows: il valore `ItsyHome Assistente` sotto `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
//! (nome diverso da `ItsyHome` della v2, che non si tocca mai), scritto da qui e solo in HKCU (mai
//! HKLM, nemmeno da processo elevato). Gestione attività può disattivarlo senza toglierlo: lo segna
//! in `…\Explorer\StartupApproved\Run` (primo byte dispari). Quindi:
//! - lo stato vero si legge, mai si suppone: acceso = valore presente e non disattivato da Windows;
//! - all'avvio dell'app non si riaccende mai nulla da solo; se la voce è nostra ma punta a un altro
//!   eseguibile (app spostata, aggiornata) si aggiorna solo il percorso;
//! - riaccendendolo dall'interruttore si toglie anche la disattivazione di Gestione attività.
//!
//! Con `ITSYHOME_ASSISTENTE_DIR` (prove) si scrive in `HKCU\Software\ItsyHomeProva\…`: una prova
//! non tocca mai la chiave `Run` vera.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Nome del valore nel registro.
pub const NOME_VOCE: &str = "ItsyHome Assistente";

/// Come sta l'avvio con il sistema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stato {
    Spento,
    /// Voce presente e attiva; `percorso_giusto` falso se punta a un altro eseguibile.
    Acceso { percorso_giusto: bool },
    /// Voce presente ma disattivata da Gestione attività.
    DisattivatoDaWindows,
}

impl Stato {
    /// Quel che mostra l'interruttore.
    pub fn attivo(&self) -> bool {
        matches!(self, Stato::Acceso { .. })
    }
}

/// Il comando da scrivere: il percorso dell'eseguibile tra virgolette (può contenere spazi).
pub fn comando(exe: &Path) -> String {
    format!("\"{}\"", exe.display())
}

/// Gestione attività scrive 12 byte: il primo pari = attivo, dispari = disattivato.
pub fn disattivato_da_windows(approvato: &[u8]) -> bool {
    approvato.first().is_some_and(|b| b & 1 == 1)
}

/// Decisione pura dallo stato del registro.
pub fn stato(valore_run: Option<&str>, approvato: Option<&[u8]>, atteso: &str) -> Stato {
    let Some(valore) = valore_run else { return Stato::Spento };
    if approvato.is_some_and(disattivato_da_windows) {
        return Stato::DisattivatoDaWindows;
    }
    Stato::Acceso { percorso_giusto: valore.trim().eq_ignore_ascii_case(atteso) }
}

static PROVA: AtomicBool = AtomicBool::new(false);

/// Prove locali (`ITSYHOME_ASSISTENTE_DIR`): chiavi del registro di prova.
pub fn usa_chiavi_di_prova() {
    PROVA.store(true, Ordering::Relaxed);
}

#[cfg(windows)]
mod registro {
    use super::*;
    use windows_registry::{Type, CURRENT_USER};

    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const APPROVATO: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
    const RUN_PROVA: &str = r"Software\ItsyHomeProva\Run";
    const APPROVATO_PROVA: &str = r"Software\ItsyHomeProva\StartupApproved";
    /// «Attivo» come lo scrive Gestione attività (primo byte 2, nessuna data).
    const ATTIVO: [u8; 12] = [2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

    fn chiavi() -> (&'static str, &'static str) {
        if PROVA.load(Ordering::Relaxed) {
            (RUN_PROVA, APPROVATO_PROVA)
        } else {
            (RUN, APPROVATO)
        }
    }

    fn atteso() -> Result<String, String> {
        let exe = std::env::current_exe().map_err(|e| format!("percorso dell'app non trovato: {e}"))?;
        Ok(comando(&exe))
    }

    fn leggi_run() -> Option<String> {
        CURRENT_USER.open(chiavi().0).ok()?.get_string(NOME_VOCE).ok()
    }

    fn leggi_approvato() -> Option<Vec<u8>> {
        CURRENT_USER.open(chiavi().1).ok()?.get_value(NOME_VOCE).ok().map(|v| v.to_vec())
    }

    pub fn leggi() -> Result<Stato, String> {
        Ok(stato(leggi_run().as_deref(), leggi_approvato().as_deref(), &atteso()?))
    }

    pub fn imposta(attivo: bool) -> Result<(), String> {
        let (run, approvato) = chiavi();
        if attivo {
            let comando = atteso()?;
            CURRENT_USER
                .create(run)
                .and_then(|k| k.set_string(NOME_VOCE, &comando))
                .map_err(|e| format!("avvio con il sistema non attivato: {e}"))?;
            // Era stato spento da Gestione attività: l'utente ora lo vuole acceso.
            if leggi_approvato().as_deref().is_some_and(disattivato_da_windows) {
                CURRENT_USER
                    .create(approvato)
                    .and_then(|k| k.set_bytes(NOME_VOCE, Type::Bytes, &ATTIVO))
                    .map_err(|e| format!("avvio con il sistema non riattivato: {e}"))?;
            }
            Ok(())
        } else {
            match CURRENT_USER.create(run).and_then(|k| k.remove_value(NOME_VOCE)) {
                Ok(()) => Ok(()),
                // Già assente: va bene così.
                Err(_) if leggi_run().is_none() => Ok(()),
                Err(e) => Err(format!("avvio con il sistema non disattivato: {e}")),
            }
        }
    }

    pub fn aggiorna_percorso() -> Result<(), String> {
        let comando = atteso()?;
        CURRENT_USER
            .create(chiavi().0)
            .and_then(|k| k.set_string(NOME_VOCE, &comando))
            .map_err(|e| format!("percorso dell'avvio con il sistema non aggiornato: {e}"))
    }
}

#[cfg(not(windows))]
mod registro {
    use super::*;

    fn lanciatore() -> Result<auto_launch::AutoLaunch, String> {
        let exe = std::env::current_exe().map_err(|e| format!("percorso dell'app non trovato: {e}"))?;
        auto_launch::AutoLaunchBuilder::new()
            .set_app_name(NOME_VOCE)
            .set_app_path(&exe.to_string_lossy())
            .build()
            .map_err(|e| e.to_string())
    }

    pub fn leggi() -> Result<Stato, String> {
        let acceso = lanciatore()?.is_enabled().map_err(|e| e.to_string())?;
        Ok(if acceso { Stato::Acceso { percorso_giusto: true } } else { Stato::Spento })
    }

    pub fn imposta(attivo: bool) -> Result<(), String> {
        if PROVA.load(Ordering::Relaxed) {
            return Err("avvio con il sistema non disponibile nelle prove".into());
        }
        let l = lanciatore()?;
        if attivo { l.enable() } else { l.disable() }.map_err(|e| e.to_string())
    }

    pub fn aggiorna_percorso() -> Result<(), String> {
        Ok(())
    }
}

/// Attiva o disattiva l'avvio con il sistema (interruttore delle Impostazioni).
pub fn imposta(attivo: bool) -> Result<(), String> {
    registro::imposta(attivo)
}

/// Lo stato vero, per l'interruttore.
pub fn leggi() -> Result<Stato, String> {
    registro::leggi()
}

/// All'avvio dell'app: legge lo stato e, se la voce è nostra ma punta a un altro eseguibile,
/// aggiorna solo il percorso. Non accende né riaccende mai nulla. Restituisce se è attivo.
pub fn all_avvio() -> Result<bool, String> {
    let s = registro::leggi()?;
    if s == (Stato::Acceso { percorso_giusto: false }) {
        registro::aggiorna_percorso()?;
    }
    Ok(s.attivo())
}

#[cfg(test)]
mod test {
    use super::*;

    const ESE: &str = r#""C:\Programmi\ItsyHome\assistente.exe""#;

    #[test]
    fn senza_voce_spento() {
        assert_eq!(stato(None, None, ESE), Stato::Spento);
        // La disattivazione rimasta in Gestione attività non accende nulla.
        assert_eq!(stato(None, Some(&[3, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8]), ESE), Stato::Spento);
    }

    #[test]
    fn voce_attiva_col_percorso_giusto_o_no() {
        assert_eq!(stato(Some(ESE), None, ESE), Stato::Acceso { percorso_giusto: true });
        assert_eq!(stato(Some(ESE), Some(&[2u8; 12][..]), ESE), Stato::Acceso { percorso_giusto: true });
        // Spazio in fondo (come lo scriveva auto-launch) e maiuscole di Windows: stesso percorso.
        let simile = r#""c:\programmi\itsyhome\ASSISTENTE.exe" "#;
        assert_eq!(stato(Some(simile), None, ESE), Stato::Acceso { percorso_giusto: true });
        let altro = r#""D:\Vecchio\assistente.exe""#;
        assert_eq!(stato(Some(altro), None, ESE), Stato::Acceso { percorso_giusto: false });
        assert!(stato(Some(altro), None, ESE).attivo());
    }

    #[test]
    fn disattivato_da_gestione_attivita_resta_spento() {
        let approvato = [3u8, 0, 0, 0, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x01];
        let s = stato(Some(ESE), Some(&approvato), ESE);
        assert_eq!(s, Stato::DisattivatoDaWindows);
        assert!(!s.attivo());
        assert!(!disattivato_da_windows(&[2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]));
        assert!(!disattivato_da_windows(&[6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]));
        assert!(disattivato_da_windows(&[7]));
    }

    #[test]
    fn nome_diverso_dalla_v2_e_percorso_tra_virgolette() {
        assert_ne!(NOME_VOCE, "ItsyHome");
        assert_eq!(comando(Path::new(r"C:\Program Files\ItsyHome\a.exe")), r#""C:\Program Files\ItsyHome\a.exe""#);
    }
}
