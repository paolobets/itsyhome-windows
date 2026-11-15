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
    /// Il numero del tocco: l'esito di un comando vale solo per la sua attesa.
    numero: u64,
    /// Quando è stato toccato (per misurare il tempo fino alla conferma).
    toccato: Instant,
    scade: Instant,
}

#[derive(Debug, Clone)]
pub struct Ottimismo<T> {
    attese: HashMap<String, Attesa<T>>,
    /// Numero dell'ultimo tocco (0 = nessuno: un comando senza attesa).
    ultimo: u64,
}

impl<T> Default for Ottimismo<T> {
    fn default() -> Self {
        Ottimismo { attese: HashMap::new(), ultimo: 0 }
    }
}

/// Le attese uscite alla scadenza.
#[derive(Debug, Default, PartialEq)]
pub struct Scadute {
    /// Senza conferma: tornano allo stato vero con l'errore.
    pub fallite: Vec<String>,
    /// Il valore vero era già quello atteso (un `toggle` doppio, un cambio arrivato da fuori): HA
    /// non ripubblica dati uguali, quindi la conferma non sarebbe arrivata mai.
    pub confermate: Vec<(String, Duration)>,
}

impl<T: PartialEq + Clone> Ottimismo<T> {
    /// Un tocco: da adesso il dispositivo si mostra con `atteso`. Un secondo tocco prima della
    /// conferma sostituisce l'attesa (e il suo tempo). Se `atteso` è già il valore `vero`, non
    /// c'è niente da aspettare (HA non manderà nessun cambio): l'attesa esce. Restituisce il
    /// numero del tocco, da passare a `fallito` con l'esito del comando (0 = nessuna attesa).
    pub fn tocca(&mut self, id: &str, atteso: T, vero: &T, ora: Instant) -> u64 {
        if atteso == *vero {
            self.attese.remove(id);
            return 0;
        }
        self.ultimo += 1;
        let numero = self.ultimo;
        self.attese.insert(id.to_owned(), Attesa { atteso, numero, toccato: ora, scade: ora + ATTESA_CONFERMA });
        numero
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

    /// Il comando del tocco `numero` è fallito: la sua attesa esce (vero se c'era). L'esito di un
    /// comando più vecchio non tocca l'attesa di un tocco più recente.
    pub fn fallito(&mut self, id: &str, numero: u64) -> bool {
        if self.attese.get(id).is_some_and(|a| a.numero == numero) {
            self.attese.remove(id);
            return true;
        }
        false
    }

    /// Le attese scadute a `ora` escono: confermate se `vero` dà già il valore atteso, fallite
    /// altrimenti.
    pub fn scadute(&mut self, ora: Instant, vero: impl Fn(&str) -> Option<T>) -> Scadute {
        let mut esito = Scadute::default();
        self.attese.retain(|id, a| {
            if a.scade > ora {
                return true;
            }
            if vero(id).is_some_and(|v| v == a.atteso) {
                esito.confermate.push((id.clone(), ora.saturating_duration_since(a.toccato)));
            } else {
                esito.fallite.push(id.clone());
            }
            false
        });
        esito
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
        o.tocca("light.cucina", true, &false, t);
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
        let n = o.tocca("light.a", true, &false, t);
        o.tocca("switch.b", false, &true, t + Duration::from_secs(1));
        assert!(o.fallito("light.a", n));
        assert!(!o.fallito("light.a", n), "già uscita");
        assert_eq!(o.atteso("light.a"), None);
        assert_eq!(o.prossima_scadenza(t + Duration::from_secs(2)), Some(Duration::from_secs(4)));
        assert_eq!(o.scadute(t + Duration::from_secs(5), |_| Some(true)), Scadute::default());
        let s = o.scadute(t + Duration::from_secs(6), |_| Some(true));
        assert_eq!(s.fallite, vec!["switch.b".to_owned()]);
        assert!(s.confermate.is_empty());
        assert_eq!(o.atteso("switch.b"), None);
        assert_eq!(o.prossima_scadenza(t), None);
    }

    /// Giro di correzioni 1, rilievo 1: alla scadenza un'attesa il cui valore vero è già quello
    /// atteso è confermata, non un errore (HA non ripubblica dati uguali).
    #[test]
    fn scaduta_col_valore_gia_vero_e_confermata() {
        let t = Instant::now();
        let mut o = Ottimismo::default();
        o.tocca("light.a", true, &false, t);
        let s = o.scadute(t + ATTESA_CONFERMA, |_| Some(true));
        assert_eq!(s.confermate, vec![("light.a".to_owned(), ATTESA_CONFERMA)]);
        assert!(s.fallite.is_empty());
    }

    /// Rilievo 5: l'errore del primo comando non cancella l'attesa del secondo tocco.
    #[test]
    fn l_esito_vale_solo_per_il_suo_tocco() {
        let t = Instant::now();
        let mut o = Ottimismo::default();
        let primo = o.tocca("light.a", true, &false, t);
        let secondo = o.tocca("light.a", false, &true, t);
        assert!(!o.fallito("light.a", primo), "il primo non tocca l'attesa del secondo");
        assert_eq!(o.atteso("light.a"), Some(&false));
        assert!(o.fallito("light.a", secondo));
        assert!(!o.fallito("light.a", 0), "0: comando senza attesa");
    }

    #[test]
    fn due_tocchi_valgono_per_l_ultimo() {
        let t = Instant::now();
        let mut o = Ottimismo::default();
        o.tocca("light.a", true, &false, t);
        // Il secondo tocco chiede di nuovo il valore vero (spenta): niente da aspettare.
        assert_eq!(o.tocca("light.a", false, &false, t + Duration::from_millis(200)), 0);
        assert_eq!(o.atteso("light.a"), None);
        // Con un valore vero diverso (accesa nel frattempo) il secondo tocco aspetta.
        o.tocca("light.a", true, &false, t);
        o.tocca("light.a", false, &true, t + Duration::from_millis(200));
        // Il primo comando arriva (accesa): non è ancora l'ultimo valore chiesto.
        assert!(o.conferma(|_| Some(true), t + Duration::from_millis(300)).is_empty());
        assert_eq!(o.atteso("light.a"), Some(&false));
        assert_eq!(o.conferma(|_| Some(false), t + Duration::from_millis(500)).len(), 1);
    }

    #[test]
    fn dispositivo_sparito_o_connessione_persa() {
        let t = Instant::now();
        let mut o = Ottimismo::default();
        o.tocca("light.a", true, &false, t);
        assert!(o.conferma(|_| None, t).is_empty());
        assert_eq!(o.atteso("light.a"), None, "sparito: niente da mostrare");
        o.tocca("light.a", true, &false, t);
        assert!(o.svuota());
        assert!(!o.svuota());
    }
}
