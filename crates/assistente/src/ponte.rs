//! Ponte casa → interfaccia: dai dati di `casa-core` ai preferiti del pannello, e dai tocchi
//! sui preferiti ai comandi verso Home Assistant. Tiene anche le voci «In attesa» (notifiche di
//! Home Assistant, casa irraggiungibile, batteria, prova) e decide umore e aspetto della pillola.

use crate::attenzione::{
    self, livello_massimo, merita_notifica_di_sistema, notifica_sistema, prossimo_cambio_aspetto, Attese, Fonte,
    Livello, SorveglianzaCasa, Voce, ID_BATTERIA, ID_CASA_OFFLINE, PREFISSO_NOTIFICA, PULSANTE_ARCHIVIA,
    PULSANTE_RIPROVA,
};
use crate::pillola::{self, VistaPillola};
use crate::umore::{Cervello, Segnale, Umore};
use crate::{Pannello, Pillola, PreferitoUi, SensoreUi, VoceAttesaUi};
use casa_core::eventi::{Ascoltatore, CasaEvento, StatoConnessione};
use casa_core::servizio::CasaService;
use casa_core::types::{AppEntity, BaseEntity, MenuData, SensorEntity};
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// La sveglia dell'umore suona appena dopo la scadenza, mai un attimo prima.
const MARGINE_SVEGLIA: Duration = Duration::from_millis(20);

/// Quanti preferiti mostra il pannello.
pub const MASSIMO_PREFERITI: usize = 8;

/// Quanti sensori preferiti stanno nei Quadranti, accanto a CPU, RAM e disco.
pub const MASSIMO_SENSORI: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tipo {
    Interruttore,
    Scena,
    Lettura,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Preferito {
    pub entity_id: String,
    pub nome: String,
    pub tipo: Tipo,
    pub acceso: bool,
    pub valore: String,
    pub disponibile: bool,
    pub errore: Option<String>,
}

/// I campi comuni di ogni entità.
pub fn base(e: &AppEntity) -> &BaseEntity {
    match e {
        AppEntity::Light(x) => &x.base,
        AppEntity::Switch(x) => &x.base,
        AppEntity::Climate(x) => &x.base,
        AppEntity::Cover(x) => &x.base,
        AppEntity::Lock(x) => &x.base,
        AppEntity::Fan(x) => &x.base,
        AppEntity::Humidifier(x) => &x.base,
        AppEntity::Valve(x) => &x.base,
        AppEntity::Sensor(x) => &x.base,
        AppEntity::Alarm(x) => &x.base,
        AppEntity::Camera(x) => &x.base,
        AppEntity::Scene(x) => &x.base,
    }
}

fn preferito(e: &AppEntity) -> Preferito {
    let b = base(e);
    let (tipo, acceso, valore) = match e {
        AppEntity::Light(l) => (Tipo::Interruttore, l.is_on, b.state.clone()),
        AppEntity::Switch(s) => (Tipo::Interruttore, s.is_on, b.state.clone()),
        AppEntity::Scene(_) => (Tipo::Scena, false, String::new()),
        AppEntity::Sensor(s) => {
            let valore = match s.unit.as_deref().filter(|u| !u.is_empty()) {
                Some(u) => format!("{} {u}", s.value),
                None => s.value.clone(),
            };
            (Tipo::Lettura, false, valore)
        }
        _ => (Tipo::Lettura, false, b.state.clone()),
    };
    Preferito {
        entity_id: b.entity_id.clone(),
        nome: b.name.clone(),
        tipo,
        acceso,
        valore,
        disponibile: b.is_available,
        errore: None,
    }
}

/// I preferiti da mostrare: quelli scelti dall'utente nell'ordine salvato; se non ce ne sono,
/// le prime entità delle stanze nell'ordine delle stanze, poi le scene. Al massimo `max`.
pub fn preferiti_ui(menu: &MenuData, max: usize) -> Vec<Preferito> {
    entita_mostrate(menu).take(max).map(|e| preferito(&e)).collect()
}

/// Le entità da cui nascono i preferiti: quelle scelte, altrimenti le stanze nell'ordine e poi
/// le scene (il mapper le tiene a parte, mai nelle stanze né nei preferiti). Copie pigre: chi
/// prende i primi `max` copia solo quelli.
fn entita_mostrate(menu: &MenuData) -> Box<dyn Iterator<Item = AppEntity> + '_> {
    if menu.favorites.is_empty() {
        Box::new(
            menu.rooms
                .iter()
                .flat_map(|r| r.entities.iter().cloned())
                .chain(menu.scenes.iter().map(|s| AppEntity::Scene(s.clone()))),
        )
    } else {
        Box::new(menu.favorites.iter().cloned())
    }
}

/// Il servizio di Home Assistant che un tocco invoca: accende/spegne luci e interruttori,
/// attiva le scene. Nulla per le entità non disponibili e per tutto il resto.
pub fn comando_per(e: &AppEntity) -> Option<(&'static str, &'static str)> {
    if !base(e).is_available {
        return None;
    }
    match e {
        AppEntity::Light(_) => Some(("light", "toggle")),
        AppEntity::Switch(_) => Some(("switch", "toggle")),
        AppEntity::Scene(_) => Some(("scene", "turn_on")),
        _ => None,
    }
}

