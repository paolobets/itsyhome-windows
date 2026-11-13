//! Ponte casa → interfaccia: dai dati di `casa-core` ai preferiti del pannello, e dai tocchi
//! sui preferiti ai comandi verso Home Assistant.

use crate::pillola;
use crate::umore::{Cervello, Segnale, Umore};
use crate::{Pannello, Pillola, PreferitoUi};
use casa_core::eventi::{Ascoltatore, CasaEvento, StatoConnessione};
use casa_core::servizio::CasaService;
use casa_core::types::{AppEntity, BaseEntity, MenuData};
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// La sveglia dell'umore suona appena dopo la scadenza, mai un attimo prima.
const MARGINE_SVEGLIA: Duration = Duration::from_millis(20);

/// Quanti preferiti mostra il pannello.
pub const MASSIMO_PREFERITI: usize = 8;

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
        (StatoConnessione::Errore(e), _) => format!("Casa: {e}"),
    }
}

/// Il sensore del quadrante CASA: il primo sensore disponibile fra i preferiti mostrati, con la
/// frazione dell'arco (umidità 0–100%, temperatura 10–30 °C / 50–86 °F; altrimenti -1 = niente arco).
pub fn sensore_quadrante(menu: &MenuData, max: usize) -> Option<(Preferito, f32)> {
    let e = entita_mostrate(menu)
        .take(max)
        .find(|e| matches!(e, AppEntity::Sensor(s) if s.base.is_available))?;
    let AppEntity::Sensor(s) = &e else { return None };
    let numero = s.value.trim().replace(',', ".").parse::<f32>().ok();
    let unita = s.unit.as_deref().unwrap_or_default();
    let frazione = match numero {
        Some(v) if unita.ends_with('%') => v / 100.0,
        Some(v) if unita.ends_with('F') => (v - 50.0) / 36.0,
        Some(v) if unita.contains('°') => (v - 10.0) / 20.0,
        _ => return Some((preferito(&e), -1.0)),
    };
    Some((preferito(&e), frazione.clamp(0.0, 1.0)))
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

/// Aggancio delle notifiche di sistema: il toast arriva nel Task 10 (`notifiche::mostra`).
fn mostra_notifica_di_sistema(_titolo: &str, _messaggio: &str) {}

pub struct Ponte {
    casa: Rc<CasaService>,
    pillola: slint::Weak<Pillola>,
    /// Il pannello esiste solo da aperto: qui il riferimento debole, vuoto da chiuso.
    pannello: RefCell<slint::Weak<Pannello>>,
    cervello: Rc<RefCell<Cervello>>,
    /// `--umore`: umore fisso per prova, gli eventi non lo cambiano.
    umore_fisso: bool,
    stato: RefCell<StatoConnessione>,
    menu: RefCell<Option<Arc<MenuData>>>,
    /// Comandi falliti, per entità: l'errore resta sulla voce fino al prossimo tocco o a un esito ok.
    errori: RefCell<HashMap<String, String>>,
    /// Il modello dei preferiti: sopravvive al pannello, che lo riprende a ogni apertura.
    preferiti: Rc<VecModel<PreferitoUi>>,
    /// Ultimo umore mostrato, per non riscrivere la pillola senza motivo.
    mostrato: RefCell<Option<(Umore, String)>>,
    /// Gira solo mentre un umore scade da solo (contento): nessun timer fisso.
    sveglia: slint::Timer,
    /// Errori da mostrare nelle Impostazioni (Task 9).
    #[allow(dead_code)]
    errore_import: Option<String>,
    #[allow(dead_code)]
    errore_config: RefCell<Option<String>>,
}

pub fn collega(
    casa: Rc<CasaService>,
    pillola: &Pillola,
    cervello: Rc<RefCell<Cervello>>,
    umore_fisso: bool,
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
        mostrato: RefCell::new(None),
        sveglia: slint::Timer::default(),
        errore_import,
        errore_config: RefCell::new(None),
    });
    PONTE.with(|p| *p.borrow_mut() = Some(ponte.clone()));
    ponte.aggiorna_umore();
    ponte
}

