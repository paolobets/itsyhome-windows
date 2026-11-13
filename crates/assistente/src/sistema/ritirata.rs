//! La ritirata: la pillola diventa una linea 300 × 4 al bordo alto, con clic passanti. Entra e
//! esce con il clic destro sulla pillola, la tray o Ctrl+Alt+Maiusc+I; esce anche tenendo il
//! mouse sulla linea per 450 ms (TwoCode) e da sola all'arrivo di un bloccante. Non si ricorda al
//! riavvio. Logica pura: il tempo e la posizione del mouse arrivano dal chiamante.

use crate::attenzione::Livello;
use crate::finestre::RettangoloFisico;
use std::time::{Duration, Instant};

/// Quanto tenere il mouse sulla linea perché la pillola torni.
pub const RITORNO_DOPO: Duration = Duration::from_millis(450);
/// Ogni quanto si guarda dov'è il mouse, solo mentre la pillola è ritirata.
pub const SONDAGGIO_MOUSE: Duration = Duration::from_millis(100);

/// Linea della ritirata, in pixel logici.
pub const LARGHEZZA_LINEA: f32 = 300.0;
pub const ALTEZZA_LINEA: f32 = 4.0;
/// Zona sensibile al mouse attorno alla linea (TwoCode: 340 × 10), dal bordo alto in giù: al
/// bordo dello schermo il mouse ci arriva da solo spingendolo in alto.
pub const LARGHEZZA_ZONA: f32 = 340.0;
pub const ALTEZZA_ZONA: f32 = 10.0;

#[derive(Debug, Default)]
pub struct Ritirata {
    attiva: bool,
    /// Da quando il mouse sta sulla linea.
    sopra_da: Option<Instant>,
}

impl Ritirata {
    pub fn attiva(&self) -> bool {
        self.attiva
    }

    /// Clic destro, tray o scorciatoia.
    pub fn commuta(&mut self) {
        self.attiva = !self.attiva;
        self.sopra_da = None;
    }

    /// La pillola torna (scorciatoia «apri», tray «Apri»).
    pub fn esci(&mut self) {
        self.attiva = false;
        self.sopra_da = None;
    }

    /// È arrivata una voce nuova di questo livello: un bloccante fa uscire dalla ritirata.
    pub fn arriva(&mut self, livello: Livello) {
        if livello == Livello::Bloccante {
            self.esci();
        }
    }

    /// Un sondaggio del mouse. Vero se la pillola deve tornare adesso.
    pub fn mouse(&mut self, sopra: bool, ora: Instant) -> bool {
        if !self.attiva || !sopra {
            self.sopra_da = None;
            return false;
        }
        let da = *self.sopra_da.get_or_insert(ora);
        if ora.saturating_duration_since(da) >= RITORNO_DOPO {
            self.esci();
            true
        } else {
            false
        }
    }
}

/// La zona sensibile (pixel fisici) attorno alla linea, al centro in alto di `area`.
pub fn zona_linea(area: RettangoloFisico, scala: f32) -> RettangoloFisico {
    let larghezza = (LARGHEZZA_ZONA * scala).round() as i32;
    let altezza = (ALTEZZA_ZONA * scala).round() as i32;
    RettangoloFisico { x: area.x + (area.larghezza - larghezza) / 2, y: area.y, larghezza, altezza }
}

pub fn dentro(zona: RettangoloFisico, (x, y): (i32, i32)) -> bool {
    x >= zona.x && x < zona.x + zona.larghezza && y >= zona.y && y < zona.y + zona.altezza
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn commuta_entra_ed_esce() {
        let mut r = Ritirata::default();
        assert!(!r.attiva());
        r.commuta();
        assert!(r.attiva());
        r.commuta();
        assert!(!r.attiva());
        r.commuta();
        r.esci();
        assert!(!r.attiva());
    }

    #[test]
    fn bloccante_esce_dalla_ritirata() {
        let mut r = Ritirata::default();
        r.commuta();
        r.arriva(Livello::Informazione);
        assert!(r.attiva(), "un'informazione non disturba");
        r.arriva(Livello::Attenzione);
        assert!(r.attiva(), "un'attenzione colora la linea, non la fa uscire");
        r.arriva(Livello::Bloccante);
        assert!(!r.attiva());
    }

    #[test]
    fn mouse_sulla_linea_450_ms_fa_tornare() {
        let t = Instant::now();
        let mut r = Ritirata::default();
        r.commuta();
        assert!(!r.mouse(true, t));
        assert!(!r.mouse(true, t + Duration::from_millis(300)));
        assert!(r.attiva());
        assert!(r.mouse(true, t + RITORNO_DOPO));
        assert!(!r.attiva());
    }

    #[test]
    fn mouse_che_passa_soltanto_non_fa_tornare() {
        let t = Instant::now();
        let mut r = Ritirata::default();
        r.commuta();
        assert!(!r.mouse(true, t));
        assert!(!r.mouse(false, t + Duration::from_millis(200)));
        // Si riparte da capo: 400 ms dal nuovo arrivo non bastano.
        assert!(!r.mouse(true, t + Duration::from_millis(300)));
        assert!(!r.mouse(true, t + Duration::from_millis(700)));
        assert!(r.attiva());
        assert!(r.mouse(true, t + Duration::from_millis(750)));
    }

    #[test]
    fn mouse_non_conta_se_non_ritirata() {
        let t = Instant::now();
        let mut r = Ritirata::default();
        assert!(!r.mouse(true, t));
        assert!(!r.mouse(true, t + Duration::from_secs(1)));
        // Rientrando, il tempo passato prima non vale.
        r.commuta();
        assert!(!r.mouse(true, t + Duration::from_secs(2)));
    }

    #[test]
    fn zona_al_centro_del_bordo_alto() {
        let area = RettangoloFisico { x: 0, y: 0, larghezza: 1920, altezza: 1040 };
        let z = zona_linea(area, 1.0);
        assert_eq!(z, RettangoloFisico { x: (1920 - 340) / 2, y: 0, larghezza: 340, altezza: 10 });
        assert!(dentro(z, (960, 0)));
        assert!(dentro(z, (z.x, 9)));
        assert!(!dentro(z, (z.x - 1, 0)));
        assert!(!dentro(z, (960, 10)));
        // Monitor a sinistra con scala 1,5 e barra in alto (area che parte da y = 40).
        let area = RettangoloFisico { x: -1920, y: 40, larghezza: 1920, altezza: 1040 };
        let z = zona_linea(area, 1.5);
        assert_eq!(z, RettangoloFisico { x: -1920 + (1920 - 510) / 2, y: 40, larghezza: 510, altezza: 15 });
    }
}