/// Un tocco sul preferito può mandare un comando: luci, interruttori e scene disponibili, e solo
/// con la casa connessa (senza connessione i preferiti restano tenui e non rispondono).
pub fn voce_toccabile(p: &Preferito, connesso: bool) -> bool {
    connesso && p.disponibile && p.tipo != Tipo::Lettura
}

/// Il testo sotto il nome di un preferito.
pub fn testo_stato(p: &Preferito) -> String {
    if p.errore.is_some() {
        return "Non riuscito".into();
    }
    if !p.disponibile {
        return "Non disponibile".into();
    }
    match p.tipo {
        Tipo::Interruttore => if p.acceso { "Acceso" } else { "Spento" }.into(),
        Tipo::Scena => "Scena".into(),
        Tipo::Lettura => p.valore.clone(),
    }
}

/// Che cosa è un preferito per l'interfaccia (icona e tinta): "luce", "presa", "scena",
/// "termo", "goccia", "sensore" o "casa".
pub fn genere(p: &Preferito) -> &'static str {
    match p.entity_id.split('.').next().unwrap_or_default() {
        "light" => "luce",
        "switch" => "presa",
        "scene" => "scena",
        "climate" => "termo",
        "sensor" if p.valore.contains('°') => "termo",
        "sensor" if p.valore.ends_with('%') => "goccia",
        "sensor" => "sensore",
        _ => "casa",
    }
}

/// La riga di stato della casa nel layout Orologio.
pub fn riga_stato(stato: &StatoConnessione, menu: Option<&MenuData>) -> String {
    match (stato, menu) {
        (StatoConnessione::Connesso, Some(m)) => {
            let accese = m
                .rooms
                .iter()
                .flat_map(|r| r.entities.iter())
                .filter(|e| matches!(e, AppEntity::Light(l) if l.is_on && l.base.is_available))
                .count();
            let luci = match accese {
                0 => "Luci spente".to_owned(),
                1 => "1 luce accesa".to_owned(),
                n => format!("{n} luci accese"),
            };
            // La prima temperatura disponibile: fra i preferiti, poi nelle stanze.
            let temperatura = m
                .favorites
                .iter()
                .chain(m.rooms.iter().flat_map(|r| r.entities.iter()))
                .find_map(|e| match e {
                    AppEntity::Sensor(s)
                        if s.base.is_available && s.device_class.as_deref() == Some("temperature") =>
                    {
                        Some(preferito(e).valore)
                    }
                    _ => None,
                });
            match temperatura {
                Some(t) => format!("{luci} · {t}"),
                None => luci,
            }
        }
        (StatoConnessione::Connesso, None) => "Casa connessa".into(),
        (StatoConnessione::InConnessione, _) => "Connessione alla casa…".into(),
        (StatoConnessione::Disconnesso, _) => "Casa non connessa".into(),
        (StatoConnessione::Errore(e), _) => frase_errore(e).into(),
    }
}

/// Una frase semplice al posto dell'errore tecnico di casa-core (il dettaglio resta per le
/// Impostazioni).
pub fn frase_errore(e: &str) -> &'static str {
    match e {
        "token non valido" => "Token non valido",
        "token mancante" => "Token della casa mancante",
        "nessun ambiente configurato" | "ambiente non trovato" => "Casa non configurata",
        e if e.starts_with("portachiavi") => "Portachiavi non leggibile",
        _ => "Casa non raggiungibile",
    }
}

/// I sensori dei Quadranti: i primi `quanti` sensori disponibili fra i preferiti mostrati (i primi
/// `max`), ognuno con la frazione dell'arco (umidità 0–100%, temperatura 10–30 °C / 50–86 °F;
/// altrimenti -1 = niente arco).
pub fn sensori_quadranti(menu: &MenuData, max: usize, quanti: usize) -> Vec<(Preferito, f32)> {
    entita_mostrate(menu)
        .take(max)
        .filter_map(|e| match &e {
            AppEntity::Sensor(s) if s.base.is_available => Some((preferito(&e), frazione_arco(s))),
            _ => None,
        })
        .take(quanti)
        .collect()
}

fn frazione_arco(s: &SensorEntity) -> f32 {
    let numero = s.value.trim().replace(',', ".").parse::<f32>().ok();
    let unita = s.unit.as_deref().unwrap_or_default();
    let frazione = match numero {
        Some(v) if unita.ends_with('%') => v / 100.0,
        Some(v) if unita.ends_with('F') => (v - 50.0) / 36.0,
        Some(v) if unita.contains('°') => (v - 10.0) / 20.0,
        _ => return -1.0,
    };
    frazione.clamp(0.0, 1.0)
}

/// L'entità con questo id fra preferiti, stanze e scene (per il comando di un tocco).
pub fn entita(menu: &MenuData, entity_id: &str) -> Option<AppEntity> {
    menu.favorites
        .iter()
        .chain(menu.rooms.iter().flat_map(|r| r.entities.iter()))
        .find(|e| e.entity_id() == entity_id)
        .cloned()
        .or_else(|| {
            menu.scenes.iter().find(|s| s.base.entity_id == entity_id).map(|s| AppEntity::Scene(s.clone()))
        })
}

// ─── Il ponte vivo, sul thread dell'interfaccia ──────────────────────────────

// Il ponte vive sul thread dell'interfaccia: l'`Ascoltatore` (thread del servizio) gli manda
// ogni evento con `invoke_from_event_loop`, senza portarsi dietro nulla che non sia `Send`.
thread_local! {
    static PONTE: RefCell<Option<Rc<Ponte>>> = const { RefCell::new(None) };
}

