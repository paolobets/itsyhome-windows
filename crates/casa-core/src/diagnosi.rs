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
    // Task 2 della M2: cambio del meteo scelto, pubblicato come una sostituzione.
    effetto_meteo,
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
    // Task 14h: eventi dei registri che non cambiano nulla di ciò che l'app mostra o segue
    // (scartati, nessuna lettura), quelli che chiedono solo i registri, e le letture dei soli
    // registri partite (le `riletture` restano quelle complete).
    registro_scartati,
    rilettura_solo_registri,
    letture_registri,
    // Giro di correzioni 1 del 14h: i campi di `changes` negli eventi dei registri (entità e
    // dispositivi), da un elenco fisso; `vuoto` = `changes: {}`, `altro` = un campo fuori elenco.
    registro_campo_name,
    registro_campo_original_name,
    registro_campo_area_id,
    registro_campo_device_id,
    registro_campo_disabled_by,
    registro_campo_hidden_by,
    registro_campo_entity_category,
    registro_campo_device_class,
    registro_campo_options,
    registro_campo_capabilities,
    registro_campo_supported_features,
    registro_campo_name_by_user,
    registro_campo_sw_version,
    registro_campo_connections,
    registro_campo_translation_key,
    registro_campo_has_entity_name,
    registro_campo_parent_device_id,
    registro_campo_vuoto,
    registro_campo_altro,
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
    // Millisecondi passati con l'umore «pensa» (l'animazione più cara), contati quando finisce.
    ui_pensa_ms,
);

/// Il contatore del campo `campo` di `changes` (elenco fisso; il resto è `registro_campo_altro`).
pub fn contatore_campo(campo: &str) -> Contatore {
    match campo {
        "name" => Contatore::registro_campo_name,
        "original_name" => Contatore::registro_campo_original_name,
        "area_id" => Contatore::registro_campo_area_id,
        "device_id" => Contatore::registro_campo_device_id,
        "disabled_by" => Contatore::registro_campo_disabled_by,
        "hidden_by" => Contatore::registro_campo_hidden_by,
        "entity_category" => Contatore::registro_campo_entity_category,
        "device_class" => Contatore::registro_campo_device_class,
        "options" => Contatore::registro_campo_options,
        "capabilities" => Contatore::registro_campo_capabilities,
        "supported_features" => Contatore::registro_campo_supported_features,
        "name_by_user" => Contatore::registro_campo_name_by_user,
        "sw_version" => Contatore::registro_campo_sw_version,
        "connections" => Contatore::registro_campo_connections,
        "translation_key" => Contatore::registro_campo_translation_key,
        "has_entity_name" => Contatore::registro_campo_has_entity_name,
        "parent_device_id" => Contatore::registro_campo_parent_device_id,
        _ => Contatore::registro_campo_altro,
    }
}

/// I valori dell'avvio (giro di correzioni 1 del 14h), scritti una volta sola e poi ripetuti in
/// ogni riga (0 = non ancora): ms dall'avvio alla prima lettura completa riuscita; ms, byte del
/// WebSocket e CPU del processo alla prima consegna dei dati all'interfaccia.
pub const AVVIO: &[&str] = &["avvio_lettura_ms", "avvio_consegna_ms", "avvio_consegna_byte", "avvio_consegna_cpu_ms"];

/// Gli stati del core di HA che si scrivono (`CoreState` di `homeassistant/core.py`); il resto è
/// «altro». 0 = nessuna lettura ancora, 1 = `state` assente.
const STATI_HA: &[&str] = &["nessuna", "assente", "altro", "NOT_RUNNING", "STARTING", "RUNNING", "STOPPING", "FINAL_WRITE", "STOPPED"];

pub struct Diagnosi {
    acceso: AtomicBool,
    contatori: Vec<AtomicU64>,
    seguite: AtomicU64,
    stato_ha: AtomicU8,
    /// Quando è nato il registro (all'avvio dell'app).
    nato: std::time::Instant,
    /// Byte ricevuti dal WebSocket da sempre (`ws_byte` si azzera a ogni riga).
    byte_totali: AtomicU64,
    /// I valori di `AVVIO`, nell'ordine.
    avvio: [AtomicU64; 4],
}

impl Diagnosi {
    pub fn new(acceso: bool) -> Diagnosi {
        Diagnosi {
            acceso: AtomicBool::new(acceso),
            contatori: NOMI.iter().map(|_| AtomicU64::new(0)).collect(),
            seguite: AtomicU64::new(0),
            stato_ha: AtomicU8::new(0),
            nato: std::time::Instant::now(),
            byte_totali: AtomicU64::new(0),
            avvio: Default::default(),
        }
    }

    /// Millisecondi da quando è nato il registro (almeno 1: 0 vuol dire «non ancora»).
    fn da_quando_nato(&self) -> u64 {
        (self.nato.elapsed().as_millis() as u64).max(1)
    }

