//! `CasaService`: un thread con runtime tokio `current_thread` che tiene la
//! connessione a Home Assistant, tiene in memoria stati e registri e consegna `CasaEvento`
//! all'`Ascoltatore`, sempre da quel thread.
//!
//! Alla connessione si legge tutto una volta (stati, registri, configurazione). Poi ci si
//! iscrive con `subscribe_entities` alle sole entità che l'app mostra (preferiti, stanze, scene,
//! telecamere, più gli `update.*` che contano gli aggiornamenti): HA manda subito i loro stati
//! interi e poi solo i loro cambi, in forma compressa. Ogni cambio aggiorna in memoria solo la sua
//! entità e si pubblica solo se cambia qualcosa che l'app mostra. Così una casa vera, dove i
//! sensori cambiano di continuo, non manda nemmeno i messaggi delle entità che l'app non mostra.
//!
//! Gli eventi `*_registry_updated` si leggono (`ha::registri`): quelli che non toccano nulla di ciò
//! che l'app mostra o segue si scartano; gli altri fanno rileggere i soli registri (poi mappatura
//! in memoria, iscrizione rifatta solo se cambiano le entità da seguire) o, per un'entità nuova o
//! rinominata, tutto. `core_config_updated` fa rileggere tutto.
//!
//! L'iscrizione si rifà dopo ogni lettura completa (il suo primo evento porta gli stati di
//! adesso: niente cambi persi fra la lettura e l'iscrizione) e quando cambia l'insieme mostrato
//! (entità nascoste, entità nuove). Si seguono anche le voci del registro che l'app mostrerà
//! appena avranno uno stato (dopo un riavvio di HA molte integrazioni caricano più tardi: HA le
//! manda con `a` quando nascono), e un'entità tolta (`r`) resta seguita fino alla prossima lettura
//! completa (un ricaricamento la rimette). Se HA sta ancora avviandosi (`get_config` → `state`
//! diverso da «RUNNING») si rilegge di nuovo poco dopo; arrivato a «RUNNING» HA manda comunque
//! `core_config_updated`, che fa rileggere.
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio::time::{sleep_until, Instant};

use crate::config::{Config, ConfigStore};
use crate::diagnosi::{Contatore, Diagnosi};
use crate::eventi::{Ascoltatore, CasaEvento, StatoConnessione};
use crate::ha::client::{self, Client, DalClient, NON_CONNESSO};
use crate::ha::eventi::{applica_diff, Aggiornamento, EventoWs};
use crate::ha::mapper::{
    aggiornamento_disponibile, meteo_da, scegli_meteo, sensore_numerico, sostituisci_in_menu, valore_numerico,
    EntityMapper, Indici, Preferenze,
};
use crate::ha::models::{HaArea, HaConfig, HaDevice, HaEntityEntry, HaState};
use crate::ha::registri::{Azione, CambioRegistro, Rilettura};
use crate::segreti::Segreti;
use crate::types::{AppEntity, MenuData, Meteo, SensoreNumerico, TestConnectionResult};

/// Cambi visibili a raffica (una scena che accende sei luci, un gruppo, un'automazione che
/// tocca più entità) arrivano entro pochi millisecondi: si aspetta che si calmino per 50 ms e
/// si pubblicano insieme, una sola volta (tre fotogrammi a 60 Hz: il tocco ha già lo stato
/// ottimistico, la conferma non ha fretta di un fotogramma)...
const RAFFICA: Duration = Duration::from_millis(50);
/// ...ma un flusso continuo di cambi visibili si pubblica almeno ogni 300 ms.
const TETTO_RAFFICA: Duration = Duration::from_millis(300);
/// Anche i registri cambiano a raffiche (un'integrazione che aggiunge dieci dispositivi, una
/// stanza rinominata): si rilegge 1 s dopo l'ultimo evento, e al più tardi 5 s dopo il primo.
const ATTESA_REGISTRI: Duration = Duration::from_secs(1);
const TETTO_REGISTRI: Duration = Duration::from_secs(5);
/// Una lettura fallita (completa o dei soli registri) a connessione attiva si riprova con una
/// lettura completa dopo 2 s, poi 4, 8, 16, 32 e al più ogni 60 s, finché riesce. L'attesa riparte
/// da 2 s dopo una lettura completa riuscita e a ogni connessione; a connessione caduta non si
/// riprova (alla riconnessione la lettura parte comunque).
const RIPROVA_LETTURA: Duration = Duration::from_secs(2);
const TETTO_RIPROVA: Duration = Duration::from_secs(60);
/// HA ancora in avvio alla lettura: si rilegge dopo 5 s, finché non è «RUNNING».
const ATTESA_AVVIO: Duration = Duration::from_secs(5);

type Fatto = Box<dyn FnOnce(TestConnectionResult) + Send>;

enum Comando {
    ConnettiAttivo,
    Connetti(String),
    Disconnetti,
    Servizio { entity_id: String, dominio: String, servizio: String, numero: u64 },
    Archivia(String),
    Prova { url: String, token: String, fatto: Fatto },
    // In una `Box`: con `Config` in linea la variante supera le altre (`clippy::large_enum_variant`).
    Salva { config: Box<Config>, rimappa: bool, risegui: bool },
    Ferma,
}

pub struct CasaService {
    tx: mpsc::UnboundedSender<Comando>,
    config: Arc<Mutex<Config>>,
    thread: Option<JoinHandle<()>>,
    /// Il registro diagnostico (spento di serie): lo usano anche il client e l'interfaccia.
    diagnosi: Arc<Diagnosi>,
}

impl CasaService {
    pub fn avvia(store: Arc<ConfigStore>, segreti: Arc<dyn Segreti>, ascolta: Ascoltatore) -> CasaService {
        Self::avvia_con_diagnosi(store, segreti, ascolta, Arc::new(Diagnosi::default()))
    }

    /// Come `avvia`, con il registro diagnostico dato (acceso o spento).
    pub fn avvia_con_diagnosi(
        store: Arc<ConfigStore>,
        segreti: Arc<dyn Segreti>,
        ascolta: Ascoltatore,
        diagnosi: Arc<Diagnosi>,
    ) -> CasaService {
        Self::avvia_per_prova(store, segreti, ascolta, client::RITARDI_RICONNESSIONE.to_vec(), diagnosi)
    }

    /// Ritardi di riconnessione e registro diagnostico scelti (per i test).
    #[doc(hidden)]
    pub fn avvia_per_prova(
        store: Arc<ConfigStore>,
        segreti: Arc<dyn Segreti>,
        ascolta: Ascoltatore,
        ritardi: Vec<Duration>,
        diagnosi: Arc<Diagnosi>,
    ) -> CasaService {
        // Unico provider di rustls, esplicito: se un'altra dipendenza attivasse aws-lc-rs
        // la scelta automatica fallirebbe e wss:// tornerebbe ad andare in panico.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let config = Arc::new(Mutex::new(store.leggi()));
        let (tx, rx) = mpsc::unbounded_channel();
        let motore = Motore::new(store, segreti, ascolta, config.clone(), ritardi, diagnosi.clone());
        let thread = std::thread::Builder::new()
            .name("casa-service".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("runtime tokio del servizio casa");
                rt.block_on(motore.esegui(rx));
            })
            .expect("thread del servizio casa");
        CasaService { tx, config, thread: Some(thread), diagnosi }
    }

    /// Il registro diagnostico (spento se non acceso all'avvio).
    pub fn diagnosi(&self) -> &Diagnosi {
        &self.diagnosi
    }

    pub fn connetti_attivo(&self) {
        self.manda(Comando::ConnettiAttivo);
    }

    pub fn connetti(&self, env_id: &str) {
        self.manda(Comando::Connetti(env_id.to_owned()));
    }

    pub fn disconnetti(&self) {
        self.manda(Comando::Disconnetti);
    }

    /// L'esito arriva come `CasaEvento::Esito` con lo stesso `numero` (scelto da chi chiama).
    pub fn comando(&self, entity_id: &str, dominio: &str, servizio: &str, numero: u64) {
        self.manda(Comando::Servizio {
            entity_id: entity_id.to_owned(),
            dominio: dominio.to_owned(),
            servizio: servizio.to_owned(),
            numero,
        });
    }

    /// Archivia in Home Assistant la notifica persistente `id` (`persistent_notification.dismiss`).
    /// Se riesce arriva `NotificaRimossa` (da HA); se no `NotificaNonArchiviata`.
    pub fn archivia_notifica(&self, id: &str) {
        self.manda(Comando::Archivia(id.to_owned()));
    }

    /// `fatto` viene chiamata dal thread del servizio.
    pub fn prova_connessione(
        &self,
        url: String,
        token: String,
        fatto: impl FnOnce(TestConnectionResult) + Send + 'static,
    ) {
        self.manda(Comando::Prova { url, token, fatto: Box::new(fatto) });
    }

    pub fn config(&self) -> Config {
        blocca(&self.config).clone()
    }

    /// Legge solo ciò che serve della configurazione, senza copiarla tutta.
    /// `f` gira col lucchetto preso: non deve richiamare il servizio.
    pub fn con_config<R>(&self, f: impl FnOnce(&Config) -> R) -> R {
        f(&blocca(&self.config))
    }

    /// Applica `f`, salva (sul thread del servizio) e, se cambiano preferiti,
    /// nascosti o ordini, rimappa gli ultimi dati senza rileggerli.
    /// `f` gira col lucchetto della configurazione preso: non deve richiamare il servizio.
    pub fn aggiorna_config(&self, f: impl FnOnce(&mut Config)) {
        let mut config = blocca(&self.config);
        let prima = config.clone();
        f(&mut config);
        if *config == prima {
            return;
        }
        let rimappa = prima.favorites != config.favorites
            || prima.hidden_entities != config.hidden_entities
            || prima.room_order != config.room_order
            || prima.device_order != config.device_order
            || prima.meteo_entita != config.meteo_entita;
        // I sensori scelti per lati e quadranti si seguono sempre: cambiandoli si rifà l'iscrizione.
        let risegui = prima.sensori_scelti() != config.sensori_scelti();
        // Inviato sotto il lucchetto: i salvataggi arrivano nell'ordine delle modifiche.
        self.manda(Comando::Salva { config: Box::new(config.clone()), rimappa, risegui });
    }

    fn manda(&self, c: Comando) {
        // Se il thread è terminato non c'è nessuno a cui consegnare: si ignora.
        let _ = self.tx.send(c);
    }
}

