//! Dove stanno le finestre: pillola e pannello in alto al centro dell'area di lavoro del monitor principale.

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

/// Margine logico dell'aura attorno alla pillola (`margine-aura` in pillola.slint).
pub const MARGINE_AURA: f32 = 10.0;
/// Larghezza logica fissa della finestra della pillola (`larghezza-finestra` in pillola.slint).
pub const LARGHEZZA_FINESTRA_PILLOLA: f32 = 380.0;
/// Larghezza logica del pannello (`width` di pannello.slint).
pub const LARGHEZZA_PANNELLO: f32 = 560.0;

/// Angolo in alto a sinistra (pixel fisici) del pannello: centrato come la pillola e con il
/// bordo alto dove sta il bordo alto della pillola, così la riga degli occhi (alta quanto la
/// pillola) cade esattamente sugli occhi della pillola.
pub fn posizione_pannello(area: RettangoloFisico, scala: f32) -> (i32, i32) {
    let larghezza = (LARGHEZZA_PANNELLO * scala).round() as i32;
    let x = area.x + (area.larghezza - larghezza) / 2;
    let y = area.y + ((MARGINE_LOGICO + MARGINE_AURA) * scala).round() as i32;
    (x, y)
}

/// Area di lavoro e scala del monitor principale (se non c'è, del primo disponibile), viste
/// dalla finestra winit di `finestra`. `None` finché la finestra winit non esiste.
pub fn schermo_di(finestra: &slint::Window) -> Option<(RettangoloFisico, f32)> {
    finestra
        .with_winit_window(|w| {
            let monitor = w.primary_monitor().or_else(|| w.available_monitors().next())?;
            Some((area_di_lavoro(&monitor), monitor.scale_factor() as f32))
        })
        .flatten()
}

/// Sposta la finestra in `(x, y)` solo se non è già lì: spostarla genera un altro `Moved`.
pub fn sposta(finestra: &slint::Window, (x, y): (i32, i32)) {
    let attuale = finestra.position();
    if (attuale.x, attuale.y) != (x, y) {
        finestra.set_position(slint::PhysicalPosition::new(x, y));
    }
}

/// Porta la pillola in alto al centro del monitor principale. Non fa nulla finché la finestra
/// winit non esiste o se è già al suo posto.
pub fn posiziona_pillola(finestra: &slint::Window) {
    if let Some((area, scala)) = schermo_di(finestra) {
        sposta(finestra, posizione_pillola(area, scala, LARGHEZZA_FINESTRA_PILLOLA, MARGINE_LOGICO));
    }
}

/// Chiama `al_cambio` appena nasce la finestra winit di `ui` e a ogni `Moved`, `Resized` e
/// `ScaleFactorChanged` (DPI o monitor cambiati), così la finestra resta al suo posto.
pub fn segui_schermo<C: ComponentHandle + 'static>(ui: &C, al_cambio: fn(&C)) {
    let debole = ui.as_weak();
    let ricalcola = move || {
        let debole = debole.clone();
        // Rimandato al giro successivo del ciclo: durante l'evento winit applica ancora
        // le sue dimensioni (es. il rettangolo suggerito da WM_DPICHANGED).
        slint::Timer::single_shot(Duration::ZERO, move || {
            if let Some(ui) = debole.upgrade() {
                al_cambio(&ui);
            }
        });
    };

    let al_evento = ricalcola.clone();
    ui.window().on_winit_window_event(move |_, evento| {
        if matches!(
            evento,
            winit::event::WindowEvent::Moved(_)
                | winit::event::WindowEvent::Resized(_)
                | winit::event::WindowEvent::ScaleFactorChanged { .. }
        ) {
            al_evento();
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

/// Limita disegno e clic della finestra a una fascia centrale larga `larghezza_logica`
/// (a tutta altezza): fuori dalla fascia la finestra non esiste per il mouse. Così la finestra
/// della pillola può restare sempre larga quanto la pillola più larga senza rubare clic.
#[cfg(windows)]
pub fn ritaglia_al_centro(finestra: &slint::Window, larghezza_logica: f32) {
    use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Graphics::Gdi::{CreateRectRgn, DeleteObject, SetWindowRgn};

    let scala = finestra.scale_factor();
    let dimensioni = finestra.size();
    let (l, a) = (dimensioni.width as i32, dimensioni.height as i32);
    let fascia = ((larghezza_logica * scala).round() as i32).min(l);
    let sinistra = (l - fascia) / 2;
    thread_local! {
        // Ultimo ritaglio applicato (hwnd, rettangolo): rifarlo uguale ridisegnerebbe per niente.
        static ULTIMO: std::cell::Cell<(isize, i32, i32, i32)> = const { std::cell::Cell::new((0, 0, 0, 0)) };
    }
    finestra.with_winit_window(|w| {
        let Ok(maniglia) = w.window_handle() else { return };
        let RawWindowHandle::Win32(h) = maniglia.as_raw() else { return };
        let chiave = (h.hwnd.get(), sinistra, fascia, a);
        if ULTIMO.with(|u| u.replace(chiave)) == chiave {
            return;
        }
        // SAFETY: hwnd valido finché esiste la finestra winit; dopo SetWindowRgn riuscita la
        // regione appartiene al sistema, se fallisce va liberata qui.
        unsafe {
            let regione = CreateRectRgn(sinistra, 0, sinistra + fascia, a);
            if regione != 0 && SetWindowRgn(h.hwnd.get(), regione, 1) == 0 {
                DeleteObject(regione);
            }
        }
    });
}

/// Altrove la finestra resta intera (fascia trasparente cliccabile ai lati).
#[cfg(not(windows))]
pub fn ritaglia_al_centro(_finestra: &slint::Window, _larghezza_logica: f32) {}

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
    fn occhi_del_pannello_sugli_occhi_della_pillola() {
        // Gli occhi della pillola stanno al centro della sua finestra (umore tranquillo) e a metà
        // altezza della pillola; quelli del pannello al centro e a metà della riga alta, alta
        // quanto la pillola. Si confrontano al pixel fisico (gli arrotondamenti valgono mezzo pixel).
        for (a, scala) in [
            (area(0, 0, 1920, 1040), 1.0),
            (area(0, 0, 2560, 1528), 1.75),
            (area(-1920, 0, 1920, 1080), 1.25),
            (area(0, 40, 1366, 728), 1.5),
        ] {
            let (px, py) = posizione_pillola(a, scala, LARGHEZZA_FINESTRA_PILLOLA, MARGINE_LOGICO);
            let (qx, qy) = posizione_pannello(a, scala);
            let centro_pillola = px as f32 + (LARGHEZZA_FINESTRA_PILLOLA * scala).round() / 2.0;
            let centro_pannello = qx as f32 + (LARGHEZZA_PANNELLO * scala).round() / 2.0;
            assert!((centro_pillola - centro_pannello).abs() <= 1.0, "x a scala {scala}: {centro_pillola} vs {centro_pannello}");
            let alto_pillola = py as f32 + MARGINE_AURA * scala;
            assert!((alto_pillola - qy as f32).abs() <= 1.0, "y a scala {scala}: {alto_pillola} vs {qy}");
            // Il pannello resta dentro l'area di lavoro.
            assert!(qx >= a.x && qy >= a.y);
        }
    }

    #[test]
    fn monitor_secondario_a_sinistra() {
        let (x, y) = posizione_pillola(area(-1920, 0, 1920, 1080), 1.0, 200.0, MARGINE_LOGICO);
        assert_eq!(x, -1920 + (1920 - 200) / 2);
        assert!(x < 0);
        assert_eq!(y, 8);
    }
}
