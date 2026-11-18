//! Chi c'è dall'altra parte del tubo (spec M3 §10): il percorso dell'eseguibile del processo server
//! (lo controlla l'aiutante prima di mandare un solo byte) o cliente (lo controlla isola prima di
//! leggere).
//!
//! Tre controlli, nell'ordine:
//! 1. il PID dal tubo (`GetNamedPipeServerProcessId` / `GetNamedPipeClientProcessId`) e il processo
//!    aperto con `PROCESS_QUERY_LIMITED_INFORMATION`;
//! 2. il processo gira **come l'utente corrente** (SID del suo token uguale al nostro): un processo
//!    di un altro utente non passa, anche se esegue lo stesso file;
//! 3. il percorso dell'immagine (`QueryFullProcessImageNameW`, formato Win32) e, **dopo** averlo
//!    letto, il tubo è ancora collegato: allora il processo aperto al punto 1 era ancora vivo, e il
//!    PID non può essere stato riusato da un altro programma nel frattempo.
//!
//! Qualunque fallimento vale «sconosciuto» (`None`): chi chiama non si fida.
//! Fuori da Windows non c'è tubo: gli stub restituiscono `None`.

use std::path::{Path, PathBuf};

#[cfg(windows)]
pub use windows_sys::Win32::Foundation::HANDLE;
/// Fuori da Windows una maniglia non esiste: solo per compilare gli stub.
#[cfg(not(windows))]
pub type HANDLE = isize;

/// `true` se i due percorsi indicano lo stesso file: percorsi canonici (collegamenti e `..`
/// risolti), confrontati senza distinzione di maiuscole come fa il file system di Windows. Un
/// percorso che non esiste non è mai «lo stesso».
pub fn stesso_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase(),
        _ => false,
    }
}

/// L'eseguibile del processo che ha creato il tubo `h` (dal lato del cliente), se gira come
/// l'utente corrente e il tubo è ancora collegato.
#[cfg(windows)]
pub fn immagine_server(h: HANDLE) -> Option<PathBuf> {
    let mut pid = 0u32;
    // SAFETY: `h` è una maniglia di tubo valida per tutta la chiamata; `pid` è un u32 nostro.
    if unsafe { windows_sys::Win32::System::Pipes::GetNamedPipeServerProcessId(h, &mut pid) } == 0 {
        return None;
    }
    sys::immagine_verificata(h, pid)
}

/// L'eseguibile del processo che ha aperto il tubo `h` (dal lato del server), se gira come
/// l'utente corrente e il tubo è ancora collegato.
#[cfg(windows)]
pub fn immagine_cliente(h: HANDLE) -> Option<PathBuf> {
    sys::immagine_verificata(h, pid_cliente(h)?)
}

/// Il PID del cliente del tubo `h`.
#[cfg(windows)]
pub fn pid_cliente(h: HANDLE) -> Option<u32> {
    let mut pid = 0u32;
    // SAFETY: come sopra.
    let ok = unsafe { windows_sys::Win32::System::Pipes::GetNamedPipeClientProcessId(h, &mut pid) };
    (ok != 0 && pid != 0).then_some(pid)
}

#[cfg(not(windows))]
pub fn immagine_server(_h: HANDLE) -> Option<PathBuf> {
    None
}

#[cfg(not(windows))]
pub fn immagine_cliente(_h: HANDLE) -> Option<PathBuf> {
    None
}

#[cfg(not(windows))]
pub fn pid_cliente(_h: HANDLE) -> Option<u32> {
    None
}

/// Il SID dell'utente corrente come testo (`S-1-5-21-…`), per il nome del tubo e il descrittore di
/// sicurezza.
#[cfg(windows)]
pub fn sid_utente() -> Option<String> {
    sys::sid_testo_corrente()
}

#[cfg(not(windows))]
pub fn sid_utente() -> Option<String> {
    None
}