    /// La prima lettura completa è riuscita (le altre non contano).
    pub fn prima_lettura(&self) {
        if self.acceso() {
            let _ = self.avvio[0].compare_exchange(0, self.da_quando_nato(), Ordering::Relaxed, Ordering::Relaxed);
        }
    }

    /// I primi dati sono arrivati all'interfaccia; `cpu_ms`: la CPU del processo da quando è nato
    /// (la misura chi chiama). Le altre consegne non contano.
    pub fn prima_consegna(&self, cpu_ms: u64) {
        if self.acceso() && self.avvio[1].compare_exchange(0, self.da_quando_nato(), Ordering::Relaxed, Ordering::Relaxed).is_ok() {
            self.avvio[2].store(self.byte_totali.load(Ordering::Relaxed), Ordering::Relaxed);
            self.avvio[3].store(cpu_ms, Ordering::Relaxed);
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
            if c == Contatore::ws_byte {
                self.byte_totali.fetch_add(n, Ordering::Relaxed);
            }
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

    /// L'ultima lettura completa non ha portato `get_config`: lo stato del core non è noto.
    pub fn stato_ha_non_letto(&self) {
        if self.acceso() {
            self.stato_ha.store(0, Ordering::Relaxed);
        }
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
        parti.extend(AVVIO.iter().zip(&self.avvio).map(|(n, v)| format!("{n}={}", v.load(Ordering::Relaxed))));
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
        assert!(riga.contains(" seguite=12 ha=RUNNING ") && riga.ends_with(" cpu_ms=3"), "{riga}");
        let dopo = d.riga(&[]);
        assert!(dopo.contains(" ev_entita=0 ") && dopo.contains(" seguite=12 ha=RUNNING"), "{dopo}");
    }

    /// Giro di correzioni 1 del 14h: i valori dell'avvio si scrivono una volta sola, restano in
    /// ogni riga, e la consegna porta i byte ricevuti fino a lì.
    #[test]
    fn i_valori_dell_avvio_si_scrivono_una_volta() {
        let d = Diagnosi::new(true);
        let valore = |riga: &str, n: &str| -> u64 {
            riga.split(' ').find_map(|p| p.strip_prefix(&format!("{n}="))).unwrap().parse().unwrap()
        };
        let riga = d.riga(&[]);
        assert!(AVVIO.iter().all(|n| valore(&riga, n) == 0), "{riga}");
        d.conta_n(Contatore::ws_byte, 1000);
        d.prima_lettura();
        let _ = d.riga(&[]); // azzera ws_byte: i byte dell'avvio no
        d.conta_n(Contatore::ws_byte, 500);
        d.prima_consegna(42);
        d.conta_n(Contatore::ws_byte, 7);
        d.prima_lettura();
        d.prima_consegna(99);
        let riga = d.riga(&[]);
        assert!(valore(&riga, "avvio_lettura_ms") >= 1, "{riga}");
        assert!(valore(&riga, "avvio_consegna_ms") >= valore(&riga, "avvio_lettura_ms"), "{riga}");
        assert_eq!((valore(&riga, "avvio_consegna_byte"), valore(&riga, "avvio_consegna_cpu_ms")), (1500, 42), "{riga}");
        let dopo = d.riga(&[]);
        assert_eq!(valore(&dopo, "avvio_consegna_byte"), 1500, "restano: {dopo}");
        // Spenta non scrive nulla.
        let spenta = Diagnosi::new(false);
        spenta.prima_lettura();
        spenta.prima_consegna(5);
        assert!(spenta.riga(&[]).contains("avvio_consegna_cpu_ms=0"));
    }

    /// Giro di correzioni 2 del 14h (N4): i campi che decidono la rilettura hanno il loro nome.
    #[test]
    fn i_campi_che_decidono_hanno_un_contatore() {
        let nome = |c: Contatore| NOMI[c as usize];
        for campo in ["translation_key", "has_entity_name", "parent_device_id", "name", "original_name", "area_id"] {
            assert_eq!(nome(contatore_campo(campo)), format!("registro_campo_{campo}"), "{campo}");
        }
        assert_eq!(contatore_campo("Casa di Paolo"), Contatore::registro_campo_altro);
    }

    #[test]
    fn uno_stato_sconosciuto_non_entra_nel_registro() {
        let d = Diagnosi::new(true);
        d.stato_ha(Some("light.cucina Casa di Paolo"));
        assert!(d.riga(&[]).contains(" ha=altro "));
        d.stato_ha(None);
        assert!(d.riga(&[]).contains(" ha=assente "));
        d.stato_ha(Some("STARTING"));
        assert!(d.riga(&[]).contains(" ha=STARTING "));
    }
}
