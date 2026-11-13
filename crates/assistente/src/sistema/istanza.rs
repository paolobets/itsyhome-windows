//! Istanza unica. Su Windows un evento con nome fa da segno e da campanello insieme: il primo
//! avvio lo crea e un thread lo aspetta (bloccato, nessun costo); un secondo avvio lo trova già
//! creato, lo suona ed esce subito. Il primo, sentito il campanello, apre il pannello (come la
//! scorciatoia: rimette anche la pillola). Costa un handle e un thread fermo, quindi vale la pena
//! rispetto al solo «esce»: chi rilancia l'app dal menu Start la vede rispondere.
//!
//! Con `ITSYHOME_ASSISTENTE_DIR` (prove locali) il nome cambia con la cartella: una prova non
//! suona mai l'istanza vera, e due prove su cartelle diverse non si escludono.

use std::ffi::OsStr;
use std::hash::{Hash, Hasher};

/// Nome dell'evento (spazio dei nomi della sessione: `Local\`).
pub fn nome(cartella_prova: Option<&OsStr>) -> String {
    match cartella_prova.filter(|c| !c.is_empty()) {
        None => r"Local\itsyhome-assistente".into(),
        Some(c) => {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            c.hash(&mut h);
            format!(r"Local\itsyhome-assistente-prova-{:016x}", h.finish())
        }
    }
}

/// L'istanza presa: tiene l'evento finché il processo vive.
pub struct Istanza {
    #[cfg(windows)]
    evento: isize,
}

/// Prende l'istanza. `None`: un'altra è già attiva (ed è stata avvisata): il processo esca.
#[cfg(windows)]
pub fn prendi(nome: &str) -> Option<Istanza> {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::{CreateEventW, SetEvent};

    let largo: Vec<u16> = nome.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: nome terminato da zero; evento a reset automatico, inizialmente spento.
    let evento = unsafe { CreateEventW(std::ptr::null(), 0, 0, largo.as_ptr()) };
    if evento == 0 {
        // Non si sa: meglio un'istanza in più che nessuna.
        eprintln!("istanza unica non verificata: {}", std::io::Error::last_os_error());
        return Some(Istanza { evento: 0 });
    }
    // SAFETY: letto subito dopo CreateEventW, nessun'altra chiamata in mezzo.
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        use windows_sys::Win32::UI::WindowsAndMessaging::{AllowSetForegroundWindow, ASFW_ANY};
        // SAFETY: handle appena aperto da noi. Chi è appena stato lanciato può cedere il primo
        // piano: così il pannello della prima istanza si apre col fuoco.
        unsafe {
            AllowSetForegroundWindow(ASFW_ANY);
            SetEvent(evento);
            CloseHandle(evento);
        }
        return None;
    }
    Some(Istanza { evento })
}

#[cfg(not(windows))]
pub fn prendi(_nome: &str) -> Option<Istanza> {
    Some(Istanza {})
}

impl Istanza {
    /// A ogni nuovo avvio di un'altra istanza chiama `f` (da un thread a parte: `f` inoltri al
    /// ciclo eventi). Il thread resta fermo sull'evento finché il processo vive.
    #[cfg(windows)]
    pub fn su_richiamo(self, f: impl Fn() + Send + 'static) {
        use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
        use windows_sys::Win32::System::Threading::{WaitForSingleObject, INFINITE};

        let evento = self.evento;
        if evento == 0 {
            return;
        }
        let avviato = std::thread::Builder::new().name("istanza".into()).spawn(move || loop {
            // SAFETY: l'handle non viene mai chiuso (vive quanto il processo).
            if unsafe { WaitForSingleObject(evento, INFINITE) } != WAIT_OBJECT_0 {
                return;
            }
            f();
        });
        if let Err(e) = avviato {
            eprintln!("richiamo dell'istanza non attivo: {e}");
        }
    }

    #[cfg(not(windows))]
    pub fn su_richiamo(self, _f: impl Fn() + Send + 'static) {}
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn nome_vero_e_nomi_di_prova_diversi() {
        assert_eq!(nome(None), r"Local\itsyhome-assistente");
        assert_eq!(nome(Some(OsStr::new(""))), nome(None));
        let a = nome(Some(OsStr::new(r"C:\prova\a")));
        let b = nome(Some(OsStr::new(r"C:\prova\b")));
        assert!(a.starts_with(r"Local\itsyhome-assistente-prova-"));
        assert_ne!(a, b);
        assert_eq!(a, nome(Some(OsStr::new(r"C:\prova\a"))));
    }

    #[cfg(windows)]
    #[test]
    fn la_seconda_presa_dello_stesso_nome_esce_e_suona() {
        use std::sync::mpsc;
        let n = nome(Some(OsStr::new(&format!("test-istanza-{}", std::process::id()))));
        let prima = prendi(&n).expect("prima istanza");
        let (tx, rx) = mpsc::channel();
        prima.su_richiamo(move || {
            let _ = tx.send(());
        });
        assert!(prendi(&n).is_none(), "seconda istanza");
        rx.recv_timeout(std::time::Duration::from_secs(5)).expect("la prima sente il campanello");
    }
}
