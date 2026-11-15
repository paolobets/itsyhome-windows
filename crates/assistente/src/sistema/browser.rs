//! Apertura di un collegamento nel browser predefinito del sistema (Task 14f): su Windows
//! `ShellExecuteW` col verbo «open», su macOS `open`, su Linux `xdg-open`. Mai nella finestra di
//! gestione, dove JavaScript è spento.
//!
//! Chi chiama passa solo indirizzi già controllati (`collegamenti::valida_indirizzo`: `http` o
//! `https`, niente spazi, virgolette o caratteri di controllo); `ammesso` è solo l'ultima difesa.
//! `ShellExecuteW` gira sul thread dell'interfaccia: di solito ritorna subito (il browser parte
//! per conto suo); con un'associazione rotta potrebbe mostrare «Come vuoi aprire?» e fermare
//! l'interfaccia finché non si risponde (accettato per M1). COM c'è già: winit chiama
//! `OleInitialize` sul thread principale.

/// Apre `url` nel browser predefinito. L'errore è per i dettagli tecnici.
#[cfg(windows)]
pub fn apri(url: &str) -> Result<(), String> {
    if !ammesso(url) {
        return Err("indirizzo non ammesso".into());
    }
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
    if !ammesso(url) {
        return Err("indirizzo non ammesso".into());
    }
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

/// Ultima difesa se in futuro qualcuno chiamasse `apri` senza passare da
/// `collegamenti::valida_indirizzo`: solo `http://` o `https://`, niente spazi né virgolette.
pub fn ammesso(url: &str) -> bool {
    (url.starts_with("http://") || url.starts_with("https://"))
        && !url.chars().any(|c| c.is_whitespace() || c.is_control() || c == '"')
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn solo_http_e_https() {
        assert!(ammesso("https://esempio.lan:8006/"));
        assert!(ammesso("http://192.0.2.10/admin"));
        for no in ["file:///C:/x", "javascript:alert(1)", "calc.exe", "https://a b", "https://a\"b", "HTTPS://maiuscolo"] {
            assert!(!ammesso(no), "{no}");
        }
    }
}
