//! Stato ottimistico dei dispositivi: un tocco cambia subito l'aspetto del dispositivo (l'icona
//! accesa o spenta nel fotogramma dopo il tocco), prima che Home Assistant risponda.
//!
//! L'attesa finisce in tre modi:
//! - **confermata**: arrivano dati in cui il dispositivo ha davvero il valore atteso;
//! - **fallita**: il comando torna con un errore;
//! - **scaduta**: entro `ATTESA_CONFERMA` non arriva nessuna conferma.
//!
//! Negli ultimi due casi l'aspetto torna quello vero e chi usa l'attesa mostra l'errore.
//! Generico sul valore atteso (`bool` per acceso/spento, domani una posizione o una
//! temperatura) e senza orologio proprio: il tempo lo dà chi chiama. Lo usano i preferiti del
//! pannello; le stanze (Task 14e) lo useranno per qualunque dispositivo.
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Quanto si aspetta la conferma di Home Assistant prima di tornare allo stato vero. Una casa
/// lenta conferma in qualche centinaio di millisecondi; oltre i 5 s qualcosa non va.
pub const ATTESA_CONFERMA: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
struct Attesa<T> {
    atteso: T,
    /// Quando è stato toccato (per misurare il tempo fino alla conferma).
    toccato: Instant,
    scade: Instant,
}

#[derive(Debug, Clone)]
pub struct Ottimismo<T> {
    attese: HashMap<String, Attesa<T>>,
}

impl<T> Default for Ottimismo<T> {
    fn default() -> Self {
        Ottimismo { attese: HashMap::new() }
    }
}

impl<T: PartialEq + Clone> Ottimismo<T> {
    /// Un tocco: da adesso il dispositivo si mostra con `atteso`. Un secondo tocco prima della
    /// conferma sostituisce l'attesa (e il suo tempo).
    pub fn tocca(&mut self, id: &str, atteso: T, ora: Instant) {
        self.attese.insert(id.to_owned(), Attesa { atteso, toccato: ora, scade: ora + ATTESA_CONFERMA });
    }

    /// Il valore da mostrare al posto di quello vero, se c'è un'attesa.
    pub fn atteso(&self, id: &str) -> Option<&T> {
        self.attese.get(id).map(|a| &a.atteso)
    }

    /// Dati nuovi: `vero` dà il valore vero di un dispositivo (`None` se non c'è più). Escono le
    /// attese confermate e quelle dei dispositivi spariti; si restituiscono quelle confermate,
    /// con il tempo dal tocco.
    pub fn conferma(&mut self, vero: impl Fn(&str) -> Option<T>, ora: Instant) -> Vec<(String, Duration)> {
        let mut confermate = Vec::new();
        self.attese.retain(|id, a| match vero(id) {
            Some(v) if v == a.atteso => {
                confermate.push((id.clone(), ora.saturating_duration_since(a.toccato)));
                false
            }
            Some(_) => true,
            None => false,
        });
        confermate
    }

    /// Il comando è fallito: l'attesa esce (vero se c'era).
    pub fn fallito(&mut self, id: &str) -> bool {
        self.attese.remove(id).is_some()
    }

    /// Le attese scadute a `ora` escono e si restituiscono.
    pub fn scadute(&mut self, ora: Instant) -> Vec<String> {
        let scadute: Vec<String> = self.attese.iter().filter(|(_, a)| a.scade <= ora).map(|(id, _)| id.clone()).collect();
        for id in &scadute {
            self.attese.remove(id);
        }
        scadute
    }

    /// Fra quanto scade la prossima attesa (`None` se non ce ne sono).
    pub fn prossima_scadenza(&self, ora: Instant) -> Option<Duration> {
        self.attese.values().map(|a| a.scade.saturating_duration_since(ora)).min()
    }

    /// Senza connessione nessuna conferma può arrivare: tutto torna vero.
    pub fn svuota(&mut self) -> bool {
        let c_erano = !self.attese.is_empty();
        self.attese.clear();
        c_erano
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn il_tocco_si_vede_subito_e_la_conferma_lo_chiude() {
        let t = Instant::now();
        let mut o = Ottimismo::default();
        o.tocca("light.cucina", true, t);
        assert_eq!(o.atteso("light.cucina"), Some(&true));
        // Dati vecchi (ancora spenta): l'attesa resta.
        assert!(o.conferma(|_| Some(false), t + Duration::from_millis(100)).is_empty());
        assert_eq!(o.atteso("light.cucina"), Some(&true));
        let c = o.conferma(|_| Some(true), t + Duration::from_millis(420));
        assert_eq!(c, vec![("light.cucina".to_owned(), Duration::from_millis(420))]);
        assert_eq!(o.atteso("light.cucina"), None);
    }

    #[test]
    fn errore_o_scadenza_tornano_allo_stato_vero() {
        let t = Instant::now();
        let mut o = Ottimismo::default();
        o.tocca("light.a", true, t);
        o.tocca("switch.b", false, t + Duration::from_secs(1));
        assert!(o.fallito("light.a"));
        assert!(!o.fallito("light.a"), "già uscita");
        assert_eq!(o.atteso("light.a"), None);
        assert_eq!(o.prossima_scadenza(t + Duration::from_secs(2)), Some(Duration::from_secs(4)));
        assert!(o.scadute(t + Duration::from_secs(5)).is_empty());
        assert_eq!(o.scadute(t + Duration::from_secs(6)), vec!["switch.b".to_owned()]);
        assert_eq!(o.atteso("switch.b"), None);
        assert_eq!(o.prossima_scadenza(t), None);
    }

    #[test]
    fn due_tocchi_valgono_per_l_ultimo() {
        let t = Instant::now();
        let mut o = Ottimismo::default();
        o.tocca("light.a", true, t);
        o.tocca("light.a", false, t + Duration::from_millis(200));
        // Il primo comando arriva (accesa): non è ancora l'ultimo valore chiesto.
        assert!(o.conferma(|_| Some(true), t + Duration::from_millis(300)).is_empty());
        assert_eq!(o.atteso("light.a"), Some(&false));
        assert_eq!(o.conferma(|_| Some(false), t + Duration::from_millis(500)).len(), 1);
    }

    #[test]
    fn dispositivo_sparito_o_connessione_persa() {
        let t = Instant::now();
        let mut o = Ottimismo::default();
        o.tocca("light.a", true, t);
        assert!(o.conferma(|_| None, t).is_empty());
        assert_eq!(o.atteso("light.a"), None, "sparito: niente da mostrare");
        o.tocca("light.a", true, t);
        assert!(o.svuota());
        assert!(!o.svuota());
    }
}
