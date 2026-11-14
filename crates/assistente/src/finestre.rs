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
pub const MARGINE_AURA: f32 = 8.0;
/// Pillola a riposo (`Faccia.larghezza-pillola` × `Faccia.altezza-pillola` in pillola.slint).
pub const LARGHEZZA_PILLOLA: f32 = 124.0;
pub const ALTEZZA_PILLOLA: f32 = 32.0;
/// Pillola che chiama (*attenzione*): icona della fonte, occhi, una riga, fino a 2 pulsanti.
pub const LARGHEZZA_ATTENZIONE: f32 = 400.0;
pub const ALTEZZA_ATTENZIONE: f32 = 36.0;
/// Pillola *bloccante*: più larga e più alta di quella che chiama, ferma, bordo più spesso.
pub const LARGHEZZA_BLOCCANTE: f32 = 440.0;
pub const ALTEZZA_BLOCCANTE: f32 = 40.0;
/// Finestra della pillola (`larghezza-finestra` × `altezza-finestra` in pillola.slint): la
/// pillola più grande (bloccante) più l'aura tutt'intorno. Fissa: cambiando aspetto la pillola
/// cresce o cala attorno al centro (in larghezza) e verso il basso (in altezza).
pub const LARGHEZZA_FINESTRA_PILLOLA: f32 = LARGHEZZA_BLOCCANTE + 2.0 * MARGINE_AURA;
pub const ALTEZZA_FINESTRA_PILLOLA: f32 = ALTEZZA_BLOCCANTE + 2.0 * MARGINE_AURA;
/// Centro degli occhi della pillola che chiama rispetto a quello degli occhi del pannello appena
/// aperto, `(x, y)` logici (`scarto-occhi-attenzione` / `scarto-occhi-bloccante` in
/// pillola.slint): x rispetto al centro della finestra, y rispetto al centro della riga alta del
/// pannello. Il pannello aperto da lì fa partire gli occhi da qui. Occhi a 36 dal bordo sinistro
/// della pillola (dopo l'icona della fonte), ingombro 21: x = -larghezza/2 + 46,5; y = metà
/// della pillola meno metà della riga del pannello.
pub const SCARTO_OCCHI_ATTENZIONE: (f32, f32) = (-153.5, 2.0);
pub const SCARTO_OCCHI_BLOCCANTE: (f32, f32) = (-173.5, 4.0);
/// Altezza della riga alta del pannello (`altezza-riga` in pannello.slint): quanto la pillola a
/// riposo, così gli occhi del pannello cadono sui suoi.
pub const ALTEZZA_RIGA_PANNELLO: f32 = ALTEZZA_PILLOLA;
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
/// finestra larga `larghezza` e alta `altezza`: la pillola con la sua aura, cioè una fascia
/// centrale larga `larghezza_logica` che parte sotto il margine dell'aura e arriva a
/// `altezza_logica` dall'alto della finestra (pillola + aura sotto; la fascia vuota che resta
/// sotto una pillola piccola ruberebbe clic). La finestra sporge di quel margine sopra l'area di
/// lavoro (`MARGINE_LOGICO`): quella striscia è fuori dallo schermo su un monitor solo, ma con un
/// monitor sopra o con la barra delle applicazioni in alto disegnerebbe e prenderebbe clic lì.
/// L'aura di sopra non si vede comunque, quindi non si perde nulla.
/// Il taglio è netto a un margine dalla pillola: l'aura deve starci dentro (vincolo scritto
/// accanto a `margine-aura` in pillola.slint).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn regione_pillola(
    larghezza: i32,
    altezza: i32,
    scala: f32,
    larghezza_logica: f32,
    altezza_logica: f32,
) -> (i32, i32, i32, i32) {
    let fascia = ((larghezza_logica * scala).round() as i32).min(larghezza);
    let sinistra = (larghezza - fascia) / 2;
    let alto = ((MARGINE_AURA * scala).round() as i32).min(altezza);
    let basso = ((altezza_logica * scala).round() as i32).clamp(alto, altezza);
    (sinistra, alto, sinistra + fascia, basso)
}

