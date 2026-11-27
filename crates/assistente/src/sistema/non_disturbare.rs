//! «Non disturbare» (F4, decisioni di Paolo del 27/11): la pillola resta a riposo (piccola, con
//! gli occhi e i lati) e non si allarga per attività, fatti o avvisi; passa solo un permesso
//! bloccante. Tre fonti, basta una:
//! - l'interruttore manuale del menu (vassoio e «⋯», stessa voce): vale fino all'uscita da isola,
//!   non si salva su disco (D2);
//! - un'app a schermo intero sul monitor della pillola (la stessa isteresi che la nasconde,
//!   `sistema::IsteresiSchermoIntero`);
//! - «Non disturbare» di Windows, seguito da un thread suo (`OsservaNonDisturbare`, come il
//!   thread `rete` di `radio`): lettura iniziale e poi solo a ogni `NotificationModeChanged`,
//!   consegnata al ciclo eventi; nessun timer, nessuna chiamata WinRT sul thread dell'interfaccia
//!   (la lettura esce dal processo). Una lettura fallita vale spento.
//!
//! Lettura di «Non disturbare» di Windows (D4: solo API documentate, niente registro né WNF).
//! Pagine lette su learn.microsoft.com il 27/11/2026:
//! - `SHQueryUserNotificationState` e `QUERY_USER_NOTIFICATION_STATE` (shellapi.h): i valori
//!   parlano di schermo intero e presentazione (`QUNS_BUSY`, `QUNS_RUNNING_D3D_FULL_SCREEN`,
//!   `QUNS_PRESENTATION_MODE`), schermo bloccato (`QUNS_NOT_PRESENT`), della prima ora dopo il
//!   primo accesso o un aggiornamento (`QUNS_QUIET_TIME`) e di un'app dello Store (`QUNS_APP`):
//!   nessuno è «Non disturbare», quindi non si usa;
//! - `Windows.UI.Shell.FocusSessionManager` e la guida «Detect and react to focus session state»
//!   (aggiornata il 23/10/2026): la sessione di concentrazione «isn't the same as the Do not
//!   disturb state», quindi non si usa;
//! - `Windows.UI.Notifications.ToastNotificationMode` e
//!   `ToastNotificationManagerForUser.NotificationMode` (da `ToastNotificationManager.GetDefault`):
//!   «the current notification mode for the associated user which determines which notifications
//!   are displayed»; `PriorityOnly` è documentato come «similar to the Do Not Disturb mode in
//!   Windows 11», `AlarmsOnly` lascia passare solo le sveglie, `Unrestricted` tutto. È l'API
//!   usata qui: «Non disturbare» è acceso se il modo non è `Unrestricted` (anche `AlarmsOnly`
//!   limita le notifiche più di «Non disturbare»). Requisito: Windows 11 10.0.23504 o successivo
//!   (UniversalApiContract v15); sui sistemi più vecchi la proprietà manca, la lettura fallisce e
//!   arriva `None` (si va avanti con le altre due fonti);
//! - `ToastNotificationManagerForUser.NotificationModeChanged` (letta il 27/11/2026): «Occurs
//!   when the value of ToastNotificationManagerForUser.NotificationMode property changes»,
//!   `TypedEventHandler<ToastNotificationManagerForUser, IInspectable>`, si toglie col suo
//!   token (`RemoveNotificationModeChanged`); stessi requisiti. La pagina non dice su che thread
//!   arriva: il gestore fa solo `unpark` del thread `non-disturbare`.

/// Lo stato di «Non disturbare»: acceso se almeno una delle tre fonti lo è.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct NonDisturbare {
    manuale: bool,
    windows: bool,
    schermo_intero: bool,
}

impl NonDisturbare {
    pub fn attivo(&self) -> bool {
        self.manuale || self.windows || self.schermo_intero
    }

    /// L'interruttore del menu (solo Windows: fuori non c'è menu).
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn imposta_manuale(&mut self, acceso: bool) {
        self.manuale = acceso;
    }

