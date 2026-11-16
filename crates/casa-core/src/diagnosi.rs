//! Registro diagnostico: **solo contatori**, spento di serie (Task 14g).
//!
//! Serve a capire da dove viene il consumo sulla casa vera senza portare fuori dati della casa:
//! qui finiscono soltanto numeri e nomi fissi scritti nel codice (tipi di messaggio, tipi di
//! evento di HA a cui ci si iscrive, cause delle riletture, stato del core fra quelli noti). Mai
//! entity_id, nomi, valori, token o indirizzi: nessuna funzione di questo modulo riceve testo
//! libero da fuori, salvo `stato_ha`, che lo confronta con un elenco fisso e scrive solo il nome
//! dell'elenco.
//!
//! Spento, ogni conteggio è una lettura di un booleano e nulla più. Acceso (con
//! `ITSYHOME_DIAGNOSI=1`, vedi l'assistente) i contatori si sommano e `riga` li legge e li azzera:
//! chi scrive il file passa ogni 60 s.
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};

macro_rules! contatori {
    ($($nome:ident),+ $(,)?) => {
        /// Cosa si conta. Il nome nel registro è quello della variante, in minuscolo.
        #[allow(non_camel_case_types)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Contatore { $($nome),+ }

        /// I nomi nel registro, nell'ordine delle varianti.
        pub const NOMI: &[&str] = &[$(stringify!($nome)),+];
    };
}

contatori!(
    // Messaggi del WebSocket ricevuti (testo), per tipo, e i loro byte.
    ws_messaggi,
    ws_byte,
    ws_result,
    ws_pong,
    ws_auth,
    ws_altro,
    // Messaggi `event`, per sorgente: iscrizione alle entità, notifiche persistenti, e gli
    // eventi del bus a cui ci si iscrive (`EVENTI_RILETTURA`).
    ev_entita,
    ev_notifiche,
    ev_area_registry_updated,
    ev_device_registry_updated,
    ev_entity_registry_updated,
    ev_core_config_updated,
    ev_altro,
    // Entità negli eventi dell'iscrizione: aggiunte (`a`, stato intero), cambiate (`c`), tolte (`r`).
    entita_a,
    entita_c,
    entita_r,
    // Cosa ha comportato ciascuna: nulla di visibile, sostituita nel menu, mostrabile ma nascosta
    // dall'utente, mappatura completa.
    effetto_niente,
    effetto_sostituita,
    effetto_nascosta,
    effetto_rimappa,
    // Raffiche chiuse (pubblicazioni), mappature complete, `Dati` consegnati all'app e quelli
    // non consegnati perché uguali all'ultimo.
    pubblicazioni,
    mappature,
    consegne,
    consegne_uguali,
    // Letture complete partite, fallite, e le richieste per causa: connessione, eventi dei
    // registri, stato illeggibile o diff non applicabile, nuovo tentativo dopo un fallimento, HA
    // in avvio; accodate perché una lettura era già in volo.
    riletture,
    letture_fallite,
    rilettura_connessione,
    rilettura_registri,
    rilettura_illeggibile,
    rilettura_riprova,
    rilettura_avvio,
    rilettura_accodata,
    // Iscrizioni `subscribe_entities` mandate.
    iscrizioni,
    // Lato interfaccia: eventi `Dati` e `Stato` ricevuti, ricostruzioni di preferiti/stanze,
    // righe dei modelli riscritte, ricalcoli dell'umore, aspetti nuovi dati alla pillola.
    ui_dati,
    ui_stato,
    ui_ricostruzioni,
    ui_righe,
    ui_umore,
    ui_pillola,
);

/// Gli stati del core di HA che si scrivono (`CoreState` di `homeassistant/core.py`); il resto è
/// «altro». 0 = nessuna lettura ancora, 1 = `state` assente.
const STATI_HA: &[&str] = &["nessuna", "assente", "altro", "NOT_RUNNING", "STARTING", "RUNNING", "STOPPING", "FINAL_WRITE", "STOPPED"];

pub struct Diagnosi {
    acceso: AtomicBool,
    contatori: Vec<AtomicU64>,
    seguite: AtomicU64,
    stato_ha: AtomicU8,
}

