//! Apertura di un collegamento nel browser predefinito del sistema (Task 14f): su Windows
//! `ShellExecuteW` col verbo «open», su macOS `open`, su Linux `xdg-open`. Mai nella finestra di
//! gestione, dove JavaScript è spento.
//!
//! Chi chiama passa solo indirizzi già controllati (`collegamenti::valida_indirizzo`: `http` o
//! `https`, niente spazi, virgolette o caratteri di controllo): qui non si ricontrolla nulla.

/// Apre `url` nel browser predefinito. L'errore è per i dettagli tecnici.
#[cfg(windows)]
pub fn apri(url: &str) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let largo = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (verbo, file) = (largo("open"), largo(url));
    // SAFETY: le due stringhe terminano con lo zero e vivono fino al ritorno; gli altri puntatori
    // nulli sono ammessi (nessuna finestra madre, nessun parametro, cartella corrente).
    let esito = unsafe { ShellExecuteW(0, verbo.as_ptr(), file.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL) };
    // Un valore oltre 32 vuol dire riuscito (documentazione di ShellExecuteW).
    if esito > 32 {
        Ok(())
    } else {
        Err(format!("il sistema non ha aperto il browser (ShellExecuteW: {esito})"))
    }
}

/// Apre `url` nel browser predefinito. L'errore è per i dettagli tecnici.
#[cfg(not(windows))]
pub fn apri(url: &str) -> Result<(), String> {
    let comando = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    let mut figlio = std::process::Command::new(comando)
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("{comando} non avviato: {e}"))?;
    // Lo si raccoglie su un thread a parte: niente processi zombie, l'interfaccia non aspetta.
    std::thread::spawn(move || {
        let _ = figlio.wait();
    });
    Ok(())
}
