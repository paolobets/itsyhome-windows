//! C'è un'app a schermo intero sul monitor della pillola? (Video, presentazioni, giochi.)
//!
//! `SHQueryUserNotificationState` dice solo «c'è uno schermo intero da qualche parte»; con due
//! monitor un video a tutto schermo sull'altro nasconderebbe la pillola per niente. Quindi si
//! guarda la finestra in primo piano: a schermo intero se copre tutto il suo monitor e non è una
//! finestra normale ingrandita (con la barra del titolo) né il desktop; e conta solo se quel
//! monitor è quello della pillola. Il D3D a schermo intero esclusivo (`QUNS_RUNNING_D3D_FULL_SCREEN`)
//! vale anche se la finestra non copre il monitor (risoluzione cambiata), sempre solo su quel monitor.

use crate::finestre::RettangoloFisico;

/// Quel che serve sapere della finestra in primo piano.
// Fuori da Windows non si guarda il primo piano: tipo e decisione restano per i test.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrimoPiano {
    /// Rettangolo della finestra (pixel fisici).
    pub finestra: RettangoloFisico,
    /// Il monitor su cui sta (intero, non l'area di lavoro) e il suo identificativo.
    pub monitor: RettangoloFisico,
    pub id_monitor: isize,
    /// Finestra normale con la barra del titolo (ingrandita copre l'area di lavoro, e con la
    /// barra delle applicazioni nascosta anche il monitor: non è schermo intero).
    pub con_titolo: bool,
    /// Desktop, barra delle applicazioni o una finestra nostra: mai schermo intero.
    pub esclusa: bool,
    /// Windows dice che gira un D3D a schermo intero esclusivo o che è attiva la modalità
    /// presentazione (`SHQueryUserNotificationState`).
    pub segnalato_da_windows: bool,
}

/// Decisione pura: la finestra in primo piano è a schermo intero sul monitor della pillola?
#[cfg_attr(not(windows), allow(dead_code))]
pub fn sul_monitor(primo_piano: Option<PrimoPiano>, monitor_pillola: isize) -> bool {
    let Some(p) = primo_piano else { return false };
    if p.esclusa || p.id_monitor != monitor_pillola {
        return false;
    }
    let (f, m) = (p.finestra, p.monitor);
    let copre = f.x <= m.x
        && f.y <= m.y
        && f.x + f.larghezza >= m.x + m.larghezza
        && f.y + f.altezza >= m.y + m.altezza;
    p.segnalato_da_windows || (copre && !p.con_titolo)
}

#[cfg(windows)]
pub fn attivo(monitor_pillola: isize) -> bool {
    sul_monitor(primo_piano(), monitor_pillola)
}

#[cfg(not(windows))]
pub fn attivo(_monitor_pillola: isize) -> bool {
    false
}

#[cfg(windows)]
fn primo_piano() -> Option<PrimoPiano> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::Shell::{
        SHQueryUserNotificationState, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetForegroundWindow, GetWindowLongW, GetWindowRect, GetWindowThreadProcessId, GWL_STYLE,
        WS_CAPTION,
    };

    fn rettangolo(r: RECT) -> RettangoloFisico {
        RettangoloFisico { x: r.left, y: r.top, larghezza: r.right - r.left, altezza: r.bottom - r.top }
    }

    // SAFETY: chiamate di sola lettura su una finestra che può sparire in qualunque momento:
    // ognuna fallisce senza danni su un hwnd non più valido; i buffer sono locali e dimensionati.
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd == 0 {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let mut classe = [0u16; 64];
        let n = GetClassNameW(hwnd, classe.as_mut_ptr(), classe.len() as i32).max(0) as usize;
        let classe = String::from_utf16_lossy(&classe[..n]);
        let esclusa = pid == GetCurrentProcessId()
            || matches!(classe.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd");

        let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        if GetWindowRect(hwnd, &mut r) == 0 {
            return None;
        }
        let id_monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(id_monitor, &mut info) == 0 {
            return None;
        }
        let stile = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let mut stato = 0;
        let segnalato = SHQueryUserNotificationState(&mut stato) == 0
            && (stato == QUNS_RUNNING_D3D_FULL_SCREEN || stato == QUNS_PRESENTATION_MODE);
        Some(PrimoPiano {
            finestra: rettangolo(r),
            monitor: rettangolo(info.rcMonitor),
            id_monitor,
            con_titolo: stile & WS_CAPTION == WS_CAPTION,
            esclusa,
            segnalato_da_windows: segnalato,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const MONITOR: RettangoloFisico = RettangoloFisico { x: 0, y: 0, larghezza: 1920, altezza: 1080 };

    fn video(id_monitor: isize) -> PrimoPiano {
        PrimoPiano {
            finestra: MONITOR,
            monitor: MONITOR,
            id_monitor,
            con_titolo: false,
            esclusa: false,
            segnalato_da_windows: false,
        }
    }

    #[test]
    fn video_a_tutto_schermo_sul_monitor_della_pillola() {
        assert!(sul_monitor(Some(video(1)), 1));
        // Più grande del monitor (bordi invisibili fuori schermo): conta lo stesso.
        let mut v = video(1);
        v.finestra = RettangoloFisico { x: -8, y: -8, larghezza: 1936, altezza: 1096 };
        assert!(sul_monitor(Some(v), 1));
    }

    #[test]
    fn schermo_intero_sull_altro_monitor_non_conta() {
        assert!(!sul_monitor(Some(video(2)), 1));
        let mut v = video(2);
        v.segnalato_da_windows = true;
        assert!(!sul_monitor(Some(v), 1));
    }

    #[test]
    fn finestra_ingrandita_o_piu_piccola_non_e_schermo_intero() {
        let mut v = video(1);
        v.con_titolo = true;
        assert!(!sul_monitor(Some(v), 1));
        let mut v = video(1);
        v.finestra = RettangoloFisico { x: 0, y: 0, larghezza: 1920, altezza: 1040 };
        assert!(!sul_monitor(Some(v), 1));
    }

    #[test]
    fn desktop_e_finestre_nostre_esclusi() {
        let mut v = video(1);
        v.esclusa = true;
        assert!(!sul_monitor(Some(v), 1));
        assert!(!sul_monitor(None, 1));
    }

    #[test]
    fn segnalato_da_windows_sul_monitor_della_pillola() {
        let mut v = video(1);
        v.finestra = RettangoloFisico { x: 0, y: 0, larghezza: 800, altezza: 600 };
        v.segnalato_da_windows = true;
        assert!(sul_monitor(Some(v), 1));
    }
}
