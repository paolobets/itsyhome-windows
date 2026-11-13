//! `CasaService`: un thread con runtime tokio `current_thread` che tiene la
//! connessione a Home Assistant, rilegge i dati (logica di `src-tauri/src/refresh.rs`)
//! e consegna `CasaEvento` all'`Ascoltatore`, sempre da quel thread.
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::mpsc;
use tokio::time::{sleep_until, Instant};

use crate::config::{Config, ConfigStore};
use crate::eventi::{Ascoltatore, CasaEvento, StatoConnessione};
use crate::ha::client::{self, Client, DalClient, NON_CONNESSO};
use crate::ha::eventi::EventoWs;
use crate::ha::mapper::EntityMapper;
use crate::ha::models::{HaArea, HaConfig, HaDevice, HaEntityEntry, HaNotification, HaState};
use crate::segreti::Segreti;
use crate::types::{MenuData, TestConnectionResult};

/// Trailing debounce di v2 sugli `state_changed`...
const DEBOUNCE: Duration = Duration::from_millis(300);
/// ...ma un flusso continuo rilegge comunque almeno una volta al secondo.
const ATTESA_MASSIMA: Duration = Duration::from_secs(1);

type Fatto = Box<dyn FnOnce(TestConnectionResult) + Send>;

enum Comando {
    ConnettiAttivo,
    Connetti(String),
    Disconnetti,
    Servizio { entity_id: String, dominio: String, servizio: String },
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

/// Risposte di Home Assistant da cui si ricostruisce `MenuData`.
struct Grezzi {
    states: Vec<HaState>,
    areas: Vec<HaArea>,
    devices: Vec<HaDevice>,
    entries: Vec<HaEntityEntry>,
    config: Option<HaConfig>,
    notifiche: u32,
}

/// Messaggi dai task del runtime al motore, con la generazione della connessione
/// che li ha prodotti: quelli di una connessione superata si scartano.
enum Interno {
    Client(u64, DalClient),
    Grezzi(u64, Grezzi),
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
    grezzi: Option<Grezzi>,
    /// (primo evento in attesa, scadenza della rilettura)
    debounce: Option<(Instant, Instant)>,
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
            grezzi: None,
            debounce: None,
        }
    }

    async fn esegui(mut self, mut rx: mpsc::UnboundedReceiver<Comando>) {
        let mut interni = self.interni_rx.take().expect("ricevitore interno");
        loop {
            let scadenza = self.debounce.map(|(_, s)| s);
            tokio::select! {
                c = rx.recv() => match c {
                    None | Some(Comando::Ferma) => break,
                    Some(c) => self.comando(c),
                },
                Some(i) = interni.recv() => self.interno(i),
                _ = sleep_until(scadenza.unwrap_or_else(Instant::now)), if scadenza.is_some() => {
                    self.debounce = None;
                    self.rileggi();
                }
            }
        }
        self.chiudi_client();
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
                        self.emetti(CasaEvento::Stato(StatoConnessione::Errore(
                            "nessun ambiente configurato".into(),
                        )));
                    }
                }
            }
            Comando::Connetti(id) => self.connetti(&id),
            Comando::Disconnetti => {
                self.chiudi_client();
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
            Comando::Prova { url, token, fatto } => {
                tokio::spawn(async move { fatto(client::prova_connessione(&url, &token).await) });
            }
            Comando::Salva { config, rimappa } => {
                // L'API non restituisce errori: la modifica resta comunque in memoria.
                let _ = self.store.salva(&config);
                if rimappa {
                    self.pubblica();
                }
            }
            Comando::Ferma => {}
        }
    }

    fn connetti(&mut self, env_id: &str) {
        self.chiudi_client();
        let url = blocca(&self.config)
            .environments
            .iter()
            .find(|e| e.id == env_id)
            .map(|e| e.ha_url.clone());
        let Some(url) = url else {
            self.emetti(CasaEvento::Stato(StatoConnessione::Errore("ambiente non trovato".into())));
            return;
        };
        let token = match self.segreti.leggi(env_id) {
            Ok(Some(t)) if !t.is_empty() => t,
            Ok(_) => {
                self.emetti(CasaEvento::Stato(StatoConnessione::Errore("token mancante".into())));
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

    /// Ferma la connessione corrente e invalida tutto ciò che ha ancora in volo.
    fn chiudi_client(&mut self) {
        if let Some((_, task)) = self.client.take() {
            task.abort();
        }
        self.generazione += 1;
        self.grezzi = None;
        self.debounce = None;
    }

    fn interno(&mut self, i: Interno) {
        match i {
            Interno::Client(g, _) | Interno::Grezzi(g, _) if g != self.generazione => {}
            Interno::Client(_, DalClient::Stato(s)) => {
                let connesso = s == StatoConnessione::Connesso;
                if !connesso {
                    self.debounce = None;
                }
                self.emetti(CasaEvento::Stato(s));
                if connesso {
                    self.rileggi();
                }
            }
            Interno::Client(_, DalClient::Evento(EventoWs::StatoCambiato)) => {
                let ora = Instant::now();
                let primo = self.debounce.map_or(ora, |(p, _)| p);
                self.debounce = Some((primo, (ora + DEBOUNCE).min(primo + ATTESA_MASSIMA)));
            }
            Interno::Client(_, DalClient::Evento(EventoWs::Notifica { titolo, messaggio })) => {
                self.emetti(CasaEvento::Notifica { titolo, messaggio });
            }
            Interno::Grezzi(_, g) => {
                self.grezzi = Some(g);
                self.pubblica();
            }
        }
    }

    /// `trigger_refresh` di v2: le sei letture in parallelo, poi la mappatura.
    fn rileggi(&self) {
        let Some((client, _)) = &self.client else { return };
        let (client, tx, g) = (client.clone(), self.interni_tx.clone(), self.generazione);
        tokio::spawn(async move {
            let (states, areas, devices, entries, config, notifiche) = tokio::join!(
                leggi::<Vec<HaState>>(&client, "get_states"),
                leggi::<Vec<HaArea>>(&client, "config/area_registry/list"),
                leggi::<Vec<HaDevice>>(&client, "config/device_registry/list"),
                leggi::<Vec<HaEntityEntry>>(&client, "config/entity_registry/list"),
                leggi::<HaConfig>(&client, "get_config"),
                leggi::<Vec<HaNotification>>(&client, "persistent_notification/get"),
            );
            // Senza gli stati non c'è nulla da mostrare: si tengono i dati precedenti.
            let Ok(states) = states else { return };
            let _ = tx.send(Interno::Grezzi(
                g,
                Grezzi {
                    states,
                    areas: areas.unwrap_or_default(),
                    devices: devices.unwrap_or_default(),
                    entries: entries.unwrap_or_default(),
                    config: config.ok(),
                    notifiche: notifiche.map(|n| n.len() as u32).unwrap_or(0),
                },
            ));
        });
    }

    /// Mappa gli ultimi dati con la configurazione corrente e li consegna.
    fn pubblica(&self) {
        let Some(g) = &self.grezzi else { return };
        let (favorites, hidden, room_order, device_order) = {
            let c = blocca(&self.config);
            (c.favorites.clone(), c.hidden_entities.clone(), c.room_order.clone(), c.device_order.clone())
        };
        let dati: MenuData = EntityMapper::new().map(
            &g.states,
            &g.areas,
            &g.devices,
            &g.entries,
            g.config.as_ref(),
            g.notifiche,
            &favorites,
            &hidden,
            &room_order,
            &device_order,
        );
        self.emetti(CasaEvento::Dati(Arc::new(dati)));
    }
}

async fn leggi<T: serde::de::DeserializeOwned>(client: &Client, tipo: &str) -> Result<T, String> {
    let v = client.richiesta(tipo, Value::Null).await?;
    serde_json::from_value(v).map_err(|e| e.to_string())
}