fn ponte() -> Option<Rc<Ponte>> {
    PONTE.with(|p| p.borrow().clone())
}

/// L'`Ascoltatore` da dare a `CasaService::avvia`: inoltra ogni evento al ponte.
pub fn ascoltatore() -> Ascoltatore {
    Arc::new(|evento: CasaEvento| {
        // Dopo la fine del ciclo eventi non c'è più nessuno a cui consegnare: si ignora.
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(p) = ponte() {
                p.ricevi(evento);
            }
        });
    })
}

/// La batteria letta dal thread `batteria` (ogni 60 s, anche a pannello chiuso).
pub fn batteria(percentuale: f32, scollegata: bool) {
    if let Some(p) = ponte() {
        p.batteria(percentuale, scollegata);
    }
}

/// Una voce «In attesa» per la riga del pannello.
/// `non_archiviata`: l'ultimo «Archivia» non è riuscito (lo dice la riga sotto il titolo).
fn voce_attesa_ui(v: &Voce, non_archiviata: bool) -> VoceAttesaUi {
    let mut sotto = format!("{} · {}", v.livello.come_stringa(), v.fonte.come_stringa());
    if non_archiviata {
        sotto.push_str(" · archiviazione non riuscita");
    }
    VoceAttesaUi {
        id: v.id.as_str().into(),
        livello: v.livello.come_stringa().into(),
        titolo: v.titolo.as_str().into(),
        sotto: sotto.into(),
        dettaglio: v.dettaglio.as_str().into(),
        icona: pillola::icona_fonte(v.fonte),
        pulsanti: pillola::pulsanti_ui(&v.pulsanti),
    }
}

pub struct Ponte {
    casa: Rc<CasaService>,
    pillola: slint::Weak<Pillola>,
    /// Il pannello esiste solo da aperto: qui il riferimento debole, vuoto da chiuso.
    pannello: RefCell<slint::Weak<Pannello>>,
    cervello: Rc<RefCell<Cervello>>,
    /// `--umore`: umore fisso per prova, gli eventi non lo cambiano.
    umore_fisso: Option<Umore>,
    stato: RefCell<StatoConnessione>,
    menu: RefCell<Option<Arc<MenuData>>>,
    /// Comandi falliti, per entità: l'errore resta sulla voce fino al prossimo tocco o a un esito ok.
    errori: RefCell<HashMap<String, String>>,
    /// I modelli dei preferiti e dei sensori dei Quadranti: sopravvivono al pannello, che li
    /// riprende a ogni apertura.
    preferiti: Rc<VecModel<PreferitoUi>>,
    sensori: Rc<VecModel<SensoreUi>>,
    /// Ultima vista della pillola mostrata, per non riscriverla senza motivo.
    mostrato: RefCell<Option<VistaPillola>>,
    /// Gira solo mentre qualcosa scade da solo (contento, i 20 s di un'attenzione): nessun timer fisso.
    sveglia: slint::Timer,
    /// Le voci «In attesa» e il loro modello per il pannello (sopravvive al pannello).
    attese: RefCell<Attese>,
    modello_attese: Rc<VecModel<VoceAttesaUi>>,
    /// Da quanto la casa non è connessa; la sua sveglia suona solo allo scadere dei 5 minuti.
    sorveglianza: RefCell<SorveglianzaCasa>,
    sveglia_casa: slint::Timer,
    /// Vero da `Connesso` ai primi dati: le notifiche di HA che arrivano lì sono quelle già
    /// presenti (elenco "current" dell'iscrizione) e non meritano una notifica di sistema.
    appena_connesso: Cell<bool>,
    /// Voci di notifiche HA il cui ultimo «Archivia» non è riuscito.
    non_archiviate: RefCell<std::collections::HashSet<String>>,
    /// Errori da mostrare nelle Impostazioni (Task 11).
    #[allow(dead_code)]
    errore_import: Option<String>,
    #[allow(dead_code)]
    errore_config: RefCell<Option<String>>,
}

pub fn collega(
    casa: Rc<CasaService>,
    pillola: &Pillola,
    cervello: Rc<RefCell<Cervello>>,
    umore_fisso: Option<Umore>,
    errore_import: Option<String>,
) -> Rc<Ponte> {
    let ponte = Rc::new(Ponte {
        casa,
        pillola: pillola.as_weak(),
        pannello: RefCell::new(slint::Weak::default()),
        cervello,
        umore_fisso,
        stato: RefCell::new(StatoConnessione::Disconnesso),
        menu: RefCell::new(None),
        errori: RefCell::new(HashMap::new()),
        preferiti: Rc::new(VecModel::default()),
        sensori: Rc::new(VecModel::default()),
        mostrato: RefCell::new(None),
        sveglia: slint::Timer::default(),
        attese: RefCell::new(Attese::default()),
        modello_attese: Rc::new(VecModel::default()),
        sorveglianza: RefCell::new(SorveglianzaCasa::new(Instant::now())),
        sveglia_casa: slint::Timer::default(),
        appena_connesso: Cell::new(false),
        non_archiviate: RefCell::new(std::collections::HashSet::new()),
        errore_import,
        errore_config: RefCell::new(None),
    });
    PONTE.with(|p| *p.borrow_mut() = Some(ponte.clone()));
    // I pulsanti della pillola allargata agiscono sulla voce che sta chiamando.
    pillola.on_azione(|pulsante| {
        if let Some(p) = self::ponte() {
            let voce = p.mostrato.borrow().as_ref().and_then(|v| v.voce_allargata().map(str::to_owned));
            if let Some(voce) = voce {
                p.azione(&voce, &pulsante);
            }
        }
    });
    ponte.aggiorna_casa_offline();
    ponte.aggiorna_umore();
    ponte
}

