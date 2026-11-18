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
//!
//! Lo stesso gancio ricorda l'ultima finestra in primo piano il cui eseguibile è un terminale
//! (`terminale::e_terminale`): il ripiego di «Apri il terminale» (M3, Task 10) quando la catena
//! dei processi di una sessione non porta a nessuna finestra. Il nome dell'eseguibile si legge con
//! `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` + `QueryFullProcessImageNameW` (basta il
//! permesso limitato; fallisce per i processi protetti, che non sono terminali).

use std::cell::Cell;
use std::marker::PhantomData;
use super::terminale::e_terminale;
use windows_sys::Win32::Foundation::{CloseHandle, HWND};
use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION};
use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetWindowThreadProcessId, IsWindow, EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
};

thread_local! {
    /// Chi avvisare: la callback di sistema non porta dati, ma arriva sul thread che ha messo il
    /// gancio, quindi basta un valore di questo thread.
    static AL_CAMBIO: Cell<Option<fn()>> = const { Cell::new(None) };
    /// L'ultima finestra in primo piano il cui eseguibile è un terminale.
    static ULTIMO_TERMINALE: Cell<Option<isize>> = const { Cell::new(None) };
}

/// Il nome del file di un percorso di Windows (da «...\pwsh.exe» a «pwsh.exe»).
fn nome_file(percorso: &str) -> &str {
    percorso.rsplit(['\\', '/']).next().unwrap_or(percorso)
}

/// Il nome dell'eseguibile del processo che possiede la finestra, se si riesce a leggere.
fn exe_della_finestra(hwnd: HWND) -> Option<String> {
    let mut pid = 0u32;
    // SAFETY: lettura del PID di una finestra; con un handle non valido restituisce zero.
    if unsafe { GetWindowThreadProcessId(hwnd, &mut pid) } == 0 || pid == 0 {
        return None;
    }
    // SAFETY: handle chiuso sempre; il buffer ha `n` caratteri e il sistema ne scrive al più tanti.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h == 0 {
            return None;
        }
        let mut buf = vec![0u16; 1024];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut n) != 0;
        CloseHandle(h);
        if !ok {
            return None;
        }
        String::from_utf16(&buf[..n as usize]).ok().map(|p| nome_file(&p).to_owned())
    }
}

/// L'ultima finestra di un terminale che è stata in primo piano, se esiste ancora.
#[cfg_attr(test, allow(dead_code))]
pub fn ultimo_terminale() -> Option<isize> {
    let h = ULTIMO_TERMINALE.with(Cell::get)?;
    // SAFETY: `IsWindow` accetta qualunque valore.
    (unsafe { IsWindow(h) } != 0).then_some(h)
}

/// Il gancio installato: toglierlo (`Drop`) spetta al thread che lo ha messo, per questo non è
/// `Send`.
pub struct GanciPrimoPiano {
    gancio: HWINEVENTHOOK,
    _stesso_thread: PhantomData<*const ()>,
}

unsafe extern "system" fn callback(_: HWINEVENTHOOK, _: u32, hwnd: HWND, _: i32, _: i32, _: u32, _: u32) {
    if hwnd != 0 && exe_della_finestra(hwnd).is_some_and(|e| e_terminale(&e)) {
        ULTIMO_TERMINALE.with(|c| c.set(Some(hwnd)));
    }
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
        ULTIMO_TERMINALE.with(|c| c.set(None));
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn il_nome_del_file_da_un_percorso() {
        assert_eq!(nome_file(r"C:\Program Files\PowerShell\pwsh.exe"), "pwsh.exe");
        assert_eq!(nome_file("cmd.exe"), "cmd.exe");
        assert!(e_terminale(nome_file(r"C:\Windows\System32\conhost.exe")));
    }

    #[test]
    fn il_gancio_si_installa_e_si_toglie() {
        let g = ascolta(|| {}).expect("gancio installato");
        assert!(AL_CAMBIO.with(|c| c.get().is_some()));
        drop(g);
        assert!(AL_CAMBIO.with(|c| c.get().is_none()));
    }
}
