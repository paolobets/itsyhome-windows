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
//!
//! Le pagine di Windows (M2, Task 8, solo Windows) sono a parte: `apri_pagina` con tre URI fissi,
//! mai scritti da fuori, e `apri` resta solo `http`/`https` (rifiuta anche `ms-settings:`).
//! Documentazione letta (learn.microsoft.com, 16/11/2026): «Launch Windows Settings»
//! (windows/apps/develop/launch/launch-settings): `ms-settings:network-wifi` è la pagina Wi-Fi
//! («solo se il PC ha un adattatore Wi-Fi»), `ms-settings:bluetooth` la pagina Bluetooth; le
//! pagine si aprono lanciando l'URI (`Launcher.LaunchUriAsync` o, da un'app desktop,
//! `ShellExecuteW` col verbo «open», che risolve i protocolli registrati come `ms-settings:`).
//! `ms-availablenetworks:` (l'elenco delle reti Wi-Fi disponibili, il riquadro di rete della
//! barra) non è in quella pagina: è un protocollo della shell, preso dal brief; **da verificare
//! dal vivo** che apra l'elenco su questo Windows. Un URI non risolto fa restituire a
//! `ShellExecuteW` un valore ≤ 32 (errore, scritto nel log).

/// Apre `url` nel browser predefinito. L'errore è per i dettagli tecnici.
#[cfg(windows)]
pub fn apri(url: &str) -> Result<(), String> {
    if !ammesso(url) {
        return Err("indirizzo non ammesso".into());
    }
    shell_execute(url)
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

/// Le pagine di Windows che il pannello apre (vista PC, Connessioni).
#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaginaWindows {
    /// «Reti Wi-Fi ›»: l'elenco delle reti disponibili.
    RetiWifi,
    /// Le Impostazioni del Wi-Fi (dopo un rifiuto del Wi-Fi).
    ImpostazioniWifi,
    /// Le Impostazioni del Bluetooth («Dispositivi Bluetooth ›» e dopo un rifiuto).
    Bluetooth,
}

/// L'URI fisso di una pagina di Windows.
#[cfg(any(windows, test))]
pub fn uri(p: PaginaWindows) -> &'static str {
    match p {
        PaginaWindows::RetiWifi => "ms-availablenetworks:",
        PaginaWindows::ImpostazioniWifi => "ms-settings:network-wifi",
        PaginaWindows::Bluetooth => "ms-settings:bluetooth",
    }
}

/// Apre una pagina di Windows con `ShellExecuteW` sul suo URI fisso. L'errore è per i log.
#[cfg(windows)]
pub fn apri_pagina(p: PaginaWindows) -> Result<(), String> {
    shell_execute(uri(p))
}

/// `ShellExecuteW` col verbo «open» (browser o pagina di Windows).
#[cfg(windows)]
fn shell_execute(cosa: &str) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let largo = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (verbo, file) = (largo("open"), largo(cosa));
    // SAFETY: le due stringhe terminano con lo zero e vivono fino al ritorno; gli altri puntatori
    // nulli sono ammessi (nessuna finestra madre, nessun parametro, cartella corrente).
    let esito = unsafe { ShellExecuteW(0, verbo.as_ptr(), file.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL) };
    // Un valore oltre 32 vuol dire riuscito (documentazione di ShellExecuteW).
    if esito > 32 {
        Ok(())
    } else {
        Err(format!("il sistema non l'ha aperto (ShellExecuteW: {esito})"))
    }
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
        for no in ["ms-settings:bluetooth", "ms-availablenetworks:", "file:///C:/x", "javascript:alert(1)", "calc.exe", "https://a b", "https://a\"b", "HTTPS://maiuscolo"] {
            assert!(!ammesso(no), "{no}");
        }
    }

    #[test]
    fn pagine_di_windows_fisse() {
        assert_eq!(uri(PaginaWindows::RetiWifi), "ms-availablenetworks:");
        assert_eq!(uri(PaginaWindows::ImpostazioniWifi), "ms-settings:network-wifi");
        assert_eq!(uri(PaginaWindows::Bluetooth), "ms-settings:bluetooth");
        // `apri` resta solo http/https: le pagine di Windows passano solo da `apri_pagina`.
        assert!(apri("ms-settings:bluetooth").is_err());
        assert!(apri("ms-availablenetworks:").is_err());
    }
}