/// Regione della finestra della pillola ritirata: la linea `LARGHEZZA_LINEA` × `ALTEZZA_LINEA`
/// al centro, attaccata al bordo alto dello schermo (sotto il margine dell'aura che sporge).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn regione_linea(larghezza: i32, altezza: i32, scala: f32) -> (i32, i32, i32, i32) {
    use crate::sistema::ritirata::{ALTEZZA_LINEA, LARGHEZZA_LINEA};
    let fascia = ((LARGHEZZA_LINEA * scala).round() as i32).min(larghezza);
    let sinistra = (larghezza - fascia) / 2;
    let alto = ((MARGINE_AURA * scala).round() as i32).min(altezza);
    let basso = (alto + ((ALTEZZA_LINEA * scala).round() as i32).max(1)).min(altezza);
    (sinistra, alto, sinistra + fascia, basso)
}

/// Limita disegno e clic della finestra alla regione di `regione_pillola`: fuori dalla fascia
/// la finestra non esiste per il mouse. Così la finestra della pillola può restare sempre grande
/// quanto la pillola più grande senza rubare clic.
#[cfg(windows)]
pub fn ritaglia_alla_pillola(finestra: &slint::Window, (larghezza_logica, altezza_logica): (f32, f32)) {
    let dimensioni = finestra.size();
    let rettangolo = regione_pillola(
        dimensioni.width as i32,
        dimensioni.height as i32,
        finestra.scale_factor(),
        larghezza_logica,
        altezza_logica,
    );
    applica_regione(finestra, rettangolo);
}

/// Ritaglia la finestra della pillola alla linea della ritirata.
#[cfg(windows)]
pub fn ritaglia_a_linea(finestra: &slint::Window) {
    let dimensioni = finestra.size();
    let rettangolo = regione_linea(dimensioni.width as i32, dimensioni.height as i32, finestra.scale_factor());
    applica_regione(finestra, rettangolo);
}

#[cfg(windows)]
fn applica_regione(finestra: &slint::Window, rettangolo: (i32, i32, i32, i32)) {
    use windows_sys::Win32::Graphics::Gdi::{CreateRectRgn, DeleteObject, SetWindowRgn};

    thread_local! {
        // Ultimo ritaglio applicato (hwnd, rettangolo): rifarlo uguale ridisegnerebbe per niente.
        static ULTIMO: std::cell::Cell<(isize, (i32, i32, i32, i32))> = const { std::cell::Cell::new((0, (0, 0, 0, 0))) };
    }
    let Some(hwnd) = maniglia(finestra) else { return };
    let chiave = (hwnd, rettangolo);
    if ULTIMO.with(|u| u.replace(chiave)) == chiave {
        return;
    }
    senza_barra_del_titolo(hwnd);
    let (sinistra, alto, destra, basso) = rettangolo;
    // SAFETY: hwnd valido finché esiste la finestra winit; dopo SetWindowRgn riuscita la
    // regione appartiene al sistema, se fallisce va liberata qui.
    unsafe {
        let regione = CreateRectRgn(sinistra, alto, destra, basso);
        if regione != 0 && SetWindowRgn(hwnd, regione, 1) == 0 {
            DeleteObject(regione);
        }
    }
}

/// L'HWND della finestra winit di `finestra` (`None` finché non esiste).
#[cfg(windows)]
pub fn maniglia(finestra: &slint::Window) -> Option<isize> {
    use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    finestra
        .with_winit_window(|w| {
            let maniglia = w.window_handle().ok()?;
            match maniglia.as_raw() {
                RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
                _ => None,
            }
        })
        .flatten()
}

/// Rimette la finestra in cima a quelle «sempre in primo piano» senza attivarla né spostarla
/// (un'altra finestra sempre in primo piano può esserle passata sopra).
#[cfg(windows)]
pub fn metti_sopra(finestra: &slint::Window) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE,
    };
    let Some(hwnd) = maniglia(finestra) else { return };
    // SAFETY: hwnd di una finestra viva di questo thread; nessun puntatore passato.
    unsafe {
        SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER);
    }
}