impl Drop for CasaService {
    fn drop(&mut self) {
        self.manda(Comando::Ferma);
        if let Some(t) = self.thread.take() {
            // Mai attendere sé stessi (servizio rilasciato dentro l'Ascoltatore).
            if t.thread().id() != std::thread::current().id() {
                let _ = t.join();
            }
        }
    }
}

fn blocca(m: &Mutex<Config>) -> MutexGuard<'_, Config> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ─── Il thread del servizio ───────────────────────────────────────────────────

/// Risposte di Home Assistant da cui si ricostruisce tutto.
struct Grezzi {
    states: Vec<HaState>,
    areas: Vec<HaArea>,
    devices: Vec<HaDevice>,
    entries: Vec<HaEntityEntry>,
    config: Option<HaConfig>,
}

/// I soli tre registri, riletti dopo un evento che li tocca (Task 14h).
struct Registri {
    areas: Vec<HaArea>,
    devices: Vec<HaDevice>,
    entries: Vec<HaEntityEntry>,
}

/// Messaggi dai task del runtime al motore, con la generazione della connessione
/// che li ha prodotti: quelli di una connessione superata si scartano.
enum Interno {
    Client(u64, DalClient),
    /// Esito di una lettura completa (`Err`: senza gli stati non c'è nulla da mostrare).
    Grezzi(u64, Result<Box<Grezzi>, String>),
    /// Esito della lettura dei soli registri (`Err`: almeno uno non è arrivato).
    Registri(u64, Result<Box<Registri>, String>),
}

/// Cosa comporta, per ciò che si vede, il cambio di stato di un'entità.
#[derive(Debug, PartialEq)]
// Valore di ritorno di vita breve, mai in un vettore: la variante grande non costa nulla.
#[allow(clippy::large_enum_variant)]
enum Effetto {
    /// Nulla che l'app mostri (entità mai mostrata, attributi che non usiamo, `last_updated`).
    Niente,
    /// Cambia solo il contenuto di un'entità mostrata: la si sostituisce dov'è.
    Sostituisci(AppEntity),
    /// Cambiano presenza, nome o conteggi: serve la mappatura completa (in memoria, niente letture).
    Rimappa,
    /// Cambia solo il meteo scelto (`None`: non disponibile): si scrive in `MenuData::meteo` e si
    /// pubblica come una sostituzione, senza mappatura completa (Task 2 della M2).
    Meteo(Option<Meteo>),
}

/// I dati di Home Assistant in memoria per la connessione corrente: stati, registri indicizzati
/// e l'ultimo `MenuData` costruito. Gli `state_changed` li aggiornano un'entità alla volta.
struct Casa {
    stati: Vec<HaState>,
    posizione: HashMap<String, usize>,
    indici: Indici,
    config: Option<HaConfig>,
    menu: Arc<MenuData>,
    /// `Config::meteo_entita` all'ultima mappatura completa (cambiarla fa rimappare).
    meteo_preferita: Option<String>,
}

impl Casa {
    fn new(g: Grezzi, mapper: &mut EntityMapper) -> Casa {
        mapper.usa_config(g.config.as_ref());
        let indici = Indici::new(g.areas, g.devices, g.entries);
        let stati: Vec<HaState> = g
            .states
            .into_iter()
            .map(|s| {
                let mostrata = mapper.mappa_entita(&indici, &s).is_some();
                alleggerisci(s, mostrata)
            })
            .collect();
        let posizione = stati.iter().enumerate().map(|(i, s)| (s.entity_id.clone(), i)).collect();
        Casa {
            stati,
            posizione,
            indici,
            config: g.config,
            menu: Arc::new(MenuData::default()),
            meteo_preferita: None,
        }
    }

    /// Il meteo scelto adesso: l'entità e ciò che se ne mostra.
    fn meteo_scelto(&self, unita: &str) -> (Option<String>, Option<Meteo>) {
        let scelta = scegli_meteo(&self.stati, self.meteo_preferita.as_deref());
        (scelta.map(|s| s.entity_id.clone()), scelta.and_then(|s| meteo_da(s, unita)))
    }

    fn stato(&self, entity_id: &str) -> Option<&HaState> {
        self.posizione.get(entity_id).map(|&i| &self.stati[i])
    }

    /// Scrive lo stato nuovo di un'entità (`None`: tolta); `mostrata`: se no, lo si tiene alleggerito.
    fn memorizza(&mut self, entity_id: &str, nuovo: Option<HaState>, mostrata: bool) {
        match (self.posizione.get(entity_id).copied(), nuovo) {
            (Some(i), Some(s)) => self.stati[i] = alleggerisci(s, mostrata),
            (None, Some(s)) => {
                self.posizione.insert(entity_id.to_owned(), self.stati.len());
                self.stati.push(alleggerisci(s, mostrata));
            }
            (Some(i), None) => {
                // Raro (un'entità tolta): si rifà l'indice.
                self.stati.remove(i);
                self.posizione = self.stati.iter().enumerate().map(|(i, s)| (s.entity_id.clone(), i)).collect();
            }
            (None, None) => {}
        }
    }

    /// Il cambio di un'entità `weather.*`. Solo il meteo scelto cambia ciò che si vede; la mappatura
    /// completa serve se un'entità meteo compare, sparisce o cambia nome (l'elenco del selettore) o
    /// se cambia quella scelta (la prima disponibile in automatico).
    fn applica_meteo(&mut self, mapper: &EntityMapper, entity_id: &str, nuovo: Option<HaState>) -> Effetto {
        let nome = |s: Option<&HaState>| s.map(|s| s.attributes["friendly_name"].as_str().map(str::to_owned));
        let nome_prima = nome(self.stato(entity_id));
        let (scelta_prima, meteo_prima) = self.meteo_scelto(mapper.unita());
        let nome_dopo = nome(nuovo.as_ref());
        self.memorizza(entity_id, nuovo, true);
        let (scelta_dopo, meteo_dopo) = self.meteo_scelto(mapper.unita());
        if nome_prima != nome_dopo || scelta_prima != scelta_dopo {
            return Effetto::Rimappa;
        }
        if meteo_prima == meteo_dopo {
            Effetto::Niente
        } else {
            Effetto::Meteo(meteo_dopo)
        }
    }

    /// Riscrive in `MenuData::sensori_numerici` il valore di adesso del sensore `entity_id` (dopo
    /// `applica`, che ha già rimappato se ne cambiano presenza, nome o unità). Vero se è cambiato:
    /// come il meteo, si pubblica senza mappatura completa.
    fn valore_numerico(&mut self, entity_id: &str) -> bool {
        let Some(i) = self.menu.sensori_numerici.iter().position(|n| n.entity_id == entity_id) else {
            return false;
        };
        let valore = self.stato(entity_id).and_then(|s| valore_numerico(&s.state));
        if self.menu.sensori_numerici[i].valore == valore {
            return false;
        }
        Arc::make_mut(&mut Arc::make_mut(&mut self.menu).sensori_numerici)[i].valore = valore;
        true
    }

    /// Mette in memoria lo stato nuovo di un'entità (`None`: tolta) e dice cosa cambia per l'app.
    fn applica(&mut self, mapper: &EntityMapper, entity_id: &str, nuovo: Option<HaState>) -> Effetto {
        if entity_id.starts_with("weather.") {
            return self.applica_meteo(mapper, entity_id, nuovo);
        }
        let vecchio = self.stato(entity_id);
        let prima = vecchio.and_then(|s| mapper.mappa_entita(&self.indici, s));
        let aggiornamento_prima = vecchio.is_some_and(aggiornamento_disponibile);
        // Il sensore numerico (quadranti e lati) per nome e unità: se compare, sparisce o cambia
        // l'uno o l'altra, cambia il catalogo e si rimappa; il solo valore lo riscrive
        // `Casa::valore_numerico` senza rimappare.
        let chiave = |n: SensoreNumerico| (n.nome, n.unita);
        let numerico_prima = vecchio.and_then(|s| sensore_numerico(&self.indici, s)).map(chiave);
        let dopo = nuovo.as_ref().and_then(|s| mapper.mappa_entita(&self.indici, s));
        let aggiornamento_dopo = nuovo.as_ref().is_some_and(aggiornamento_disponibile);
        let numerico_dopo = nuovo.as_ref().and_then(|s| sensore_numerico(&self.indici, s)).map(chiave);
        self.memorizza(entity_id, nuovo, dopo.is_some());
        if aggiornamento_prima != aggiornamento_dopo || numerico_prima != numerico_dopo {
            return Effetto::Rimappa;
        }
        match (prima, dopo) {
            (a, b) if a == b => Effetto::Niente,
            // Stesso nome: l'ordine nelle stanze non cambia, basta sostituirla.
            (Some(a), Some(b)) if a.name() == b.name() => Effetto::Sostituisci(b),
            _ => Effetto::Rimappa,
        }
    }
}

