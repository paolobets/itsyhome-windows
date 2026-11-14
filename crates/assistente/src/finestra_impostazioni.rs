//! La finestra delle Impostazioni: nasce quando la si apre e viene distrutta alla chiusura (da
//! chiusa non tiene né finestra né buffer). Una sola alla volta: riaprirla la riporta davanti.
//! Dati e azioni li collega il `Ponte`; qui solo la vita della finestra.

use crate::{ponte, FinestraImpostazioni, Movimento};
use slint::winit_030::WinitWindowAccessor;
use slint::ComponentHandle;
use std::cell::RefCell;
use std::time::Duration;

/// Titolo della finestra: il gancio di winit in `main` la lascia nella barra delle applicazioni
/// (pillola e pannello no).
pub const TITOLO: &str = "ItsyHome — Impostazioni";

thread_local! {
    static FINESTRA: RefCell<Option<FinestraImpostazioni>> = const { RefCell::new(None) };
}

/// Apre la finestra delle Impostazioni, o porta davanti quella già aperta. Va chiamata dal ciclo
/// eventi (anche dalla tray, Task 12).
pub fn apri_impostazioni() {
    let aperta = FINESTRA.with(|f| f.borrow().as_ref().map(|w| w.as_weak()));
    if let Some(w) = aperta.and_then(|w| w.upgrade()) {
        // Ridotta a icona: prima la si rimette su, poi davanti.
        w.window().set_minimized(false);
        let _ = w.show();
        w.window().with_winit_window(|w| w.focus_window());
        return;
    }
    let finestra = match FinestraImpostazioni::new() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("impostazioni non aperte: {e}");
            return;
        }
    };
    finestra.global::<Movimento>().set_ridotto(crate::sistema::servizi::movimento_ridotto());
    if let Some(p) = ponte::ponte() {
        p.collega_finestra(&finestra);
    }
    finestra.on_chiudi(chiudi_dopo);
    finestra.window().on_close_requested(|| {
        chiudi_dopo();
        slint::CloseRequestResponse::HideWindow
    });
    if let Err(e) = finestra.show() {
        eprintln!("impostazioni non mostrate: {e}");
        return;
    }
    FINESTRA.with(|f| *f.borrow_mut() = Some(finestra));
}

/// Fuori dal gestore in corso: nasconde e distrugge la finestra.
fn chiudi_dopo() {
    slint::Timer::single_shot(Duration::ZERO, || {
        let finestra = FINESTRA.with(|f| f.borrow_mut().take());
        if let Some(f) = finestra {
            let _ = f.hide();
            if let Some(p) = ponte::ponte() {
                p.finestra_chiusa();
            }
            drop(f);
        }
    });
}