#[cfg(not(windows))]
pub fn metti_sopra(_finestra: &slint::Window) {}

/// Il monitor (HMONITOR) su cui sta la finestra; 0 se non si sa.
#[cfg(windows)]
pub fn monitor_di(finestra: &slint::Window) -> isize {
    use windows_sys::Win32::Graphics::Gdi::{MonitorFromWindow, MONITOR_DEFAULTTONEAREST};
    // SAFETY: MonitorFromWindow accetta qualunque hwnd (anche nascosta) e non scrive nulla.
    maniglia(finestra).map(|h| unsafe { MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST) }).unwrap_or(0)
}

#[cfg(not(windows))]
pub fn monitor_di(_finestra: &slint::Window) -> isize {
    0
}

/// Dov'è il mouse (pixel fisici dello schermo virtuale).
#[cfg(windows)]
pub fn cursore() -> Option<(i32, i32)> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut p = POINT { x: 0, y: 0 };
    // SAFETY: `p` è un POINT valido da riempire.
    (unsafe { GetCursorPos(&mut p) } != 0).then_some((p.x, p.y))
}

#[cfg(not(windows))]
pub fn cursore() -> Option<(i32, i32)> {
    None
}

/// Clic passanti: con `si` il mouse attraversa la finestra e arriva a quella sotto (Windows:
/// `WS_EX_TRANSPARENT | WS_EX_LAYERED`, impostati da winit). Un layered senza attributi può non
/// essere disegnato: si fissa l'opacità piena (la linea è opaca e la regione la ritaglia esatta).
pub fn clic_passanti(finestra: &slint::Window, si: bool) {
    finestra.with_winit_window(|w| {
        if let Err(e) = w.set_cursor_hittest(!si) {
            eprintln!("clic passanti non impostati: {e}");
        }
    });
    #[cfg(windows)]
    if si {
        use windows_sys::Win32::UI::WindowsAndMessaging::{SetLayeredWindowAttributes, LWA_ALPHA};
        if let Some(hwnd) = maniglia(finestra) {
            // SAFETY: hwnd di una finestra viva di questo thread, ora layered.
            unsafe {
                SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA);
            }
        }
    }
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
pub fn ritaglia_alla_pillola(_finestra: &slint::Window, _visibile: (f32, f32)) {}

