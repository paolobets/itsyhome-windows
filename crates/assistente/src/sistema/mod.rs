//! Servizi di sistema: istanza unica, menu dell'isola e icona nella barra, scorciatoie globali,
//! avvio con il sistema, notifiche, schermo intero, inattività, modalità «ritirata» (spec §3bis),
//! apertura dei collegamenti nel browser predefinito e delle pagine di Windows, Wi-Fi, Bluetooth e
//! icona della rete (`radio`), batteria dei dispositivi Bluetooth (`batterie_bt`), terminale di una
//! sessione di Claude Code da portare davanti (`terminale`).
//!
//! Qui le decisioni pure (tempo e osservazioni arrivano dal chiamante); `servizi` le applica alla
//! pillola con un solo timer da 5 s più sveglie singole, e un sondaggio del mouse (100 ms) solo
//! mentre la pillola è ritirata.

pub mod percorso;
pub mod autoavvio;
// Fuori da Windows il modulo c'è solo per i test (la lettura e il thread sono solo Windows).
#[cfg(any(windows, test))]
pub mod batterie_bt;
pub mod browser;
pub mod inattivita;
pub mod istanza;
pub mod menu;
pub mod movimento;
pub mod non_disturbare;
pub mod notifiche;
#[cfg(windows)]
pub mod primo_piano;
pub mod radio;
pub mod ritirata;
pub mod schermo_intero;
pub mod scorciatoie;
pub mod servizi;
// Le decisioni pure (catena, scelta della finestra) si provano ovunque; fuori da Windows non le usa nessuno.
#[cfg_attr(not(windows), allow(dead_code))]
pub mod terminale;
pub mod tray;
pub mod volume;

use std::time::{Duration, Instant};

/// Come si vede la pillola.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibilita {
    Pillola,
    /// Ritirata: una linea 300 × 5 al bordo alto, con clic passanti.
    Linea,
    Nascosta,
}

/// La pillola sparisce mentre il pannello è aperto (al suo posto c'è il pannello) e sotto un'app
/// a schermo intero (sul suo monitor), salvo un permesso bloccante in attesa (`bloccante`, D1 di
/// F4): allora compare, larga, finché c'è, anche se era ritirata (sotto un video la linea non
/// c'era). Altrimenti è la linea se ritirata, sennò la pillola. Il pannello aperto non si tocca
/// mai: questa decisione riguarda solo la pillola.
pub fn pillola_visibile(schermo_intero: bool, pannello_aperto: bool, ritirata: bool, bloccante: bool) -> Visibilita {
    if pannello_aperto {
        Visibilita::Nascosta
    } else if schermo_intero {
        if bloccante {
            Visibilita::Pillola
        } else {
            Visibilita::Nascosta
        }
    } else if ritirata {
        Visibilita::Linea
    } else {
        Visibilita::Pillola
    }
}

/// Cosa fare alla finestra della pillola passando da come si vedeva (`prima`) a come deve
/// vedersi (`nuova`). Con il pannello aperto la pillola la gestisce il pannello: solo il
/// sondaggio del mouse si ferma.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Passi {
    pub ferma_sondaggio: bool,
    /// `Some(true)`: linea (anche solo per ricolorarla); `Some(false)`: via la linea.
    pub linea: Option<bool>,
    /// Timer e aggiornamenti della pillola accesi o spenti.
    pub attiva: Option<bool>,
    /// La pillola riprende l'aspetto di adesso.
    pub ridisegna: bool,
    pub nascondi: bool,
    pub mostra: bool,
    pub sopra: bool,
    pub avvia_sondaggio: bool,
}

pub fn passi(prima: Visibilita, nuova: Visibilita, pannello_aperto: bool) -> Passi {
    let cambia = prima != nuova;
    match (pannello_aperto, nuova) {
        (true, _) => Passi { ferma_sondaggio: true, ..Passi::default() },
        (false, Visibilita::Nascosta) => Passi {
            ferma_sondaggio: true,
            attiva: cambia.then_some(false),
            nascondi: cambia,
            ..Passi::default()
        },
        (false, Visibilita::Pillola) => Passi {
            ferma_sondaggio: true,
            linea: Some(false),
            attiva: cambia.then_some(true),
            ridisegna: cambia,
            mostra: cambia,
            sopra: cambia,
            ..Passi::default()
        },
        (false, Visibilita::Linea) => Passi {
            linea: Some(true),
            attiva: cambia.then_some(false),
            mostra: prima == Visibilita::Nascosta,
            sopra: cambia,
            avvia_sondaggio: cambia,
            ..Passi::default()
        },
    }
}

