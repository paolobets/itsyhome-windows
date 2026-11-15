//! `CasaService`: un thread con runtime tokio `current_thread` che tiene la
//! connessione a Home Assistant, tiene in memoria stati e registri e consegna `CasaEvento`
//! all'`Ascoltatore`, sempre da quel thread.
//!
//! Alla connessione si legge tutto una volta (stati, registri, configurazione). Poi ogni
//! `state_changed` aggiorna in memoria solo la sua entità (dal `new_state` dell'evento) e si
//! pubblica solo se cambia qualcosa che l'app mostra; i registri si rileggono solo sui loro
//! eventi `*_registry_updated`. Così una casa vera, dove i sensori cambiano di continuo, non
//! costa letture né mappature.
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio::time::{sleep_until, Instant};

use crate::config::{Config, ConfigStore};
use crate::eventi::{Ascoltatore, CasaEvento, StatoConnessione};
use crate::ha::client::{self, Client, DalClient, NON_CONNESSO};
use crate::ha::eventi::{Aggiornamento, EventoWs};
use crate::ha::mapper::{aggiornamento_disponibile, sostituisci_in_menu, EntityMapper, Indici, Preferenze};
use crate::ha::models::{HaArea, HaConfig, HaDevice, HaEntityEntry, HaState};
use crate::segreti::Segreti;
use crate::types::{AppEntity, MenuData, TestConnectionResult};

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
/// Se la prima lettura completa della connessione fallisce non c'è nulla da aggiornare con gli
/// eventi: si riprova dopo 2 s, finché riesce (prima ogni `state_changed` faceva rileggere).
const RIPROVA_LETTURA: Duration = Duration::from_secs(2);

type Fatto = Box<dyn FnOnce(TestConnectionResult) + Send>;

enum Comando {
    ConnettiAttivo,
    Connetti(String),
    Disconnetti,
    Servizio { entity_id: String, dominio: String, servizio: String },
    Archivia(String),
    Prova { url: String, token: String, fatto: Fatto },
    Salva { config: Config, rimappa: bool },
    Ferma,
}

pub struct CasaService {
    tx: mpsc::UnboundedSender<Comando>,
    config: Arc<Mutex<Config>>,
    thread: Option<JoinHandle<()>>,
}

impl CasaService {
    pub fn avvia(store: Arc<ConfigStore>, segreti: Arc<dyn Segreti>, ascolta: Ascoltatore) -> CasaService {
        Self::avvia_con_ritardi(store, segreti, ascolta, client::RITARDI_RICONNESSIONE.to_vec())
    }

