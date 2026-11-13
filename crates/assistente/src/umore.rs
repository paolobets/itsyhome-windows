//! Il "cervello" dell'umore: logica pura, il tempo arriva sempre dal chiamante.

use casa_core::eventi::StatoConnessione;
use std::time::{Duration, Instant};

pub const CONTENTO_PER: Duration = Duration::from_secs(3);
pub const DORME_DOPO: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Umore {
    Tranquillo,
    Pensa,
    TiCerca,
    Contento,
    Dorme,
}

impl Umore {
    pub fn come_stringa(self) -> &'static str {
        match self {
            Umore::Tranquillo => "calm",
            Umore::Pensa => "think",
            Umore::TiCerca => "alert",
            Umore::Contento => "happy",
            Umore::Dorme => "sleep",
        }
    }
}

#[derive(Debug, Clone)]
pub enum Segnale {
    Casa(StatoConnessione),
    Notifica { titolo: String },
    PannelloAperto,
    ComandoRiuscito,
    ComandoFallito,
    /// Da quanto tempo l'utente è inattivo.
    Inattivo(Duration),
}

pub struct Cervello {
    connessione: StatoConnessione,
    non_vista: Option<String>,
    contento_da: Option<Instant>,
    inattivo: Duration,
}

impl Cervello {
    pub fn new(_ora: Instant) -> Self {
        Self {
            connessione: StatoConnessione::Disconnesso,
            non_vista: None,
            contento_da: None,
            inattivo: Duration::ZERO,
        }
    }

    pub fn ricevi(&mut self, s: Segnale, ora: Instant) {
        match s {
            Segnale::Casa(c) => self.connessione = c,
            Segnale::Notifica { titolo } => self.non_vista = Some(titolo),
            Segnale::PannelloAperto => self.non_vista = None,
            Segnale::ComandoRiuscito => self.contento_da = Some(ora),
            Segnale::ComandoFallito => self.contento_da = None,
            Segnale::Inattivo(d) => self.inattivo = d,
        }
    }

    pub fn umore(&self, ora: Instant) -> Umore {
        if self.non_vista.is_some() {
            Umore::TiCerca
        } else if matches!(self.connessione, StatoConnessione::InConnessione) {
            Umore::Pensa
        } else if self.contento_da.is_some_and(|t| ora.saturating_duration_since(t) < CONTENTO_PER) {
            Umore::Contento
        } else if self.inattivo >= DORME_DOPO {
            Umore::Dorme
        } else {
            Umore::Tranquillo
        }
    }

    pub fn messaggio(&self) -> Option<&str> {
        self.non_vista.as_deref()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn notifica() -> Segnale {
        Segnale::Notifica { titolo: "Lavatrice finita".into() }
    }

    #[test]
    fn parte_tranquillo() {
        let t = Instant::now();
        let c = Cervello::new(t);
        assert_eq!(c.umore(t), Umore::Tranquillo);
        assert_eq!(c.messaggio(), None);
    }

    #[test]
    fn in_connessione_pensa_poi_connesso_tranquillo() {
        let t = Instant::now();
        let mut c = Cervello::new(t);
        c.ricevi(Segnale::Casa(StatoConnessione::InConnessione), t);
        assert_eq!(c.umore(t), Umore::Pensa);
        c.ricevi(Segnale::Casa(StatoConnessione::Connesso), t);
        assert_eq!(c.umore(t), Umore::Tranquillo);
    }

    #[test]
    fn errore_non_resta_su_pensa() {
        let t = Instant::now();
        let mut c = Cervello::new(t);
        c.ricevi(Segnale::Casa(StatoConnessione::InConnessione), t);
        c.ricevi(Segnale::Casa(StatoConnessione::Errore("x".into())), t);
        assert_eq!(c.umore(t), Umore::Tranquillo);
        c.ricevi(Segnale::Casa(StatoConnessione::Disconnesso), t);
        assert_eq!(c.umore(t), Umore::Tranquillo);
    }

    #[test]
    fn notifica_ti_cerca_finche_non_apro_il_pannello() {
        let t = Instant::now();
        let mut c = Cervello::new(t);
        c.ricevi(notifica(), t);
        assert_eq!(c.umore(t), Umore::TiCerca);
        assert_eq!(c.messaggio(), Some("Lavatrice finita"));
        c.ricevi(Segnale::PannelloAperto, t);
        assert_eq!(c.umore(t), Umore::Tranquillo);
        assert_eq!(c.messaggio(), None);
    }

    #[test]
    fn contento_dura_tre_secondi() {
        let t = Instant::now();
        let mut c = Cervello::new(t);
        c.ricevi(Segnale::ComandoRiuscito, t);
        assert_eq!(c.umore(t + Duration::from_millis(2900)), Umore::Contento);
        assert_eq!(c.umore(t + Duration::from_millis(3100)), Umore::Tranquillo);
    }

    #[test]
    fn comando_fallito_non_e_contento() {
        let t = Instant::now();
        let mut c = Cervello::new(t);
        c.ricevi(Segnale::ComandoFallito, t);
        assert_eq!(c.umore(t), Umore::Tranquillo);
        c.ricevi(Segnale::ComandoRiuscito, t);
        c.ricevi(Segnale::ComandoFallito, t);
        assert_eq!(c.umore(t), Umore::Tranquillo);
    }

    #[test]
    fn inattivo_dieci_minuti_dorme() {
        let t = Instant::now();
        let mut c = Cervello::new(t);
        c.ricevi(Segnale::Inattivo(Duration::from_secs(9 * 60)), t);
        assert_eq!(c.umore(t), Umore::Tranquillo);
        c.ricevi(Segnale::Inattivo(Duration::from_secs(10 * 60)), t);
        assert_eq!(c.umore(t), Umore::Dorme);
        c.ricevi(Segnale::Inattivo(Duration::ZERO), t);
        assert_eq!(c.umore(t), Umore::Tranquillo);
    }

    #[test]
    fn ti_cerca_vince_su_dorme_e_pensa() {
        let t = Instant::now();
        let mut c = Cervello::new(t);
        c.ricevi(Segnale::Inattivo(DORME_DOPO), t);
        c.ricevi(Segnale::Casa(StatoConnessione::InConnessione), t);
        c.ricevi(notifica(), t);
        assert_eq!(c.umore(t), Umore::TiCerca);
    }
}