/// Uno stato che l'app non mostra si tiene senza i suoi attributi (un lettore multimediale o
/// un'automazione ne hanno tanti): resta solo `device_class`, che decide se un sensore si mostra.
/// Fanno eccezione le `weather.*` (poche, mai mostrate in stanze): l'entità scelta per il meteo
/// ha bisogno dei suoi attributi, e scegliere un'altra dalle Impostazioni non deve rileggere tutto.
/// Un'entità può diventare visibile in due modi: per il suo stato (un sensore che diventa di
/// temperatura), e allora lo stato nuovo arriva intero con lo `state_changed`; o per un
/// registro (un'entità non più nascosta o disattivata), e allora nessuno `state_changed` arriva
/// ma l'evento del registro fa rileggere tutto, stati interi compresi.
///
/// Ai `sensor.*` restano anche nome, unità e `state_class`: decidono se il sensore è numerico
/// (`sensore_numerico`, catalogo di quadranti e lati) e come si chiama, anche nelle mappature
/// fatte dalla memoria.
fn alleggerisci(mut s: HaState, mostrata: bool) -> HaState {
    if !mostrata && !s.entity_id.starts_with("weather.") {
        let sensore = s.entity_id.starts_with("sensor.");
        let mut tenuti = serde_json::Map::new();
        if let Value::Object(attributi) = &mut s.attributes {
            for chiave in ["device_class", "friendly_name", "unit_of_measurement", "state_class"] {
                if chiave != "device_class" && !sensore {
                    continue;
                }
                if let Some(v) = attributi.get_mut(chiave).map(Value::take) {
                    tenuti.insert(chiave.to_owned(), v);
                }
            }
        }
        s.attributes = if tenuti.is_empty() { Value::Null } else { Value::Object(tenuti) };
    }
    s
}

/// Un'attesa che si rinvia a ogni nuovo arrivo, con un tetto dal primo: (primo arrivo, scadenza).
#[derive(Debug, Clone, Copy, Default)]
struct Attesa(Option<(Instant, Instant)>);

impl Attesa {
    fn rinvia(&mut self, ora: Instant, attesa: Duration, tetto: Duration) {
        let primo = self.0.map_or(ora, |(p, _)| p);
        self.0 = Some((primo, (ora + attesa).min(primo + tetto)));
    }

    /// Arma da capo: scade `dopo` da `ora`, senza tener conto di un'attesa già armata (`rinvia`
    /// terrebbe il primo arrivo, e la scadenza potrebbe essere già passata).
    fn arma(&mut self, ora: Instant, dopo: Duration) {
        self.0 = Some((ora, ora + dopo));
    }

    fn scadenza(&self) -> Option<Instant> {
        self.0.map(|(_, s)| s)
    }

    fn annulla(&mut self) {
        self.0 = None;
    }
}

struct Motore {
    store: Arc<ConfigStore>,
    segreti: Arc<dyn Segreti>,
    ascolta: Ascoltatore,
    config: Arc<Mutex<Config>>,
    ritardi: Vec<Duration>,
    interni_tx: mpsc::UnboundedSender<Interno>,
    interni_rx: Option<mpsc::UnboundedReceiver<Interno>>,
    client: Option<(Client, tokio::task::JoinHandle<()>)>,
    generazione: u64,
    mapper: EntityMapper,
    /// I dati in memoria, dopo la prima lettura completa della connessione.
    casa: Option<Casa>,
    /// L'ultimo `MenuData` consegnato all'applicazione (non si riconsegna uguale).
    consegnato: Option<Arc<MenuData>>,
    /// Una lettura completa è in volo. I cambi che arrivano intanto non servono ai dati nuovi:
    /// dopo la lettura ci si riscrive e il primo evento dell'iscrizione porta gli stati di adesso.
    in_lettura: bool,
    /// Numero dell'ultima iscrizione `subscribe_entities` chiesta: gli eventi di quelle superate
    /// si scartano. Cresce sempre, anche fra una connessione e l'altra.
    iscrizione: u64,
    /// Le entità seguite dall'iscrizione della connessione corrente (`None`: nessuna ancora).
    seguite: Option<BTreeSet<String>>,
    /// Una lettura (completa o dei soli registri) chiesta mentre un'altra era in volo: parte
    /// alla fine di quella (`Niente`: nessuna).
    rileggi_dopo: Rilettura,
    /// Pubblicazione dei cambi visibili, raccolti in una raffica.
    pubblica: Attesa,
    /// Alla pubblicazione serve la mappatura completa (non bastano le sostituzioni).
    rimappa: bool,
    /// Rilettura in sospeso dopo gli eventi dei registri o della configurazione, e quale: la più
    /// ampia chiesta dagli eventi raccolti.
    registri: Attesa,
    rilettura_registri: Rilettura,
    /// Nuovo tentativo (una lettura completa) dopo una lettura fallita a connessione attiva, o con
    /// HA ancora in avvio, e l'attesa del prossimo (cresce fino al tetto). Si annulla quando parte
    /// una lettura completa, a ogni cambio di stato della connessione e alla riuscita.
    riprova: Attesa,
    attesa_riprova: Duration,
    /// La connessione è attiva (`Connesso` e non ancora caduta): solo allora si arma un tentativo.
    connesso: bool,
    /// Notifiche persistenti dell'ambiente connesso già consegnate all'applicazione.
    notifiche: BTreeSet<String>,
    /// L'ambiente a cui appartengono quelle notifiche.
    ambiente_notifiche: Option<String>,
    /// Il nuovo tentativo armato è per HA in avvio (non per una lettura fallita): per il registro.
    riprova_per_avvio: bool,
    diagnosi: Arc<Diagnosi>,
}

impl Motore {
    fn new(
        store: Arc<ConfigStore>,
        segreti: Arc<dyn Segreti>,
        ascolta: Ascoltatore,
        config: Arc<Mutex<Config>>,
        ritardi: Vec<Duration>,
        diagnosi: Arc<Diagnosi>,
    ) -> Motore {
        let (interni_tx, interni_rx) = mpsc::unbounded_channel();
        Motore {
            store,
            segreti,
            ascolta,
            config,
            ritardi,
            interni_tx,
            interni_rx: Some(interni_rx),
            client: None,
            generazione: 0,
            mapper: EntityMapper::new(),
            casa: None,
            consegnato: None,
            in_lettura: false,
            iscrizione: 0,
            seguite: None,
            rileggi_dopo: Rilettura::Niente,
            pubblica: Attesa::default(),
            rimappa: false,
            registri: Attesa::default(),
            rilettura_registri: Rilettura::Niente,
            riprova: Attesa::default(),
            attesa_riprova: RIPROVA_LETTURA,
            connesso: false,
            notifiche: BTreeSet::new(),
            ambiente_notifiche: None,
            riprova_per_avvio: false,
            diagnosi,
        }
    }

    async fn esegui(mut self, mut rx: mpsc::UnboundedReceiver<Comando>) {
        let mut interni = self.interni_rx.take().expect("ricevitore interno");
        loop {
            let (pubblica, registri, riprova) =
                (self.pubblica.scadenza(), self.registri.scadenza(), self.riprova.scadenza());
            tokio::select! {
                c = rx.recv() => match c {
                    None | Some(Comando::Ferma) => break,
                    Some(c) => self.comando(c),
                },
                Some(i) = interni.recv() => self.interno(i),
                _ = sleep_until(pubblica.unwrap_or_else(Instant::now)), if pubblica.is_some() => {
                    self.pubblica.annulla();
                    self.diagnosi.conta(Contatore::pubblicazioni);
                    if std::mem::take(&mut self.rimappa) {
                        self.mappa_tutto();
                        self.segui(false);
                    }
                    self.consegna();
                }
                _ = sleep_until(registri.unwrap_or_else(Instant::now)), if registri.is_some() => {
                    self.registri.annulla();
                    match std::mem::replace(&mut self.rilettura_registri, Rilettura::Niente) {
                        Rilettura::Registri => self.rileggi_registri(),
                        _ => self.rileggi(),
                    }
                }
                _ = sleep_until(riprova.unwrap_or_else(Instant::now)), if riprova.is_some() => {
                    self.riprova.annulla();
                    let causa = if self.riprova_per_avvio { Contatore::rilettura_avvio } else { Contatore::rilettura_riprova };
                    self.diagnosi.conta(causa);
                    self.rileggi();
                }
            }
        }
        self.chiudi_client();
        self.dimentica_notifiche();
    }

    fn emetti(&self, e: CasaEvento) {
        (self.ascolta)(e);
    }

    fn comando(&mut self, c: Comando) {
        match c {
            Comando::ConnettiAttivo => {
                let attivo = blocca(&self.config).active_env_id.clone();
                match attivo {
                    Some(id) => self.connetti(&id),
                    None => {
                        self.chiudi_client();
                        self.dimentica_notifiche();
                        self.emetti(CasaEvento::Stato(StatoConnessione::NonConfigurato));
                    }
                }
            }
            Comando::Connetti(id) => self.connetti(&id),
            Comando::Disconnetti => {
                self.chiudi_client();
                self.dimentica_notifiche();
                self.emetti(CasaEvento::Stato(StatoConnessione::Disconnesso));
            }
            Comando::Servizio { entity_id, dominio, servizio, numero } => {
                let Some((client, _)) = &self.client else {
                    self.emetti(CasaEvento::Esito { entity_id, numero, ok: false, errore: Some(NON_CONNESSO.into()) });
                    return;
                };
                let client = client.clone();
                let ascolta = self.ascolta.clone();
                tokio::spawn(async move {
                    let r = client.call_service(&dominio, &servizio, &entity_id).await;
                    ascolta(CasaEvento::Esito { entity_id, numero, ok: r.is_ok(), errore: r.err() });
                });
            }
            Comando::Archivia(id) => {
                let Some((client, _)) = &self.client else {
                    self.emetti(CasaEvento::NotificaNonArchiviata { id, errore: NON_CONNESSO.into() });
                    return;
                };
                let client = client.clone();
                let ascolta = self.ascolta.clone();
                tokio::spawn(async move {
                    let dati = json!({ "notification_id": id });
                    if let Err(errore) = client.servizio_con_dati("persistent_notification", "dismiss", dati).await {
                        ascolta(CasaEvento::NotificaNonArchiviata { id, errore });
                    }
                });
            }
            Comando::Prova { url, token, fatto } => {
                tokio::spawn(async move { fatto(client::prova_connessione(&url, &token).await) });
            }
            Comando::Salva { config, rimappa, risegui } => {
                self.salva(&config);
                if rimappa {
                    self.mappa_tutto();
                    // Un'entità nascosta o mostrata cambia quelle da seguire.
                    self.segui(false);
                    self.consegna();
                } else if risegui {
                    // Un sensore scelto per un lato o un quadrante: solo l'iscrizione.
                    self.segui(false);
                }
            }
            Comando::Ferma => {}
        }
    }

