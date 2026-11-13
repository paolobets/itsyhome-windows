//! Dove sta la pillola: in alto al centro dell'area di lavoro del monitor principale.

use slint::winit_030::{winit, EventResult, WinitWindowAccessor};
use slint::ComponentHandle;
use std::time::Duration;

/// Rettangolo in pixel fisici (coordinate dello schermo virtuale: possono essere negative).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RettangoloFisico {
    pub x: i32,
    pub y: i32,
    pub larghezza: i32,
    pub altezza: i32,
}

/// Distanza logica della finestra dal bordo alto dell'area di lavoro.
pub const MARGINE_LOGICO: f32 = 8.0;

/// Angolo in alto a sinistra (pixel fisici) di una finestra larga `larghezza_logica`,
/// centrata orizzontalmente in `area` e staccata di `margine_logico` dal suo bordo alto.
pub fn posizione_pillola(
    area: RettangoloFisico,
    scala: f32,
    larghezza_logica: f32,
    margine_logico: f32,
) -> (i32, i32) {
    let larghezza = (larghezza_logica * scala).round() as i32;
    let x = area.x + (area.larghezza - larghezza) / 2;
    let y = area.y + (margine_logico * scala).round() as i32;
    (x, y)
}

/// Porta la finestra in alto al centro del monitor principale (se non c'è, del primo
/// disponibile). Non fa nulla finché la finestra winit non esiste o se è già al suo posto.
pub fn applica_posizione(finestra: &slint::Window, larghezza_logica: f32) {
    let calcolata = finestra
        .with_winit_window(|w| {
            let monitor = w.primary_monitor().or_else(|| w.available_monitors().next())?;
            let scala = monitor.scale_factor() as f32;
            Some(posizione_pillola(area_di_lavoro(&monitor), scala, larghezza_logica, MARGINE_LOGICO))
        })
        .flatten();
    if let Some((x, y)) = calcolata {
        let attuale = finestra.position();
        // Solo se cambia: spostare la finestra genera un altro `Moved`.
        if (attuale.x, attuale.y) != (x, y) {
            finestra.set_position(slint::PhysicalPosition::new(x, y));
        }
    }
}

/// Tiene la finestra di `ui` al suo posto: appena nasce la finestra winit e a ogni
/// `Moved`, `Resized` (cambio d'umore) e `ScaleFactorChanged` (DPI o monitor cambiati).
/// `larghezza` dice la larghezza logica attuale della finestra.
pub fn segui_schermo<C: ComponentHandle + 'static>(ui: &C, larghezza: fn(&C) -> f32) {
    let debole = ui.as_weak();
    let ricalcola = move || {
        let debole = debole.clone();
        // Rimandato al giro successivo del ciclo: durante l'evento winit applica ancora
        // le sue dimensioni (es. il rettangolo suggerito da WM_DPICHANGED).
        slint::Timer::single_shot(Duration::ZERO, move || {
            if let Some(ui) = debole.upgrade() {
                applica_posizione(ui.window(), larghezza(&ui));
            }
        });
    };

    let al_cambio = ricalcola.clone();
    ui.window().on_winit_window_event(move |_, evento| {
        if matches!(
            evento,
            winit::event::WindowEvent::Moved(_)
                | winit::event::WindowEvent::Resized(_)
                | winit::event::WindowEvent::ScaleFactorChanged { .. }
        ) {
            al_cambio();
        }
        EventResult::Propagate
    });

    let debole = ui.as_weak();
    let _ = slint::spawn_local(async move {
        let Some(ui) = debole.upgrade() else { return };
        if ui.window().winit_window().await.is_ok() {
            ricalcola();
        }
    });
}

#[cfg(windows)]
fn area_di_lavoro(monitor: &winit::monitor::MonitorHandle) -> RettangoloFisico {
    use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITORINFO};
    use winit::platform::windows::MonitorHandleExtWindows;

    let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    // SAFETY: `hmonitor` viene da un MonitorHandle valido; `info` è inizializzata con cbSize.
    if unsafe { GetMonitorInfoW(monitor.hmonitor(), &mut info) } != 0 {
        let r = info.rcWork;
        return RettangoloFisico { x: r.left, y: r.top, larghezza: r.right - r.left, altezza: r.bottom - r.top };
    }
    monitor_intero(monitor)
}

#[cfg(not(windows))]
fn area_di_lavoro(monitor: &winit::monitor::MonitorHandle) -> RettangoloFisico {
    monitor_intero(monitor)
}

fn monitor_intero(monitor: &winit::monitor::MonitorHandle) -> RettangoloFisico {
    let p = monitor.position();
    let s = monitor.size();
    RettangoloFisico { x: p.x, y: p.y, larghezza: s.width as i32, altezza: s.height as i32 }
}

#[cfg(test)]
mod test {
    use super::*;

    fn area(x: i32, y: i32, larghezza: i32, altezza: i32) -> RettangoloFisico {
        RettangoloFisico { x, y, larghezza, altezza }
    }

    #[test]
    fn centrata_su_monitor_1920_scala_1() {
        assert_eq!(posizione_pillola(area(0, 0, 1920, 1040), 1.0, 200.0, MARGINE_LOGICO), (860, 8));
    }

    #[test]
    fn centrata_con_scala_175() {
        let (x, y) = posizione_pillola(area(0, 0, 2560, 1528), 1.75, 200.0, MARGINE_LOGICO);
        assert_eq!(x, (2560 - 350) / 2);
        assert_eq!(y, 14);
    }

    #[test]
    fn monitor_secondario_a_sinistra() {
        let (x, y) = posizione_pillola(area(-1920, 0, 1920, 1080), 1.0, 200.0, MARGINE_LOGICO);
        assert_eq!(x, -1920 + (1920 - 200) / 2);
        assert!(x < 0);
        assert_eq!(y, 8);
    }
}