fn voce_ui(p: &Preferito) -> PreferitoUi {
    PreferitoUi {
        entity_id: p.entity_id.as_str().into(),
        nome: p.nome.as_str().into(),
        genere: genere(p).into(),
        acceso: p.acceso && p.disponibile,
        stato: testo_stato(p).into(),
        disponibile: p.disponibile,
        errore: p.errore.as_deref().unwrap_or_default().into(),
        toccabile: p.disponibile && p.tipo != Tipo::Lettura,
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
                *self.stato.borrow_mut() = s;
                self.aggiorna_pannello();
            }
            CasaEvento::Dati(menu) => {
                *self.menu.borrow_mut() = Some(menu);
                self.ricostruisci_preferiti();
                self.aggiorna_pannello();
            }
            CasaEvento::Notifica { titolo, messaggio } => {
                mostra_notifica_di_sistema(&titolo, &messaggio);
                self.cervello.borrow_mut().ricevi(Segnale::Notifica { titolo }, ora);
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

    /// Ricalcola l'umore e lo porta alla pillola e, se aperto, al pannello. Se l'umore scadrà da
    /// solo (contento), una sveglia singola lo ricalcola allora; altrimenti nessun timer.
    fn aggiorna_umore(&self) {
        if self.umore_fisso {
            return;
        }
        let ora = Instant::now();
        let (umore, messaggio, scadenza) = {
            let c = self.cervello.borrow();
            (c.umore(ora), c.messaggio().unwrap_or_default().to_owned(), c.prossimo_cambio(ora))
        };
        let nuovo = (umore, messaggio);
        if self.mostrato.borrow().as_ref() != Some(&nuovo) {
            // La pillola nascosta (pannello aperto) non si tocca: ridisegnarla mentre Windows la
            // dissolve la farebbe intravedere. Riprende l'umore appena prima di ricomparire.
            if let Some(p) = self.pillola.upgrade().filter(|p| p.get_attiva()) {
                pillola::imposta_umore(&p, nuovo.0, &nuovo.1);
            }
            if let Some(p) = self.pannello.borrow().upgrade() {
                p.set_umore(nuovo.0.come_stringa().into());
            }
            *self.mostrato.borrow_mut() = Some(nuovo);
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

    /// Preferiti dai dati più recenti con gli errori ancora aperti; si riscrivono solo le righe
    /// cambiate (una riga uguale non ridisegna nulla).
    fn ricostruisci_preferiti(&self) {
        let voci: Vec<PreferitoUi> = {
            let menu = self.menu.borrow();
            let Some(menu) = menu.as_ref() else { return };
            let errori = self.errori.borrow();
            preferiti_ui(menu, MASSIMO_PREFERITI)
                .into_iter()
                .map(|mut p| {
                    p.errore = errori.get(&p.entity_id).cloned();
                    voce_ui(&p)
                })
                .collect()
        };
        let modello = &self.preferiti;
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

    /// Riga di stato e quadrante CASA sul pannello aperto.
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
        match menu.as_deref().and_then(|m| sensore_quadrante(m, MASSIMO_PREFERITI)) {
            Some((s, f)) => {
                p.set_sensore(voce_ui(&s));
                p.set_sensore_frazione(f);
                p.set_ha_sensore(true);
            }
            None => p.set_ha_sensore(false),
        }
    }

    /// Il pannello appena creato: modello dei preferiti, stato della casa, tocchi.
    pub fn riempi(&self, p: &Pannello) {
        p.set_preferiti(ModelRc::from(self.preferiti.clone()));
        self.riempi_stato(p);
        p.on_tocca(|id| {
            if let Some(ponte) = ponte() {
                ponte.tocca(&id);
            }
        });
    }

    /// Il pannello si è aperto: la notifica in attesa è vista, e il pannello prende l'umore.
    pub fn pannello_aperto(&self, p: &Pannello) {
        *self.pannello.borrow_mut() = p.as_weak();
        if self.umore_fisso {
            if let Some(pillola) = self.pillola.upgrade() {
                p.set_umore(pillola.get_umore());
            }
            return;
        }
        *self.mostrato.borrow_mut() = None; // il pannello nuovo non ha ancora l'umore
        self.cervello.borrow_mut().ricevi(Segnale::PannelloAperto, Instant::now());
        self.aggiorna_umore();
    }

    /// Il pannello si è chiuso e la pillola (di nuovo attiva) sta per ricomparire: riprende
    /// l'umore di adesso.
    pub fn pannello_chiuso(&self) {
        *self.pannello.borrow_mut() = slint::Weak::default();
        *self.mostrato.borrow_mut() = None;
        self.aggiorna_umore();
    }

    /// Un tocco su un preferito: il comando se ce n'è uno (mai su entità non disponibili).
    pub fn tocca(&self, entity_id: &str) {
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
        assert_eq!(riga_stato(&StatoConnessione::Errore("token mancante".into()), None), "Casa: token mancante");
    }

    #[test]
    fn sensore_del_quadrante_con_arco() {
        let m = menu(vec![luce("light.a", true), sensore("sensor.t", "20", Some("°C"))], vec![]);
        let (s, f) = sensore_quadrante(&m, MASSIMO_PREFERITI).unwrap();
        assert_eq!(s.entity_id, "sensor.t");
        assert!((f - 0.5).abs() < 1e-6);
        let u = menu(vec![sensore("sensor.u", "48", Some("%"))], vec![]);
        assert!((sensore_quadrante(&u, 8).unwrap().1 - 0.48).abs() < 1e-6);
        let caldo = menu(vec![sensore("sensor.t", "45", Some("°C"))], vec![]);
        assert_eq!(sensore_quadrante(&caldo, 8).unwrap().1, 1.0);
        let strano = menu(vec![sensore("sensor.x", "abc", Some("°C"))], vec![]);
        assert_eq!(sensore_quadrante(&strano, 8).unwrap().1, -1.0);
        // Fuori dai preferiti mostrati non conta.
        assert!(sensore_quadrante(&m, 1).is_none());
    }
}
