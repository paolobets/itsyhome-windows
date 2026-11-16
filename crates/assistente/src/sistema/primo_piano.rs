//! Il gancio sulla finestra in primo piano: lo schermo intero si coglie appena cambia, non al
//! giro dei 5 s. Solo Windows (fuori da Windows il modulo non esiste: `servizi` non lo chiama).
//!
//! Documentazione Microsoft letta prima di scrivere (learn.microsoft.com, 16/11/2026):
//!
//! `SetWinEventHook` (winuser.h, user32):
//! - firma `SetWinEventHook(eventMin, eventMax, hmodWinEventProc, pfnWinEventProc, idProcess,
//!   idThread, dwFlags) -> HWINEVENTHOOK`; zero se non riesce;
//! - `WINEVENT_OUTOFCONTEXT`: la funzione NON è mappata nel processo che genera l'evento; il
//!   sistema accoda gli eventi (asincrono, ma in ordine) e `hmodWinEventProc` è NULL;
//! - «per gli eventi out-of-context l'evento è consegnato sullo stesso thread che ha chiamato
//!   `SetWinEventHook`», e quel thread **deve avere un ciclo di messaggi** per riceverli (qui il
//!   thread dell'interfaccia, che ha quello di winit/Slint);
//! - `WINEVENT_SKIPOWNPROCESS`: niente eventi generati da questo processo (la pillola, il pannello);
//! - `idProcess = 0` e `idThread = 0`: eventi da tutti i processi e thread del desktop corrente;
//! - la callback può rientrare mentre ne gestisce un'altra: la nostra si limita a chiamare
//!   `al_cambio`, che non pompa messaggi.
//!
//! `UnhookWinEvent` (winuser.h):
//! - `UnhookWinEvent(hook) -> BOOL`; fallisce se l'handle è nullo o non valido, se è già stato
//!   tolto, o se è chiamata **da un thread diverso** da quello che ha installato il gancio;
//! - se il thread finisce, il sistema la chiama da sé; dopo il ritorno la callback non arriva più.
//!
//! Scelte: solo `EVENT_SYSTEM_FOREGROUND` (scatta quando cambia la finestra in primo piano, poche
//! volte al minuto); niente `EVENT_OBJECT_LOCATIONCHANGE`, che scatta a ogni movimento del cursore
//! e sveglierebbe l'app di continuo. L'F11 di un browser non cambia la finestra in primo piano:
//! per quello resta il controllo da 5 s.

use std::cell::Cell;
use std::marker::PhantomData;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
};

thread_local! {
    /// Chi avvisare: la callback di sistema non porta dati, ma arriva sul thread che ha messo il
    /// gancio, quindi basta un valore di questo thread.
    static AL_CAMBIO: Cell<Option<fn()>> = const { Cell::new(None) };
}

/// Il gancio installato: toglierlo (`Drop`) spetta al thread che lo ha messo, per questo non è
/// `Send`.
pub struct GanciPrimoPiano {
    gancio: HWINEVENTHOOK,
    _stesso_thread: PhantomData<*const ()>,
}

unsafe extern "system" fn callback(_: HWINEVENTHOOK, _: u32, _: HWND, _: i32, _: i32, _: u32, _: u32) {
    if let Some(f) = AL_CAMBIO.with(Cell::get) {
        f();
    }
}

/// Chiama `al_cambio` (sul thread dell'interfaccia) a ogni cambio della finestra in primo piano.
/// `None` se Windows rifiuta il gancio: resta il controllo da 5 s.
pub fn ascolta(al_cambio: fn()) -> Option<GanciPrimoPiano> {
    AL_CAMBIO.with(|c| c.set(Some(al_cambio)));
    // SAFETY: la callback è una funzione `extern "system"` statica; out-of-context quindi nessun
    // modulo da mappare (hmod = 0); l'handle restituito si toglie in `Drop`, sullo stesso thread.
    let gancio = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            0,
            Some(callback),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        )
    };
    if gancio == 0 {
        AL_CAMBIO.with(|c| c.set(None));
        return None;
    }
    Some(GanciPrimoPiano { gancio, _stesso_thread: PhantomData })
}

impl Drop for GanciPrimoPiano {
    fn drop(&mut self) {
        // SAFETY: handle valido (non nullo, tolto una volta sola), stesso thread dell'installazione.
        unsafe {
            UnhookWinEvent(self.gancio);
        }
        AL_CAMBIO.with(|c| c.set(None));
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn il_gancio_si_installa_e_si_toglie() {
        let g = ascolta(|| {}).expect("gancio installato");
        assert!(AL_CAMBIO.with(|c| c.get().is_some()));
        drop(g);
        assert!(AL_CAMBIO.with(|c| c.get().is_none()));
    }
}