fn voce_ui(p: &Preferito, connesso: bool) -> PreferitoUi {
    PreferitoUi {
        entity_id: p.entity_id.as_str().into(),
        nome: p.nome.as_str().into(),
        genere: genere(p).into(),
        acceso: p.acceso && p.disponibile,
        stato: testo_stato(p).into(),
        disponibile: p.disponibile,
        errore: p.errore.as_deref().unwrap_or_default().into(),
        toccabile: voce_toccabile(p, connesso),
        tenue: !p.disponibile || !connesso,
    }
}

/// Riscrive solo le righe cambiate (una riga uguale non ridisegna nulla).
fn aggiorna_modello<T: Clone + PartialEq + 'static>(modello: &VecModel<T>, voci: Vec<T>) {
    if modello.row_count() != voci.len() {
        modello.set_vec(voci);
        return;
    }
    for (i, v) in voci.into_iter().enumerate() {
        if modello.row_data(i).as_ref() != Some(&v) {
            modello.set_row_data(i, v);
        }
    }
}

impl Ponte {
    pub fn casa(&self) -> &CasaService {
        &self.casa
    }

    fn ricevi(&self, evento: CasaEvento) {
        let ora = Instant::now();
        match evento {
            CasaEvento::Stato(s) => {
                self.cervello.borrow_mut().ricevi(Segnale::Casa(s.clone()), ora);
                self.sorveglianza.borrow_mut().ricevi(&s, ora);
                self.appena_connesso.set(s == StatoConnessione::Connesso);
                *self.stato.borrow_mut() = s;
                // Con la connessione cambia anche se i preferiti si possono toccare.
                self.ricostruisci_preferiti();
                self.aggiorna_pannello();
                self.aggiorna_casa_offline();
            }
            CasaEvento::Dati(menu) => {
                self.appena_connesso.set(false);
                *self.menu.borrow_mut() = Some(menu);
                self.ricostruisci_preferiti();
                self.aggiorna_pannello();
            }
            CasaEvento::NotificaAggiunta { id, titolo, messaggio } => {
                let silenziosa = self.appena_connesso.get();
                self.metti_voce(attenzione::voce_notifica(&id, &titolo, &messaggio, ora), !silenziosa);
            }
            CasaEvento::NotificaRimossa { id } => self.togli_voce(&format!("{PREFISSO_NOTIFICA}{id}")),
            CasaEvento::NotificaNonArchiviata { id, errore } => {
                eprintln!("notifica {id} non archiviata: {errore}");
                self.non_archiviate.borrow_mut().insert(format!("{PREFISSO_NOTIFICA}{id}"));
                self.attese_cambiate();
            }
            CasaEvento::Esito { entity_id, ok, errore } => {
                if ok {
                    self.errori.borrow_mut().remove(&entity_id);
                    self.cervello.borrow_mut().ricevi(Segnale::ComandoRiuscito, ora);
                } else {
                    let errore = errore.unwrap_or_else(|| "comando non riuscito".into());
                    eprintln!("comando su {entity_id} non riuscito: {errore}");
                    self.errori.borrow_mut().insert(entity_id, errore);
                    self.cervello.borrow_mut().ricevi(Segnale::ComandoFallito, ora);
                }
                self.ricostruisci_preferiti();
            }
            CasaEvento::ConfigNonSalvata(e) => {
                eprintln!("{e}");
                *self.errore_config.borrow_mut() = Some(e);
            }
        }
        self.aggiorna_umore();
    }

    /// Ricalcola umore e aspetto della pillola e li porta alla pillola e, se aperto, al pannello.
    /// Se qualcosa scadrà da solo (contento, i 20 s di un'attenzione), una sveglia singola li
    /// ricalcola allora; altrimenti nessun timer.
    fn aggiorna_umore(&self) {
        let ora = Instant::now();
        let (umore, scadenza_umore) = {
            let c = self.cervello.borrow();
            (self.umore_fisso.unwrap_or_else(|| c.umore(ora)), c.prossimo_cambio(ora))
        };
        let (nuova, scadenza_aspetto) = {
            let attese = self.attese.borrow();
            (pillola::vista(umore, &attese, ora), prossimo_cambio_aspetto(&attese, ora))
        };
        let scadenza = match (scadenza_umore, scadenza_aspetto) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        if self.mostrato.borrow().as_ref() != Some(&nuova) {
            // La pillola nascosta (pannello aperto) non si tocca: ridisegnarla mentre Windows la
            // dissolve la farebbe intravedere. Riprende l'aspetto appena prima di ricomparire.
            if let Some(p) = self.pillola.upgrade().filter(|p| p.get_attiva()) {
                pillola::imposta(&p, &nuova);
            }
            if let Some(p) = self.pannello.borrow().upgrade() {
                p.set_umore(nuova.umore.come_stringa().into());
            }
            *self.mostrato.borrow_mut() = Some(nuova);
        }
        match scadenza {
            Some(tra) => self.sveglia.start(slint::TimerMode::SingleShot, tra + MARGINE_SVEGLIA, || {
                if let Some(p) = ponte() {
                    p.aggiorna_umore();
                }
            }),
            None => self.sveglia.stop(),
        }
    }