/// Dopo quanto schermo intero continuo la pillola sparisce (TwoCode: niente sfarfallio per un
/// video che passa un attimo a tutto schermo).
pub const NASCONDI_DOPO: Duration = Duration::from_secs(3);
/// Dopo quanto dalla fine dello schermo intero la pillola torna.
pub const MOSTRA_DOPO: Duration = Duration::from_millis(1500);

/// Isteresi dello schermo intero: la pillola si nasconde solo dopo `NASCONDI_DOPO` di schermo
/// intero continuo e torna solo dopo `MOSTRA_DOPO` senza.
#[derive(Debug, Default)]
pub struct IsteresiSchermoIntero {
    nascosta: bool,
    /// Da quando l'osservazione dice il contrario di `nascosta`.
    diverso_da: Option<Instant>,
}

impl IsteresiSchermoIntero {
    pub fn nascosta(&self) -> bool {
        self.nascosta
    }

    /// Un'osservazione. Restituisce fra quanto ricontrollare se un cambio è in sospeso (la
    /// sveglia singola che lo conferma), `None` se non c'è nulla in sospeso.
    pub fn osserva(&mut self, schermo_intero: bool, ora: Instant) -> Option<Duration> {
        if schermo_intero == self.nascosta {
            self.diverso_da = None;
            return None;
        }
        let da = *self.diverso_da.get_or_insert(ora);
        let soglia = if schermo_intero { NASCONDI_DOPO } else { MOSTRA_DOPO };
        let passato = ora.saturating_duration_since(da);
        if passato >= soglia {
            self.nascosta = schermo_intero;
            self.diverso_da = None;
            None
        } else {
            Some(soglia - passato)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn nascosta_a_schermo_intero() {
        // Pannello chiuso: sparisce, anche ritirata. Pannello aperto: la pillola è comunque
        // nascosta (c'è il pannello), e il pannello resta.
        assert_eq!(pillola_visibile(true, false, false, false), Visibilita::Nascosta);
        assert_eq!(pillola_visibile(true, false, true, false), Visibilita::Nascosta);
        assert_eq!(pillola_visibile(true, true, false, false), Visibilita::Nascosta);
    }

    #[test]
    fn visibile_normalmente() {
        assert_eq!(pillola_visibile(false, false, false, false), Visibilita::Pillola);
        assert_eq!(pillola_visibile(false, false, false, true), Visibilita::Pillola);
    }

    #[test]
    fn pannello_aperto_pillola_nascosta() {
        assert_eq!(pillola_visibile(false, true, false, false), Visibilita::Nascosta);
        assert_eq!(pillola_visibile(false, true, true, false), Visibilita::Nascosta);
        // Col pannello aperto il bloccante si vede lì: la pillola resta nascosta.
        assert_eq!(pillola_visibile(false, true, false, true), Visibilita::Nascosta);
        assert_eq!(pillola_visibile(true, true, true, true), Visibilita::Nascosta);
    }

    #[test]
    fn ritirata_mostra_la_linea() {
        assert_eq!(pillola_visibile(false, false, true, false), Visibilita::Linea);
    }

    /// Review Focus 1 (D1): a schermo intero passa solo il bloccante, e larga (la pillola, mai la
    /// linea: sotto un video la linea non c'era); senza bloccante resta nascosta.
    #[test]
    fn bloccante_passa_lo_schermo_intero() {
        assert_eq!(pillola_visibile(true, false, false, true), Visibilita::Pillola);
        assert_eq!(pillola_visibile(true, false, false, false), Visibilita::Nascosta);
        assert_eq!(pillola_visibile(true, false, true, true), Visibilita::Pillola, "ritirata: il bloccante non resta sulla linea");
    }

    use Visibilita::{Linea, Nascosta, Pillola};

    #[test]
    fn pannello_aperto_tocca_solo_il_sondaggio() {
        for prima in [Pillola, Linea, Nascosta] {
            assert_eq!(passi(prima, Nascosta, true), Passi { ferma_sondaggio: true, ..Passi::default() });
        }
    }

    #[test]
    fn dalla_pillola_alla_linea_e_ritorno() {
        let p = passi(Pillola, Linea, false);
        assert_eq!(p.linea, Some(true));
        assert_eq!(p.attiva, Some(false), "timer della pillola fermi sotto la linea");
        assert!(!p.mostra, "era già visibile");
        assert!(p.sopra && p.avvia_sondaggio && !p.ferma_sondaggio && !p.nascondi);
        let p = passi(Linea, Pillola, false);
        assert_eq!(p.linea, Some(false));
        assert_eq!(p.attiva, Some(true));
        assert!(p.ridisegna && p.mostra && p.sopra && p.ferma_sondaggio && !p.avvia_sondaggio);
    }

    #[test]
    fn linea_dopo_schermo_intero_o_pannello_si_rimostra() {
        let p = passi(Nascosta, Linea, false);
        assert!(p.mostra && p.avvia_sondaggio);
        assert_eq!(p.linea, Some(true));
    }

    #[test]
    fn verso_nascosta_ferma_tutto() {
        for prima in [Pillola, Linea] {
            let p = passi(prima, Nascosta, false);
            assert!(p.nascondi && p.ferma_sondaggio && !p.mostra && !p.avvia_sondaggio);
            assert_eq!(p.attiva, Some(false));
            assert_eq!(p.linea, None, "la linea resta pronta per quando si torna");
        }
    }

    #[test]
    fn stesso_stato_non_rifa_nulla_salvo_il_colore_della_linea() {
        let p = passi(Pillola, Pillola, false);
        assert!(!p.mostra && !p.ridisegna && !p.sopra && p.attiva.is_none());
        let p = passi(Linea, Linea, false);
        assert_eq!(p.linea, Some(true), "ricolora");
        assert!(!p.mostra && !p.sopra && !p.avvia_sondaggio && p.attiva.is_none());
        assert_eq!(passi(Nascosta, Nascosta, false), Passi { ferma_sondaggio: true, ..Passi::default() });
    }

    #[test]
    fn schermo_intero_nasconde_dopo_tre_secondi_continui() {
        let t = Instant::now();
        let mut i = IsteresiSchermoIntero::default();
        assert_eq!(i.osserva(false, t), None);
        // Primo avvistamento: ricontrolla fra 3 s, intanto resta visibile.
        assert_eq!(i.osserva(true, t), Some(NASCONDI_DOPO));
        assert!(!i.nascosta());
        assert_eq!(i.osserva(true, t + Duration::from_secs(1)), Some(Duration::from_secs(2)));
        assert!(!i.nascosta());
        assert_eq!(i.osserva(true, t + NASCONDI_DOPO), None);
        assert!(i.nascosta());
    }

    #[test]
    fn schermo_intero_breve_non_nasconde() {
        let t = Instant::now();
        let mut i = IsteresiSchermoIntero::default();
        i.osserva(true, t);
        // Finito prima dei 3 s: si riparte da zero.
        assert_eq!(i.osserva(false, t + Duration::from_secs(2)), None);
        assert!(!i.nascosta());
        assert_eq!(i.osserva(true, t + Duration::from_secs(4)), Some(NASCONDI_DOPO));
        assert!(!i.nascosta());
    }

    #[test]
    fn torna_un_secondo_e_mezzo_dopo_la_fine() {
        let t = Instant::now();
        let mut i = IsteresiSchermoIntero::default();
        i.osserva(true, t);
        i.osserva(true, t + NASCONDI_DOPO);
        assert!(i.nascosta());
        let fine = t + Duration::from_secs(10);
        assert_eq!(i.osserva(false, fine), Some(MOSTRA_DOPO));
        assert!(i.nascosta());
        // Torna a schermo intero prima di 1,5 s: resta nascosta, nulla in sospeso.
        assert_eq!(i.osserva(true, fine + Duration::from_secs(1)), None);
        assert!(i.nascosta());
        assert_eq!(i.osserva(false, fine + Duration::from_secs(2)), Some(MOSTRA_DOPO));
        assert_eq!(i.osserva(false, fine + Duration::from_secs(2) + MOSTRA_DOPO), None);
        assert!(!i.nascosta());
    }
}
