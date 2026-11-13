//! Avvio con il sistema (`auto-launch`). Su Windows: il valore `ItsyHome` sotto
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, solo per l'utente (mai HKLM), con il
//! percorso di questo eseguibile tra virgolette (può contenere spazi).

use auto_launch::AutoLaunch;

/// Nome del valore nel registro (come la v2).
pub const NOME_APP: &str = "ItsyHome";

fn lanciatore() -> Result<AutoLaunch, String> {
    let exe = std::env::current_exe().map_err(|e| format!("percorso dell'app non trovato: {e}"))?;
    let percorso = exe.to_string_lossy();
    #[cfg(windows)]
    let percorso = format!("\"{percorso}\"");
    let mut b = auto_launch::AutoLaunchBuilder::new();
    b.set_app_name(NOME_APP).set_app_path(&percorso);
    #[cfg(windows)]
    b.set_windows_enable_mode(auto_launch::WindowsEnableMode::CurrentUser);
    b.build().map_err(|e| e.to_string())
}

/// Attiva o disattiva l'avvio con il sistema.
pub fn imposta(attivo: bool) -> Result<(), String> {
    let l = lanciatore()?;
    let esito = if attivo { l.enable() } else { l.disable() };
    esito.map_err(|e| format!("avvio con il sistema non {}: {e}", if attivo { "attivato" } else { "disattivato" }))
}

/// L'avvio con il sistema è attivo?
pub fn attivo() -> Result<bool, String> {
    lanciatore()?.is_enabled().map_err(|e| e.to_string())
}