    /// Un errore di scrittura non annulla la modifica in memoria: lo si segnala.
    fn salva(&self, config: &Config) {
        if let Err(e) = self.store.salva(config) {
            self.emetti(CasaEvento::ConfigNonSalvata(format!(
                "impossibile salvare la configurazione: {e}"
            )));
        }
    }

    fn connetti(&mut self, env_id: &str) {
        self.chiudi_client();
        // Riconnettersi allo stesso ambiente (es. «Riprova») non toglie le sue notifiche: le
        // riallinea l'elenco "current" alla connessione. Un altro ambiente invece le fa uscire.
        if self.ambiente_notifiche.as_deref() != Some(env_id) {
            self.dimentica_notifiche();
            self.ambiente_notifiche = Some(env_id.to_owned());
        }
        let url = blocca(&self.config)
            .environments
            .iter()
            .find(|e| e.id == env_id)
            .map(|e| e.ha_url.clone());
        let Some(url) = url else {
            // L'ambiente non esiste (più): come non averne nessuno.
            self.emetti(CasaEvento::Stato(StatoConnessione::NonConfigurato));
            return;
        };
        // L'ambiente scelto diventa quello attivo e sopravvive al riavvio.
        let da_salvare = {
            let mut c = blocca(&self.config);
            (c.active_env_id.as_deref() != Some(env_id)).then(|| {
                c.active_env_id = Some(env_id.to_owned());
                c.clone()
            })
        };
        if let Some(c) = da_salvare {
            self.salva(&c);
        }
        let token = match self.segreti.leggi(env_id) {
            Ok(Some(t)) if !t.is_empty() => t,
            Ok(_) => {
                self.emetti(CasaEvento::Stato(StatoConnessione::TokenMancante));
                return;
            }
            Err(e) => {
                self.emetti(CasaEvento::Stato(StatoConnessione::Errore(e)));
                return;
            }
        };
        let generazione = self.generazione;
        let tx = self.interni_tx.clone();
        let uscita: client::Uscita = Box::new(move |e| {
            let _ = tx.send(Interno::Client(generazione, e));
        });
        self.client = Some(client::avvia(url, token, self.ritardi.clone(), uscita, self.diagnosi.clone()));
    }

    /// Ferma la connessione corrente e invalida tutto ciò che ha ancora in volo. Le notifiche
    /// consegnate restano: alla prossima connessione allo stesso ambiente l'elenco "current" le
    /// riallinea (`dimentica_notifiche` solo per disconnessione o cambio d'ambiente).
    fn chiudi_client(&mut self) {
        if let Some((_, task)) = self.client.take() {
            task.abort();
        }
        self.generazione += 1;
        self.casa = None;
        self.consegnato = None;
        self.in_lettura = false;
        self.seguite = None;
        self.rileggi_dopo = Rilettura::Niente;
        self.connesso = false;
        self.ferma_attese();
    }

    fn ferma_attese(&mut self) {
        self.pubblica.annulla();
        self.registri.annulla();
        self.rilettura_registri = Rilettura::Niente;
        self.riprova.annulla();
        self.rimappa = false;
    }

    /// Le notifiche dell'ambiente lasciato non sono più raggiungibili: escono.
    fn dimentica_notifiche(&mut self) {
        self.ambiente_notifiche = None;
        for id in std::mem::take(&mut self.notifiche) {
            self.emetti(CasaEvento::NotificaRimossa { id });
        }
    }

    fn notifiche(&mut self, tipo: Aggiornamento, notifiche: Vec<crate::ha::eventi::NotificaHa>) {
        if tipo == Aggiornamento::Attuali {
            // L'elenco completo: escono quelle sparite mentre non eravamo connessi.
            let presenti: BTreeSet<&str> = notifiche.iter().map(|n| n.id.as_str()).collect();
            let sparite: Vec<String> =
                self.notifiche.iter().filter(|id| !presenti.contains(id.as_str())).cloned().collect();
            for id in sparite {
                self.notifiche.remove(&id);
                self.emetti(CasaEvento::NotificaRimossa { id });
            }
        }
        for n in notifiche {
            if tipo == Aggiornamento::Rimosse {
                if self.notifiche.remove(&n.id) {
                    self.emetti(CasaEvento::NotificaRimossa { id: n.id });
                }
            } else {
                self.notifiche.insert(n.id.clone());
                self.emetti(CasaEvento::NotificaAggiunta { id: n.id, titolo: n.titolo, messaggio: n.messaggio });
            }
        }
    }

    fn interno(&mut self, i: Interno) {
        match i {
            Interno::Client(g, _) | Interno::Grezzi(g, _) | Interno::Registri(g, _) if g != self.generazione => {}
            Interno::Client(_, DalClient::Stato(s)) => {
                let connesso = s == StatoConnessione::Connesso;
                self.connesso = connesso;
                if !connesso {
                    self.ferma_attese();
                }
                self.emetti(CasaEvento::Stato(s));
                if connesso {
                    // Una (ri)connessione consegna sempre i dati, anche se uguali a prima, e
                    // deve fare la sua lettura (finché non riesce si riprova).
                    self.consegnato = None;
                    // Nessun tentativo della connessione di prima; la lettura parte qui sotto.
                    self.riprova.annulla();
                    self.attesa_riprova = RIPROVA_LETTURA;
                    // La sessione nuova non ha ancora nessuna iscrizione agli stati.
                    self.seguite = None;
                    self.diagnosi.conta(Contatore::rilettura_connessione);
                    self.rileggi();
                }
            }
            Interno::Client(_, DalClient::Evento(EventoWs::Entita { iscrizione, aggiunte, cambiate, tolte })) => {
                if iscrizione == self.iscrizione {
                    self.entita_cambiate(aggiunte, cambiate, tolte);
                }
            }
            Interno::Client(_, DalClient::Evento(EventoWs::StatoIlleggibile)) => {
                self.diagnosi.conta(Contatore::rilettura_illeggibile);
                self.rileggi();
            }
            Interno::Client(_, DalClient::Evento(EventoWs::DaRileggere)) => self.chiedi_rilettura(Rilettura::Tutto),
            Interno::Client(_, DalClient::Evento(EventoWs::Registro(c))) => {
                self.conta_campi(&c);
                let serve = self.rilettura_per(&c);
                if serve == Rilettura::Niente {
                    self.diagnosi.conta(Contatore::registro_scartati);
                } else {
                    self.chiedi_rilettura(serve);
                }
            }
            Interno::Client(_, DalClient::Evento(EventoWs::Notifiche { tipo, notifiche })) => {
                self.notifiche(tipo, notifiche);
            }
            Interno::Grezzi(_, esito) => self.letti(esito),
            Interno::Registri(_, esito) => self.registri_letti(esito),
        }
    }

    /// I campi di `changes` nel registro diagnostico (nomi da un elenco fisso, mai valori).
    fn conta_campi(&self, c: &CambioRegistro) {
        let (CambioRegistro::Entita { azione: Azione::Aggiorna, campi, .. }
        | CambioRegistro::Dispositivo { azione: Azione::Aggiorna, campi, .. }) = c
        else {
            return;
        };
        if campi.is_empty() {
            self.diagnosi.conta(Contatore::registro_campo_vuoto);
        }
        for campo in campi {
            self.diagnosi.conta(crate::diagnosi::contatore_campo(campo));
        }
    }

    /// Cosa rileggere dopo un evento di un registro (Task 14h). Senza dati in memoria non si sa
    /// cosa l'app mostra: si rilegge tutto, come prima.
    fn rilettura_per(&self, c: &CambioRegistro) -> Rilettura {
        // Una lettura in volo o in coda: i registri in memoria possono essere di prima dell'evento
        // (HA può averlo mandato dopo una delle risposte). Si decide dai soli campi e dominio, e
        // ciò che dipende dai registri li rilegge dopo (giro di correzioni 1, I2). Durante la prima
        // lettura (nessun dato ancora) basta lo stesso: porterà comunque gli stati di adesso (M3).
        let lettura_pendente =
            self.in_lettura || self.rilettura_registri != Rilettura::Niente || self.rileggi_dopo != Rilettura::Niente;
        if lettura_pendente {
            return c.rilettura_prudente();
        }
        let Some(casa) = &self.casa else { return Rilettura::Tutto };
        let mostrata = |id: &str| {
            casa.stato(id).is_some_and(|s| {
                self.mapper.mappa_entita(&casa.indici, s).is_some() || sensore_numerico(&casa.indici, s).is_some()
            }) || self.seguite.as_ref().is_some_and(|ids| ids.contains(id))
        };
        c.rilettura(&casa.indici, &mostrata)
    }

    /// Un evento dei registri o della configurazione chiede una rilettura: parte 1 s dopo l'ultimo
    /// (al più 5 s dopo il primo), la più ampia fra quelle chieste.
    fn chiedi_rilettura(&mut self, serve: Rilettura) {
        self.diagnosi.conta(Contatore::rilettura_registri);
        if serve == Rilettura::Registri {
            self.diagnosi.conta(Contatore::rilettura_solo_registri);
        }
        self.rilettura_registri = self.rilettura_registri.max(serve);
        self.registri.rinvia(Instant::now(), ATTESA_REGISTRI, TETTO_REGISTRI);
    }