    fn connesso(&self) -> bool {
        *self.stato.borrow() == StatoConnessione::Connesso
    }

    /// Preferiti e sensori dei Quadranti dai dati più recenti, con gli errori ancora aperti e
    /// lo stato della connessione.
    fn ricostruisci_preferiti(&self) {
        let connesso = self.connesso();
        let (voci, sensori) = {
            let menu = self.menu.borrow();
            let Some(menu) = menu.as_ref() else { return };
            let errori = self.errori.borrow();
            let voci: Vec<PreferitoUi> = preferiti_ui(menu, MASSIMO_PREFERITI)
                .into_iter()
                .map(|mut p| {
                    p.errore = errori.get(&p.entity_id).cloned();
                    voce_ui(&p, connesso)
                })
                .collect();
            let sensori: Vec<SensoreUi> = sensori_quadranti(menu, MASSIMO_PREFERITI, MASSIMO_SENSORI)
                .into_iter()
                .map(|(p, frazione)| SensoreUi { voce: voce_ui(&p, connesso), frazione })
                .collect();
            (voci, sensori)
        };
        aggiorna_modello(&self.preferiti, voci);
        aggiorna_modello(&self.sensori, sensori);
    }

    /// Riga di stato della casa sul pannello aperto.
    fn aggiorna_pannello(&self) {
        if let Some(p) = self.pannello.borrow().upgrade() {
            self.riempi_stato(&p);
        }
    }

    fn riempi_stato(&self, p: &Pannello) {
        let menu = self.menu.borrow();
        let stato = self.stato.borrow();
        let riga = riga_stato(&stato, menu.as_deref());
        if p.get_stato_casa() != riga.as_str() {
            p.set_stato_casa(riga.into());
        }
        p.set_casa_pronta(*stato == StatoConnessione::Connesso && menu.is_some());
    }

    /// Il pannello appena creato: modelli dei preferiti, dei sensori e delle voci in attesa,
    /// stato della casa, tocchi e pulsanti.
    pub fn riempi(&self, p: &Pannello) {
        p.set_preferiti(ModelRc::from(self.preferiti.clone()));
        p.set_sensori(ModelRc::from(self.sensori.clone()));
        p.set_attese(ModelRc::from(self.modello_attese.clone()));
        self.riempi_stato(p);
        p.on_tocca(|id| {
            if let Some(ponte) = ponte() {
                ponte.tocca(&id);
            }
        });
        p.on_azione(|voce, pulsante| {
            if let Some(ponte) = ponte() {
                ponte.azione(&voce, &pulsante);
            }
        });
    }

    /// Posizione nella riga della voce che la pillola allargata sta chiamando: il pannello aperto
    /// da lì la espande.
    pub fn voce_della_pillola(&self) -> Option<i32> {
        let id = self.mostrato.borrow().as_ref()?.voce_allargata()?.to_owned();
        let indice = self.attese.borrow().ordinate().iter().position(|v| v.id == id)?;
        Some(indice as i32)
    }

    /// Il pannello si è aperto: le voci in attesa sono viste (un'attenzione non allarga più la
    /// pillola) e il pannello prende l'umore.
    pub fn pannello_aperto(&self, p: &Pannello) {
        *self.pannello.borrow_mut() = p.as_weak();
        self.attese.borrow_mut().segna_viste();
        *self.mostrato.borrow_mut() = None; // il pannello nuovo non ha ancora l'umore
        self.aggiorna_umore();
    }

    /// Il pannello si è chiuso e la pillola (di nuovo attiva) sta per ricomparire: riprende
    /// l'umore di adesso.
    pub fn pannello_chiuso(&self) {
        *self.pannello.borrow_mut() = slint::Weak::default();
        *self.mostrato.borrow_mut() = None;
        self.aggiorna_umore();
    }

    /// Un tocco su un preferito: il comando se ce n'è uno (mai su entità non disponibili né con la
    /// casa non connessa: l'interfaccia già non lo permette, qui lo si garantisce).
    pub fn tocca(&self, entity_id: &str) {
        if !self.connesso() {
            return;
        }
        let comando = {
            let menu = self.menu.borrow();
            let Some(menu) = menu.as_ref() else { return };
            entita(menu, entity_id).as_ref().and_then(comando_per)
        };
        let Some((dominio, servizio)) = comando else { return };
        if self.errori.borrow_mut().remove(entity_id).is_some() {
            self.ricostruisci_preferiti();
        }
        self.casa.comando(entity_id, dominio, servizio);
    }

    // ─── Voci «In attesa» ────────────────────────────────────────────────────

    /// Mette (o aggiorna) una voce. Una voce nuova che lo merita manda la notifica di sistema,
    /// salvo `avvisa` falso (notifiche di HA già presenti alla connessione).
    pub fn metti_voce(&self, v: Voce, avvisa: bool) {
        let nuova = !self.attese.borrow().contiene(&v.id);
        if nuova && avvisa && merita_notifica_di_sistema(&v) {
            notifica_sistema(&v);
        }
        self.attese.borrow_mut().metti(v);
        self.attese_cambiate();
    }

    pub fn togli_voce(&self, id: &str) {
        if !self.attese.borrow().contiene(id) {
            return;
        }
        self.attese.borrow_mut().togli(id);
        self.non_archiviate.borrow_mut().remove(id);
        self.attese_cambiate();
    }