impl Diagnosi {
    pub fn new(acceso: bool) -> Diagnosi {
        Diagnosi {
            acceso: AtomicBool::new(acceso),
            contatori: NOMI.iter().map(|_| AtomicU64::new(0)).collect(),
            seguite: AtomicU64::new(0),
            stato_ha: AtomicU8::new(0),
        }
    }

    pub fn acceso(&self) -> bool {
        self.acceso.load(Ordering::Relaxed)
    }

    pub fn conta(&self, c: Contatore) {
        self.conta_n(c, 1);
    }

    pub fn conta_n(&self, c: Contatore, n: u64) {
        if self.acceso() && n > 0 {
            self.contatori[c as usize].fetch_add(n, Ordering::Relaxed);
        }
    }

    /// Quante entità segue l'ultima iscrizione (un valore, non si azzera).
    pub fn seguite(&self, n: usize) {
        if self.acceso() {
            self.seguite.store(n as u64, Ordering::Relaxed);
        }
    }

    /// Lo stato del core all'ultima lettura di `get_config` (`None`: senza `state`). Si tiene solo
    /// l'indice nell'elenco fisso: un valore sconosciuto diventa «altro».
    pub fn stato_ha(&self, stato: Option<&str>) {
        if !self.acceso() {
            return;
        }
        let i = match stato {
            None => 1,
            Some(s) => STATI_HA.iter().skip(3).position(|n| *n == s).map_or(2, |i| i + 3),
        };
        self.stato_ha.store(i as u8, Ordering::Relaxed);
    }

    /// Il valore di un contatore (per i test), senza azzerarlo.
    pub fn valore(&self, c: Contatore) -> u64 {
        self.contatori[c as usize].load(Ordering::Relaxed)
    }

    /// Una riga del registro: ogni contatore come `nome=numero` (anche gli zeri: un'assenza si
    /// legge), poi `seguite=` e `ha=`, e le `misure` di chi scrive (es. CPU e memoria del
    /// processo). I contatori ripartono da zero.
    pub fn riga(&self, misure: &[(&'static str, u64)]) -> String {
        let mut parti: Vec<String> = NOMI
            .iter()
            .zip(&self.contatori)
            .map(|(n, c)| format!("{n}={}", c.swap(0, Ordering::Relaxed)))
            .collect();
        parti.push(format!("seguite={}", self.seguite.load(Ordering::Relaxed)));
        let stato = STATI_HA.get(self.stato_ha.load(Ordering::Relaxed) as usize).copied().unwrap_or("altro");
        parti.push(format!("ha={stato}"));
        parti.extend(misure.iter().map(|(n, v)| format!("{n}={v}")));
        parti.join(" ")
    }
}

impl Default for Diagnosi {
    /// Spenta.
    fn default() -> Diagnosi {
        Diagnosi::new(false)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn spenta_non_conta_nulla() {
        let d = Diagnosi::new(false);
        d.conta(Contatore::ws_messaggi);
        d.stato_ha(Some("STARTING"));
        assert_eq!(d.valore(Contatore::ws_messaggi), 0);
        assert!(d.riga(&[]).contains("ha=nessuna"));
    }

    #[test]
    fn la_riga_azzera_e_tiene_i_valori() {
        let d = Diagnosi::new(true);
        d.conta(Contatore::ev_entita);
        d.conta_n(Contatore::entita_c, 5);
        d.seguite(12);
        d.stato_ha(Some("RUNNING"));
        let riga = d.riga(&[("cpu_ms", 3)]);
        assert!(riga.starts_with("ws_messaggi=0 "), "{riga}");
        assert!(riga.contains(" ev_entita=1 ") && riga.contains(" entita_c=5 "), "{riga}");
        assert!(riga.ends_with(" seguite=12 ha=RUNNING cpu_ms=3"), "{riga}");
        let dopo = d.riga(&[]);
        assert!(dopo.contains(" ev_entita=0 ") && dopo.contains(" seguite=12 ha=RUNNING"), "{dopo}");
    }

    #[test]
    fn uno_stato_sconosciuto_non_entra_nel_registro() {
        let d = Diagnosi::new(true);
        d.stato_ha(Some("light.cucina Casa di Paolo"));
        assert!(d.riga(&[]).ends_with(" ha=altro"));
        d.stato_ha(None);
        assert!(d.riga(&[]).ends_with(" ha=assente"));
        d.stato_ha(Some("STARTING"));
        assert!(d.riga(&[]).ends_with(" ha=STARTING"));
    }
}