    /// Un evento dell'iscrizione alle entità: stati interi (`aggiunte`, il primo evento li porta
    /// tutti), cambi compressi da applicare allo stato in memoria, entità tolte. Si aggiorna in
    /// memoria solo ciò che arriva; se qualcosa si vede, la pubblicazione parte alla fine della
    /// raffica. Un cambio che non si applica (entità sconosciuta, diff illeggibile) fa rileggere.
    fn entita_cambiate(&mut self, aggiunte: Vec<HaState>, cambiate: Vec<(String, Value)>, tolte: Vec<String>) {
        let Some(casa) = &self.casa else { return };
        self.diagnosi.conta_n(Contatore::entita_a, aggiunte.len() as u64);
        self.diagnosi.conta_n(Contatore::entita_c, cambiate.len() as u64);
        self.diagnosi.conta_n(Contatore::entita_r, tolte.len() as u64);
        let mut nuovi: Vec<(String, Option<HaState>)> =
            aggiunte.into_iter().map(|s| (s.entity_id.clone(), Some(s))).collect();
        for (id, diff) in cambiate {
            match casa.stato(&id).and_then(|vecchio| applica_diff(vecchio, &diff)) {
                Some(s) => nuovi.push((id, Some(s))),
                None => {
                    self.diagnosi.conta(Contatore::rilettura_illeggibile);
                    return self.rileggi();
                }
            }
        }
        nuovi.extend(tolte.into_iter().map(|id| (id, None)));
        let mut visibile = false;
        for (id, nuovo) in nuovi {
            visibile |= self.stato_cambiato(&id, nuovo);
        }
        if visibile {
            self.pubblica.rinvia(Instant::now(), RAFFICA, TETTO_RAFFICA);
        }
    }

    /// Lo stato nuovo di un'entità (`None`: tolta) in memoria. Vero se cambia qualcosa che si vede.
    fn stato_cambiato(&mut self, entity_id: &str, nuovo: Option<HaState>) -> bool {
        let Some(casa) = &mut self.casa else { return false };
        let effetto = casa.applica(&self.mapper, entity_id, nuovo);
        // Il valore di un sensore numerico (quadranti e lati): riscritto dov'è, senza rimappare,
        // anche se l'entità non è mostrata in stanze e preferiti (una potenza) o è nascosta.
        let valore = !matches!(effetto, Effetto::Rimappa) && entity_id.starts_with("sensor.") && casa.valore_numerico(entity_id);
        if valore {
            self.diagnosi.conta(Contatore::effetto_sensore);
        }
        valore
            | match effetto {
            Effetto::Niente => {
                self.diagnosi.conta(Contatore::effetto_niente);
                false
            }
            // Falso se è mostrabile ma nascosta dall'utente.
            Effetto::Sostituisci(e) => {
                let visibile = sostituisci_in_menu(Arc::make_mut(&mut casa.menu), &e);
                self.diagnosi.conta(if visibile { Contatore::effetto_sostituita } else { Contatore::effetto_nascosta });
                visibile
            }
            Effetto::Meteo(meteo) => {
                Arc::make_mut(&mut casa.menu).meteo = meteo;
                self.diagnosi.conta(Contatore::effetto_meteo);
                true
            }
            Effetto::Rimappa => {
                self.diagnosi.conta(Contatore::effetto_rimappa);
                self.rimappa = true;
                true
            }
        }
    }

    /// Le entità da seguire: quelle che l'app mostra (preferiti, stanze, scene, telecamere, e il
    /// catalogo delle Impostazioni, dove ci sono anche quelle nascoste dall'utente), le voci del
    /// registro che mostrerà appena avranno uno stato e gli `update.*`, che contano gli
    /// aggiornamenti in `MenuData`. Le nascoste dall'utente si seguono (giro di correzioni 1 del
    /// 14h): il loro nome nelle Impostazioni viene dal `friendly_name` dello stato, che HA riscrive
    /// quando l'integrazione o il dispositivo cambiano nome; e mostrandole lo stato è già quello di
    /// adesso. Più le `weather.*`, per il meteo (Task 2 della M2), e i sensori scelti per i lati
    /// della pillola e per i quadranti (Task 4 della M2: di solito sono già fra quelli mostrati, ma
    /// uno nascosto più tardi resterebbe senza cambi). Solo aggiunte: l'insieme non si svuota per
    /// colpa loro (un elenco vuoto, per HA, è «tutte le entità»: lo esclude `iscrivi_entita`).
    fn da_seguire(casa: &Casa, config: &Config) -> BTreeSet<String> {
        let m = &casa.menu;
        m.favorites
            .iter()
            .chain(m.rooms.iter().flat_map(|r| &r.entities))
            .chain(&m.cameras)
            .map(|e| e.entity_id().to_owned())
            .chain(m.scenes.iter().map(|s| s.base.entity_id.clone()))
            .chain(m.catalogo.iter().flat_map(|s| &s.voci).map(|v| v.entity_id.clone()))
            .chain(casa.indici.mostrabili_dal_registro().map(str::to_owned))
            // Il meteo: tutte le `weather.*` (poche). Seguire solo la scelta perderebbe il ritorno
            // di un'entità tornata disponibile (l'automatico passerebbe a un'altra e quella non
            // sarebbe più seguita), e cambiare scelta dalle Impostazioni avrebbe stati vecchi.
            .chain(casa.stati.iter().filter(|s| s.entity_id.starts_with("weather.")).map(|s| s.entity_id.clone()))
            .chain(casa.stati.iter().filter(|s| s.entity_id.starts_with("update.")).map(|s| s.entity_id.clone()))
            .chain(config.sensori_scelti())
            .collect()
    }

    /// Rifà l'iscrizione `subscribe_entities` se cambiano le entità da seguire, o sempre se
    /// `sempre` (dopo una lettura completa: il primo evento porta gli stati di adesso). Fra due
    /// letture un'entità seguita che non c'è più (tolta con `r`) resta seguita: se torna (un
    /// ricaricamento) HA la rimanda con `a`.
    fn segui(&mut self, sempre: bool) {
        let (Some(casa), Some((client, _))) = (&self.casa, &self.client) else { return };
        let mut ids = Self::da_seguire(casa, &blocca(&self.config));
        if !sempre {
            if let Some(prima) = &self.seguite {
                let tolte = prima.iter().filter(|id| casa.stato(id).is_none()).cloned();
                ids.extend(tolte.collect::<Vec<_>>());
            }
        }
        if !sempre && self.seguite.as_ref() == Some(&ids) {
            return;
        }
        self.iscrizione += 1;
        self.diagnosi.conta(Contatore::iscrizioni);
        self.diagnosi.seguite(ids.len());
        client.iscrivi_entita(self.iscrizione, ids.iter().cloned().collect());
        self.seguite = Some(ids);
    }

    /// La lettura completa (`trigger_refresh` di v2): stati, registri e configurazione in
    /// parallelo. Solo alla connessione, dopo un cambio dei registri o con uno stato illeggibile.
    fn rileggi(&mut self) {
        let Some((client, _)) = &self.client else { return };
        if self.in_lettura {
            self.diagnosi.conta(Contatore::rilettura_accodata);
            self.rileggi_dopo = Rilettura::Tutto;
            return;
        }
        self.in_lettura = true;
        // Parte una lettura completa: un tentativo armato non serve più (giro 3, P2).
        self.riprova.annulla();
        self.diagnosi.conta(Contatore::riletture);
        let (client, tx, g) = (client.clone(), self.interni_tx.clone(), self.generazione);
        tokio::spawn(async move {
            let (states, areas, devices, entries, config) = tokio::join!(
                leggi::<Vec<HaState>>(&client, "get_states"),
                leggi::<Vec<HaArea>>(&client, "config/area_registry/list"),
                leggi::<Vec<HaDevice>>(&client, "config/device_registry/list"),
                leggi::<Vec<HaEntityEntry>>(&client, "config/entity_registry/list"),
                leggi::<HaConfig>(&client, "get_config"),
            );
            let esito = states.map(|states| Box::new(Grezzi {
                states,
                areas: areas.unwrap_or_default(),
                devices: devices.unwrap_or_default(),
                entries: entries.unwrap_or_default(),
                config: config.ok(),
            }));
            let _ = tx.send(Interno::Grezzi(g, esito));
        });
    }

    /// Rilegge i soli tre registri (Task 14h): dopo un evento che cambia nomi, stanze o ciò che si
    /// mostra. Gli stati restano quelli seguiti dall'iscrizione, che si rifà solo se cambiano le
    /// entità da seguire. Su una casa vera `get_states` e il primo evento di un'iscrizione rifatta
    /// sono la gran parte dei byte di una lettura completa.
    fn rileggi_registri(&mut self) {
        let Some((client, _)) = &self.client else { return };
        if self.in_lettura {
            self.diagnosi.conta(Contatore::rilettura_accodata);
            self.rileggi_dopo = self.rileggi_dopo.max(Rilettura::Registri);
            return;
        }
        self.in_lettura = true;
        self.diagnosi.conta(Contatore::letture_registri);
        let (client, tx, g) = (client.clone(), self.interni_tx.clone(), self.generazione);
        tokio::spawn(async move {
            let (areas, devices, entries) = tokio::join!(
                leggi::<Vec<HaArea>>(&client, "config/area_registry/list"),
                leggi::<Vec<HaDevice>>(&client, "config/device_registry/list"),
                leggi::<Vec<HaEntityEntry>>(&client, "config/entity_registry/list"),
            );
            let esito = match (areas, devices, entries) {
                (Ok(areas), Ok(devices), Ok(entries)) => Ok(Box::new(Registri { areas, devices, entries })),
                (a, d, e) => Err(a.err().or(d.err()).or(e.err()).unwrap_or_default()),
            };
            let _ = tx.send(Interno::Registri(g, esito));
        });
    }