    /// La spunta della voce del menu: solo l'interruttore, non le fonti automatiche (scegliere la
    /// voce lo inverte).
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn manuale(&self) -> bool {
        self.manuale
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn da_windows(&mut self, acceso: bool) {
        self.windows = acceso;
    }

    /// Una lettura di «Non disturbare» di Windows (dal thread `non-disturbare`).
    #[cfg_attr(not(windows), allow(dead_code))]
    /// `None` (non si sa più) vale spento: un «acceso» vecchio non resta appeso.
    pub fn da_lettura_windows(&mut self, letto: Option<bool>) {
        self.da_windows(letto.unwrap_or(false));
    }

    pub fn da_schermo_intero(&mut self, acceso: bool) {
        self.schermo_intero = acceso;
    }
}

/// La lettura `letto` va consegnata? Sì se diversa dall'ultima consegnata (`ultimo`, aggiornato).
#[cfg_attr(not(windows), allow(dead_code))]
fn da_consegnare(ultimo: &mut Option<Option<bool>>, letto: Option<bool>) -> bool {
    if *ultimo == Some(letto) {
        return false;
    }
    *ultimo = Some(letto);
    true
}

#[cfg(windows)]
pub use osserva::OsservaNonDisturbare;

/// Il thread `non-disturbare` (come il thread `rete` di `radio`): COM/WinRT multithread, mai sul
/// thread dell'interfaccia (la lettura esce dal processo e potrebbe attendere).
#[cfg(windows)]
mod osserva {
    use super::{acceso_dal_modo, da_consegnare};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use windows::Foundation::TypedEventHandler;
    use windows::UI::Notifications::{ToastNotificationManager, ToastNotificationManagerForUser};
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

    pub struct OsservaNonDisturbare {
        ferma: Arc<AtomicBool>,
        thread: Option<std::thread::Thread>,
    }

    impl OsservaNonDisturbare {
        /// Avvia il thread: la prima lettura arriva subito a `consegna`, poi una a ogni cambio.
        /// `consegna` è chiamata dal thread `non-disturbare`: deve solo accodare al ciclo eventi.
        pub fn avvia(consegna: fn(Option<bool>)) -> OsservaNonDisturbare {
            let ferma = Arc::new(AtomicBool::new(false));
            let f = ferma.clone();
            let thread = std::thread::Builder::new().name("non-disturbare".into()).spawn(move || gira(consegna, &f));
            let thread = match thread {
                Ok(t) => Some(t.thread().clone()),
                Err(e) => {
                    eprintln!("«Non disturbare» di Windows non osservato: {e}");
                    None
                }
            };
            OsservaNonDisturbare { ferma, thread }
        }
    }

    impl Drop for OsservaNonDisturbare {
        /// Il thread toglie il gestore, chiude COM e finisce (nessuna attesa qui).
        fn drop(&mut self) {
            self.ferma.store(true, Ordering::Relaxed);
            if let Some(t) = &self.thread {
                t.unpark();
            }
        }
    }

    /// Il modo di adesso: `None` se la lettura fallisce (Windows prima di 10.0.23504: la
    /// proprietà manca).
    fn leggi(gestore: &ToastNotificationManagerForUser) -> Option<bool> {
        gestore.NotificationMode().ok().map(acceso_dal_modo)
    }

    fn gira(consegna: fn(Option<bool>), ferma: &AtomicBool) {
        // SAFETY: COM/WinRT su questo thread, multithread; chiuso alla fine.
        if let Err(e) = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok() {
            eprintln!("non-disturbare: COM non inizializzato: {e}");
            return;
        }
        if let Ok(notifiche) = ToastNotificationManager::GetDefault() {
            segui(&notifiche, consegna, ferma);
        } else {
            consegna(None);
        }
        // SAFETY: bilancia il `CoInitializeEx` riuscito qui sopra, sullo stesso thread.
        unsafe { CoUninitialize() };
    }

