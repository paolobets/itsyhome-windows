//! «Non disturbare» (F4, decisioni di Paolo del 27/11): la pillola resta a riposo (piccola, con
//! gli occhi e i lati) e non si allarga per attività, fatti o avvisi; passa solo un permesso
//! bloccante. Tre fonti, basta una:
//! - l'interruttore manuale del menu (vassoio e «⋯», stessa voce): vale fino all'uscita da isola,
//!   non si salva su disco (D2);
//! - un'app a schermo intero sul monitor della pillola (la stessa isteresi che la nasconde,
//!   `sistema::IsteresiSchermoIntero`);
//! - «Non disturbare» di Windows, letto al giro dei 5 s dei servizi (nessun timer nuovo).
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
//!   `windows_attivo` dice `None` (si va avanti con le altre due fonti).

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

    pub fn da_windows(&mut self, acceso: bool) {
        self.windows = acceso;
    }

    pub fn da_schermo_intero(&mut self, acceso: bool) {
        self.schermo_intero = acceso;
    }
}

/// «Non disturbare» di Windows adesso: `None` se non si sa (sistema senza l'API documentata,
/// lettura fallita, fuori da Windows).
#[cfg(windows)]
pub fn windows_attivo() -> Option<bool> {
    use windows::UI::Notifications::ToastNotificationManager;
    // Su un Windows senza la proprietà (prima di 10.0.23504) la chiamata fallisce: `None`.
    let modo = ToastNotificationManager::GetDefault().ok()?.NotificationMode().ok()?;
    Some(acceso_dal_modo(modo))
}

/// Il modo delle notifiche di Windows dice «Non disturbare» acceso? Sì se non passano tutte:
/// `PriorityOnly` («simile a Non disturbare» per la documentazione) e `AlarmsOnly` (più stretto).
#[cfg(windows)]
fn acceso_dal_modo(modo: windows::UI::Notifications::ToastNotificationMode) -> bool {
    modo != windows::UI::Notifications::ToastNotificationMode::Unrestricted
}

#[cfg(not(windows))]
pub fn windows_attivo() -> Option<bool> {
    None
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

    #[cfg(windows)]
    #[test]
    fn acceso_se_le_notifiche_non_passano_tutte() {
        use windows::UI::Notifications::ToastNotificationMode;
        assert!(!acceso_dal_modo(ToastNotificationMode::Unrestricted));
        assert!(acceso_dal_modo(ToastNotificationMode::PriorityOnly), "«simile a Non disturbare» per la documentazione");
        assert!(acceso_dal_modo(ToastNotificationMode::AlarmsOnly));
    }
}
