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

/// Distanza logica della finestra della pillola dal bordo alto dell'area di lavoro: negativa
/// quanto il margine dell'aura, così la finestra sporge sopra lo schermo e il bordo superiore
/// della pillola tocca il bordo alto, come il notch di macOS (la pillola resta tutta arrotondata).
pub const MARGINE_LOGICO: f32 = -MARGINE_AURA;

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
/// Centro degli occhi della pillola in "ti cerca" rispetto al centro della sua finestra
/// (`scarto-occhi-allerta` in pillola.slint): pillola larga 360, occhi a 20 dal bordo sinistro,
/// ingombro 30. Il pannello aperto da "ti cerca" fa partire gli occhi da qui.
pub const SCARTO_OCCHI_ALLERTA: f32 = -145.0;
/// Larghezza logica del pannello (`width` di pannello.slint).
pub const LARGHEZZA_PANNELLO: f32 = 800.0;

/// Angolo in alto a sinistra (pixel fisici) del pannello: centrato come la pillola e con il
/// bordo alto dove sta il bordo alto della pillola (cioè sul bordo alto dell'area di lavoro),
/// così la riga degli occhi (alta quanto la pillola) cade esattamente sugli occhi della pillola.
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

/// Regione della finestra della pillola, `(sinistra, alto, destra, basso)` in pixel fisici della
/// finestra larga `larghezza` e alta `altezza`: una fascia centrale larga `larghezza_logica`
/// che parte sotto il margine dell'aura. La finestra sporge di quel margine sopra l'area di
/// lavoro (`MARGINE_LOGICO`): quella striscia è fuori dallo schermo su un monitor solo, ma
/// con un monitor sopra o con la barra delle applicazioni in alto disegnerebbe e prenderebbe
/// clic lì. L'aura di sopra non si vede comunque, quindi non si perde nulla.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn regione_pillola(larghezza: i32, altezza: i32, scala: f32, larghezza_logica: f32) -> (i32, i32, i32, i32) {
    let fascia = ((larghezza_logica * scala).round() as i32).min(larghezza);
    let sinistra = (larghezza - fascia) / 2;
    let alto = ((MARGINE_AURA * scala).round() as i32).min(altezza);
    (sinistra, alto, sinistra + fascia, altezza)
}

/// Limita disegno e clic della finestra alla regione di `regione_pillola`: fuori dalla fascia
/// la finestra non esiste per il mouse. Così la finestra della pillola può restare sempre larga
/// quanto la pillola più larga senza rubare clic.
#[cfg(windows)]
pub fn ritaglia_al_centro(finestra: &slint::Window, larghezza_logica: f32) {
    use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Graphics::Gdi::{CreateRectRgn, DeleteObject, SetWindowRgn};

    let dimensioni = finestra.size();
    let rettangolo = regione_pillola(
        dimensioni.width as i32,
        dimensioni.height as i32,
        finestra.scale_factor(),
        larghezza_logica,
    );
    thread_local! {
        // Ultimo ritaglio applicato (hwnd, rettangolo): rifarlo uguale ridisegnerebbe per niente.
        static ULTIMO: std::cell::Cell<(isize, (i32, i32, i32, i32))> = const { std::cell::Cell::new((0, (0, 0, 0, 0))) };
    }
    finestra.with_winit_window(|w| {
        let Ok(maniglia) = w.window_handle() else { return };
        let RawWindowHandle::Win32(h) = maniglia.as_raw() else { return };
        let chiave = (h.hwnd.get(), rettangolo);
        if ULTIMO.with(|u| u.replace(chiave)) == chiave {
            return;
        }
        senza_barra_del_titolo(h.hwnd.get());
        let (sinistra, alto, destra, basso) = rettangolo;
        // SAFETY: hwnd valido finché esiste la finestra winit; dopo SetWindowRgn riuscita la
        // regione appartiene al sistema, se fallisce va liberata qui.
        unsafe {
            let regione = CreateRectRgn(sinistra, alto, destra, basso);
            if regione != 0 && SetWindowRgn(h.hwnd.get(), regione, 1) == 0 {
                DeleteObject(regione);
            }
        }
    });
}