    /// Legge, consegna i cambi e dorme fino al prossimo `NotificationModeChanged` (o alla fine).
    fn segui(notifiche: &ToastNotificationManagerForUser, consegna: fn(Option<bool>), ferma: &AtomicBool) {
        // Il gestore arriva su un thread di Windows: sveglia soltanto questo.
        let io = std::thread::current();
        let gestore = TypedEventHandler::new(move |_, _| {
            io.unpark();
            Ok(())
        });
        let cookie = match notifiche.NotificationModeChanged(&gestore) {
            Ok(c) => c,
            Err(_) => {
                // Senza l'evento i cambi non si vedrebbero: meglio «non si sa» di un valore vecchio.
                consegna(None);
                return;
            }
        };
        let mut ultimo = None;
        while !ferma.load(Ordering::Relaxed) {
            let letto = leggi(notifiche);
            if da_consegnare(&mut ultimo, letto) {
                consegna(letto);
            }
            std::thread::park();
        }
        let _ = notifiche.RemoveNotificationModeChanged(cookie);
    }
}

/// Il modo delle notifiche di Windows dice «Non disturbare» acceso? Sì se non passano tutte:
/// `PriorityOnly` («simile a Non disturbare» per la documentazione) e `AlarmsOnly` (più stretto).
#[cfg(windows)]
fn acceso_dal_modo(modo: windows::UI::Notifications::ToastNotificationMode) -> bool {
    modo != windows::UI::Notifications::ToastNotificationMode::Unrestricted
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn attivo_se_una_delle_tre() {
        for i in 0..8u8 {
            let (manuale, windows, schermo_intero) = (i & 1 != 0, i & 2 != 0, i & 4 != 0);
            let mut nd = NonDisturbare::default();
            nd.imposta_manuale(manuale);
            nd.da_windows(windows);
            nd.da_schermo_intero(schermo_intero);
            assert_eq!(nd.attivo(), manuale || windows || schermo_intero, "manuale {manuale}, windows {windows}, schermo intero {schermo_intero}");
            assert_eq!(nd.manuale(), manuale, "la spunta dice solo l'interruttore");
        }
    }

    /// m1: una lettura fallita dopo un «acceso» non lascia acceso «Non disturbare» di Windows
    /// (non si sa più: vale spento, restano le altre due fonti).
    #[test]
    fn lettura_fallita_azzera_windows() {
        let mut nd = NonDisturbare::default();
        nd.da_lettura_windows(Some(true));
        assert!(nd.attivo());
        nd.da_lettura_windows(None);
        assert!(!nd.attivo(), "None dopo Some(true) azzera");
        nd.imposta_manuale(true);
        nd.da_lettura_windows(None);
        assert!(nd.attivo(), "l'interruttore resta");
        nd.imposta_manuale(false);
        nd.da_lettura_windows(Some(false));
        assert!(!nd.attivo());
    }

    /// Il thread consegna solo i cambi (anche verso `None`), la prima lettura sempre.
    #[test]
    fn si_consegnano_solo_i_cambi() {
        let mut ultimo = None;
        assert!(da_consegnare(&mut ultimo, Some(false)), "la prima sempre");
        assert!(!da_consegnare(&mut ultimo, Some(false)));
        assert!(da_consegnare(&mut ultimo, Some(true)));
        assert!(da_consegnare(&mut ultimo, None));
        assert!(!da_consegnare(&mut ultimo, None));
        assert!(da_consegnare(&mut ultimo, Some(true)));
    }

    /// Il thread vero consegna la prima lettura (qualunque sia su questo PC) e si ferma col drop.
    #[cfg(windows)]
    #[test]
    fn il_thread_consegna_la_prima_lettura() {
        use std::sync::atomic::{AtomicU8, Ordering};
        use std::time::{Duration, Instant};
        // 0 = niente ancora, 1 = None, 2 = spento, 3 = acceso.
        static ARRIVATA: AtomicU8 = AtomicU8::new(0);
        fn consegna(letto: Option<bool>) {
            ARRIVATA.store(match letto { None => 1, Some(false) => 2, Some(true) => 3 }, Ordering::SeqCst);
        }
        let osserva = OsservaNonDisturbare::avvia(consegna);
        let fine = Instant::now() + Duration::from_secs(10);
        while ARRIVATA.load(Ordering::SeqCst) == 0 && Instant::now() < fine {
            std::thread::sleep(Duration::from_millis(10));
        }
        drop(osserva);
        assert_ne!(ARRIVATA.load(Ordering::SeqCst), 0, "la prima lettura arriva");
    }

    #[cfg(windows)]
    #[test]
    fn acceso_se_le_notifiche_non_passano_tutte() {
        use windows::UI::Notifications::ToastNotificationMode;
        assert!(!acceso_dal_modo(ToastNotificationMode::Unrestricted));
        assert!(acceso_dal_modo(ToastNotificationMode::PriorityOnly), "«simile a Non disturbare» per la documentazione");
        assert!(acceso_dal_modo(ToastNotificationMode::AlarmsOnly));
    }
}
