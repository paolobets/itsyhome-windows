//! Chi c'è dall'altra parte del tubo (spec M3 §10). L'aiutante lo controlla prima di mandare un
//! solo byte, isola prima di leggere.
//!
//! Controlli, nell'ordine:
//! 1. (solo l'aiutante) il **proprietario del tubo** (`GetSecurityInfo(SE_KERNEL_OBJECT,
//!    OWNER_SECURITY_INFORMATION)` sulla maniglia aperta) è il SID dell'utente corrente. Lo fissa
//!    il kernel alla creazione: isola lo scrive esplicito (`O:<SID>`), e un altro utente non può
//!    impostare il nostro SID come proprietario senza privilegi di ripristino. Non dipende da PID.
//! 2. il PID dal tubo (`GetNamedPipeServerProcessId` / `GetNamedPipeClientProcessId`) e il processo
//!    aperto con `PROCESS_QUERY_LIMITED_INFORMATION`;
//! 3. il processo gira **come l'utente corrente** (SID del token uguale al nostro) e con
//!    **integrità almeno media** (`TokenIntegrityLevel`): un processo di un altro utente, o un
//!    nostro processo a bassa integrità (sandbox), non passa anche se esegue lo stesso file;
//! 4. il percorso dell'immagine (`QueryFullProcessImageNameW`, formato Win32) e, dopo averlo letto,
//!    il tubo è ancora collegato. Questo **riduce** il rischio di un PID riusato ma non lo esclude:
//!    se chi ha creato il tubo ha passato la maniglia a un altro processo ed è uscito, il tubo resta
//!    collegato con un PID libero. Per questo il PID non è mai l'unica prova (vedi il punto 1).
//!
//! Qualunque fallimento vale «sconosciuto» (`None`/`false`): chi chiama non si fida.
//! Fuori da Windows non c'è tubo: gli stub restituiscono `None`/`false`.

use std::path::{Path, PathBuf};

#[cfg(windows)]
pub use windows_sys::Win32::Foundation::HANDLE;
/// Fuori da Windows una maniglia non esiste: solo per compilare gli stub.
#[cfg(not(windows))]
pub type HANDLE = isize;

/// `SECURITY_MANDATORY_MEDIUM_RID`: il livello d'integrità di un normale programma dell'utente.
pub const INTEGRITA_MEDIA: u32 = 0x2000;
/// Quanti processi al più nella catena del mittente (aiutante, `claude.exe`, terminale…).
pub const CATENA_MASSIMA: usize = 6;

/// `true` se il livello d'integrità (l'ultimo RID dell'etichetta) è almeno medio.
pub fn integrita_sufficiente(rid: u32) -> bool {
    rid >= INTEGRITA_MEDIA
}

/// `true` se i due percorsi indicano lo stesso file: percorsi canonici (collegamenti e `..`
/// risolti), confrontati senza distinzione di maiuscole come fa il file system di Windows. Un
/// percorso che non esiste non è mai «lo stesso». (Le maiuscole si piegano con le regole Unicode e
/// non con la tabella di NTFS: al più due nomi uguali risultano diversi, e allora si rifiuta.)
pub fn stesso_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase(),
        _ => false,
    }
}

/// `true` se il proprietario dell'oggetto tubo `h` è l'utente corrente.
#[cfg(windows)]
pub fn tubo_dell_utente(h: HANDLE) -> bool {
    sys::proprietario_e_utente(h)
}

/// L'eseguibile del processo che ha creato il tubo `h` (dal lato del cliente), se gira come
/// l'utente corrente con integrità almeno media e il tubo è ancora collegato.
#[cfg(windows)]
pub fn immagine_server(h: HANDLE) -> Option<PathBuf> {
    let mut pid = 0u32;
    // SAFETY: `h` è una maniglia di tubo valida per tutta la chiamata; `pid` è un u32 nostro.
    if unsafe { windows_sys::Win32::System::Pipes::GetNamedPipeServerProcessId(h, &mut pid) } == 0 {
        return None;
    }
    sys::immagine_verificata(h, pid)
}

/// L'eseguibile del processo che ha aperto il tubo `h` (dal lato del server), con le stesse
/// condizioni di `immagine_server`.
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

