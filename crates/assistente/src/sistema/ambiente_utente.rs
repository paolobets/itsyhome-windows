//! La variabile d'ambiente dell'utente che portava il codice degli hook HTTP di Claude Code (M3,
//! Task 7). Dal Task 7A non si scrive più: «Scollega» la toglie se è rimasta (il Task 7B decide se
//! il modulo serve ancora).
//!
//! Su Windows è `HKCU\Environment` (solo il ramo dell'utente, mai `HKLM`); dopo ogni cambio si
//! avvisa il sistema (`WM_SETTINGCHANGE`) perché i programmi nuovi la vedano. I terminali già
//! aperti no: vanno chiusi e riaperti. In prova (`ITSYHOME_ASSISTENTE_DIR`) si usa una chiave a
//! parte, `HKCU\Software\ItsyHomeProva\Environment`, e nessun avviso: la variabile vera non si
//! tocca mai.

/// Il nome della variabile (stesso di `claude_hook::installa`: una sola fonte).
pub const VARIABILE: &str = claude_hook::installa::VARIABILE_CODICE;

/// Dove vive la variabile dell'utente. Un'interfaccia piccola, perché i test usano `AmbienteFinto`
/// e non toccano mai il registro.
pub trait AmbienteUtente {
    /// Toglie la variabile; una variabile assente non è un errore.
    fn togli(&self, nome: &str) -> Result<(), String>;
    fn leggi(&self, nome: &str) -> Option<String>;
    /// Falso dove l'app non può scrivere la variabile (fuori da Windows): la imposta l'utente nella
    /// sua shell e le Impostazioni lo spiegano.
    fn gestisce(&self) -> bool {
        true
    }
}

/// Fuori da Windows: nessuna variabile da scrivere, e l'interfaccia lo dice.
#[cfg(not(windows))]
pub struct AmbienteNessuno;

#[cfg(not(windows))]
impl AmbienteUtente for AmbienteNessuno {
    fn togli(&self, _nome: &str) -> Result<(), String> {
        Err("solo su Windows".into())
    }
    fn leggi(&self, _nome: &str) -> Option<String> {
        None
    }
    fn gestisce(&self) -> bool {
        false
    }
}

/// L'ambiente vero di questa piattaforma.
#[cfg(windows)]
pub fn ambiente(in_prova: bool) -> RegistroUtente {
    RegistroUtente { prova: in_prova }
}

/// L'ambiente vero di questa piattaforma.
#[cfg(not(windows))]
pub fn ambiente(_in_prova: bool) -> AmbienteNessuno {
    AmbienteNessuno
}

#[cfg(windows)]
pub struct RegistroUtente {
    prova: bool,
}

#[cfg(windows)]
impl RegistroUtente {
    fn chiave(&self) -> &'static str {
        if self.prova {
            r"Software\ItsyHomeProva\Environment"
        } else {
            "Environment"
        }
    }

    /// Avvisa i programmi aperti che l'ambiente è cambiato. Su un thread a parte: con una finestra
    /// bloccata l'attesa arriva a 5 secondi e l'interfaccia non deve aspettarla. In prova non si
    /// avvisa nessuno.
    fn avvisa(&self) {
        if self.prova {
            return;
        }
        std::thread::spawn(|| {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
            };
            let ambiente: Vec<u16> = "Environment\0".encode_utf16().collect();
            let mut esito = 0usize;
            // SAFETY: `ambiente` è una stringa UTF-16 terminata da zero che vive fino alla fine della
            // chiamata; l'esito è una variabile locale.
            unsafe {
                SendMessageTimeoutW(
                    HWND_BROADCAST,
                    WM_SETTINGCHANGE,
                    0,
                    ambiente.as_ptr() as isize,
                    SMTO_ABORTIFHUNG,
                    5000,
                    &mut esito,
                );
            }
        });
    }
}