#[cfg(not(windows))]
pub fn ritaglia_a_linea(_finestra: &slint::Window) {}

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
        assert_eq!(posizione_pillola(area(0, 0, 1920, 1040), 1.0, 456.0, MARGINE_LOGICO), (732, -8));
    }

    #[test]
    fn centrata_con_scala_175() {
        let (x, y) = posizione_pillola(area(0, 0, 2560, 1528), 1.75, 456.0, MARGINE_LOGICO);
        assert_eq!(x, (2560 - 798) / 2);
        // Margine dell'aura 8 × 1,75: la finestra sporge di 14 px sopra lo schermo.
        assert_eq!(y, -14);
        // Un margine che non cade sul pixel (-10 × 1,75 = -17,5) si arrotonda lontano da zero.
        assert_eq!(posizione_pillola(area(0, 0, 2560, 1528), 1.75, 456.0, -10.0).1, -18);
    }

    fn aree() -> [(RettangoloFisico, f32); 4] {
        [
            (area(0, 0, 1920, 1040), 1.0),
            (area(0, 0, 2560, 1528), 1.75),
            (area(-1920, 0, 1920, 1080), 1.25),
            (area(0, 40, 1366, 728), 1.5),
        ]
    }

    #[test]
    fn misure_della_pillola_come_twocode() {
        // Decisione di Paolo (13/11): pillola più piccola, sulle proporzioni di TwoCode.
        assert_eq!((LARGHEZZA_PILLOLA, ALTEZZA_PILLOLA), (124.0, 32.0), "a riposo");
        assert_eq!((LARGHEZZA_ATTENZIONE, ALTEZZA_ATTENZIONE), (400.0, 36.0), "che chiama");
        assert_eq!((LARGHEZZA_BLOCCANTE, ALTEZZA_BLOCCANTE), (440.0, 40.0), "bloccante");
        // La finestra: la bloccante più l'aura (8) tutt'intorno.
        assert_eq!(MARGINE_AURA, 8.0);
        assert_eq!((LARGHEZZA_FINESTRA_PILLOLA, ALTEZZA_FINESTRA_PILLOLA), (456.0, 56.0));
        // La riga alta del pannello è alta quanto la pillola a riposo.
        assert_eq!(ALTEZZA_RIGA_PANNELLO, 32.0);
    }

    #[test]
    fn occhi_del_pannello_sugli_occhi_della_pillola() {
        // Gli occhi della pillola stanno al centro della sua finestra (umore tranquillo) e a metà
        // altezza della pillola; quelli del pannello al centro e a metà della riga alta. Si
        // confrontano al pixel fisico (gli arrotondamenti valgono mezzo pixel).
        for (a, scala) in aree() {
            let (px, py) = posizione_pillola(a, scala, LARGHEZZA_FINESTRA_PILLOLA, MARGINE_LOGICO);
            let (qx, qy) = posizione_pannello(a, scala);
            let centro_pillola = px as f32 + (LARGHEZZA_FINESTRA_PILLOLA * scala).round() / 2.0;
            let centro_pannello = qx as f32 + (LARGHEZZA_PANNELLO * scala).round() / 2.0;
            assert!((centro_pillola - centro_pannello).abs() <= 1.0, "x a scala {scala}: {centro_pillola} vs {centro_pannello}");
            // Occhi a metà della pillola a riposo (32: 16 sotto il bordo alto) e a metà della riga.
            let occhi_pillola = py as f32 + (MARGINE_AURA + 16.0) * scala;
            let occhi_pannello = qy as f32 + ALTEZZA_RIGA_PANNELLO / 2.0 * scala;
            assert!((occhi_pillola - occhi_pannello).abs() <= 1.0, "y a scala {scala}: {occhi_pillola} vs {occhi_pannello}");
            // Il pannello resta dentro l'area di lavoro, anche largo 800.
            assert!(qx >= a.x && qy >= a.y);
            assert!(qx + (LARGHEZZA_PANNELLO * scala).round() as i32 <= a.x + a.larghezza);
        }
    }

    #[test]
    fn da_ti_cerca_gli_occhi_del_pannello_partono_da_quelli_della_pillola() {
        // Nella pillola che chiama gli occhi stanno a sinistra, dopo l'icona della fonte: occhi a
        // 36 px dal bordo sinistro (14 + icona 14 + 8), larghi 7 e distanti 7 (ingombro 21):
        // centro a 46,5 px dal bordo. In altezza a metà della pillola (36 o 40), che è più alta
        // della riga del pannello (32). Il pannello, aperto da lì, fa partire i suoi occhi da
        // `partenza-occhi` = scarto rispetto al proprio centro, poi li porta al centro.
        let occhi_dal_bordo = 36.0 + (2.0 * 7.0 + 7.0) / 2.0;
        for (larghezza, altezza, scarto, nome) in [
            (400.0, 36.0, SCARTO_OCCHI_ATTENZIONE, "attenzione"),
            (440.0, 40.0, SCARTO_OCCHI_BLOCCANTE, "bloccante"),
        ] {
            for (a, scala) in aree() {
                let (px, py) = posizione_pillola(a, scala, LARGHEZZA_FINESTRA_PILLOLA, MARGINE_LOGICO);
                let (qx, qy) = posizione_pannello(a, scala);
                let bordo_pillola = px as f32 + (LARGHEZZA_FINESTRA_PILLOLA - larghezza) / 2.0 * scala;
                let occhi_x = bordo_pillola + occhi_dal_bordo * scala;
                let centro_pannello = qx as f32 + (LARGHEZZA_PANNELLO * scala).round() / 2.0;
                let partenza_x = centro_pannello + scarto.0 * scala;
                assert!((occhi_x - partenza_x).abs() <= 1.0, "{nome} x a scala {scala}: {occhi_x} vs {partenza_x}");
                let occhi_y = py as f32 + (MARGINE_AURA + altezza / 2.0) * scala;
                let partenza_y = qy as f32 + (ALTEZZA_RIGA_PANNELLO / 2.0 + scarto.1) * scala;
                assert!((occhi_y - partenza_y).abs() <= 1.0, "{nome} y a scala {scala}: {occhi_y} vs {partenza_y}");
            }
        }
    }

    #[test]
    fn pillola_e_pannello_attaccati_al_bordo_alto() {
        // Come il notch: il bordo superiore della pillola (sotto il margine dell'aura) tocca il
        // bordo alto dell'area di lavoro e il pannello parte da lì. Entro mezzo pixel fisico.
        for (a, scala) in aree() {
            let (_, py) = posizione_pillola(a, scala, LARGHEZZA_FINESTRA_PILLOLA, MARGINE_LOGICO);
            let bordo_pillola = py as f32 + MARGINE_AURA * scala;
            assert!((bordo_pillola - a.y as f32).abs() <= 0.5, "pillola a scala {scala}: {bordo_pillola} vs {}", a.y);
            let (_, qy) = posizione_pannello(a, scala);
            assert_eq!(qy, a.y, "pannello a scala {scala}");
        }
    }

    #[test]
    fn regione_della_pillola_sotto_il_margine_dell_aura() {
        // Finestra 456 × 56 logici. La regione è la pillola più l'aura a destra, a sinistra e
        // sotto: parte dove inizia lo schermo (sotto l'aura che sporge) e finisce sotto l'aura
        // della pillola, non in fondo alla finestra (la fascia vuota sotto ruberebbe clic).
        // A riposo: 124 + 16 = 140 di larghezza, fino a 8 + 32 + 8 = 48.
        assert_eq!(regione_pillola(456, 56, 1.0, 140.0, 48.0), (158, 8, 298, 48));
        // Che chiama: 400 + 16, fino a 8 + 36 + 8; pulsanti a destra compresi.
        assert_eq!(regione_pillola(456, 56, 1.0, 416.0, 52.0), (20, 8, 436, 52));
        // Bloccante: tutta la finestra sotto il margine.
        assert_eq!(regione_pillola(456, 56, 1.0, 456.0, 56.0), (0, 8, 456, 56));
        // A 1,75 (finestra 798 × 98): margine 14, riposo 245 × fino a 84.
        assert_eq!(regione_pillola(798, 98, 1.75, 140.0, 48.0), (276, 14, 521, 84));
        // Mai fuori dalla finestra.
        assert_eq!(regione_pillola(456, 56, 1.0, 600.0, 90.0), (0, 8, 456, 56));
        // L'alto della regione coincide col bordo alto dell'area di lavoro.
        let (_, py) = posizione_pillola(area(0, 0, 2560, 1528), 1.75, LARGHEZZA_FINESTRA_PILLOLA, MARGINE_LOGICO);
        assert_eq!(py + 14, 0);
    }

    #[test]
    fn regione_della_linea_al_bordo_alto() {
        // Finestra 456 × 56: linea 300 × 5 al centro, dal bordo alto dello schermo (y = 8).
        assert_eq!(regione_linea(456, 56, 1.0), (78, 8, 378, 13));
        // A 1,75 (798 × 98): 525 × 9, sotto i 14 px di aura che sporgono.
        assert_eq!(regione_linea(798, 98, 1.75), (136, 14, 661, 23));
    }

    #[test]
    fn pannello_largo_800_centrato() {
        assert_eq!(LARGHEZZA_PANNELLO, 800.0);
        assert_eq!(posizione_pannello(area(0, 0, 1920, 1040), 1.0), ((1920 - 800) / 2, 0));
    }

    #[test]
    fn monitor_secondario_a_sinistra() {
        let (x, y) = posizione_pillola(area(-1920, 0, 1920, 1080), 1.0, 456.0, MARGINE_LOGICO);
        assert_eq!(x, -1920 + (1920 - 456) / 2);
        assert!(x < 0);
        assert_eq!(y, -8);
    }
}
