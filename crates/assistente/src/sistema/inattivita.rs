//! Da quanto l'utente non tocca tastiera e mouse (per l'umore «dorme»). Si legge ogni 5 s dal
//! timer dei servizi: una chiamata di sistema, nessun gancio.

use std::time::Duration;

/// Windows: `GetLastInputInfo` (ultimo input della sessione). Altrove `None`.
#[cfg(windows)]
pub fn durata() -> Option<Duration> {
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

    let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    // SAFETY: `info` inizializzata con cbSize.
    if unsafe { GetLastInputInfo(&mut info) } == 0 {
        return None;
    }
    // SAFETY: nessun argomento. Entrambi i contatori girano a 49,7 giorni: differenza modulare.
    let ora = unsafe { GetTickCount() };
    Some(Duration::from_millis(u64::from(ora.wrapping_sub(info.dwTime))))
}

#[cfg(not(windows))]
pub fn durata() -> Option<Duration> {
    None
}