    /// I registri riletti: si rimappa in memoria e si consegna (se cambia qualcosa), e ci si
    /// riscrive solo se cambiano le entità da seguire. Se un'entità diventa mostrata ma il suo
    /// stato in memoria è alleggerito (non era mostrata: senza attributi), o una lettura non è
    /// riuscita, si rilegge tutto.
    fn registri_letti(&mut self, esito: Result<Box<Registri>, String>) {
        self.in_lettura = false;
        let completa = match esito {
            Err(e) => {
                self.diagnosi.conta(Contatore::letture_fallite);
                eprintln!("lettura dei registri di Home Assistant non riuscita: {e}");
                // Come una lettura completa fallita: nuovo tentativo (completo) con l'attesa
                // crescente, non subito (HA che rifiuta ripeterebbe il rifiuto a raffica).
                self.arma_riprova();
                return self.dopo_lettura();
            }
            Ok(r) => match &mut self.casa {
                None => true,
                Some(casa) => {
                    let nuovi = Indici::new(r.areas, r.devices, r.entries);
                    let mapper = &self.mapper;
                    let scoperta = casa.stati.iter().any(|s| {
                        mapper.mappa_entita(&casa.indici, s).is_none() && mapper.mappa_entita(&nuovi, s).is_some()
                    });
                    if !scoperta {
                        casa.indici = nuovi;
                    }
                    scoperta
                }
            },
        };
        if completa {
            self.rileggi_dopo = Rilettura::Niente;
            return self.rileggi();
        }
        self.pubblica.annulla();
        self.rimappa = false;
        self.mappa_tutto();
        self.consegna();
        self.segui(false);
        self.dopo_lettura();
    }

    /// Una lettura finita: parte quella chiesta intanto, se c'è.
    fn dopo_lettura(&mut self) {
        match std::mem::replace(&mut self.rileggi_dopo, Rilettura::Niente) {
            Rilettura::Tutto => self.rileggi(),
            Rilettura::Registri => self.rileggi_registri(),
            Rilettura::Niente => {}
        }
    }

    /// La lettura completa è arrivata: dati nuovi, mappati e consegnati subito; poi l'iscrizione
    /// alle entità da seguire, rifatta: il suo primo evento porta gli stati di adesso, anche di
    /// ciò che è cambiato mentre la lettura era in volo.
    fn letti(&mut self, esito: Result<Box<Grezzi>, String>) {
        self.in_lettura = false;
        match esito {
            // Senza gli stati si tengono i dati precedenti.
            Err(e) => {
                self.diagnosi.conta(Contatore::letture_fallite);
                eprintln!("lettura di Home Assistant non riuscita: {e}");
                // Si riprova, ogni volta aspettando il doppio fino al tetto, anche dopo una lettura
                // riuscita (giro di correzioni 2 del 14h: una lettura chiesta da un evento non si
                // perde). Intanto si tengono i dati di prima e, dopo una riconnessione, ci si
                // iscrive comunque agli stati: il primo evento porta quelli di adesso.
                self.arma_riprova();
                if self.casa.is_some() && self.seguite.is_none() {
                    self.segui(true);
                }
            }
            Ok(g) => {
                // Riuscita (anche da una riconnessione): il tentativo in sospeso non serve più.
                self.riprova.annulla();
                self.attesa_riprova = RIPROVA_LETTURA;
                self.diagnosi.prima_lettura();
                let in_avvio = g.config.as_ref().and_then(|c| c.state.as_deref()).is_some_and(|s| s != "RUNNING");
                // Senza `get_config` (fallita) lo stato del core non è noto: «nessuna», non quello
                // della lettura di prima.
                match &g.config {
                    Some(c) => self.diagnosi.stato_ha(c.state.as_deref()),
                    None => self.diagnosi.stato_ha_non_letto(),
                }
                self.casa = Some(Casa::new(*g, &mut self.mapper));
                self.pubblica.annulla();
                self.rimappa = false;
                self.mappa_tutto();
                self.consegna();
                self.segui(true);
                // HA ancora in avvio: molte entità non ci sono ancora; si rilegge poco dopo, ma solo
                // a connessione attiva (come ogni altra riprova: alla riconnessione la lettura
                // parte comunque).
                if in_avvio && self.connesso {
                    self.riprova.arma(Instant::now(), ATTESA_AVVIO);
                    self.riprova_per_avvio = true;
                }
            }
        }
        // Anche una lettura dei soli registri chiesta intanto parte: quelli di questa lettura
        // possono essere di prima dell'evento.
        self.dopo_lettura();
    }

    /// Arma il nuovo tentativo di una lettura fallita (una lettura completa) con l'attesa di adesso,
    /// e raddoppia la prossima fino al tetto; la prima lettura completa riuscita la riporta a 2 s.
    fn arma_riprova(&mut self) {
        // A connessione caduta l'errore è delle letture che erano in volo: alla riconnessione la
        // lettura parte comunque (giro di correzioni 3, P1).
        if !self.connesso {
            return;
        }
        let attesa = self.attesa_riprova;
        self.riprova.arma(Instant::now(), attesa);
        self.riprova_per_avvio = false;
        self.attesa_riprova = (attesa * 2).min(TETTO_RIPROVA);
    }

    /// Ricostruisce tutto `MenuData` dai dati in memoria con la configurazione corrente.
    fn mappa_tutto(&mut self) {
        let Some(casa) = &mut self.casa else { return };
        self.diagnosi.conta(Contatore::mappature);
        let (favorites, hidden, room_order, device_order, meteo_entita) = {
            let c = blocca(&self.config);
            (
                c.favorites.clone(),
                c.hidden_entities.clone(),
                c.room_order.clone(),
                c.device_order.clone(),
                c.meteo_entita.clone(),
            )
        };
        let dati = self.mapper.map_indicizzato(
            &casa.indici,
            &casa.stati,
            casa.config.as_ref(),
            self.notifiche.len() as u32,
            Preferenze {
                favorites_order: &favorites,
                hidden_entities: &hidden,
                room_order: &room_order,
                device_orders: &device_order,
                meteo_entita: meteo_entita.as_deref(),
            },
        );
        casa.menu = Arc::new(dati);
        casa.meteo_preferita = meteo_entita;
    }

    /// Consegna l'ultimo `MenuData` se è diverso da quello già consegnato.
    fn consegna(&mut self) {
        let Some(casa) = &self.casa else { return };
        if self.consegnato.as_deref() == Some(&*casa.menu) {
            self.diagnosi.conta(Contatore::consegne_uguali);
            return;
        }
        self.diagnosi.conta(Contatore::consegne);
        self.consegnato = Some(casa.menu.clone());
        self.emetti(CasaEvento::Dati(casa.menu.clone()));
    }
}