#[cfg(windows)]
mod sys {
    use std::ffi::c_void;
    use std::path::PathBuf;
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE};
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows_sys::Win32::Security::{
        EqualSid, GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER,
    };
    use windows_sys::Win32::System::Pipes::PeekNamedPipe;
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    /// Una maniglia chiusa all'uscita.
    struct Chiudi(HANDLE);

    impl Drop for Chiudi {
        fn drop(&mut self) {
            // SAFETY: maniglia aperta da noi e chiusa una volta sola.
            unsafe { CloseHandle(self.0) };
        }
    }

    /// Il blocco `TOKEN_USER` del token del processo (il SID punta dentro al blocco).
    fn utente(processo: HANDLE) -> Option<Vec<u64>> {
        let mut token: HANDLE = 0;
        // SAFETY: `processo` è valido; `token` riceve una maniglia che chiudiamo noi.
        if unsafe { OpenProcessToken(processo, TOKEN_QUERY, &mut token) } == 0 {
            return None;
        }
        let token = Chiudi(token);
        let mut serve = 0u32;
        // SAFETY: prima chiamata con buffer nullo e lunghezza 0: solo la dimensione richiesta.
        unsafe { GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut serve) };
        if serve == 0 || serve > 4096 {
            return None;
        }
        // Vec<u64>: allineamento adatto alla struttura.
        let mut blocco = vec![0u64; (serve as usize).div_ceil(8)];
        // SAFETY: `blocco` è grande almeno `serve` byte e vive per tutta la chiamata.
        let ok = unsafe {
            GetTokenInformation(
                token.0,
                TokenUser,
                blocco.as_mut_ptr().cast::<c_void>(),
                serve,
                &mut serve,
            )
        };
        (ok != 0).then_some(blocco)
    }

    fn sid_di(blocco: &[u64]) -> *mut c_void {
        // SAFETY: il blocco contiene una `TOKEN_USER` scritta dal sistema.
        unsafe { (*blocco.as_ptr().cast::<TOKEN_USER>()).User.Sid }
    }

    /// `true` se il processo gira come l'utente corrente.
    fn stesso_utente(processo: HANDLE) -> bool {
        // SAFETY: pseudo-maniglia del processo corrente, non va chiusa.
        let mio = utente(unsafe { GetCurrentProcess() });
        let suo = utente(processo);
        match (mio, suo) {
            // SAFETY: i due SID puntano dentro ai blocchi, vivi per tutta la chiamata.
            (Some(a), Some(b)) => unsafe { EqualSid(sid_di(&a), sid_di(&b)) != 0 },
            _ => false,
        }
    }

    pub fn sid_testo_corrente() -> Option<String> {
        // SAFETY: pseudo-maniglia del processo corrente.
        let blocco = utente(unsafe { GetCurrentProcess() })?;
        let mut testo: *mut u16 = null_mut();
        // SAFETY: il SID è valido; `testo` riceve un buffer da liberare con `LocalFree`.
        if unsafe { ConvertSidToStringSidW(sid_di(&blocco), &mut testo) } == 0 || testo.is_null() {
            return None;
        }
        // SAFETY: stringa terminata da zero scritta dal sistema; la si copia e poi si libera.
        let s = unsafe {
            let mut n = 0;
            while *testo.add(n) != 0 {
                n += 1;
            }
            let s = String::from_utf16(std::slice::from_raw_parts(testo, n)).ok();
            LocalFree(testo.cast());
            s
        };
        s.filter(|s| {
            s.starts_with("S-") && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
    }

    /// `true` se il tubo è ancora collegato all'altro capo.
    fn ancora_collegato(h: HANDLE) -> bool {
        let mut disponibili = 0u32;
        // SAFETY: nessun buffer: chiede solo quanti byte ci sono, senza bloccare.
        unsafe { PeekNamedPipe(h, null_mut(), 0, null_mut(), &mut disponibili, null_mut()) != 0 }
    }

    pub fn immagine_verificata(h: HANDLE, pid: u32) -> Option<PathBuf> {
        if pid == 0 {
            return None;
        }
        // SAFETY: apertura in sola interrogazione; la maniglia si chiude con `Chiudi`.
        let p = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if p == 0 {
            return None;
        }
        let p = Chiudi(p);
        if !stesso_utente(p.0) {
            return None;
        }
        let mut buf = vec![0u16; 32_768];
        let mut n = buf.len() as u32;
        // SAFETY: `buf` è lungo `n` caratteri; il sistema scrive al più tanto.
        if unsafe { QueryFullProcessImageNameW(p.0, 0, buf.as_mut_ptr(), &mut n) } == 0 {
            return None;
        }
        let percorso = PathBuf::from(String::from_utf16(&buf[..n as usize]).ok()?);
        // Il processo aperto sopra era ancora vivo dopo la lettura: il PID era il suo.
        if !ancora_collegato(h) {
            return None;
        }
        Some(percorso)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn stesso_file_canonico_senza_maiuscole() {
        let d = tempfile::TempDir::new().unwrap();
        let f = d.path().join("Assistente.exe");
        std::fs::write(&f, b"x").unwrap();
        let maiuscolo = d.path().join("ASSISTENTE.EXE");
        let con_giro = d.path().join("sotto").join("..").join("assistente.exe");
        std::fs::create_dir(d.path().join("sotto")).unwrap();
        assert!(stesso_file(&f, &f));
        if cfg!(windows) {
            assert!(stesso_file(&f, &maiuscolo));
        }
        assert!(stesso_file(&f, &con_giro));
        let altro = d.path().join("isola-hook.exe");
        std::fs::write(&altro, b"x").unwrap();
        assert!(!stesso_file(&f, &altro));
        assert!(
            !stesso_file(&f, &d.path().join("manca.exe")),
            "un file che non c'è non è mai lo stesso"
        );
        assert!(!stesso_file(
            &d.path().join("manca.exe"),
            &d.path().join("manca.exe")
        ));
    }

    #[cfg(windows)]
    #[test]
    fn sid_dell_utente() {
        let s = sid_utente().expect("SID dell'utente");
        assert!(s.starts_with("S-1-"), "{s}");
    }
}