/// Procedura che la pillola aveva prima della nostra. NON è la procedura di classe di winit:
/// prima di mostrare la finestra Slint vi installa AccessKit (`SubclassingAdapter` di
/// accesskit_windows), che a sua volta inoltra a winit. Va quindi inoltrato sempre a questa
/// (mai a `GetClassLongPtrW`, che salterebbe AccessKit). Solo la pillola la usa.
#[cfg(windows)]
static PROCEDURA_PRECEDENTE: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// Toglie la barra del titolo "classica" che Windows disegnerebbe sopra la pillola.
///
/// Le finestre di winit senza cornice tengono gli stili `WS_CAPTION | WS_BORDER` e allargano
/// l'area client a tutta la finestra. Finché la finestra non ha una regione, la cornice la
/// disegna DWM, e non si vede. Con `SetWindowRgn` DWM smette di disegnarla e a ogni cambio di
/// attivazione (`WM_NCACTIVATE`: clic sulla pillola, pannello che si chiude, clic altrove)
/// `DefWindowProc` dipinge con GDI la barra del titolo classica (una striscia chiara a gradiente)
/// sopra la pillola, che resta lì finché Slint non ridisegna quella zona. Con `lParam = -1`
/// `DefWindowProc` aggiorna lo stato di attivazione senza ridisegnare l'area non client (vedi la
/// documentazione di WM_NCACTIVATE); AccessKit e winit ricevono il messaggio come prima.
#[cfg(windows)]
fn senza_barra_del_titolo(hwnd: isize) {
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallWindowProcW, GetWindowLongPtrW, SetWindowLongPtrW, GWLP_WNDPROC, WM_NCACTIVATE, WNDPROC,
    };
    use std::sync::atomic::Ordering;

    unsafe extern "system" fn procedura(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        // SAFETY: il valore è la procedura precedente letta da GetWindowLongPtrW prima di
        // sostituirla (mai zero quando questa procedura è installata).
        let originale: WNDPROC = unsafe { std::mem::transmute(PROCEDURA_PRECEDENTE.load(Ordering::Relaxed)) };
        let lparam = if msg == WM_NCACTIVATE { -1 } else { lparam };
        unsafe { CallWindowProcW(originale, hwnd, msg, wparam, lparam) }
    }

    let nostra = procedura as unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT as usize as isize;
    // SAFETY: hwnd è una finestra viva del thread dell'interfaccia; la procedura sostituita è
    // salvata prima dell'installazione, così nessun messaggio arriva a `procedura` senza di essa.
    unsafe {
        let attuale = GetWindowLongPtrW(hwnd, GWLP_WNDPROC);
        if attuale == 0 || attuale == nostra {
            return;
        }
        PROCEDURA_PRECEDENTE.store(attuale, Ordering::Relaxed);
        SetWindowLongPtrW(hwnd, GWLP_WNDPROC, nostra);
    }
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
        assert_eq!(posizione_pillola(area(0, 0, 1920, 1040), 1.0, 200.0, MARGINE_LOGICO), (860, -10));
    }

    #[test]
    fn centrata_con_scala_175() {
        let (x, y) = posizione_pillola(area(0, 0, 2560, 1528), 1.75, 200.0, MARGINE_LOGICO);
        assert_eq!(x, (2560 - 350) / 2);
        // -17,5 arrotondato lontano da zero: la finestra sporge di 18 px sopra lo schermo.
        assert_eq!(y, -18);
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
            // Il pannello resta dentro l'area di lavoro, anche largo 800.
            assert!(qx >= a.x && qy >= a.y);
            assert!(qx + (LARGHEZZA_PANNELLO * scala).round() as i32 <= a.x + a.larghezza);
        }
    }

    #[test]
    fn da_ti_cerca_gli_occhi_del_pannello_partono_da_quelli_della_pillola() {
        // In "ti cerca" gli occhi della pillola stanno a sinistra (pillola larga 360 al centro
        // della finestra, occhi a 20 px dal bordo, ingombro 30): centro a 35 px dal bordo sinistro.
        // Il pannello, aperto da lì, fa partire i suoi occhi da `partenza-occhi` =
        // SCARTO_OCCHI_ALLERTA rispetto al proprio centro, poi li porta al centro.
        let occhi_dal_bordo = 20.0 + (2.0 * 10.0 + 10.0) / 2.0;
        for (a, scala) in [
            (area(0, 0, 1920, 1040), 1.0),
            (area(0, 0, 2560, 1528), 1.75),
            (area(-1920, 0, 1920, 1080), 1.25),
            (area(0, 40, 1366, 728), 1.5),
        ] {
            let (px, _) = posizione_pillola(a, scala, LARGHEZZA_FINESTRA_PILLOLA, MARGINE_LOGICO);
            let (qx, _) = posizione_pannello(a, scala);
            let bordo_pillola = px as f32 + (LARGHEZZA_FINESTRA_PILLOLA - 360.0) / 2.0 * scala;
            let occhi_pillola = bordo_pillola + occhi_dal_bordo * scala;
            let centro_pannello = qx as f32 + (LARGHEZZA_PANNELLO * scala).round() / 2.0;
            let partenza = centro_pannello + SCARTO_OCCHI_ALLERTA * scala;
            assert!((occhi_pillola - partenza).abs() <= 1.0, "scala {scala}: {occhi_pillola} vs {partenza}");
        }
    }

    #[test]
    fn pillola_e_pannello_attaccati_al_bordo_alto() {
        // Come il notch: il bordo superiore della pillola (sotto il margine dell'aura) tocca il
        // bordo alto dell'area di lavoro e il pannello parte da lì. Entro mezzo pixel fisico.
        for (a, scala) in [
            (area(0, 0, 1920, 1040), 1.0),
            (area(0, 0, 2560, 1528), 1.75),
            (area(-1920, 0, 1920, 1080), 1.25),
            (area(0, 40, 1366, 728), 1.5),
        ] {
            let (_, py) = posizione_pillola(a, scala, LARGHEZZA_FINESTRA_PILLOLA, MARGINE_LOGICO);
            let bordo_pillola = py as f32 + MARGINE_AURA * scala;
            assert!((bordo_pillola - a.y as f32).abs() <= 0.5, "pillola a scala {scala}: {bordo_pillola} vs {}", a.y);
            let (_, qy) = posizione_pannello(a, scala);
            assert_eq!(qy, a.y, "pannello a scala {scala}");
        }
    }

    #[test]
    fn regione_della_pillola_sotto_il_margine_dell_aura() {
        // Finestra 380 × 68 logici; fascia calm 220 al centro. La regione parte dove inizia lo
        // schermo (sotto l'aura che sporge) e arriva in fondo alla finestra.
        assert_eq!(regione_pillola(380, 68, 1.0, 220.0), (80, 10, 300, 68));
        let (l, a) = ((380.0_f32 * 1.75).round() as i32, (68.0_f32 * 1.75).round() as i32);
        let (sinistra, alto, destra, basso) = regione_pillola(l, a, 1.75, 220.0);
        assert_eq!(alto, 18, "margine dell'aura a 1,75: 17,5 arrotondato");
        assert_eq!(basso - alto, a - 18);
        assert_eq!(destra - sinistra, 385);
        assert_eq!(sinistra, (l - 385) / 2);
        // L'alto della regione coincide col bordo alto dell'area di lavoro.
        let (_, py) = posizione_pillola(area(0, 0, 2560, 1528), 1.75, LARGHEZZA_FINESTRA_PILLOLA, MARGINE_LOGICO);
        assert_eq!(py + alto, 0);
    }

    #[test]
    fn pannello_largo_800_centrato() {
        assert_eq!(LARGHEZZA_PANNELLO, 800.0);
        assert_eq!(posizione_pannello(area(0, 0, 1920, 1040), 1.0), ((1920 - 800) / 2, 0));
    }

    #[test]
    fn monitor_secondario_a_sinistra() {
        let (x, y) = posizione_pillola(area(-1920, 0, 1920, 1080), 1.0, 200.0, MARGINE_LOGICO);
        assert_eq!(x, -1920 + (1920 - 200) / 2);
        assert!(x < 0);
        assert_eq!(y, -10);
    }
}
