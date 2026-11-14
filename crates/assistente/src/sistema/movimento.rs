//! Movimento ridotto (spec §3bis, Leggibilità): l'impostazione di Windows «Effetti di animazione»
//! (`SPI_GETCLIENTAREAANIMATION`), portata nel global Slint `Movimento.ridotto`. Con il movimento
//! ridotto niente pulsazioni né scivolamenti: aura ferma, pillola e occhi che cambiano di colpo,
//! pannello che compare solo in dissolvenza breve. Fuori da Windows (per ora): mai ridotto.
//!
//! Si legge all'avvio, a ogni apertura del pannello e delle Impostazioni e nel controllo da 5 s
//! dei servizi (una chiamata di sistema, nessun timer in più), che lo porta anche nel pannello e
//! nelle Impostazioni se sono aperti: un cambio fatto mentre l'app gira
//! arriva entro 5 s.

/// Ridotto solo se Windows dice esplicitamente che le animazioni sono spente; se la lettura non
/// riesce (o fuori da Windows) si tengono le animazioni.
pub fn ridotto_da(animazioni: Option<bool>) -> bool {
    animazioni == Some(false)
}

/// Il movimento ridotto chiesto adesso dal sistema.
pub fn ridotto() -> bool {
    ridotto_da(animazioni_di_sistema())
}

#[cfg(windows)]
fn animazioni_di_sistema() -> Option<bool> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION};
    let mut attive: i32 = 1;
    // SAFETY: `attive` è un BOOL locale valido per tutta la chiamata, come chiede
    // SPI_GETCLIENTAREAANIMATION (pvParam = *BOOL, uiParam = 0).
    let ok = unsafe { SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, (&mut attive as *mut i32).cast(), 0) };
    (ok != 0).then_some(attive != 0)
}

#[cfg(not(windows))]
fn animazioni_di_sistema() -> Option<bool> {
    None
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn ridotto_solo_se_windows_spegne_le_animazioni() {
        assert!(ridotto_da(Some(false)), "«Effetti di animazione» spento");
        assert!(!ridotto_da(Some(true)));
        assert!(!ridotto_da(None), "lettura non riuscita o fuori Windows: animazioni come sempre");
    }

    /// SPI_GETCLIENTAREAANIMATION deve rispondere: un parametro o un flag sbagliati la fanno
    /// fallire (`None`) e il test con lei.
    #[cfg(windows)]
    #[test]
    fn windows_risponde_sulle_animazioni() {
        assert!(animazioni_di_sistema().is_some(), "SPI_GETCLIENTAREAANIMATION deve rispondere");
    }
}
