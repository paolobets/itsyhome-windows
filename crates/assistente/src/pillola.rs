//! La pillola: umore e ritaglio della finestra.
//!
//! La finestra della pillola è sempre larga quanto la pillola più larga e la pillola le sta al
//! centro, quindi cambiando umore nulla si sposta. Per non rubare clic, la finestra è ritagliata
//! alla fascia visibile: si allarga subito quando la pillola cresce e si stringe solo a fine
//! animazione quando la pillola cala.

use crate::finestre;
use crate::umore::Umore;
use crate::Pillola;
use slint::ComponentHandle;
use std::cell::Cell;
use std::time::Duration;

/// L'animazione della larghezza in pillola.slint dura 220 ms: si stringe dopo, con un margine.
const DOPO_ANIMAZIONE: Duration = Duration::from_millis(260);

thread_local! {
    /// Larghezza logica del ritaglio in vigore (la riapplica chi cambia schermo o DPI).
    static RITAGLIO: Cell<f32> = const { Cell::new(0.0) };
    /// Conta i cambi d'umore: un restringimento rimandato vale solo se nel frattempo non ne è
    /// arrivato un altro.
    static GIRO: Cell<u64> = const { Cell::new(0) };
}

fn ritaglia(ui: &Pillola, larghezza: f32) {
    RITAGLIO.with(|r| r.set(larghezza));
    finestre::ritaglia_al_centro(ui.window(), larghezza);
}

pub fn imposta_umore(ui: &Pillola, umore: Umore, messaggio: &str) {
    let prima = RITAGLIO.with(Cell::get);
    ui.set_umore(umore.come_stringa().into());
    ui.set_messaggio(messaggio.into());
    let dopo = ui.get_larghezza_visibile();
    let giro = GIRO.with(|g| {
        g.set(g.get() + 1);
        g.get()
    });
    if dopo >= prima {
        ritaglia(ui, dopo);
    } else {
        let debole = ui.as_weak();
        slint::Timer::single_shot(DOPO_ANIMAZIONE, move || {
            if GIRO.with(Cell::get) != giro {
                return;
            }
            if let Some(ui) = debole.upgrade() {
                ritaglia(&ui, ui.get_larghezza_visibile());
            }
        });
    }
}

/// Finestra appena nata, spostata, DPI o monitor cambiati: posizione e ritaglio di nuovo.
pub fn al_cambio_schermo(ui: &Pillola) {
    finestre::posiziona_pillola(ui.window());
    finestre::ritaglia_al_centro(ui.window(), RITAGLIO.with(Cell::get));
}