#[cfg(windows)]
impl AmbienteUtente for RegistroUtente {
    fn togli(&self, nome: &str) -> Result<(), String> {
        if self.leggi(nome).is_none() {
            return Ok(());
        }
        // `create` apre in scrittura una chiave che c'è (`open` la apre in sola lettura: «Accesso negato»).
        windows_registry::CURRENT_USER
            .create(self.chiave())
            .and_then(|k| k.remove_value(nome))
            .map_err(|e| format!("registro: {e}"))?;
        self.avvisa();
        Ok(())
    }

    fn leggi(&self, nome: &str) -> Option<String> {
        windows_registry::CURRENT_USER.open(self.chiave()).ok()?.get_string(nome).ok()
    }
}

/// Il percorso corto 8.3 di un file che esiste (`C:\PROGRA~1\…`), o `None` (volume senza nomi corti,
/// file assente). Serve al comando della barra di stato quando il percorso ha spazi.
#[cfg(windows)]
pub fn percorso_corto(p: &std::path::Path) -> Option<std::path::PathBuf> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;
    let largo: Vec<u16> = p.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    // SAFETY: `largo` è terminato da zero; il buffer è lungo quanto dichiarato.
    unsafe {
        let n = GetShortPathNameW(largo.as_ptr(), std::ptr::null_mut(), 0);
        if n == 0 {
            return None;
        }
        let mut buf = vec![0u16; n as usize];
        let scritti = GetShortPathNameW(largo.as_ptr(), buf.as_mut_ptr(), n);
        if scritti == 0 || scritti >= n {
            return None;
        }
        Some(std::ffi::OsString::from_wide(&buf[..scritti as usize]).into())
    }
}

/// Fuori da Windows non ci sono nomi corti.
#[cfg(not(windows))]
pub fn percorso_corto(_p: &std::path::Path) -> Option<std::path::PathBuf> {
    None
}

/// Un ambiente in memoria per i test: nessun registro, nessuna variabile vera.
#[cfg(test)]
#[derive(Default)]
pub struct AmbienteFinto {
    valori: std::cell::RefCell<std::collections::HashMap<String, String>>,
    /// Quando è vero ogni scrittura fallisce (per provare il percorso d'errore).
    pub rifiuta: std::cell::Cell<bool>,
    /// Quante scritture (togli) sono state chieste.
    pub scritture: std::cell::Cell<usize>,
}

#[cfg(test)]
impl AmbienteFinto {
    /// Una variabile già presente (come se l'avesse lasciata il Task 7).
    pub fn metti(&self, nome: &str, valore: &str) {
        self.valori.borrow_mut().insert(nome.into(), valore.into());
    }
}

#[cfg(test)]
impl AmbienteUtente for AmbienteFinto {
    fn togli(&self, nome: &str) -> Result<(), String> {
        self.scritture.set(self.scritture.get() + 1);
        if self.rifiuta.get() {
            return Err("rifiutato".into());
        }
        self.valori.borrow_mut().remove(nome);
        Ok(())
    }
    fn leggi(&self, nome: &str) -> Option<String> {
        self.valori.borrow().get(nome).cloned()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn la_variabile_e_quella_dell_hook() {
        assert_eq!(VARIABILE, "ISOLA_HOOK_TOKEN");
    }

    #[test]
    fn ambiente_finto_imposta_legge_e_toglie() {
        let a = AmbienteFinto::default();
        assert_eq!(a.leggi(VARIABILE), None);
        a.metti(VARIABILE, "uno");
        assert_eq!(a.leggi(VARIABILE).as_deref(), Some("uno"));
        a.togli(VARIABILE).unwrap();
        a.togli(VARIABILE).unwrap(); // già assente: non è un errore
        assert_eq!(a.leggi(VARIABILE), None);
    }

    #[cfg(not(windows))]
    #[test]
    fn fuori_da_windows_nessuna_variabile() {
        let a = AmbienteNessuno;
        assert!(!a.gestisce());
        assert!(a.togli(VARIABILE).is_err());
        assert_eq!(a.leggi(VARIABILE), None);
    }

    #[cfg(windows)]
    #[test]
    fn percorso_corto_di_un_file_che_non_esiste_e_none() {
        assert_eq!(percorso_corto(std::path::Path::new(r"C:\questo\non\esiste\proprio.exe")), None);
    }
}