async fn leggi<T: serde::de::DeserializeOwned>(client: &Client, tipo: &str) -> Result<T, String> {
    let v = client.richiesta(tipo, Value::Null).await?;
    serde_json::from_str(v.get()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod test {
    use super::*;

    fn stato(id: &str, s: &str, attributi: Value) -> HaState {
        HaState {
            entity_id: id.into(),
            state: s.into(),
            attributes: attributi,
            last_changed: String::new(),
            last_updated: String::new(),
        }
    }

    fn luce(s: &str) -> HaState {
        stato("light.cucina", s, json!({ "friendly_name": "Cucina", "supported_color_modes": ["onoff"] }))
    }

    fn casa(stati: Vec<HaState>) -> (Casa, EntityMapper) {
        let mut mapper = EntityMapper::new();
        let g = Grezzi { states: stati, areas: vec![], devices: vec![], entries: vec![], config: None };
        (Casa::new(g, &mut mapper), mapper)
    }

    #[test]
    fn cambio_di_stato_visibile_si_sostituisce() {
        let (mut c, m) = casa(vec![luce("off")]);
        let Effetto::Sostituisci(e) = c.applica(&m, "light.cucina", Some(luce("on"))) else { panic!() };
        assert!(matches!(e, AppEntity::Light(l) if l.is_on));
        assert_eq!(c.stato("light.cucina").unwrap().state, "on");
    }

    #[test]
    fn attributi_inutili_e_entita_nascoste_non_cambiano_nulla() {
        let potenza = |v: &str| stato("sensor.frigo", v, json!({ "device_class": "power", "unit_of_measurement": "W" }));
        let (mut c, m) = casa(vec![luce("off"), potenza("40")]);
        let mut con_attributo = luce("off");
        con_attributo.attributes["linkquality"] = json!(80);
        con_attributo.last_updated = "2026-11-15T10:00:00+00:00".into();
        assert_eq!(c.applica(&m, "light.cucina", Some(con_attributo)), Effetto::Niente);
        assert_eq!(c.applica(&m, "sensor.frigo", Some(potenza("41"))), Effetto::Niente);
        assert_eq!(c.applica(&m, "binary_sensor.porta", Some(stato("binary_sensor.porta", "on", json!({})))), Effetto::Niente);
    }

    #[test]
    fn nome_presenza_e_aggiornamenti_rimappano() {
        let (mut c, m) = casa(vec![luce("off")]);
        let mut rinominata = luce("off");
        rinominata.attributes["friendly_name"] = json!("Cucina grande");
        assert_eq!(c.applica(&m, "light.cucina", Some(rinominata)), Effetto::Rimappa);
        assert_eq!(c.applica(&m, "light.cucina", None), Effetto::Rimappa, "tolta");
        assert!(c.stato("light.cucina").is_none());
        assert_eq!(c.applica(&m, "switch.nuovo", Some(stato("switch.nuovo", "on", json!({})))), Effetto::Rimappa);
        assert_eq!(c.stato("switch.nuovo").unwrap().state, "on");
        assert_eq!(c.applica(&m, "update.core", Some(stato("update.core", "on", json!({})))), Effetto::Rimappa);
        assert_eq!(c.applica(&m, "update.core", Some(stato("update.core", "on", json!({ "x": 1 })))), Effetto::Niente);
    }

    fn tempo(id: &str, s: &str, temperatura: f64) -> HaState {
        stato(id, s, json!({ "friendly_name": id, "temperature": temperatura, "temperature_unit": "°C" }))
    }

    #[test]
    fn cambio_del_meteo_aggiorna_senza_rimappare() {
        let (mut c, m) = casa(vec![tempo("weather.a", "cloudy", 9.0), luce("off")]);
        let Effetto::Meteo(Some(meteo)) = c.applica(&m, "weather.a", Some(tempo("weather.a", "cloudy", 10.0))) else {
            panic!("atteso Effetto::Meteo")
        };
        assert_eq!((meteo.entity_id.as_str(), meteo.condizione.as_str(), meteo.temperatura), ("weather.a", "cloudy", Some(10.0)));
        assert_eq!(meteo.unita, "°C");
        // Gli attributi dell'entità scelta restano (non si alleggerisce).
        assert_eq!(c.stato("weather.a").unwrap().attributes["temperature"], json!(10.0));
        // Stesso stato: niente. Non disponibile: il meteo diventa `None`.
        assert_eq!(c.applica(&m, "weather.a", Some(tempo("weather.a", "cloudy", 10.0))), Effetto::Niente);
        // Con la sola entità: non disponibile cambia la scelta automatica (nessuna), quindi rimappa.
        assert_eq!(c.applica(&m, "weather.a", Some(tempo("weather.a", "unavailable", 10.0))), Effetto::Rimappa);
    }

    #[test]
    fn meteo_preferita_non_disponibile_e_nessuno_senza_rimappare() {
        let (mut c, m) = casa(vec![tempo("weather.a", "cloudy", 9.0), tempo("weather.b", "rainy", 5.0)]);
        c.meteo_preferita = Some("weather.b".into());
        assert_eq!(c.applica(&m, "weather.b", Some(tempo("weather.b", "unavailable", 5.0))), Effetto::Meteo(None));
        // Un'altra entità meteo che cambia non si vede.
        assert_eq!(c.applica(&m, "weather.a", Some(tempo("weather.a", "sunny", 12.0))), Effetto::Niente);
        // Una nuova entità meteo (elenco del selettore) o un nome nuovo rimappano.
        assert_eq!(c.applica(&m, "weather.c", Some(tempo("weather.c", "fog", 3.0))), Effetto::Rimappa);
    }

    #[test]
    fn meteo_seguito_nell_iscrizione() {
        let (c, _) = casa(vec![tempo("weather.a", "cloudy", 9.0), tempo("weather.b", "unavailable", 5.0), luce("on")]);
        let seguite = Motore::da_seguire(&c, &Config::default());
        assert!(seguite.contains("weather.a"));
        assert!(seguite.contains("weather.b"), "anche non disponibile: se torna, il suo cambio arriva");
    }

    /// M2 Task 4: un sensore scelto per un quadrante (o un lato) entra nell'insieme seguito, anche
    /// se l'app non lo mostra (nascosto, o non ancora mappato).
    #[test]
    fn sensore_scelto_e_seguito() {
        let (c, _) = casa(vec![luce("on"), tempo("weather.a", "cloudy", 9.0)]);
        let config = Config {
            quadranti: vec!["cpu".into(), "sensore:sensor.produzione_fv".into()],
            lato_destro: "sensore:sensor.sala_t".into(),
            ..Config::default()
        };
        let seguite = Motore::da_seguire(&c, &config);
        assert!(seguite.contains("sensor.produzione_fv"), "{seguite:?}");
        assert!(seguite.contains("sensor.sala_t"), "{seguite:?}");
        assert!(seguite.contains("weather.a"), "quelle di prima restano");
        assert!(!seguite.contains("cpu"), "le fonti del PC non sono entità");
        // Senza scelte l'insieme è quello di prima.
        assert_eq!(Motore::da_seguire(&c, &Config::default()).len(), seguite.len() - 2);
    }

    #[test]
    fn stati_non_mostrati_senza_attributi() {
        let lettore = stato("media_player.tv", "idle", json!({ "source_list": ["a", "b"], "device_class": "tv" }));
        let (c, _) = casa(vec![lettore, luce("on")]);
        assert_eq!(c.stato("media_player.tv").unwrap().attributes, json!({ "device_class": "tv" }));
        assert_eq!(c.stato("light.cucina").unwrap().attributes["supported_color_modes"], json!(["onoff"]));
    }

    #[test]
    fn un_sensore_che_diventa_temperatura_si_mostra() {
        let (mut c, m) = casa(vec![stato("sensor.x", "3", json!({ "device_class": "power" }))]);
        let ora = stato("sensor.x", "21", json!({ "device_class": "temperature", "unit_of_measurement": "°C" }));
        assert_eq!(c.applica(&m, "sensor.x", Some(ora)), Effetto::Rimappa);
        assert_eq!(c.stato("sensor.x").unwrap().attributes["unit_of_measurement"], "°C");
    }

    /// M2, Task 4 (giro di correzioni 1): il valore di un sensore numerico (una potenza, che stanze
    /// e preferiti non mostrano) si riscrive in `sensori_numerici` senza rimappare, come il meteo;
    /// nome o unità nuovi rimappano. Il sensore non mostrato tiene nome e unità in memoria.
    #[test]
    fn valore_di_un_sensore_numerico_senza_rimappare() {
        let potenza = |v: &str, unita: &str| {
            stato("sensor.fv", v, json!({ "friendly_name": "FV", "device_class": "power", "unit_of_measurement": unita, "x": 1 }))
        };
        let (mut c, m) = casa(vec![potenza("1500", "W"), luce("off")]);
        assert_eq!(
            c.stato("sensor.fv").unwrap().attributes,
            json!({ "friendly_name": "FV", "device_class": "power", "unit_of_measurement": "W" }),
            "alleggerito, ma con nome e unità"
        );
        let nessuno: Vec<String> = vec![];
        let ordini = HashMap::new();
        let mut mapper = EntityMapper::new();
        c.menu = Arc::new(mapper.map_indicizzato(
            &c.indici,
            &c.stati,
            None,
            0,
            Preferenze {
                favorites_order: &nessuno,
                hidden_entities: &nessuno,
                room_order: &nessuno,
                device_orders: &ordini,
                meteo_entita: None,
            },
        ));
        assert_eq!(c.menu.sensori_numerici[0].valore, Some(1500.0));
        let prima = c.menu.clone();
        assert_eq!(c.applica(&m, "sensor.fv", Some(potenza("1720.5", "W"))), Effetto::Niente);
        assert!(c.valore_numerico("sensor.fv"), "il valore nuovo si riscrive");
        assert_eq!(c.menu.sensori_numerici[0].valore, Some(1720.5));
        assert_eq!(prima.sensori_numerici[0].valore, Some(1500.0), "il MenuData già consegnato non cambia");
        assert!(!c.valore_numerico("sensor.fv"), "stesso valore: niente");
        assert!(!c.valore_numerico("light.cucina"), "non è un sensore numerico");
        // Non disponibile: il valore diventa `None`, il sensore resta fra le scelte.
        assert_eq!(c.applica(&m, "sensor.fv", Some(potenza("unavailable", "W"))), Effetto::Niente);
        assert!(c.valore_numerico("sensor.fv"));
        assert_eq!(c.menu.sensori_numerici[0].valore, None);
        // Un'unità nuova cambia il catalogo: si rimappa.
        assert_eq!(c.applica(&m, "sensor.fv", Some(potenza("1.7", "kW"))), Effetto::Rimappa);
    }

    fn motore() -> (Motore, tempfile::TempDir) {
        let cartella = tempfile::tempdir().unwrap();
        let store = Arc::new(ConfigStore::apri(cartella.path()));
        let segreti: Arc<dyn Segreti> = Arc::new(crate::segreti::MemoriaSegreti::new());
        let config = Arc::new(Mutex::new(Config::default()));
        let mut m = Motore::new(store, segreti, Arc::new(|_| {}), config, vec![], Arc::new(Diagnosi::default()));
        // Come dopo `Connesso` (senza client la lettura non parte).
        m.interno(Interno::Client(m.generazione, DalClient::Stato(StatoConnessione::Connesso)));
        (m, cartella)
    }

    fn grezzi_vuoti() -> Box<Grezzi> {
        Box::new(Grezzi { states: vec![luce("off")], areas: vec![], devices: vec![], entries: vec![], config: None })
    }

    /// N1: la prima lettura che fallisce si riprova con un'attesa che raddoppia (2, 4, 8 … fino
    /// a 60 s), e che riparte da 2 s dopo una lettura riuscita.
    #[test]
    fn la_prima_lettura_fallita_si_riprova_aspettando_sempre_di_piu() {
        let (mut m, _c) = motore();
        let mut attese = Vec::new();
        for _ in 0..8 {
            let prima = Instant::now();
            m.letti(Err("rifiutata".into()));
            let scade = m.riprova.scadenza().expect("un nuovo tentativo armato");
            attese.push((scade - prima).as_secs_f32().round() as u64);
            m.riprova.annulla();
        }
        assert_eq!(attese, [2, 4, 8, 16, 32, 60, 60, 60]);
        m.letti(Ok(grezzi_vuoti()));
        let prima = Instant::now();
        m.letti(Err("rifiutata".into()));
        let scade = m.riprova.scadenza().unwrap();
        assert_eq!((scade - prima).as_secs_f32().round() as u64, 2, "dopo una lettura riuscita si riparte da 2 s");
    }

    /// N2: un nuovo tentativo in sospeso si annulla se intanto una lettura (di una riconnessione)
    /// riesce; una rilettura chiesta dai registri invece resta.
    #[test]
    fn una_lettura_riuscita_annulla_il_tentativo_in_sospeso() {
        let (mut m, _c) = motore();
        m.letti(Err("rifiutata".into()));
        assert!(m.riprova.scadenza().is_some());
        m.registri.rinvia(Instant::now(), ATTESA_REGISTRI, TETTO_REGISTRI);
        m.letti(Ok(grezzi_vuoti()));
        assert_eq!(m.riprova.scadenza(), None, "nessuna lettura in più dopo quella riuscita");
        assert!(m.registri.scadenza().is_some(), "la rilettura dei registri non si perde");
    }

    /// Giro di correzioni 1, I1: dopo una riconnessione la lettura fallisce ma ci sono i dati di
    /// prima: si riprova lo stesso, con l'attesa crescente.
    #[test]
    fn lettura_fallita_dopo_la_riconnessione_si_riprova() {
        let (mut m, _c) = motore();
        m.letti(Ok(grezzi_vuoti()));
        m.interno(Interno::Client(m.generazione, DalClient::Stato(StatoConnessione::Connesso)));
        m.letti(Err("HA si sta avviando".into()));
        assert!(m.riprova.scadenza().is_some(), "nuovo tentativo anche con i dati di prima");
        m.riprova.annulla();
        m.letti(Err("ancora".into()));
        assert!(m.riprova.scadenza().is_some());
        m.riprova.annulla();
        // Giro di correzioni 2 del 14h: anche dopo una lettura riuscita, una lettura fallita (chiesta
        // da un evento: entità nuova, rinominata) si riprova, ripartendo da 2 s; i dati restano.
        m.letti(Ok(grezzi_vuoti()));
        let prima = Instant::now();
        m.letti(Err("dopo una riuscita".into()));
        let scade = m.riprova.scadenza().expect("nuovo tentativo anche dopo una lettura riuscita");
        assert_eq!((scade - prima).as_secs_f32().round() as u64, 2);
        assert!(m.casa.is_some(), "si tengono i dati");
        m.riprova.annulla();
        m.letti(Err("ancora".into()));
        let scade = m.riprova.scadenza().unwrap();
        assert_eq!((scade - Instant::now()).as_secs_f32().round() as u64, 4, "attesa crescente");
    }

    /// B1: con HA ancora in avvio (`get_config` → «STARTING») si rilegge dopo 5 s; pronto no.
    #[test]
    fn ha_in_avvio_arma_una_rilettura() {
        let (mut m, _c) = motore();
        let config = |stato: &str| {
            Some(HaConfig {
                unit_system: crate::ha::models::HaUnitSystem { temperature: "°C".into() },
                location_name: "Casa".into(),
                state: Some(stato.into()),
            })
        };
        let mut g = grezzi_vuoti();
        g.config = config("STARTING");
        let prima = Instant::now();
        m.letti(Ok(g));
        let scade = m.riprova.scadenza().expect("rilettura armata");
        assert_eq!((scade - prima).as_secs_f32().round() as u64, 5);
        m.riprova.annulla();
        let mut g = grezzi_vuoti();
        g.config = config("RUNNING");
        m.letti(Ok(g));
        assert_eq!(m.riprova.scadenza(), None);
    }

    /// Giro finale, N1 del 14h: con HA in avvio la rilettura dei 5 s non si arma a connessione
    /// caduta (la lettura arrivata dopo la caduta è di prima); a connessione attiva sì.
    #[test]
    fn ha_in_avvio_a_connessione_caduta_non_arma_la_rilettura() {
        let (mut m, _c) = motore();
        let avvio = || {
            let mut g = grezzi_vuoti();
            g.config = Some(HaConfig {
                unit_system: crate::ha::models::HaUnitSystem { temperature: "°C".into() },
                location_name: "Casa".into(),
                state: Some("STARTING".into()),
            });
            g
        };
        let caduta = StatoConnessione::Errore("connessione chiusa da Home Assistant".into());
        m.interno(Interno::Client(m.generazione, DalClient::Stato(caduta)));
        m.letti(Ok(avvio()));
        assert_eq!(m.riprova.scadenza(), None, "nessun tentativo a connessione caduta");
        assert!(!m.riprova_per_avvio);
        m.interno(Interno::Client(m.generazione, DalClient::Stato(StatoConnessione::Connesso)));
        m.letti(Ok(avvio()));
        assert!(m.riprova.scadenza().is_some(), "a connessione attiva sì");
        assert!(m.riprova_per_avvio);
    }

    /// Gli eventi di un'iscrizione alle entità già sostituita (arrivati dopo che il servizio ne
    /// ha chiesta un'altra) si scartano: il primo evento della nuova porta gli stati di adesso.
    #[test]
    fn eventi_di_un_iscrizione_superata_scartati() {
        let (mut m, _c) = motore();
        m.letti(Ok(grezzi_vuoti()));
        m.iscrizione = 2;
        let evento = |iscrizione| {
            Interno::Client(m.generazione, DalClient::Evento(EventoWs::Entita {
                iscrizione,
                aggiunte: vec![luce("on")],
                cambiate: vec![],
                tolte: vec![],
            }))
        };
        let (vecchio, nuovo) = (evento(1), evento(2));
        m.interno(vecchio);
        assert_eq!(m.casa.as_ref().unwrap().stato("light.cucina").unwrap().state, "off");
        m.interno(nuovo);
        assert_eq!(m.casa.as_ref().unwrap().stato("light.cucina").unwrap().state, "on");
    }

    /// Un cambio compresso per un'entità che non c'è in memoria non si può applicare: si rilegge.
    #[test]
    fn diff_senza_stato_di_partenza_fa_rileggere() {
        let (mut m, _c) = motore();
        m.letti(Ok(grezzi_vuoti()));
        m.entita_cambiate(vec![], vec![("light.sconosciuta".into(), json!({ "+": { "s": "on" } }))], vec![]);
        // Senza client la lettura non parte, ma la luce nota non è cambiata e nulla è in pubblicazione.
        assert_eq!(m.pubblica.scadenza(), None);
        m.entita_cambiate(vec![], vec![("light.cucina".into(), json!({ "+": { "s": "on" } }))], vec![]);
        assert_eq!(m.casa.as_ref().unwrap().stato("light.cucina").unwrap().state, "on");
        assert!(m.pubblica.scadenza().is_some(), "cambio visibile in pubblicazione");
    }

    /// Giro di correzioni 1: `ha=` dice lo stato dell'ultima lettura, anche quando `get_config` non
    /// è riuscita (prima restava quello della lettura precedente).
    #[test]
    fn stato_ha_della_lettura_senza_configurazione() {
        let cartella = tempfile::tempdir().unwrap();
        let store = Arc::new(ConfigStore::apri(cartella.path()));
        let segreti: Arc<dyn Segreti> = Arc::new(crate::segreti::MemoriaSegreti::new());
        let diagnosi = Arc::new(Diagnosi::new(true));
        let config = Arc::new(Mutex::new(Config::default()));
        let mut m = Motore::new(store, segreti, Arc::new(|_| {}), config, vec![], diagnosi.clone());
        let mut g = grezzi_vuoti();
        g.config = Some(HaConfig {
            unit_system: crate::ha::models::HaUnitSystem { temperature: "°C".into() },
            location_name: "Casa".into(),
            state: Some("RUNNING".into()),
        });
        m.letti(Ok(g));
        assert!(diagnosi.riga(&[]).contains(" ha=RUNNING "));
        m.letti(Ok(grezzi_vuoti()));
        let riga = diagnosi.riga(&[]);
        assert!(riga.contains(" ha=nessuna "), "{riga}");
    }

    /// Giro di correzioni 3 del 14h, P1: l'errore delle letture in volo arriva dopo la caduta della
    /// connessione (stessa generazione: il client si riconnette da solo). Il nuovo tentativo non si
    /// arma a connessione caduta; alla riconnessione nessun tentativo resta armato e l'attesa
    /// riparte da 2 s.
    #[test]
    fn nessuna_riprova_a_connessione_caduta() {
        let (mut m, _c) = motore();
        m.letti(Ok(grezzi_vuoti()));
        m.letti(Err("rifiutata".into()));
        assert!(m.riprova.scadenza().is_some());
        // L'attesa è cresciuta (4 s per il prossimo). Cade la connessione, poi arriva l'errore
        // di una lettura che era in volo.
        let caduta = StatoConnessione::Errore("connessione chiusa da Home Assistant".into());
        m.interno(Interno::Client(m.generazione, DalClient::Stato(caduta)));
        m.letti(Err(NON_CONNESSO.into()));
        assert_eq!(m.riprova.scadenza(), None, "nessun tentativo mentre si è disconnessi");
        m.interno(Interno::Client(m.generazione, DalClient::Stato(StatoConnessione::Connesso)));
        assert_eq!(m.riprova.scadenza(), None, "nessun tentativo rimasto dalla connessione di prima");
        let prima = Instant::now();
        m.letti(Err("la prima della connessione nuova".into()));
        let scade = m.riprova.scadenza().expect("si riprova");
        assert_eq!((scade - prima).as_secs_f32().round() as u64, 2, "l'attesa riparte da 2 s");
    }

    /// P2: un tentativo armato si arma da capo, senza tenere l'istante di prima (dopo il tentativo
    /// dell'avvio da 5 s non può uscire una scadenza già passata).
    #[test]
    fn attesa_armata_da_capo() {
        let t = Instant::now();
        let mut a = Attesa::default();
        a.rinvia(t, ATTESA_AVVIO, ATTESA_AVVIO);
        a.arma(t + Duration::from_secs(3), RIPROVA_LETTURA);
        assert_eq!(a.scadenza(), Some(t + Duration::from_secs(5)), "3 s + 2 s, non 0 + 2 s");
    }

    #[test]
    fn attesa_con_tetto() {
        let t = Instant::now();
        let mut a = Attesa::default();
        a.rinvia(t, RAFFICA, TETTO_RAFFICA);
        assert_eq!(a.scadenza(), Some(t + RAFFICA));
        a.rinvia(t + Duration::from_millis(40), RAFFICA, TETTO_RAFFICA);
        assert_eq!(a.scadenza(), Some(t + Duration::from_millis(90)));
        a.rinvia(t + Duration::from_millis(280), RAFFICA, TETTO_RAFFICA);
        assert_eq!(a.scadenza(), Some(t + TETTO_RAFFICA));
        a.annulla();
        assert_eq!(a.scadenza(), None);
    }
}