/// Il processo `pid` e i suoi genitori (PID e nome dell'eseguibile, al più `CATENA_MASSIMA`), da
/// un'istantanea dei processi. Va presa mentre il processo è vivo; i PID dei genitori possono
/// essere già stati riusati, chi la usa lo sa.
#[cfg(windows)]
pub fn catena_processi(pid: u32) -> Vec<(u32, String)> {
    sys::catena(pid)
}

#[cfg(not(windows))]
pub fn tubo_dell_utente(_h: HANDLE) -> bool {
    false
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

#[cfg(not(windows))]
pub fn catena_processi(_pid: u32) -> Vec<(u32, String)> {
    Vec::new()
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

/// Il descrittore del tubo `h` (proprietario e DACL) in SDDL, per i test.
#[cfg(all(test, windows))]
pub(crate) fn sddl_del_tubo(h: HANDLE) -> Option<String> {
    sys::sddl_di(h)
}

/// Il livello d'integrità del processo corrente, per i test.
#[cfg(all(test, windows))]
pub(crate) fn integrita_corrente() -> Option<u32> {
    // SAFETY: pseudo-maniglia del processo corrente.
    sys::integrita(unsafe { windows_sys::Win32::System::Threading::GetCurrentProcess() })
}

#[cfg(windows)]
mod sys {
    use std::ffi::c_void;
    use std::mem::{size_of, zeroed};
    use std::path::PathBuf;
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, GetSecurityInfo, SE_KERNEL_OBJECT,
    };
    use windows_sys::Win32::Security::{
        EqualSid, GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation,
        TokenIntegrityLevel, TokenUser, OWNER_SECURITY_INFORMATION, TOKEN_INFORMATION_CLASS,
        TOKEN_MANDATORY_LABEL, TOKEN_QUERY, TOKEN_USER,
    };
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Pipes::PeekNamedPipe;
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    use super::{integrita_sufficiente, CATENA_MASSIMA};

    /// Una maniglia chiusa all'uscita.
    struct Chiudi(HANDLE);

    impl Drop for Chiudi {
        fn drop(&mut self) {
            // SAFETY: maniglia aperta da noi e chiusa una volta sola.
            unsafe { CloseHandle(self.0) };
        }
    }

    /// Un blocco d'informazioni del token del processo (i SID puntano dentro al blocco).
    fn informazione(processo: HANDLE, classe: TOKEN_INFORMATION_CLASS) -> Option<Vec<u64>> {
        let mut token: HANDLE = 0;
        // SAFETY: `processo` è valido; `token` riceve una maniglia che chiudiamo noi.
        if unsafe { OpenProcessToken(processo, TOKEN_QUERY, &mut token) } == 0 {
            return None;
        }
        let token = Chiudi(token);
        let mut serve = 0u32;
        // SAFETY: prima chiamata con buffer nullo e lunghezza 0: solo la dimensione richiesta.
        unsafe { GetTokenInformation(token.0, classe, null_mut(), 0, &mut serve) };
        if serve == 0 || serve > 4096 {
            return None;
        }
        // Vec<u64>: allineamento adatto alla struttura.
        let mut blocco = vec![0u64; (serve as usize).div_ceil(8)];
        // SAFETY: `blocco` è grande almeno `serve` byte e vive per tutta la chiamata.
        let ok = unsafe {
            GetTokenInformation(
                token.0,
                classe,
                blocco.as_mut_ptr().cast::<c_void>(),
                serve,
                &mut serve,
            )
        };
        (ok != 0).then_some(blocco)
    }

    fn sid_utente_di(blocco: &[u64]) -> *mut c_void {
        // SAFETY: il blocco contiene una `TOKEN_USER` scritta dal sistema.
        unsafe { (*blocco.as_ptr().cast::<TOKEN_USER>()).User.Sid }
    }

    /// Il blocco `TOKEN_USER` del processo corrente.
    fn utente_corrente() -> Option<Vec<u64>> {
        // SAFETY: pseudo-maniglia del processo corrente, non va chiusa.
        informazione(unsafe { GetCurrentProcess() }, TokenUser)
    }

    /// `true` se il processo gira come l'utente corrente.
    fn stesso_utente(processo: HANDLE) -> bool {
        match (utente_corrente(), informazione(processo, TokenUser)) {
            // SAFETY: i due SID puntano dentro ai blocchi, vivi per tutta la chiamata.
            (Some(a), Some(b)) => unsafe { EqualSid(sid_utente_di(&a), sid_utente_di(&b)) != 0 },
            _ => false,
        }
    }

    /// Il livello d'integrità del processo (ultimo RID dell'etichetta obbligatoria).
    pub fn integrita(processo: HANDLE) -> Option<u32> {
        let blocco = informazione(processo, TokenIntegrityLevel)?;
        // SAFETY: il blocco contiene una `TOKEN_MANDATORY_LABEL` scritta dal sistema; il SID
        // dell'etichetta punta dentro al blocco, vivo per tutto il blocco `unsafe`.
        unsafe {
            let sid = (*blocco.as_ptr().cast::<TOKEN_MANDATORY_LABEL>()).Label.Sid;
            if sid.is_null() {
                return None;
            }
            let n = *GetSidSubAuthorityCount(sid);
            if n == 0 {
                return None;
            }
            Some(*GetSidSubAuthority(sid, u32::from(n) - 1))
        }
    }

    /// `true` se il proprietario dell'oggetto `h` è l'utente corrente.
    pub fn proprietario_e_utente(h: HANDLE) -> bool {
        let Some(mio) = utente_corrente() else {
            return false;
        };
        let mut proprietario: *mut c_void = null_mut();
        let mut sd: *mut c_void = null_mut();
        // SAFETY: maniglia valida (aperta con `GENERIC_READ`, che comprende `READ_CONTROL`);
        // `proprietario` punta dentro a `sd`, che si libera con `LocalFree` dopo il confronto.
        unsafe {
            let e = GetSecurityInfo(
                h,
                SE_KERNEL_OBJECT,
                OWNER_SECURITY_INFORMATION,
                &mut proprietario,
                null_mut(),
                null_mut(),
                null_mut(),
                &mut sd,
            );
            let uguale = e == 0
                && !proprietario.is_null()
                && EqualSid(proprietario, sid_utente_di(&mio)) != 0;
            if !sd.is_null() {
                LocalFree(sd);
            }
            uguale
        }
    }

    pub fn sid_testo_corrente() -> Option<String> {
        let blocco = utente_corrente()?;
        let mut testo: *mut u16 = null_mut();
        // SAFETY: il SID è valido; `testo` riceve un buffer da liberare con `LocalFree`.
        if unsafe { ConvertSidToStringSidW(sid_utente_di(&blocco), &mut testo) } == 0
            || testo.is_null()
        {
            return None;
        }
        // SAFETY: stringa terminata da zero scritta dal sistema; la si copia e poi si libera.
        let s = unsafe { testo_e_libera(testo) };
        s.filter(|s| {
            s.starts_with("S-") && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
    }

    /// Copia una stringa larga terminata da zero data dal sistema e la libera con `LocalFree`.
    ///
    /// # Safety
    /// `testo` è una stringa terminata da zero allocata con `LocalAlloc`.
    unsafe fn testo_e_libera(testo: *mut u16) -> Option<String> {
        let mut n = 0;
        while *testo.add(n) != 0 {
            n += 1;
        }
        let s = String::from_utf16(std::slice::from_raw_parts(testo, n)).ok();
        LocalFree(testo.cast());
        s
    }

    /// Il descrittore (proprietario e DACL) dell'oggetto `h` in SDDL.
    #[cfg(test)]
    pub fn sddl_di(h: HANDLE) -> Option<String> {
        use windows_sys::Win32::Security::Authorization::{
            ConvertSecurityDescriptorToStringSecurityDescriptorW, SDDL_REVISION_1,
        };
        use windows_sys::Win32::Security::DACL_SECURITY_INFORMATION;
        let quali = OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
        let mut sd: *mut c_void = null_mut();
        // SAFETY: come in `proprietario_e_utente`; il testo si copia e si libera.
        unsafe {
            let e = GetSecurityInfo(
                h,
                SE_KERNEL_OBJECT,
                quali,
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
                &mut sd,
            );
            if e != 0 || sd.is_null() {
                return None;
            }
            let mut testo: *mut u16 = null_mut();
            let ok = ConvertSecurityDescriptorToStringSecurityDescriptorW(
                sd,
                SDDL_REVISION_1,
                quali,
                &mut testo,
                null_mut(),
            );
            LocalFree(sd);
            if ok == 0 || testo.is_null() {
                return None;
            }
            testo_e_libera(testo)
        }
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
        if !stesso_utente(p.0) || !integrita(p.0).is_some_and(integrita_sufficiente) {
            return None;
        }
        let mut buf = vec![0u16; 32_768];
        let mut n = buf.len() as u32;
        // SAFETY: `buf` è lungo `n` caratteri; il sistema scrive al più tanto.
        if unsafe { QueryFullProcessImageNameW(p.0, 0, buf.as_mut_ptr(), &mut n) } == 0 {
            return None;
        }
        let percorso = PathBuf::from(String::from_utf16(&buf[..n as usize]).ok()?);
        // Il tubo è ancora collegato dopo la lettura (riduce, non esclude, un PID riusato).
        if !ancora_collegato(h) {
            return None;
        }
        Some(percorso)
    }

    pub fn catena(pid: u32) -> Vec<(u32, String)> {
        let mut processi: Vec<(u32, u32, String)> = Vec::new();
        // SAFETY: istantanea chiusa sempre; la struttura parte azzerata con `dwSize` impostato.
        unsafe {
            let h = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if h == INVALID_HANDLE_VALUE {
                return Vec::new();
            }
            let h = Chiudi(h);
            let mut e: PROCESSENTRY32W = zeroed();
            e.dwSize = size_of::<PROCESSENTRY32W>() as u32;
            let mut ok = Process32FirstW(h.0, &mut e) != 0;
            while ok {
                let fine = e
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(e.szExeFile.len());
                processi.push((
                    e.th32ProcessID,
                    e.th32ParentProcessID,
                    String::from_utf16_lossy(&e.szExeFile[..fine]),
                ));
                ok = Process32NextW(h.0, &mut e) != 0;
            }
        }
        let mut catena: Vec<(u32, String)> = Vec::new();
        let mut corrente = pid;
        while catena.len() < CATENA_MASSIMA {
            let Some((_, padre, nome)) = processi.iter().find(|p| p.0 == corrente) else {
                break;
            };
            catena.push((corrente, nome.clone()));
            if *padre == 0 || catena.iter().any(|(p, _)| p == padre) {
                break;
            }
            corrente = *padre;
        }
        catena
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
        let con_giro = d.path().join("sotto").join("..").join("Assistente.exe");
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

    #[test]
    fn integrita_bassa_non_basta() {
        assert!(!integrita_sufficiente(0x0000), "non attendibile");
        assert!(!integrita_sufficiente(0x1000), "bassa");
        assert!(integrita_sufficiente(0x2000), "media");
        assert!(integrita_sufficiente(0x2100), "media più");
        assert!(integrita_sufficiente(0x3000), "alta");
        assert!(integrita_sufficiente(0x4000), "sistema");
    }

    #[cfg(windows)]
    #[test]
    fn sid_e_integrita_del_processo_dei_test() {
        let s = sid_utente().expect("SID dell'utente");
        assert!(s.starts_with("S-1-"), "{s}");
        let i = integrita_corrente().expect("integrità");
        assert!(integrita_sufficiente(i), "{i:#x}");
    }

    #[cfg(windows)]
    #[test]
    fn catena_del_processo_dei_test() {
        let c = catena_processi(std::process::id());
        assert!(!c.is_empty() && c.len() <= CATENA_MASSIMA, "{c:?}");
        assert_eq!(c[0].0, std::process::id());
        let mio = std::env::current_exe().unwrap();
        let nome = mio.file_name().unwrap().to_string_lossy().to_lowercase();
        assert_eq!(c[0].1.to_lowercase(), nome);
        // Un PID che non c'è: catena vuota.
        assert!(catena_processi(u32::MAX - 3).is_empty());
    }
}