    /// Umore, pillola, riga del pannello (e voce espansa) dopo un cambio delle voci.
    fn attese_cambiate(&self) {
        let livello = livello_massimo(&self.attese.borrow());
        self.cervello.borrow_mut().ricevi(Segnale::InAttesa(livello), Instant::now());
        let voci: Vec<VoceAttesaUi> = {
            let non_archiviate = self.non_archiviate.borrow();
            self.attese
                .borrow()
                .ordinate()
                .into_iter()
                .map(|v| voce_attesa_ui(v, non_archiviate.contains(&v.id)))
                .collect()
        };
        aggiorna_modello(&self.modello_attese, voci);
        if let Some(p) = self.pannello.borrow().upgrade() {
            // La voce espansa può aver cambiato posto o essere sparita.
            let aperta = p.get_voce_aperta().to_string();
            if !aperta.is_empty() {
                match self.attese.borrow().ordinate().iter().position(|v| v.id == aperta) {
                    Some(i) => p.set_indice_aperta(i as i32),
                    None => {
                        p.set_voce_aperta("".into());
                        p.set_indice_aperta(-1);
                    }
                }
            }
        }
        self.aggiorna_umore();
    }

    /// Un pulsante di una voce (nella pillola o nel pannello).
    pub fn azione(&self, voce: &str, pulsante: &str) {
        if voce == ID_CASA_OFFLINE && pulsante == PULSANTE_RIPROVA {
            self.casa.connetti_attivo();
        } else if let Some(id) = voce.strip_prefix(PREFISSO_NOTIFICA) {
            // Sparisce quando HA la rimuove (arriva `NotificaRimossa`); se non riesce, la voce lo dice.
            if pulsante == PULSANTE_ARCHIVIA {
                if self.non_archiviate.borrow_mut().remove(voce) {
                    self.attese_cambiate();
                }
                self.casa.archivia_notifica(id);
            }
        } else if self.attese.borrow().voce(voce).is_some_and(|v| v.fonte == Fonte::Prova) {
            eprintln!("avviso di prova: «{pulsante}»");
            self.togli_voce(voce);
        }
    }

    /// Casa non connessa da oltre 5 minuti → voce; connessione tornata → via. La sveglia suona
    /// solo allo scadere dei 5 minuti (nessun controllo periodico).
    fn aggiorna_casa_offline(&self) {
        let ora = Instant::now();
        let (voce, scadenza) = {
            let s = self.sorveglianza.borrow();
            (s.voce(ora), s.scadenza(ora))
        };
        match voce {
            Some(v) if !self.attese.borrow().contiene(&v.id) => self.metti_voce(v, true),
            Some(_) => {}
            None => self.togli_voce(ID_CASA_OFFLINE),
        }
        match scadenza {
            Some(tra) => self.sveglia_casa.start(slint::TimerMode::SingleShot, tra + MARGINE_SVEGLIA, || {
                if let Some(p) = ponte() {
                    p.aggiorna_casa_offline();
                }
            }),
            None => self.sveglia_casa.stop(),
        }
    }

    fn batteria(&self, percentuale: f32, scollegata: bool) {
        let presente = self.attese.borrow().contiene(ID_BATTERIA);
        match attenzione::voce_batteria_con_isteresi(percentuale, scollegata, presente, Instant::now()) {
            Some(v) => self.metti_voce(v, true),
            None => self.togli_voce(ID_BATTERIA),
        }
    }

    /// `--prova-avviso <livello>`.
    pub fn prova_avviso(&self, livello: Livello) {
        self.metti_voce(attenzione::voce_di_prova(livello, Instant::now()), true);
    }
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;
    use casa_core::types::*;

    fn base(id: &str, disponibile: bool) -> BaseEntity {
        BaseEntity {
            entity_id: id.into(),
            name: format!("Nome {id}"),
            area_id: None,
            state: if disponibile { "on".into() } else { "unavailable".into() },
            is_available: disponibile,
        }
    }

    pub fn luce(id: &str, accesa: bool) -> AppEntity {
        AppEntity::Light(LightEntity {
            base: BaseEntity { state: if accesa { "on" } else { "off" }.into(), ..base(id, true) },
            is_on: accesa,
            brightness: None,
            hue: None,
            saturation: None,
            color_temp: None,
            min_color_temp: None,
            max_color_temp: None,
            supports_brightness: false,
            supports_color_temp: false,
            supports_rgb: false,
        })
    }

    pub fn luce_non_disponibile(id: &str) -> AppEntity {
        let AppEntity::Light(mut l) = luce(id, false) else { unreachable!() };
        l.base = base(id, false);
        AppEntity::Light(l)
    }

    pub fn interruttore(id: &str, acceso: bool) -> AppEntity {
        AppEntity::Switch(SwitchEntity {
            base: BaseEntity { state: if acceso { "on" } else { "off" }.into(), ..base(id, true) },
            is_on: acceso,
        })
    }

    pub fn scena(id: &str) -> AppEntity {
        AppEntity::Scene(SceneEntity { base: BaseEntity { state: "2026-11-13T10:00:00".into(), ..base(id, true) } })
    }

    pub fn sensore(id: &str, valore: &str, unita: Option<&str>) -> AppEntity {
        AppEntity::Sensor(SensorEntity {
            base: BaseEntity { state: valore.into(), ..base(id, true) },
            value: valore.into(),
            unit: unita.map(str::to_owned),
            device_class: Some("temperature".into()),
        })
    }

