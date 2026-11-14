//! La finestra delle Impostazioni: nasce quando la si apre e viene distrutta alla chiusura (da
//! chiusa non tiene né finestra né buffer). Una sola alla volta: riaprirla la riporta davanti.
//! Dati e azioni li collega il `Ponte`; qui solo la vita della finestra.

use crate::{ponte, FinestraImpostazioni, Movimento};
use slint::winit_030::WinitWindowAccessor;
use slint::ComponentHandle;
use std::cell::{Cell, RefCell};
use std::time::Duration;

/// Titolo della finestra: il gancio di winit in `main` la lascia nella barra delle applicazioni
/// (pillola e pannello no).
pub const TITOLO: &str = "ItsyHome — Impostazioni";

thread_local! {
    static FINESTRA: RefCell<Option<FinestraImpostazioni>> = const { RefCell::new(None) };
    /// Sezione da mostrare alla prossima apertura (voce «Informazioni» della tray).
    static SEZIONE: Cell<Option<i32>> = const { Cell::new(None) };
}

/// Sezione «Informazioni» delle Impostazioni (0 Home Assistant, 1 Sistema, 2 Informazioni).
pub const SEZIONE_INFORMAZIONI: i32 = 2;

/// La prossima `apri_impostazioni` mostra questa sezione (la si chiede dalla tray, prima che il
/// pannello si chiuda e la finestra si apra).
fn alla_sezione(sezione: i32) {
    SEZIONE.with(|s| s.set(Some(sezione)));
}

/// Chiede le Impostazioni su `sezione` (voce «Informazioni» della tray). `apri` dice se una
/// finestra si è aperta davvero: se no, la sezione si dimentica (la prossima apertura normale
/// non deve finire lì).
pub fn apri_alla_sezione(sezione: i32, apri: impl FnOnce() -> bool) {
    alla_sezione(sezione);
    if !apri() {
        SEZIONE.with(|s| s.set(None));
    }
}

fn mostra_sezione_chiesta(f: &FinestraImpostazioni) {
    if let Some(s) = SEZIONE.with(Cell::take) {
        f.set_sezione(s);
    }
}

/// Apre la finestra delle Impostazioni, o porta davanti quella già aperta. Va chiamata dal ciclo
/// eventi (anche dalla tray, Task 12).
pub fn apri_impostazioni() {
    let aperta = FINESTRA.with(|f| f.borrow().as_ref().map(|w| w.as_weak()));
    if let Some(w) = aperta.and_then(|w| w.upgrade()) {
        // Ridotta a icona: prima la si rimette su, poi davanti.
        w.window().set_minimized(false);
        mostra_sezione_chiesta(&w);
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
    mostra_sezione_chiesta(&finestra);
    finestra.on_chiudi(chiudi_dopo);
    finestra.window().on_close_requested(|| {
        chiudi_dopo();
        slint::CloseRequestResponse::HideWindow
    });
    if let Err(e) = finestra.show() {
        eprintln!("impostazioni non mostrate: {e}");
        return;
    }
    // Spessori sul pixel fisico: la scala di adesso e, se la finestra cambia monitor, la nuova.
    crate::finestre::scrivi_scala(&finestra.global::<crate::Tema>(), finestra.window().scale_factor());
    let debole = finestra.as_weak();
    finestra.window().on_winit_window_event(move |_, evento| {
        if let slint::winit_030::winit::event::WindowEvent::ScaleFactorChanged { scale_factor, .. } = evento {
            if let Some(f) = debole.upgrade() {
                crate::finestre::scrivi_scala(&f.global::<crate::Tema>(), *scale_factor as f32);
            }
        }
        slint::winit_030::EventResult::Propagate
    });
    FINESTRA.with(|f| *f.borrow_mut() = Some(finestra));
}

/// Il movimento ridotto cambiato a finestra aperta (controllo da 5 s dei servizi).
pub fn movimento(ridotto: bool) {
    FINESTRA.with(|f| {
        if let Some(w) = f.borrow().as_ref() {
            if w.global::<Movimento>().get_ridotto() != ridotto {
                w.global::<Movimento>().set_ridotto(ridotto);
            }
        }
    });
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

#[cfg(test)]
mod test {
    use super::*;

    fn in_attesa() -> Option<i32> {
        SEZIONE.with(Cell::get)
    }

    #[test]
    fn sezione_dimenticata_se_non_si_apre_nulla() {
        // La tray chiede «Informazioni» ma le Impostazioni non si aprono (nessuno sa aprirle):
        // la prossima apertura normale non deve finire su «Informazioni».
        apri_alla_sezione(SEZIONE_INFORMAZIONI, || false);
        assert_eq!(in_attesa(), None);
    }

    #[test]
    fn sezione_tenuta_per_la_finestra_che_si_apre() {
        let mut vista = None;
        apri_alla_sezione(SEZIONE_INFORMAZIONI, || {
            vista = in_attesa();
            true
        });
        assert_eq!(vista, Some(SEZIONE_INFORMAZIONI), "l'apertura la trova");
        SEZIONE.with(|s| s.set(None));
    }
}
