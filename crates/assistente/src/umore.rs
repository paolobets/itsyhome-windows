//! Il "cervello" dell'umore: logica pura, il tempo arriva sempre dal chiamante.

use crate::attenzione::Livello;
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

/// I segnali arrivano dal ponte con la casa e dal pannello.
#[derive(Debug, Clone)]
pub enum Segnale {
    Casa(StatoConnessione),
    /// Il livello più urgente fra le voci «In attesa» (`None` se non ce ne sono): bloccante e
    /// attenzione = «ti cerca» finché ci sono; l'informazione non cambia umore.
    InAttesa(Option<Livello>),
    ComandoRiuscito,
    ComandoFallito,
    /// Da quanto tempo l'utente è inattivo (il controllo da 5 s dei servizi di sistema).
    Inattivo(Duration),
}

pub struct Cervello {
    connessione: StatoConnessione,
    in_attesa: Option<Livello>,
    contento_da: Option<Instant>,
    inattivo: Duration,
}

impl Cervello {
    pub fn new(_ora: Instant) -> Self {
        Self {
            connessione: StatoConnessione::Disconnesso,
            in_attesa: None,
            contento_da: None,
            inattivo: Duration::ZERO,
        }
    }

    pub fn ricevi(&mut self, s: Segnale, ora: Instant) {
        match s {
            Segnale::Casa(c) => self.connessione = c,
            Segnale::InAttesa(l) => self.in_attesa = l,
            Segnale::ComandoRiuscito => self.contento_da = Some(ora),
            Segnale::ComandoFallito => self.contento_da = None,
            Segnale::Inattivo(d) => self.inattivo = d,
        }
    }

    pub fn umore(&self, ora: Instant) -> Umore {
        if matches!(self.in_attesa, Some(Livello::Bloccante | Livello::Attenzione)) {
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

    /// Fra quanto l'umore cambierà da solo (fine di "contento"); `None` se nulla scade:
    /// solo allora serve un timer.
    pub fn prossimo_cambio(&self, ora: Instant) -> Option<Duration> {
        let fine = self.contento_da? + CONTENTO_PER;
        (fine > ora).then(|| fine - ora)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn attenzione() -> Segnale {
        Segnale::InAttesa(Some(Livello::Attenzione))
    }

    #[test]
    fn parte_tranquillo() {
        let t = Instant::now();
        let c = Cervello::new(t);
        assert_eq!(c.umore(t), Umore::Tranquillo);
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
    fn solo_contento_ha_una_scadenza() {
        let t = Instant::now();
        let mut c = Cervello::new(t);
        assert_eq!(c.prossimo_cambio(t), None);
        c.ricevi(Segnale::ComandoRiuscito, t);
        assert_eq!(c.prossimo_cambio(t + Duration::from_secs(1)), Some(Duration::from_secs(2)));
        assert_eq!(c.prossimo_cambio(t + CONTENTO_PER), None);
        c.ricevi(Segnale::ComandoRiuscito, t);
        c.ricevi(Segnale::ComandoFallito, t);
        assert_eq!(c.prossimo_cambio(t), None);
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
    fn ti_cerca_solo_per_bloccante_e_attenzione() {
        let t = Instant::now();
        let mut c = Cervello::new(t);
        c.ricevi(Segnale::InAttesa(Some(Livello::Informazione)), t);
        assert_eq!(c.umore(t), Umore::Tranquillo);
        c.ricevi(Segnale::InAttesa(Some(Livello::Attenzione)), t);
        assert_eq!(c.umore(t), Umore::TiCerca);
        c.ricevi(Segnale::InAttesa(Some(Livello::Bloccante)), t);
        assert_eq!(c.umore(t), Umore::TiCerca);
        c.ricevi(Segnale::InAttesa(None), t);
        assert_eq!(c.umore(t), Umore::Tranquillo);
    }

    #[test]
    fn ti_cerca_vince_su_dorme_e_pensa() {
        let t = Instant::now();
        let mut c = Cervello::new(t);
        c.ricevi(Segnale::Inattivo(DORME_DOPO), t);
        c.ricevi(Segnale::Casa(StatoConnessione::InConnessione), t);
        c.ricevi(attenzione(), t);
        assert_eq!(c.umore(t), Umore::TiCerca);
    }
}