    pub fn serratura(id: &str) -> AppEntity {
        AppEntity::Lock(LockEntity {
            base: BaseEntity { state: "locked".into(), ..base(id, true) },
            lock_state: "locked".into(),
        })
    }

    pub fn menu(preferiti: Vec<AppEntity>, stanze: Vec<Vec<AppEntity>>) -> MenuData {
        MenuData {
            favorites: preferiti,
            rooms: stanze
                .into_iter()
                .enumerate()
                .map(|(i, entities)| Room { area_id: format!("a{i}"), name: format!("Stanza {i}"), entities })
                .collect(),
            scenes: vec![],
            cameras: vec![],
            temp_unit: "°C".into(),
            notification_count: 0,
            update_count: 0,
        }
    }

    fn ids(v: &[Preferito]) -> Vec<&str> {
        v.iter().map(|p| p.entity_id.as_str()).collect()
    }

    #[test]
    fn preferiti_nell_ordine_e_massimo_otto() {
        let preferiti: Vec<AppEntity> = (0..10).map(|i| luce(&format!("light.l{i}"), i % 2 == 0)).collect();
        let m = menu(preferiti, vec![vec![interruttore("switch.stanza", true)]]);
        let p = preferiti_ui(&m, MASSIMO_PREFERITI);
        assert_eq!(p.len(), 8);
        let attesi: Vec<String> = (0..8).map(|i| format!("light.l{i}")).collect();
        assert_eq!(ids(&p), attesi.iter().map(String::as_str).collect::<Vec<_>>());
        assert_eq!(p[0].tipo, Tipo::Interruttore);
        assert!(p[0].acceso);
        assert!(!p[1].acceso);
        assert_eq!(p[0].nome, "Nome light.l0");
        assert!(p[0].disponibile);
        assert_eq!(p[0].errore, None);
    }

    #[test]
    fn senza_preferiti_usa_le_prime_entita_delle_stanze() {
        let m = menu(
            vec![],
            vec![
                vec![luce("light.a", true), scena("scene.b")],
                vec![interruttore("switch.c", false), sensore("sensor.d", "20", Some("°C"))],
            ],
        );
        let p = preferiti_ui(&m, 3);
        assert_eq!(ids(&p), ["light.a", "scene.b", "switch.c"]);
        assert_eq!(p[1].tipo, Tipo::Scena);
        assert_eq!(p[2].tipo, Tipo::Interruttore);
    }

    #[test]
    fn senza_preferiti_dopo_le_stanze_vengono_le_scene() {
        // Il mapper mette le scene in `menu.scenes`, mai nelle stanze: senza preferiti
        // arrivano dopo le entità delle stanze, così un tocco può attivarle.
        let mut m = menu(vec![], vec![vec![luce("light.a", true)]]);
        let AppEntity::Scene(s) = scena("scene.cinema") else { unreachable!() };
        m.scenes = vec![s];
        let p = preferiti_ui(&m, MASSIMO_PREFERITI);
        assert_eq!(ids(&p), ["light.a", "scene.cinema"]);
        assert_eq!(p[1].tipo, Tipo::Scena);
        assert_eq!(entita(&m, "scene.cinema").as_ref().and_then(comando_per), Some(("scene", "turn_on")));
        assert!(entita(&m, "light.zzz").is_none());
    }

    #[test]
    fn sensore_mostra_valore_con_unita() {
        let m = menu(vec![sensore("sensor.t", "21.5", Some("°C")), sensore("sensor.n", "7", None), serratura("lock.porta")], vec![]);
        let p = preferiti_ui(&m, MASSIMO_PREFERITI);
        assert_eq!(p[0].tipo, Tipo::Lettura);
        assert_eq!(p[0].valore, "21.5 °C");
        assert_eq!(p[1].valore, "7");
        // Altri tipi: lettura con lo stato.
        assert_eq!(p[2].tipo, Tipo::Lettura);
        assert_eq!(p[2].valore, "locked");
    }

    #[test]
    fn comando_luce_interruttore_scena() {
        assert_eq!(comando_per(&luce("light.a", false)), Some(("light", "toggle")));
        assert_eq!(comando_per(&interruttore("switch.b", true)), Some(("switch", "toggle")));
        assert_eq!(comando_per(&scena("scene.c")), Some(("scene", "turn_on")));
    }

    #[test]
    fn niente_comando_su_entita_non_disponibile() {
        assert_eq!(comando_per(&luce_non_disponibile("light.giardino")), None);
        let m = menu(vec![luce_non_disponibile("light.giardino")], vec![]);
        assert!(!preferiti_ui(&m, MASSIMO_PREFERITI)[0].disponibile);
    }

    #[test]
    fn niente_comando_su_sensore() {
        assert_eq!(comando_per(&sensore("sensor.t", "21.5", Some("°C"))), None);
        assert_eq!(comando_per(&serratura("lock.porta")), None);
    }

    fn p(id: &str, tipo: Tipo, acceso: bool, valore: &str) -> Preferito {
        Preferito {
            entity_id: id.into(),
            nome: id.into(),
            tipo,
            acceso,
            valore: valore.into(),
            disponibile: true,
            errore: None,
        }
    }