    /// Come `avvia`, con ritardi di riconnessione scelti (per i test).
    #[doc(hidden)]
    pub fn avvia_con_ritardi(
        store: Arc<ConfigStore>,
        segreti: Arc<dyn Segreti>,
        ascolta: Ascoltatore,
        ritardi: Vec<Duration>,
    ) -> CasaService {
        // Unico provider di rustls, esplicito: se un'altra dipendenza attivasse aws-lc-rs
        // la scelta automatica fallirebbe e wss:// tornerebbe ad andare in panico.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let config = Arc::new(Mutex::new(store.leggi()));
        let (tx, rx) = mpsc::unbounded_channel();
        let motore = Motore::new(store, segreti, ascolta, config.clone(), ritardi);
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
        CasaService { tx, config, thread: Some(thread) }
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

    /// L'esito arriva come `CasaEvento::Esito`.
    pub fn comando(&self, entity_id: &str, dominio: &str, servizio: &str) {
        self.manda(Comando::Servizio {
            entity_id: entity_id.to_owned(),
            dominio: dominio.to_owned(),
            servizio: servizio.to_owned(),
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
            || prima.device_order != config.device_order;
        // Inviato sotto il lucchetto: i salvataggi arrivano nell'ordine delle modifiche.
        self.manda(Comando::Salva { config: config.clone(), rimappa });
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

/// Messaggi dai task del runtime al motore, con la generazione della connessione
/// che li ha prodotti: quelli di una connessione superata si scartano.
enum Interno {
    Client(u64, DalClient),
    /// Esito di una lettura completa (`Err`: senza gli stati non c'è nulla da mostrare).
    Grezzi(u64, Result<Box<Grezzi>, String>),
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
}

/// I dati di Home Assistant in memoria per la connessione corrente: stati, registri indicizzati
/// e l'ultimo `MenuData` costruito. Gli `state_changed` li aggiornano un'entità alla volta.
struct Casa {
    stati: Vec<HaState>,
    posizione: HashMap<String, usize>,
    indici: Indici,
    config: Option<HaConfig>,
    menu: Arc<MenuData>,
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
        Casa { stati, posizione, indici, config: g.config, menu: Arc::new(MenuData::default()) }
    }

    fn stato(&self, entity_id: &str) -> Option<&HaState> {
        self.posizione.get(entity_id).map(|&i| &self.stati[i])
    }

    /// Mette in memoria lo stato nuovo di un'entità (`None`: tolta) e dice cosa cambia per l'app.
    fn applica(&mut self, mapper: &EntityMapper, entity_id: &str, nuovo: Option<HaState>) -> Effetto {
        let vecchio = self.stato(entity_id);
        let prima = vecchio.and_then(|s| mapper.mappa_entita(&self.indici, s));
        let aggiornamento_prima = vecchio.is_some_and(aggiornamento_disponibile);
        let dopo = nuovo.as_ref().and_then(|s| mapper.mappa_entita(&self.indici, s));
        let aggiornamento_dopo = nuovo.as_ref().is_some_and(aggiornamento_disponibile);
        match (self.posizione.get(entity_id).copied(), nuovo) {
            (Some(i), Some(s)) => self.stati[i] = alleggerisci(s, dopo.is_some()),
            (None, Some(s)) => {
                self.posizione.insert(entity_id.to_owned(), self.stati.len());
                self.stati.push(alleggerisci(s, dopo.is_some()));
            }
            (Some(i), None) => {
                // Raro (un'entità tolta): si rifà l'indice.
                self.stati.remove(i);
                self.posizione = self.stati.iter().enumerate().map(|(i, s)| (s.entity_id.clone(), i)).collect();
            }
            (None, None) => {}
        }
        if aggiornamento_prima != aggiornamento_dopo {
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
/// Un'entità può diventare visibile in due modi: per il suo stato (un sensore che diventa di
/// temperatura), e allora lo stato nuovo arriva intero con lo `state_changed`; o per un
/// registro (un'entità non più nascosta o disattivata), e allora nessuno `state_changed` arriva
/// ma l'evento del registro fa rileggere tutto, stati interi compresi.
fn alleggerisci(mut s: HaState, mostrata: bool) -> HaState {
    if !mostrata {
        let classe = s.attributes.get_mut("device_class").map(Value::take);
        s.attributes = match classe {
            Some(c) => json!({ "device_class": c }),
            None => Value::Null,
        };
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
    /// Una lettura completa è in volo: gli `state_changed` che arrivano intanto si tengono qui e
    /// si riapplicano ai dati nuovi (un evento può arrivare dopo la risposta di `get_states` ma
    /// prima che le altre letture finiscano).
    in_lettura: Option<Vec<(String, Option<HaState>)>>,
    /// Un'altra lettura completa è stata chiesta mentre una era in volo.
    rileggi_dopo: bool,
    /// Pubblicazione dei cambi visibili, raccolti in una raffica.
    pubblica: Attesa,
    /// Alla pubblicazione serve la mappatura completa (non bastano le sostituzioni).
    rimappa: bool,
    /// Rilettura completa in sospeso: dopo gli eventi dei registri o della configurazione, o per
    /// riprovare una prima lettura fallita.
    registri: Attesa,
    /// Notifiche persistenti dell'ambiente connesso già consegnate all'applicazione.
    notifiche: BTreeSet<String>,
    /// L'ambiente a cui appartengono quelle notifiche.
    ambiente_notifiche: Option<String>,
}

impl Motore {
    fn new(
        store: Arc<ConfigStore>,
        segreti: Arc<dyn Segreti>,
        ascolta: Ascoltatore,
        config: Arc<Mutex<Config>>,
        ritardi: Vec<Duration>,
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
            in_lettura: None,
            rileggi_dopo: false,
            pubblica: Attesa::default(),
            rimappa: false,
            registri: Attesa::default(),
            notifiche: BTreeSet::new(),
            ambiente_notifiche: None,
        }
    }

    async fn esegui(mut self, mut rx: mpsc::UnboundedReceiver<Comando>) {
        let mut interni = self.interni_rx.take().expect("ricevitore interno");
        loop {
            let (pubblica, registri) = (self.pubblica.scadenza(), self.registri.scadenza());
            tokio::select! {
                c = rx.recv() => match c {
                    None | Some(Comando::Ferma) => break,
                    Some(c) => self.comando(c),
                },
                Some(i) = interni.recv() => self.interno(i),
                _ = sleep_until(pubblica.unwrap_or_else(Instant::now)), if pubblica.is_some() => {
                    self.pubblica.annulla();
                    if std::mem::take(&mut self.rimappa) {
                        self.mappa_tutto();
                    }
                    self.consegna();
                }
                _ = sleep_until(registri.unwrap_or_else(Instant::now)), if registri.is_some() => {
                    self.registri.annulla();
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
            Comando::Servizio { entity_id, dominio, servizio } => {
                let Some((client, _)) = &self.client else {
                    self.emetti(CasaEvento::Esito { entity_id, ok: false, errore: Some(NON_CONNESSO.into()) });
                    return;
                };
                let client = client.clone();
                let ascolta = self.ascolta.clone();
                tokio::spawn(async move {
                    let r = client.call_service(&dominio, &servizio, &entity_id).await;
                    ascolta(CasaEvento::Esito { entity_id, ok: r.is_ok(), errore: r.err() });
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
            Comando::Salva { config, rimappa } => {
                self.salva(&config);
                if rimappa {
                    self.mappa_tutto();
                    self.consegna();
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
        self.client = Some(client::avvia(url, token, self.ritardi.clone(), uscita));
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
        self.in_lettura = None;
        self.rileggi_dopo = false;
        self.ferma_attese();
    }

    fn ferma_attese(&mut self) {
        self.pubblica.annulla();
        self.registri.annulla();
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
            Interno::Client(g, _) | Interno::Grezzi(g, _) if g != self.generazione => {}
            Interno::Client(_, DalClient::Stato(s)) => {
                let connesso = s == StatoConnessione::Connesso;
                if !connesso {
                    self.ferma_attese();
                }
                self.emetti(CasaEvento::Stato(s));
                if connesso {
                    // Una (ri)connessione consegna sempre i dati, anche se uguali a prima.
                    self.consegnato = None;
                    self.rileggi();
                }
            }
            Interno::Client(_, DalClient::Evento(EventoWs::StatoCambiato { entity_id, nuovo })) => {
                self.stato_cambiato(entity_id, nuovo);
            }
            Interno::Client(_, DalClient::Evento(EventoWs::StatoIlleggibile)) => self.rileggi(),
            Interno::Client(_, DalClient::Evento(EventoWs::DaRileggere)) => {
                self.registri.rinvia(Instant::now(), ATTESA_REGISTRI, TETTO_REGISTRI);
            }
            Interno::Client(_, DalClient::Evento(EventoWs::Notifiche { tipo, notifiche })) => {
                self.notifiche(tipo, notifiche);
            }
            Interno::Grezzi(_, esito) => self.letti(esito),
        }
    }

    /// Un `state_changed`: aggiorna in memoria solo quell'entità; se il cambio si vede, la
    /// pubblicazione parte alla fine della raffica.
    fn stato_cambiato(&mut self, entity_id: String, nuovo: Option<HaState>) {
        if let Some(attesi) = &mut self.in_lettura {
            attesi.push((entity_id.clone(), nuovo.clone()));
        }
        let Some(casa) = &mut self.casa else { return };
        match casa.applica(&self.mapper, &entity_id, nuovo) {
            Effetto::Niente => return,
            Effetto::Sostituisci(e) => {
                if !sostituisci_in_menu(Arc::make_mut(&mut casa.menu), &e) {
                    return; // mostrabile ma nascosta dall'utente
                }
            }
            Effetto::Rimappa => self.rimappa = true,
        }
        self.pubblica.rinvia(Instant::now(), RAFFICA, TETTO_RAFFICA);
    }

    /// La lettura completa (`trigger_refresh` di v2): stati, registri e configurazione in
    /// parallelo. Solo alla connessione, dopo un cambio dei registri o con uno stato illeggibile.
    fn rileggi(&mut self) {
        let Some((client, _)) = &self.client else { return };
        if self.in_lettura.is_some() {
            self.rileggi_dopo = true;
            return;
        }
        self.in_lettura = Some(Vec::new());
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

    /// La lettura completa è arrivata: dati nuovi, con sopra gli `state_changed` arrivati intanto
    /// (solo quelli non più vecchi dello stato letto), mappati e consegnati subito.
    fn letti(&mut self, esito: Result<Box<Grezzi>, String>) {
        let attesi = self.in_lettura.take().unwrap_or_default();
        match esito {
            // Senza gli stati si tengono i dati precedenti.
            Err(e) => {
                eprintln!("lettura di Home Assistant non riuscita: {e}");
                // Senza nessun dato gli eventi non hanno su cosa lavorare: si riprova.
                if self.casa.is_none() {
                    self.registri.rinvia(Instant::now(), RIPROVA_LETTURA, RIPROVA_LETTURA);
                }
            }
            Ok(g) => {
                let mut casa = Casa::new(*g, &mut self.mapper);
                for (entity_id, nuovo) in attesi {
                    let piu_vecchio = match (&nuovo, casa.stato(&entity_id)) {
                        // Stessa forma ISO di HA: il confronto fra testi segue il tempo.
                        (Some(n), Some(letto)) => n.last_updated < letto.last_updated,
                        _ => false,
                    };
                    if !piu_vecchio {
                        casa.applica(&self.mapper, &entity_id, nuovo);
                    }
                }
                self.casa = Some(casa);
                self.pubblica.annulla();
                self.rimappa = false;
                self.mappa_tutto();
                self.consegna();
            }
        }
        if std::mem::take(&mut self.rileggi_dopo) {
            self.rileggi();
        }
    }

    /// Ricostruisce tutto `MenuData` dai dati in memoria con la configurazione corrente.
    fn mappa_tutto(&mut self) {
        let Some(casa) = &mut self.casa else { return };
        let (favorites, hidden, room_order, device_order) = {
            let c = blocca(&self.config);
            (c.favorites.clone(), c.hidden_entities.clone(), c.room_order.clone(), c.device_order.clone())
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
            },
        );
        casa.menu = Arc::new(dati);
    }

    /// Consegna l'ultimo `MenuData` se è diverso da quello già consegnato.
    fn consegna(&mut self) {
        let Some(casa) = &self.casa else { return };
        if self.consegnato.as_deref() == Some(&*casa.menu) {
            return;
        }
        self.consegnato = Some(casa.menu.clone());
        self.emetti(CasaEvento::Dati(casa.menu.clone()));
    }
}

async fn leggi<T: serde::de::DeserializeOwned>(client: &Client, tipo: &str) -> Result<T, String> {
    let v = client.richiesta(tipo, Value::Null).await?;
    serde_json::from_value(v).map_err(|e| e.to_string())
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