    #[test]
    fn preferiti_non_toccabili_se_disconnesso() {
        let luce = p("light.a", Tipo::Interruttore, true, "on");
        assert!(!voce_toccabile(&luce, false));
        assert!(voce_toccabile(&luce, true));
        assert!(!voce_toccabile(&p("sensor.t", Tipo::Lettura, false, "21.5 °C"), true));
    }

    #[test]
    fn testo_dello_stato() {
        assert_eq!(testo_stato(&p("light.a", Tipo::Interruttore, true, "on")), "Acceso");
        assert_eq!(testo_stato(&p("switch.a", Tipo::Interruttore, false, "off")), "Spento");
        assert_eq!(testo_stato(&p("scene.a", Tipo::Scena, false, "")), "Scena");
        assert_eq!(testo_stato(&p("sensor.t", Tipo::Lettura, false, "21.5 °C")), "21.5 °C");
        let mut x = p("light.a", Tipo::Interruttore, true, "on");
        x.disponibile = false;
        assert_eq!(testo_stato(&x), "Non disponibile");
        x.disponibile = true;
        x.errore = Some("not_found".into());
        assert_eq!(testo_stato(&x), "Non riuscito");
    }

    #[test]
    fn genere_per_icona_e_tinta() {
        assert_eq!(genere(&p("light.a", Tipo::Interruttore, true, "on")), "luce");
        assert_eq!(genere(&p("switch.a", Tipo::Interruttore, true, "on")), "presa");
        assert_eq!(genere(&p("scene.a", Tipo::Scena, false, "")), "scena");
        assert_eq!(genere(&p("sensor.t", Tipo::Lettura, false, "21.5 °C")), "termo");
        assert_eq!(genere(&p("sensor.u", Tipo::Lettura, false, "48 %")), "goccia");
        assert_eq!(genere(&p("sensor.x", Tipo::Lettura, false, "7")), "sensore");
        assert_eq!(genere(&p("lock.porta", Tipo::Lettura, false, "locked")), "casa");
    }

    #[test]
    fn riga_di_stato_della_casa() {
        let m = menu(
            vec![sensore("sensor.t", "21.5", Some("°C"))],
            vec![vec![luce("light.a", true), luce("light.b", true), luce("light.c", false)], vec![sensore("sensor.t", "21.5", Some("°C"))]],
        );
        assert_eq!(riga_stato(&StatoConnessione::Connesso, Some(&m)), "2 luci accese · 21.5 °C");
        let una = menu(vec![], vec![vec![luce("light.a", true)]]);
        assert_eq!(riga_stato(&StatoConnessione::Connesso, Some(&una)), "1 luce accesa");
        let spente = menu(vec![], vec![vec![luce("light.a", false)]]);
        assert_eq!(riga_stato(&StatoConnessione::Connesso, Some(&spente)), "Luci spente");
        assert_eq!(riga_stato(&StatoConnessione::Connesso, None), "Casa connessa");
        assert_eq!(riga_stato(&StatoConnessione::InConnessione, Some(&m)), "Connessione alla casa…");
        assert_eq!(riga_stato(&StatoConnessione::Disconnesso, Some(&m)), "Casa non connessa");
        assert_eq!(riga_stato(&StatoConnessione::Errore("token mancante".into()), None), "Token della casa mancante");
    }

    #[test]
    fn errori_in_frasi_semplici() {
        let riga = |e: &str| riga_stato(&StatoConnessione::Errore(e.into()), None);
        assert_eq!(riga("connessione non riuscita: IO error: Impossibile stabilire la connessione"), "Casa non raggiungibile");
        assert_eq!(riga("Home Assistant non risponde"), "Casa non raggiungibile");
        assert_eq!(riga("token non valido"), "Token non valido");
        assert_eq!(riga("nessun ambiente configurato"), "Casa non configurata");
        assert_eq!(riga("portachiavi: accesso negato"), "Portachiavi non leggibile");
    }

    #[test]
    fn sensori_dei_quadranti_con_arco() {
        let m = menu(vec![luce("light.a", true), sensore("sensor.t", "20", Some("°C"))], vec![]);
        let v = sensori_quadranti(&m, MASSIMO_PREFERITI, MASSIMO_SENSORI);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].0.entity_id, "sensor.t");
        assert!((v[0].1 - 0.5).abs() < 1e-6);
        let primo = |m: &MenuData| sensori_quadranti(m, 8, 3)[0].1;
        assert!((primo(&menu(vec![sensore("sensor.u", "48", Some("%"))], vec![])) - 0.48).abs() < 1e-6);
        assert_eq!(primo(&menu(vec![sensore("sensor.t", "45", Some("°C"))], vec![])), 1.0);
        assert_eq!(primo(&menu(vec![sensore("sensor.x", "abc", Some("°C"))], vec![])), -1.0);
        // Fuori dai preferiti mostrati non conta.
        assert!(sensori_quadranti(&m, 1, 3).is_empty());
    }

    #[test]
    fn al_massimo_tre_sensori_nell_ordine() {
        let s = |i: usize| sensore(&format!("sensor.s{i}"), "20", Some("°C"));
        let m = menu(vec![s(0), luce("light.a", true), s(1), s(2), s(3)], vec![]);
        let v = sensori_quadranti(&m, MASSIMO_PREFERITI, MASSIMO_SENSORI);
        let ids: Vec<&str> = v.iter().map(|(p, _)| p.entity_id.as_str()).collect();
        assert_eq!(ids, ["sensor.s0", "sensor.s1", "sensor.s2"]);
    }
}
