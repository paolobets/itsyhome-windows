//! Ponte casa → interfaccia: dai dati di `casa-core` ai preferiti del pannello, e dai tocchi
//! sui preferiti ai comandi verso Home Assistant. Tiene anche le voci «In attesa» (notifiche di
//! Home Assistant, casa irraggiungibile, batteria, prova) e decide umore e aspetto della pillola.

use crate::attenzione::{
    self, livello_massimo, notifica_sistema, prossimo_cambio_aspetto, Attese, Fonte, Livello, SorveglianzaCasa,
    Voce, ID_BATTERIA, ID_CASA_OFFLINE, ID_CASA_TOKEN, PREFISSO_NOTIFICA, PREFISSO_SISTEMA, PULSANTE_ARCHIVIA,
    PULSANTE_APRI, PULSANTE_IMPOSTAZIONI, PULSANTE_RIPROVA,
};
use crate::sistema::{autoavvio, servizi};
use crate::impostazioni::{self, Modulo, Tono};
use crate::ottimismo::Ottimismo;
use crate::pillola::{self, VistaPillola};
use crate::umore::{Cervello, Segnale, Umore};
use crate::dispositivi::{self, Riga, TipoRiga};
use crate::collegamenti;
use crate::{
    AmbienteUi, CollegamentoUi, FinestraImpostazioni, GenereUi, Pannello, Pillola, PreferitoUi, RigaDispositivoUi, SensoreUi, StanzaUi,
    TipoRigaUi, TonoUi, VoceAttesaUi,
};
use casa_core::diagnosi::Contatore;
use casa_core::eventi::{Ascoltatore, CasaEvento, StatoConnessione};
use casa_core::segreti::Segreti;
use casa_core::servizio::CasaService;
use casa_core::types::{AppEntity, BaseEntity, MenuData, SensorEntity, TestConnectionResult, GRUPPO_SCENE};
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
        // Il testo con la virgola italiana; l'arco dei Quadranti legge il valore grezzo.
        AppEntity::Sensor(s) => {
            let numero = numero_italiano(&s.value);
            let valore = match s.unit.as_deref().filter(|u| !u.is_empty()) {
                Some(u) => format!("{numero} {u}"),
                None => numero,
            };
            (Tipo::Lettura, false, valore)
        }
        // Tutto il resto si legge soltanto (i controlli arrivano con la milestone Casa).
        _ => (Tipo::Lettura, false, stato_in_lettura(e)),
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

/// Lo stato in italiano di un dispositivo che per ora si legge soltanto (tapparelle, serrature,
/// termostati, ventilatori…). Uno stato che non conosciamo resta com'è, con la virgola italiana.
pub fn stato_in_lettura(e: &AppEntity) -> String {
    let b = base(e);
    let parola = |s: &str| -> Option<&'static str> {
        Some(match s {
            "on" => "Acceso",
            "off" => "Spento",
            _ => return None,
        })
    };
    let testo: Option<String> = match e {
        AppEntity::Cover(c) => {
            let parola = match c.cover_state.as_str() {
                "open" => "Aperta",
                "closed" => "Chiusa",
                "opening" => "In apertura",
                "closing" => "In chiusura",
                _ => "Ferma",
            };
            Some(match c.position.filter(|_| c.supports_position) {
                Some(p) => format!("{parola} · {}%", p.round()),
                None => parola.to_owned(),
            })
        }
        AppEntity::Lock(l) => Some(
            match l.lock_state.as_str() {
                "locked" => "Chiusa",
                "unlocked" => "Aperta",
                "locking" => "In chiusura",
                "unlocking" => "In apertura",
                "jammed" => "Bloccata",
                _ => "Sconosciuto",
            }
            .to_owned(),
        ),
        AppEntity::Climate(c) => {
            let modo = match c.hvac_mode.as_str() {
                "off" => "Spento",
                "heat" => "Riscaldamento",
                "cool" => "Raffrescamento",
                "heat_cool" | "auto" => "Automatico",
                "dry" => "Deumidifica",
                "fan_only" => "Solo ventola",
                altro => altro,
            };
            Some(match c.current_temp {
                Some(t) => format!("{modo} · {}°", numero_italiano(&t.to_string())),
                None => modo.to_owned(),
            })
        }
        AppEntity::Fan(f) => Some(if f.is_on { "Acceso" } else { "Spento" }.to_owned()),
        AppEntity::Humidifier(h) => Some(if h.is_on { "Acceso" } else { "Spento" }.to_owned()),
        AppEntity::Valve(v) => Some(if v.is_open { "Aperta" } else { "Chiusa" }.to_owned()),
        AppEntity::Alarm(a) => Some(
            match a.alarm_state.as_str() {
                "disarmed" => "Disinserito",
                "armed_home" => "Inserito in casa",
                "armed_away" => "Inserito fuori casa",
                "armed_night" => "Inserito notte",
                "armed_vacation" => "Inserito vacanza",
                "armed_custom_bypass" => "Inserito personalizzato",
                "pending" | "arming" => "In inserimento",
                "triggered" => "Allarme scattato",
                _ => "Sconosciuto",
            }
            .to_owned(),
        ),
        AppEntity::Camera(_) => Some("Telecamera".to_owned()),
        _ => parola(&b.state).map(str::to_owned),
    };
    testo.unwrap_or_else(|| numero_italiano(&b.state))
}

/// Le stanze della vista Casa: quelle con qualcosa da mostrare, nell'ordine salvato (ci pensa il
/// mapper), con l'icona scelta o quella predefinita dal nome; «Altro» (entità senza stanza) con
/// l'icona della casa; in fondo le scene, se ci sono (il mapper le tiene fuori dalle stanze,
/// come la v2, che le mostrava in una sezione loro dopo le stanze).
pub fn stanze_ui(menu: &MenuData, icone: &HashMap<String, String>) -> Vec<StanzaUi> {
    let mut stanze: Vec<StanzaUi> = menu
        .rooms
        .iter()
        .map(|r| StanzaUi {
            area_id: r.area_id.as_str().into(),
            nome: r.name.as_str().into(),
            icona: if r.area_id.is_empty() {
                -1
            } else {
                crate::stanze::icona(icone.get(&r.area_id).map(String::as_str), &r.name) as i32
            },
            quanti: r.entities.len() as i32,
        })
        .collect();
    if !menu.scenes.is_empty() {
        stanze.push(StanzaUi {
            area_id: GRUPPO_SCENE.into(),
            nome: "Scene".into(),
            icona: -2,
            quanti: menu.scenes.len() as i32,
        });
    }
    stanze
}

/// I dispositivi di una stanza della vista Casa, nell'ordine salvato (senza le nascoste: il mapper
/// non le mette); le scene per il gruppo delle scene.
pub fn dispositivi_stanza(menu: &MenuData, area_id: &str) -> Vec<AppEntity> {
    if area_id == GRUPPO_SCENE {
        return menu.scenes.iter().map(|s| AppEntity::Scene(s.clone())).collect();
    }
    menu.rooms.iter().find(|r| r.area_id == area_id).map(|r| r.entities.clone()).unwrap_or_default()
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
        // Il mapper fa «interruttore» anche di un `input_boolean`: il servizio è del suo dominio.
        AppEntity::Switch(s) if s.base.entity_id.starts_with("input_boolean.") => Some(("input_boolean", "toggle")),
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

/// Un numero di Home Assistant con la virgola italiana («21.5» → «21,5»); tutto il resto com'è.
/// Numero = cifre, al più un punto con cifre da entrambi i lati, segno davanti facoltativo.
pub fn numero_italiano(v: &str) -> String {
    let senza_segno = v.strip_prefix('-').unwrap_or(v);
    let numero = match senza_segno.split_once('.') {
        Some((intera, decimali)) => {
            !intera.is_empty()
                && !decimali.is_empty()
                && intera.bytes().all(|b| b.is_ascii_digit())
                && decimali.bytes().all(|b| b.is_ascii_digit())
        }
        None => false,
    };
    if numero { v.replacen('.', ",", 1) } else { v.to_owned() }
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

/// Che cosa è un preferito per l'interfaccia (icona e tinta): luce, presa, scena, termo, goccia,
/// sensore o casa (tutto il resto).
pub fn genere(p: &Preferito) -> GenereUi {
    match p.entity_id.split('.').next().unwrap_or_default() {
        "light" => GenereUi::Luce,
        "switch" => GenereUi::Presa,
        "scene" => GenereUi::Scena,
        "climate" => GenereUi::Termo,
        "sensor" if p.valore.contains('°') => GenereUi::Termo,
        "sensor" if p.valore.ends_with('%') => GenereUi::Goccia,
        "sensor" => GenereUi::Sensore,
        _ => GenereUi::Casa,
    }
}

/// Il genere (icona) di un dispositivo dal suo tipo nel catalogo.
fn genere_di_tipo(kind: &str) -> GenereUi {
    match kind {
        "light" => GenereUi::Luce,
        "switch" => GenereUi::Presa,
        "scene" => GenereUi::Scena,
        "climate" => GenereUi::Termo,
        "sensor" => GenereUi::Sensore,
        _ => GenereUi::Casa,
    }
}

/// L'id unico di una riga di Impostazioni › Dispositivi (un'entità compare due volte: fra i
/// preferiti e nella sua stanza).
fn id_riga(r: &Riga) -> String {
    match r.tipo {
        TipoRiga::TestataPreferiti => "h:".into(),
        TipoRiga::Preferito => format!("p:{}", r.chiave),
        TipoRiga::TestataStanza => format!("s:{}", r.chiave),
        TipoRiga::Dispositivo => format!("d:{}", r.chiave),
    }
}

/// Una riga di Impostazioni › Dispositivi per l'interfaccia.
pub fn riga_ui(r: &Riga) -> RigaDispositivoUi {
    RigaDispositivoUi {
        id: id_riga(r).into(),
        tipo: match r.tipo {
            TipoRiga::TestataPreferiti => TipoRigaUi::TestataPreferiti,
            TipoRiga::Preferito => TipoRigaUi::Preferito,
            TipoRiga::TestataStanza => TipoRigaUi::TestataStanza,
            TipoRiga::Dispositivo => TipoRigaUi::Dispositivo,
        },
        chiave: r.chiave.as_str().into(),
        area: r.area.as_str().into(),
        nome: r.nome.as_str().into(),
        sotto: r.sotto.as_str().into(),
        icona: r.icona,
        genere: genere_di_tipo(&r.kind),
        nascosto: r.nascosto,
        preferito: r.preferito,
        su: r.su,
        giu: r.giu,
        compressa: r.compressa,
        puo_icona: r.tipo == TipoRiga::TestataStanza && dispositivi::stanza_vera(&r.area),
    }
}

/// Il tono di uno stato come lo conosce l'interfaccia (colore del pallino).
fn tono_ui(t: Tono) -> TonoUi {
    match t {
        Tono::Ok => TonoUi::Ok,
        Tono::Attesa => TonoUi::Attesa,
        Tono::Errore => TonoUi::Errore,
        Tono::Spento => TonoUi::Spento,
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
        (StatoConnessione::NonConfigurato, _) => "Casa non configurata".into(),
        (StatoConnessione::TokenNonValido, _) => "Token di Home Assistant non valido".into(),
        (StatoConnessione::TokenMancante, _) => "Token di Home Assistant mancante".into(),
        (StatoConnessione::Errore(e), _) => frase_errore(e).into(),
    }
}

/// «Riprova» ha senso mentre la casa non risponde (errore o tentativo in corso): non dopo un
/// «Disconnetti» voluto, senza ambiente o con un token da sistemare (lì servono le Impostazioni).
pub fn riprovabile(s: &StatoConnessione) -> bool {
    matches!(s, StatoConnessione::Errore(_) | StatoConnessione::InConnessione)
}

/// Una frase semplice al posto dell'errore tecnico di casa-core (il dettaglio resta per le
/// Impostazioni, sezione Informazioni).
pub fn frase_errore(e: &str) -> &'static str {
    if e.starts_with("portachiavi") {
        "Token salvato non leggibile"
    } else {
        "Casa non raggiungibile"
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

/// Acceso o spento, per i dispositivi che un tocco accende e spegne (il valore dello stato
/// ottimistico); `None` per tutto il resto.
pub fn acceso_di(e: &AppEntity) -> Option<bool> {
    match e {
        AppEntity::Light(l) => Some(l.is_on),
        AppEntity::Switch(s) => Some(s.is_on),
        _ => None,
    }
}

/// Un preferito con lo stato ottimistico al posto di quello vero, se ce n'è uno.
pub fn con_ottimismo(mut p: Preferito, ottimismo: &Ottimismo<bool>) -> Preferito {
    if let Some(&acceso) = ottimismo.atteso(&p.entity_id) {
        p.acceso = acceso;
        p.valore = if acceso { "on" } else { "off" }.into();
    }
    p
}

/// L'errore di un tocco che Home Assistant non ha confermato in tempo.
pub const ERRORE_SENZA_CONFERMA: &str = "Home Assistant non ha confermato il comando";

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

pub(crate) fn ponte() -> Option<Rc<Ponte>> {
    PONTE.with(|p| p.borrow().clone())
}

/// A fine programma, prima che `main` ritorni: il ponte (e con lui il `CasaService`) va lasciato
/// qui. Lasciato al thread_local verrebbe distrutto dentro `ExitProcess`, quando il thread del
/// servizio è già stato ucciso, e la sua `join` andrebbe in panico.
pub fn spegni() {
    let ponte = PONTE.with(|p| p.borrow_mut().take());
    drop(ponte);
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
    let mut sotto = format!("{} · {}", v.livello.parola(), v.fonte.come_stringa());
    if non_archiviata {
        sotto.push_str(" · archiviazione non riuscita");
    }
    VoceAttesaUi {
        id: v.id.as_str().into(),
        livello: pillola::livello_ui(v.livello),
        titolo: v.titolo.as_str().into(),
        sotto: sotto.into(),
        dettaglio: v.dettaglio.as_str().into(),
        icona: pillola::icona_fonte(v.fonte),
        pulsanti: pillola::pulsanti_ui(&v.pulsanti),
    }
}

/// Chi apre un indirizzo nel browser (vedi `Ponte::apri_url`).
type ApriUrl = dyn Fn(&str) -> Result<(), String>;

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
    /// Le stanze della vista Casa e i dispositivi della stanza aperta (`stanza_aperta`, un'area).
    stanze: Rc<VecModel<StanzaUi>>,
    dispositivi: Rc<VecModel<PreferitoUi>>,
    stanza_aperta: RefCell<Option<String>>,
    /// I modelli della vista Casa seguono i dati solo a pannello aperto (Task 14h): da chiuso
    /// nessuno li guarda (la pillola mostra umore e voci in attesa). `modelli_vecchi`: da chiuso è
    /// arrivato qualcosa che li cambierebbe, si rifanno all'apertura.
    modelli_vivi: Cell<bool>,
    modelli_vecchi: Cell<bool>,
    /// Da quando l'umore è «pensa» (per il registro diagnostico: si conta quando finisce).
    pensa_da: Cell<Option<Instant>>,
    /// Impostazioni › Dispositivi: le righe e le stanze compresse (solo finché l'app gira).
    righe: Rc<VecModel<RigaDispositivoUi>>,
    /// I collegamenti veloci, per il pannello e per Impostazioni › Collegamenti.
    collegamenti: Rc<VecModel<CollegamentoUi>>,
    /// Chi apre un collegamento nel browser (`sistema::browser::apri`; i test lo sostituiscono:
    /// nessun browser vero durante le prove).
    apri_url: RefCell<Rc<ApriUrl>>,
    /// Ultimo collegamento non aperto, col dettaglio tecnico, per Impostazioni › Informazioni.
    errore_collegamento: RefCell<Option<String>>,
    /// Un collegamento è già partito da questo pannello: Invio ripetuto o doppio clic, prima che
    /// il pannello si chiuda, non aprono altre schede. Torna falso alla chiusura del pannello.
    collegamento_aperto: Cell<bool>,
    compresse: RefCell<std::collections::HashSet<String>>,
    /// Ora (HH:MM) degli ultimi dati arrivati dalla casa: senza connessione la vista Casa dice di
    /// quando sono i valori che mostra.
    dati_delle: RefCell<Option<String>>,
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
    /// Il portachiavi dei token: le Impostazioni ci scrivono e ci cancellano, mai altro.
    segreti: Arc<dyn Segreti>,
    /// Gli ambienti per le Impostazioni (sopravvive al pannello come gli altri modelli).
    modello_ambienti: Rc<VecModel<AmbienteUi>>,
    /// Ultimi errori, col dettaglio tecnico, per Impostazioni › Informazioni.
    errore_connessione: RefCell<Option<String>>,
    errore_import: Option<String>,
    errore_config: RefCell<Option<String>>,
    /// Conta le «Prova connessione»: vale solo l'esito dell'ultima (un modulo chiuso o una prova
    /// rifatta scartano quelle ancora in volo).
    prova: Cell<u64>,
    /// La finestra delle Impostazioni, se aperta (la possiede `finestra_impostazioni`).
    finestra: RefCell<slint::Weak<FinestraImpostazioni>>,
    /// Apre la finestra delle Impostazioni chiudendo il pannello (lo collega `Apertura`): icona
    /// del dock e pulsante «Impostazioni» della voce del token.
    apri_impostazioni: RefCell<Option<Box<dyn Fn()>>>,
    /// Acceso/spento mostrati subito dopo un tocco, finché Home Assistant non conferma.
    ottimismo: RefCell<Ottimismo<bool>>,
    /// Suona alla scadenza della prossima attesa di conferma (solo se ce ne sono).
    sveglia_ottimismo: slint::Timer,
    /// `ITSYHOME_TRACCIA_TOCCO=1`: tempi del tocco sullo stderr (icona cambiata nel modello,
    /// conferma di Home Assistant). Il fotogramma si misura da fuori, sulle foto della zona: il
    /// renderer software di Skia non chiama il notificatore di rendering.
    traccia_tocco: bool,
}

pub fn collega(
    casa: Rc<CasaService>,
    segreti: Arc<dyn Segreti>,
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
        dati_delle: RefCell::new(None),
        errori: RefCell::new(HashMap::new()),
        preferiti: Rc::new(VecModel::default()),
        sensori: Rc::new(VecModel::default()),
        stanze: Rc::new(VecModel::default()),
        dispositivi: Rc::new(VecModel::default()),
        stanza_aperta: RefCell::new(None),
        modelli_vivi: Cell::new(false),
        modelli_vecchi: Cell::new(false),
        pensa_da: Cell::new(None),
        righe: Rc::new(VecModel::default()),
        collegamenti: Rc::new(VecModel::default()),
        apri_url: RefCell::new(Rc::new(crate::sistema::browser::apri)),
        errore_collegamento: RefCell::new(None),
        collegamento_aperto: Cell::new(false),
        compresse: RefCell::new(std::collections::HashSet::new()),
        mostrato: RefCell::new(None),
        sveglia: slint::Timer::default(),
        attese: RefCell::new(Attese::default()),
        modello_attese: Rc::new(VecModel::default()),
        sorveglianza: RefCell::new(SorveglianzaCasa::new(Instant::now())),
        sveglia_casa: slint::Timer::default(),
        appena_connesso: Cell::new(false),
        non_archiviate: RefCell::new(std::collections::HashSet::new()),
        segreti,
        modello_ambienti: Rc::new(VecModel::default()),
        errore_connessione: RefCell::new(None),
        errore_import,
        errore_config: RefCell::new(None),
        prova: Cell::new(0),
        finestra: RefCell::new(slint::Weak::default()),
        apri_impostazioni: RefCell::new(None),
        ottimismo: RefCell::new(Ottimismo::default()),
        sveglia_ottimismo: slint::Timer::default(),
        traccia_tocco: std::env::var_os("ITSYHOME_TRACCIA_TOCCO").is_some_and(|v| v == "1"),
    });
    PONTE.with(|p| *p.borrow_mut() = Some(ponte.clone()));
    // I pulsanti della pillola allargata agiscono sulla voce che sta chiamando; «Apri» (un
    // consenso col testo tagliato) apre il pannello su quella voce, come un clic sulla pillola.
    pillola.on_azione(|pulsante| {
        if pulsante == PULSANTE_APRI {
            if let Some(pl) = self::ponte().and_then(|p| p.pillola.upgrade()) {
                pl.invoke_cliccata();
            }
            return;
        }
        if let Some(p) = self::ponte() {
            let voce = p.mostrato.borrow().as_ref().and_then(|v| v.voce_allargata().map(str::to_owned));
            if let Some(voce) = voce {
                p.azione(&voce, &pulsante);
            }
        }
    });
    ponte.aggiorna_casa_offline();
    ponte.aggiorna_ambienti();
    // Un file scritto a mano (id vuoti o doppi, nomi con caratteri invisibili) si ripara qui;
    // se non c'è nulla da riparare non si salva nulla.
    if ponte.casa.con_config(|c| {
        let mut prova = c.clone();
        collegamenti::ripara(&mut prova)
    }) {
        ponte.casa.aggiorna_config(|c| {
            collegamenti::ripara(c);
        });
    }
    ponte.aggiorna_collegamenti();
    ponte.aggiorna_umore();
    ponte
}

/// L'ora per i dettagli tecnici («14:02 · …»).
fn adesso_hh_mm() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

/// Il dettaglio tecnico di uno stato di connessione andato male (`None` se è andato bene).
fn dettaglio_stato(s: &StatoConnessione) -> Option<String> {
    let testo = match s {
        StatoConnessione::Errore(e) => e.clone(),
        StatoConnessione::TokenNonValido => "token rifiutato da Home Assistant (auth_invalid)".into(),
        StatoConnessione::TokenMancante => "nessun token nell'archivio sicuro del sistema per l'ambiente attivo".into(),
        _ => return None,
    };
    Some(format!("{} · {testo}", adesso_hh_mm()))
}

fn voce_ui(p: &Preferito, connesso: bool) -> PreferitoUi {
    PreferitoUi {
        entity_id: p.entity_id.as_str().into(),
        nome: p.nome.as_str().into(),
        genere: genere(p),
        acceso: p.acceso && p.disponibile,
        stato: testo_stato(p).into(),
        disponibile: p.disponibile,
        errore: p.errore.as_deref().unwrap_or_default().into(),
        toccabile: voce_toccabile(p, connesso),
        tenue: !p.disponibile || !connesso,
    }
}

/// Riscrive solo le righe cambiate (una riga uguale non ridisegna nulla). Restituisce quante
/// righe ha riscritto (per il registro diagnostico).
fn aggiorna_modello<T: Clone + PartialEq + 'static>(modello: &VecModel<T>, voci: Vec<T>) -> u64 {
    if modello.row_count() != voci.len() {
        // Un modello svuotato conta come una riga riscritta (c'è stato un cambio).
        let n = voci.len().max(1) as u64;
        modello.set_vec(voci);
        return n;
    }
    let mut riscritte = 0;
    for (i, v) in voci.into_iter().enumerate() {
        if modello.row_data(i).as_ref() != Some(&v) {
            modello.set_row_data(i, v);
            riscritte += 1;
        }
    }
    riscritte
}

impl Ponte {
    pub fn casa(&self) -> &CasaService {
        &self.casa
    }

    fn ricevi(&self, evento: CasaEvento) {
        let ora = Instant::now();
        match evento {
            CasaEvento::Stato(s) => {
                self.casa.diagnosi().conta(Contatore::ui_stato);
                self.cervello.borrow_mut().ricevi(Segnale::Casa(s.clone()), ora);
                self.sorveglianza.borrow_mut().ricevi(&s, ora);
                self.appena_connesso.set(s == StatoConnessione::Connesso);
                if let Some(d) = dettaglio_stato(&s) {
                    *self.errore_connessione.borrow_mut() = Some(d);
                }
                // Senza connessione nessuna conferma può arrivare: i tocchi in attesa tornano veri.
                if s != StatoConnessione::Connesso && self.ottimismo.borrow_mut().svuota() {
                    self.sveglia_ottimismo.stop();
                }
                *self.stato.borrow_mut() = s;
                // Con la connessione cambia anche se i preferiti si possono toccare.
                self.ricostruisci_preferiti();
                self.aggiorna_ambienti();
                self.aggiorna_pannello();
                self.aggiorna_casa_offline();
            }
            CasaEvento::Dati(menu) => {
                self.casa.diagnosi().conta(Contatore::ui_dati);
                // Solo la prima consegna conta (giro di correzioni 1 del 14h); spento, nulla.
                if self.casa.diagnosi().acceso() {
                    self.casa.diagnosi().prima_consegna(crate::diagnosi::cpu_processo_ms());
                }
                self.appena_connesso.set(false);
                let vero = |id: &str| entita(&menu, id).as_ref().and_then(acceso_di);
                let confermati = self.ottimismo.borrow_mut().conferma(vero, ora);
                if self.traccia_tocco {
                    for (id, dopo) in &confermati {
                        eprintln!("tocco {id}: confermato da Home Assistant dopo {} ms", dopo.as_millis());
                    }
                }
                *self.menu.borrow_mut() = Some(menu);
                *self.dati_delle.borrow_mut() = Some(adesso_hh_mm());
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
            CasaEvento::Esito { entity_id, numero, ok, errore } => {
                // `numero` è quello del tocco che ha mandato il comando (0 = nessuna attesa).
                if ok {
                    self.errori.borrow_mut().remove(&entity_id);
                    self.cervello.borrow_mut().ricevi(Segnale::ComandoRiuscito, ora);
                } else {
                    let errore = errore.unwrap_or_else(|| "comando non riuscito".into());
                    eprintln!("comando su {entity_id} non riuscito: {errore}");
                    // L'icona torna com'è davvero, con l'errore; l'attesa di un tocco più recente
                    // (un secondo tocco già partito) resta.
                    self.ottimismo.borrow_mut().fallito(&entity_id, numero);
                    self.errori.borrow_mut().insert(entity_id, errore);
                    self.cervello.borrow_mut().ricevi(Segnale::ComandoFallito, ora);
                }
                self.ricostruisci_preferiti();
            }
            CasaEvento::ConfigNonSalvata(e) => {
                eprintln!("{e}");
                *self.errore_config.borrow_mut() = Some(format!("{} · {e}", adesso_hh_mm()));
                self.aggiorna_pannello();
            }
        }
        self.aggiorna_umore();
    }

    /// Ricalcola umore e aspetto della pillola e li porta alla pillola e, se aperto, al pannello.
    /// Se qualcosa scadrà da solo (contento, i 20 s di un'attenzione), una sveglia singola li
    /// ricalcola allora; altrimenti nessun timer.
    fn aggiorna_umore(&self) {
        self.casa.diagnosi().conta(Contatore::ui_umore);
        let ora = Instant::now();
        let (umore, scadenza_umore) = {
            let c = self.cervello.borrow();
            (self.umore_fisso.unwrap_or_else(|| c.umore(ora)), c.prossimo_cambio(ora))
        };
        match (umore == Umore::Pensa, self.pensa_da.get()) {
            (true, None) => self.pensa_da.set(Some(ora)),
            (false, Some(da)) => {
                self.pensa_da.set(None);
                self.casa.diagnosi().conta_n(Contatore::ui_pensa_ms, ora.duration_since(da).as_millis() as u64);
            }
            _ => {}
        }
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
                self.casa.diagnosi().conta(Contatore::ui_pillola);
                pillola::imposta(&p, &nuova);
            }
            if let Some(p) = self.pannello.borrow().upgrade() {
                p.set_umore(pillola::umore_ui(nuova.umore));
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

    /// L'utente ha chiesto una connessione (Salva, Connetti, Riprova): il tentativo che parte
    /// adesso fa «pensa» (quelli automatici no, vedi `umore::Cervello`).
    fn azione_utente(&self) {
        self.cervello.borrow_mut().ricevi(Segnale::AzioneUtente, Instant::now());
    }

    /// «Riprova» (voce della casa irraggiungibile, testata della vista Casa).
    pub fn riprova_casa(&self) {
        self.azione_utente();
        self.casa.connetti_attivo();
    }

    fn connesso(&self) -> bool {
        *self.stato.borrow() == StatoConnessione::Connesso
    }

    /// Preferiti, sensori dei Quadranti, stanze e dispositivi della stanza aperta dai dati più
    /// recenti, con lo stato ottimistico dei tocchi, gli errori ancora aperti e lo stato della
    /// connessione.
    fn ricostruisci_preferiti(&self) {
        if !self.modelli_vivi.get() {
            self.modelli_vecchi.set(true);
            return;
        }
        let connesso = self.connesso();
        let icone = self.casa.con_config(|c| c.area_icons.clone());
        let (voci, sensori, stanze, dispositivi) = {
            let menu = self.menu.borrow();
            let Some(menu) = menu.as_ref() else { return };
            let errori = self.errori.borrow();
            let ottimismo = self.ottimismo.borrow();
            let pronta = |p: Preferito| {
                let mut p = con_ottimismo(p, &ottimismo);
                p.errore = errori.get(&p.entity_id).cloned();
                voce_ui(&p, connesso)
            };
            let voci: Vec<PreferitoUi> = preferiti_ui(menu, MASSIMO_PREFERITI).into_iter().map(pronta).collect();
            let sensori: Vec<SensoreUi> = sensori_quadranti(menu, MASSIMO_PREFERITI, MASSIMO_SENSORI)
                .into_iter()
                .map(|(p, frazione)| SensoreUi { voce: voce_ui(&p, connesso), frazione })
                .collect();
            let stanze = stanze_ui(menu, &icone);
            let dispositivi: Vec<PreferitoUi> = match self.stanza_aperta.borrow().as_deref() {
                Some(area) => dispositivi_stanza(menu, area).iter().map(|e| pronta(preferito(e))).collect(),
                None => Vec::new(),
            };
            (voci, sensori, stanze, dispositivi)
        };
        let d = self.casa.diagnosi();
        d.conta(Contatore::ui_ricostruzioni);
        d.conta_n(Contatore::ui_righe, aggiorna_modello(&self.preferiti, voci));
        d.conta_n(Contatore::ui_righe, aggiorna_modello(&self.sensori, sensori));
        // La stanza aperta può aver cambiato posto (ordine) o essere sparita (nascosta).
        let aperta = self.stanza_aperta.borrow().clone();
        if let Some(area) = aperta {
            let indice = stanze.iter().position(|s| s.area_id == area.as_str());
            if indice.is_none() {
                *self.stanza_aperta.borrow_mut() = None;
            }
            if let Some(p) = self.pannello.borrow().upgrade() {
                let i = indice.map_or(-1, |i| i as i32);
                if p.get_stanza_aperta() != i {
                    p.set_stanza_aperta(i);
                }
            }
        }
        d.conta_n(Contatore::ui_righe, aggiorna_modello(&self.stanze, stanze));
        d.conta_n(Contatore::ui_righe, aggiorna_modello(&self.dispositivi, dispositivi));
    }

    /// Una stanza della vista Casa aperta (`Some(area)`) o chiusa.
    fn stanza(&self, area: Option<String>) {
        *self.stanza_aperta.borrow_mut() = area;
        self.ricostruisci_preferiti();
    }

    /// Riga di stato della casa sul pannello aperto e dettagli della finestra delle Impostazioni.
    fn aggiorna_pannello(&self) {
        if let Some(p) = self.pannello.borrow().upgrade() {
            self.riempi_stato(&p);
        }
        if let Some(f) = self.finestra.borrow().upgrade() {
            self.riempi_impostazioni(&f);
        }
    }

    /// Gli ambienti della configurazione con lo stato della connessione (solo l'attivo ne ha una).
    fn aggiorna_ambienti(&self) {
        let config = self.casa.config();
        let stato = self.stato.borrow();
        let voci: Vec<AmbienteUi> = config
            .environments
            .iter()
            .map(|e| {
                let attivo = config.active_env_id.as_deref() == Some(e.id.as_str());
                let (testo, tono) = impostazioni::stato_ambiente(attivo, &stato);
                AmbienteUi {
                    id: e.id.as_str().into(),
                    nome: e.name.as_str().into(),
                    url: e.ha_url.as_str().into(),
                    stato: testo.into(),
                    tono: tono_ui(tono),
                    attivo,
                    collegato: impostazioni::collegato(attivo, &stato),
                }
            })
            .collect();
        aggiorna_modello(&self.modello_ambienti, voci);
    }

    /// Ciò che le Impostazioni leggono dalla configurazione e dagli errori.
    fn riempi_impostazioni(&self, p: &FinestraImpostazioni) {
        let config = self.casa.config();
        if p.get_avvio_con_sistema() != config.launch_at_login {
            p.set_avvio_con_sistema(config.launch_at_login);
        }
        if p.get_icona_barra() != config.mostra_icona_barra {
            p.set_icona_barra(config.mostra_icona_barra);
        }
        p.set_config_non_salvata(self.errore_config.borrow().is_some());
        self.aggiorna_righe();
        let dettagli = impostazioni::dettagli_tecnici(
            self.errore_connessione.borrow().as_deref(),
            self.errore_import.as_deref(),
            self.errore_config.borrow().as_deref(),
            self.errore_collegamento.borrow().as_deref(),
        );
        if p.get_dettagli_tecnici() != dettagli.as_str() {
            p.set_dettagli_tecnici(dettagli.into());
        }
    }

    /// Le righe di Impostazioni › Dispositivi dal catalogo degli ultimi dati e dalla configurazione
    /// di adesso (una modifica appena fatta si vede subito, prima dei dati rimappati).
    fn aggiorna_righe(&self) {
        let config = self.casa.config();
        let righe: Vec<RigaDispositivoUi> = match self.menu.borrow().as_ref() {
            Some(m) if !m.catalogo.is_empty() => {
                dispositivi::righe(&m.catalogo, &config, &self.compresse.borrow()).iter().map(riga_ui).collect()
            }
            _ => Vec::new(),
        };
        aggiorna_modello(&self.righe, righe);
    }

    /// Un pulsante di Impostazioni › Dispositivi: la modifica va in configurazione (salvata dal
    /// servizio, che rimappa e consegna i dati nuovi al pannello); l'elenco si rifà subito e il
    /// fuoco resta sul pulsante premuto, anche se la riga si è spostata.
    fn azione_dispositivo(&self, azione: &str, id: &str, chiave: &str, area: &str) {
        let catalogo = match self.menu.borrow().as_ref() {
            Some(m) => m.catalogo.clone(),
            None => return,
        };
        let verso = match azione {
            "su" => -1,
            "giu" => 1,
            _ => 0,
        };
        let preferito = id.starts_with("p:");
        match azione {
            "comprimi" => {
                let mut compresse = self.compresse.borrow_mut();
                if !compresse.remove(chiave) {
                    compresse.insert(chiave.to_owned());
                }
            }
            "su" | "giu" if preferito => {
                self.casa.aggiorna_config(|c| {
                    dispositivi::sposta_preferito(c, &catalogo, chiave, verso);
                });
            }
            "su" | "giu" if id.starts_with("s:") => {
                self.casa.aggiorna_config(|c| {
                    dispositivi::sposta_stanza(c, &catalogo, chiave, verso);
                });
            }
            "su" | "giu" => {
                self.casa.aggiorna_config(|c| {
                    dispositivi::sposta_dispositivo(c, &catalogo, area, chiave, verso);
                });
            }
            "preferito" => self.casa.aggiorna_config(|c| dispositivi::commuta_preferito(c, chiave)),
            "nascondi" if id.starts_with("s:") => {
                self.casa.aggiorna_config(|c| dispositivi::commuta_stanza_nascosta(c, &catalogo, chiave))
            }
            "nascondi" => self.casa.aggiorna_config(|c| dispositivi::commuta_nascosto(c, chiave)),
            _ => return,
        }
        self.aggiorna_righe();
        self.ricostruisci_preferiti();
        // Il fuoco: sullo stesso pulsante della stessa riga; se la riga è arrivata in cima (o in
        // fondo) quel pulsante si spegne e il fuoco va sull'altra freccia. Togliendo la stella da
        // un preferito la sua riga sparisce: il fuoco va sulla stella dello stesso dispositivo
        // nella sua stanza.
        let mut riga = id.to_owned();
        let mut quale = azione.to_owned();
        if preferito && azione == "preferito" {
            riga = format!("d:{chiave}");
        }
        if let Some(r) = (0..self.righe.row_count()).filter_map(|i| self.righe.row_data(i)).find(|r| r.id == riga.as_str()) {
            if quale == "su" && !r.su && r.giu {
                quale = "giu".into();
            } else if quale == "giu" && !r.giu && r.su {
                quale = "su".into();
            }
        }
        if let Some(f) = self.finestra.borrow().upgrade() {
            f.set_fuoco_riga(riga.into());
            f.set_fuoco_azione(quale.into());
            f.set_richiesta_fuoco(f.get_richiesta_fuoco().wrapping_add(1));
        }
    }

    /// L'icona scelta per una stanza (indice in `stanze::ICONE`): in `areaIcons`, come la v2.
    fn scegli_icona(&self, area: &str, indice: usize) {
        let Some((emoji, _)) = crate::stanze::ICONE.get(indice) else { return };
        self.casa.aggiorna_config(|c| {
            dispositivi::imposta_icona(c, area, emoji);
        });
        // L'icona non cambia i dati della casa: elenco e pannello si rifanno da qui.
        self.aggiorna_righe();
        self.ricostruisci_preferiti();
    }

    // ─── Collegamenti veloci (Task 14f) ──────────────────────────────────────

    /// I collegamenti della configurazione per il pannello e le Impostazioni.
    fn aggiorna_collegamenti(&self) {
        let voci: Vec<CollegamentoUi> = self.casa.con_config(|c| {
            c.collegamenti
                .iter()
                .map(|c| CollegamentoUi {
                    id: c.id.as_str().into(),
                    nome: c.nome.as_str().into(),
                    url: c.url.as_str().into(),
                    corto: collegamenti::indirizzo_corto(&c.url).into(),
                })
                .collect()
        });
        aggiorna_modello(&self.collegamenti, voci);
    }

    /// Dopo una modifica il fuoco delle Impostazioni va su un pulsante di una riga (`id`, azione
    /// «su», «giu», «modifica», «elimina») o su «Aggiungi collegamento» (`id` vuoto, «aggiungi»).
    fn fuoco_collegamento(&self, id: &str, azione: &str) {
        if let Some(f) = self.finestra.borrow().upgrade() {
            f.set_fuoco_collegamento(id.into());
            f.set_fuoco_azione_collegamento(azione.into());
            f.set_richiesta_fuoco(f.get_richiesta_fuoco().wrapping_add(1));
        }
    }

    /// Il modulo di Impostazioni › Collegamenti (`id` vuoto = nuovo): salvato subito e visibile
    /// subito nel pannello. Restituisce l'errore da mostrare, vuoto se è andato bene.
    fn salva_collegamento(&self, id: &str, nome: &str, url: &str) -> String {
        let mut esito = Err(String::new());
        self.casa.aggiorna_config(|c| esito = collegamenti::salva(c, id, nome, url));
        match esito {
            Ok(id) => {
                self.aggiorna_collegamenti();
                self.fuoco_collegamento(&id, "modifica");
                String::new()
            }
            Err(e) => e,
        }
    }

    /// Elimina (la conferma l'ha già chiesta la finestra); il fuoco va sulla riga che prende il
    /// suo posto, o su «Aggiungi collegamento» se non ne resta nessuna.
    fn elimina_collegamento(&self, id: &str) {
        let indice = self.casa.con_config(|c| c.collegamenti.iter().position(|x| x.id == id));
        self.casa.aggiorna_config(|c| {
            collegamenti::elimina(c, id);
        });
        self.aggiorna_collegamenti();
        let n = self.collegamenti.row_count();
        let resta = indice.filter(|_| n > 0).and_then(|i| self.collegamenti.row_data(i.min(n - 1)));
        match resta {
            Some(r) => self.fuoco_collegamento(&r.id, "modifica"),
            None => self.fuoco_collegamento("", "aggiungi"),
        }
    }

    /// Su (-1) o giù (+1) di un posto; il fuoco resta sulla stessa freccia della riga spostata o,
    /// arrivata al bordo, passa all'altra.
    fn sposta_collegamento(&self, id: &str, verso: i32) {
        self.casa.aggiorna_config(|c| {
            collegamenti::sposta(c, id, verso);
        });
        self.aggiorna_collegamenti();
        let n = self.collegamenti.row_count();
        let Some(i) = (0..n).find(|i| self.collegamenti.row_data(*i).is_some_and(|r| r.id == id)) else { return };
        let azione = match verso {
            v if v < 0 && i == 0 => "giu",
            v if v < 0 => "su",
            _ if i + 1 == n => "su",
            _ => "giu",
        };
        self.fuoco_collegamento(id, azione);
    }

    /// Sostituisce chi apre i collegamenti (i test: nessun browser vero).
    #[cfg(test)]
    pub fn su_apri_url(&self, f: impl Fn(&str) -> Result<(), String> + 'static) {
        *self.apri_url.borrow_mut() = Rc::new(f);
    }

    /// Un collegamento scelto nel pannello: l'indirizzo salvato, ricontrollato, va al browser
    /// predefinito. Vero se il browser è partito (allora il pannello si chiude); se no il pannello
    /// resta aperto con una riga che lo dice e il dettaglio va in Impostazioni › Informazioni.
    pub fn apri_collegamento(&self, id: &str) -> bool {
        if self.collegamento_aperto.get() {
            return false;
        }
        let trovato = self.casa.con_config(|c| {
            let nome = c.collegamenti.iter().find(|x| x.id == id).map(|x| x.nome.clone());
            nome.zip(collegamenti::da_aprire(c, id))
        });
        let Some((nome, indirizzo)) = trovato else { return false };
        let esito = indirizzo.and_then(|url| {
            let apri = self.apri_url.borrow().clone();
            apri(&url)
        });
        let errore = match esito {
            Ok(()) => {
                self.collegamento_aperto.set(true);
                self.scrivi_errore_collegamento("");
                return true;
            }
            Err(e) => e,
        };
        eprintln!("collegamento «{nome}» non aperto: {errore}");
        *self.errore_collegamento.borrow_mut() = Some(format!("{} · «{nome}» non aperto: {errore}", adesso_hh_mm()));
        self.scrivi_errore_collegamento(&format!("«{nome}» non si è aperto: il dettaglio è in Impostazioni › Informazioni"));
        if let Some(f) = self.finestra.borrow().upgrade() {
            self.riempi_impostazioni(&f);
        }
        false
    }

    fn scrivi_errore_collegamento(&self, testo: &str) {
        if let Some(p) = self.pannello.borrow().upgrade() {
            if p.get_errore_collegamenti() != testo {
                p.set_errore_collegamenti(testo.into());
            }
        }
    }

    /// `Apertura` dice come aprire la finestra delle Impostazioni (chiudendo il pannello).
    pub fn su_apri_impostazioni(&self, f: impl Fn() + 'static) {
        *self.apri_impostazioni.borrow_mut() = Some(Box::new(f));
    }

    /// Icona del dock o pulsante di una voce: la finestra delle Impostazioni. Falso se nessuno sa
    /// aprirla. Vero vuol dire solo «chiesta»: se poi la finestra non nasce, la sezione in sospeso
    /// la dimentica `finestra_impostazioni::apri_impostazioni`.
    pub fn chiedi_impostazioni(&self) -> bool {
        if let Some(apri) = self.apri_impostazioni.borrow().as_ref() {
            apri();
            return true;
        }
        false
    }

    /// La finestra delle Impostazioni appena creata: modelli, dati di adesso e azioni.
    pub fn collega_finestra(&self, f: &FinestraImpostazioni) {
        *self.finestra.borrow_mut() = f.as_weak();
        // L'avvio con il sistema può essere stato spento da Gestione attività mentre l'app
        // girava: l'interruttore mostra lo stato vero del registro.
        match autoavvio::leggi() {
            Ok(stato) if self.casa.config().launch_at_login != stato.attivo() => {
                self.casa.aggiorna_config(|c| c.launch_at_login = stato.attivo())
            }
            Ok(_) => {}
            Err(e) => eprintln!("{e}"),
        }
        self.aggiorna_ambienti();
        f.set_ambienti(ModelRc::from(self.modello_ambienti.clone()));
        f.set_versione(env!("CARGO_PKG_VERSION").into());
        f.set_icona_barra_disponibile(cfg!(windows));
        f.set_righe_dispositivi(ModelRc::from(self.righe.clone()));
        f.set_collegamenti(ModelRc::from(self.collegamenti.clone()));
        f.on_controlla_nome_collegamento(|nome| collegamenti::valida_nome(&nome).err().unwrap_or_default().into());
        f.on_controlla_indirizzo_collegamento(|url| collegamenti::valida_indirizzo(&url).err().unwrap_or_default().into());
        f.on_salva_collegamento(|id, nome, url| {
            ponte().map(|p| p.salva_collegamento(&id, &nome, &url)).unwrap_or_default().into()
        });
        f.on_elimina_collegamento(|id| {
            if let Some(p) = ponte() {
                p.elimina_collegamento(&id);
            }
        });
        f.on_sposta_collegamento(|id, verso| {
            if let Some(p) = ponte() {
                p.sposta_collegamento(&id, verso);
            }
        });
        let nomi: Vec<slint::SharedString> = crate::stanze::ICONE.iter().map(|(_, n)| (*n).into()).collect();
        f.set_nomi_icone(ModelRc::new(VecModel::from(nomi)));
        f.on_azione_dispositivo(|azione, id, chiave, area| {
            if let Some(p) = ponte() {
                p.azione_dispositivo(&azione, &id, &chiave, &area);
            }
        });
        f.on_scegli_icona(|area, indice| {
            if let Some(p) = ponte() {
                p.scegli_icona(&area, indice.max(0) as usize);
            }
        });
        self.riempi_impostazioni(f);
        f.on_salva_ambiente(|id, nome, url, token| {
            let Some(ponte) = ponte() else { return "".into() };
            let m = Modulo { id: id.into(), nome: nome.into(), url: url.into(), token: token.into() };
            ponte.salva_ambiente(m).into()
        });
        f.on_rimuovi_ambiente(|id| ponte().map(|p| p.rimuovi_ambiente(&id)).unwrap_or_default().into());
        f.on_prova_connessione(|id, url, token| {
            if let Some(p) = ponte() {
                p.prova_connessione(&id, &url, &token);
            }
        });
        f.on_annulla_prova(|| {
            if let Some(p) = ponte() {
                p.prova.set(p.prova.get() + 1);
            }
        });
        f.on_connetti_ambiente(|id| {
            if let Some(p) = ponte() {
                p.azione_utente();
                p.casa.connetti(&id);
            }
        });
        f.on_disconnetti_casa(|| {
            if let Some(p) = ponte() {
                p.casa.disconnetti();
            }
        });
        // L'icona nella barra: salvata e accesa o spenta subito, senza riavvio.
        f.on_cambia_icona_barra(|si| {
            let Some(p) = ponte() else { return };
            p.casa.aggiorna_config(|c| c.mostra_icona_barra = si);
            servizi::mostra_icona(si);
        });
        f.on_cambia_avvio(|attivo| {
            let Some(p) = ponte() else { return };
            // Prima il registro, poi la configurazione: se il sistema rifiuta, l'interruttore
            // torna com'era e la configurazione non mente.
            match autoavvio::imposta(attivo) {
                Ok(()) => p.casa.aggiorna_config(|c| c.launch_at_login = attivo),
                Err(e) => {
                    eprintln!("{e}");
                    *p.errore_config.borrow_mut() = Some(format!("{} · {e}", adesso_hh_mm()));
                    if let Some(f) = p.finestra.borrow().upgrade() {
                        f.set_avvio_con_sistema(!attivo);
                        p.riempi_impostazioni(&f);
                    }
                }
            }
        });
    }

    /// La finestra delle Impostazioni si è chiusa: nessuna prova in volo vale più.
    pub fn finestra_chiusa(&self) {
        *self.finestra.borrow_mut() = slint::Weak::default();
        self.prova.set(self.prova.get() + 1);
    }

    // ─── Impostazioni: ambienti di Home Assistant ────────────────────────────

    /// Salva il modulo (nuovo ambiente o modifica). Il token, se scritto, va solo nel
    /// portachiavi; se il portachiavi non lo accetta non si salva nulla (mai token in chiaro).
    /// Restituisce l'errore da mostrare sotto il modulo, vuoto se è andato tutto bene.
    fn salva_ambiente(&self, m: Modulo) -> String {
        let config = self.casa.config();
        let a = match impostazioni::prepara(&config, &m) {
            Ok(a) => a,
            Err(e) => return e,
        };
        if let Some(t) = &a.token {
            if let Err(e) = self.segreti.salva(&a.id, t) {
                *self.errore_connessione.borrow_mut() = Some(format!("{} · salvataggio del token: {e}", adesso_hh_mm()));
                self.aggiorna_pannello();
                return "Archivio sicuro del sistema non disponibile: il token non è stato salvato".into();
            }
        }
        let mut url_cambiato = false;
        self.casa.aggiorna_config(|c| url_cambiato = impostazioni::applica(c, &a));
        // Il primo ambiente si connette da solo; quello attivo si riconnette se cambiano URL o token.
        let attivo = config.active_env_id.as_deref();
        if attivo.is_none() || (attivo == Some(a.id.as_str()) && (url_cambiato || a.token.is_some())) {
            self.azione_utente();
            self.casa.connetti(&a.id);
        }
        self.aggiorna_ambienti();
        String::new()
    }

    /// Toglie l'ambiente e il suo token dal portachiavi; se era l'attivo la casa resta senza
    /// ambiente (nessuna voce «non raggiungibile»). Restituisce l'errore, vuoto se tutto bene.
    fn rimuovi_ambiente(&self, id: &str) -> String {
        let mut era_attivo = false;
        self.casa.aggiorna_config(|c| era_attivo = impostazioni::togli(c, id));
        if era_attivo {
            self.casa.connetti_attivo();
        }
        self.aggiorna_ambienti();
        match self.segreti.cancella(id) {
            Ok(()) => String::new(),
            Err(e) => {
                *self.errore_connessione.borrow_mut() = Some(format!("{} · cancellazione del token: {e}", adesso_hh_mm()));
                self.aggiorna_pannello();
                "Ambiente rimosso, ma il token è rimasto nell'archivio sicuro del sistema".into()
            }
        }
    }

    /// «Prova connessione» col modulo: in modifica, senza un token nuovo, si prova quello salvato.
    fn prova_connessione(&self, id: &str, url: &str, token: &str) {
        let numero = self.prova.get() + 1;
        self.prova.set(numero);
        let url = match impostazioni::valida_url(url) {
            Ok(u) => u,
            Err(e) => return self.esito_prova(numero, e, Tono::Errore),
        };
        let token = match token.trim() {
            "" if id.is_empty() => return self.esito_prova(numero, "Scrivi il token per provarlo".into(), Tono::Errore),
            "" => match self.segreti.leggi(id) {
                Ok(Some(t)) if !t.is_empty() => t,
                Ok(_) => return self.esito_prova(numero, "Nessun token salvato: scrivilo".into(), Tono::Errore),
                Err(_) => return self.esito_prova(numero, "Token salvato non leggibile".into(), Tono::Errore),
            },
            t => t.to_owned(),
        };
        self.esito_prova(numero, "Prova in corso…".into(), Tono::Attesa);
        self.casa.prova_connessione(url, token, move |r: TestConnectionResult| {
            let _ = slint::invoke_from_event_loop(move || {
                let Some(p) = ponte() else { return };
                if !r.ok {
                    let e = r.error.as_deref().unwrap_or("errore sconosciuto");
                    *p.errore_connessione.borrow_mut() = Some(format!("{} · prova connessione: {e}", adesso_hh_mm()));
                    p.aggiorna_pannello();
                }
                let (testo, tono) = impostazioni::esito_prova(&r);
                p.esito_prova(numero, testo, tono);
            });
        });
    }

    fn esito_prova(&self, numero: u64, testo: String, tono: Tono) {
        if numero != self.prova.get() {
            return;
        }
        if let Some(f) = self.finestra.borrow().upgrade() {
            f.set_esito_prova(testo.into());
            f.set_tono_prova(tono_ui(tono));
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
        let dati = self.dati_delle.borrow().clone().unwrap_or_default();
        if p.get_dati_delle() != dati.as_str() {
            p.set_dati_delle(dati.into());
        }
        p.set_casa_riprovabile(riprovabile(&stato));
    }

    /// I modelli della vista Casa tornano a seguire i dati (pannello aperto): se da chiuso è
    /// cambiato qualcosa si rifanno ora, una volta.
    pub fn apri_modelli(&self) {
        self.modelli_vivi.set(true);
        if self.modelli_vecchi.replace(false) {
            self.ricostruisci_preferiti();
        }
    }

    /// Il pannello appena creato: modelli dei preferiti, dei sensori e delle voci in attesa,
    /// stato della casa, tocchi e pulsanti.
    pub fn riempi(&self, p: &Pannello) {
        self.apri_modelli();
        p.set_preferiti(ModelRc::from(self.preferiti.clone()));
        p.set_sensori(ModelRc::from(self.sensori.clone()));
        p.set_stanze(ModelRc::from(self.stanze.clone()));
        p.set_dispositivi(ModelRc::from(self.dispositivi.clone()));
        p.set_collegamenti(ModelRc::from(self.collegamenti.clone()));
        p.on_apri_stanza(|area| {
            if let Some(ponte) = ponte() {
                ponte.stanza(Some(area.to_string()));
            }
        });
        p.on_chiudi_stanza(|| {
            if let Some(ponte) = ponte() {
                ponte.stanza(None);
            }
        });
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
        p.on_riprova_casa(|| {
            if let Some(ponte) = ponte() {
                ponte.riprova_casa();
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
        self.modelli_vivi.set(false);
        self.collegamento_aperto.set(false);
        // Il pannello si riapre sui preferiti.
        if self.stanza_aperta.borrow_mut().take().is_some() {
            self.dispositivi.set_vec(Vec::new());
        }
        self.ridisegna_pillola();
    }

    /// La pillola (di nuovo attiva) sta per ricomparire: riprende l'aspetto di adesso anche se
    /// non è cambiato da quando si era nascosta.
    pub fn ridisegna_pillola(&self) {
        *self.mostrato.borrow_mut() = None;
        self.aggiorna_umore();
    }

    /// Il livello più urgente in attesa (colore della linea della ritirata).
    pub fn livello_massimo(&self) -> Option<Livello> {
        livello_massimo(&self.attese.borrow())
    }

    /// Da quanto l'utente non tocca tastiera e mouse (controllo da 5 s dei servizi).
    pub fn inattivo(&self, durata: Duration) {
        self.cervello.borrow_mut().ricevi(Segnale::Inattivo(durata), Instant::now());
        self.aggiorna_umore();
    }

    /// Un tocco su un preferito: il comando se ce n'è uno (mai su entità non disponibili né con la
    /// casa non connessa: l'interfaccia già non lo permette, qui lo si garantisce).
    pub fn tocca(&self, entity_id: &str) {
        if !self.connesso() {
            return;
        }
        let ora = Instant::now();
        let (comando, acceso) = {
            let menu = self.menu.borrow();
            let Some(menu) = menu.as_ref() else { return };
            let e = entita(menu, entity_id);
            (e.as_ref().and_then(comando_per), e.as_ref().and_then(acceso_di))
        };
        let Some((dominio, servizio)) = comando else { return };
        self.errori.borrow_mut().remove(entity_id);
        // Stato ottimistico: l'icona cambia adesso, rispetto a come si vede adesso (anche se un
        // tocco precedente aspetta ancora la conferma).
        // Un tocco che riporta al valore vero (un doppio tocco con `toggle`) non aspetta nulla:
        // HA non ripubblica dati uguali.
        // Il numero del tocco viaggia col comando e torna con il suo esito.
        let mut numero = 0;
        if let Some(vero) = acceso {
            let mostrato = self.ottimismo.borrow().atteso(entity_id).copied().unwrap_or(vero);
            numero = self.ottimismo.borrow_mut().tocca(entity_id, !mostrato, &vero, ora);
            self.arma_sveglia_ottimismo();
        }
        self.ricostruisci_preferiti();
        if self.traccia_tocco && acceso.is_some() {
            eprintln!("tocco {entity_id}: icona cambiata nel modello dopo {} µs", ora.elapsed().as_micros());
        }
        self.casa.comando(entity_id, dominio, servizio, numero);
    }

    /// Una sveglia singola alla prossima scadenza di un tocco non confermato.
    fn arma_sveglia_ottimismo(&self) {
        match self.ottimismo.borrow().prossima_scadenza(Instant::now()) {
            Some(tra) => self.sveglia_ottimismo.start(slint::TimerMode::SingleShot, tra + MARGINE_SVEGLIA, || {
                if let Some(p) = ponte() {
                    p.ottimismo_scaduto(Instant::now());
                }
            }),
            None => self.sveglia_ottimismo.stop(),
        }
    }

    /// Tocchi che Home Assistant non ha confermato in tempo: tornano allo stato vero, con l'errore.
    fn ottimismo_scaduto(&self, ora: Instant) {
        let scadute = {
            let menu = self.menu.borrow();
            let vero = |id: &str| menu.as_ref().and_then(|m| entita(m, id)).as_ref().and_then(acceso_di);
            self.ottimismo.borrow_mut().scadute(ora, vero)
        };
        if self.traccia_tocco {
            for (id, dopo) in &scadute.confermate {
                eprintln!("tocco {id}: già com'era chiesto alla scadenza ({} ms)", dopo.as_millis());
            }
        }
        if !scadute.confermate.is_empty() {
            self.ricostruisci_preferiti();
        }
        if !scadute.fallite.is_empty() {
            for id in scadute.fallite {
                eprintln!("comando su {id} non confermato entro {} s", crate::ottimismo::ATTESA_CONFERMA.as_secs());
                self.errori.borrow_mut().insert(id, ERRORE_SENZA_CONFERMA.into());
            }
            self.cervello.borrow_mut().ricevi(Segnale::ComandoFallito, ora);
            self.ricostruisci_preferiti();
            self.aggiorna_umore();
        }
        self.arma_sveglia_ottimismo();
    }

    // ─── Voci «In attesa» ────────────────────────────────────────────────────

    /// Mette (o aggiorna) una voce. Una voce nuova che lo merita manda la notifica di sistema,
    /// salvo `avvisa` falso (notifiche di HA già presenti alla connessione) o pannello aperto (la
    /// voce compare subito nella riga «In attesa»). Un bloccante nuovo fa uscire dalla ritirata.
    pub fn metti_voce(&self, v: Voce, avvisa: bool) {
        let nuova = !self.attese.borrow().contiene(&v.id);
        let pannello_aperto = self.pannello.borrow().upgrade().is_some();
        if nuova && avvisa && attenzione::toast(&v, pannello_aperto) {
            notifica_sistema(&v);
        }
        let livello = v.livello;
        self.attese.borrow_mut().metti(v);
        self.attese_cambiate();
        if nuova {
            servizi::voce_arrivata(livello);
        }
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
        servizi::colora_linea();
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
            self.riprova_casa();
        } else if voce == ID_CASA_TOKEN && pulsante == PULSANTE_IMPOSTAZIONI {
            self.chiedi_impostazioni();
        } else if let Some(id) = voce.strip_prefix(PREFISSO_NOTIFICA) {
            // Sparisce quando HA la rimuove (arriva `NotificaRimossa`); se non riesce, la voce lo dice.
            if pulsante == PULSANTE_ARCHIVIA {
                if self.non_archiviate.borrow_mut().remove(voce) {
                    self.attese_cambiate();
                }
                self.casa.archivia_notifica(id);
            }
        } else if voce.starts_with(PREFISSO_SISTEMA) {
            self.togli_voce(voce);
        } else if self.attese.borrow().voce(voce).is_some_and(|v| v.fonte == Fonte::Prova) {
            eprintln!("avviso di prova: «{pulsante}»");
            self.togli_voce(voce);
        }
    }

    /// La voce della casa: non raggiungibile da oltre 5 minuti, o token da sistemare; connessione
    /// tornata, «Disconnetti» o nessun ambiente → via. La sveglia suona solo allo scadere dei 5
    /// minuti (nessun controllo periodico).
    fn aggiorna_casa_offline(&self) {
        let ora = Instant::now();
        let (voce, scadenza) = {
            let s = self.sorveglianza.borrow();
            (s.voce(ora), s.scadenza(ora))
        };
        let tenuta = voce.as_ref().map(|v| v.id.clone());
        for id in [ID_CASA_OFFLINE, ID_CASA_TOKEN] {
            if tenuta.as_deref() != Some(id) {
                self.togli_voce(id);
            }
        }
        if let Some(v) = voce {
            // Nuova: con notifica di sistema; cambiata (token mancante → non valido): aggiornata.
            let prima = self.attese.borrow().voce(&v.id).map(|x| x.titolo.clone());
            match prima {
                None => self.metti_voce(v, true),
                Some(t) if t != v.titolo => self.metti_voce(v, false),
                Some(_) => {}
            }
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
    pub fn prova_avviso(&self, livello: Livello, lungo: bool) {
        self.metti_voce(attenzione::voce_di_prova(livello, lungo, Instant::now()), true);
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
            catalogo: Default::default(),
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
        assert_eq!(p[0].valore, "21,5 °C");
        assert_eq!(p[1].valore, "7");
        // Altri tipi: lettura con lo stato, in italiano (Task 14e).
        assert_eq!(p[2].tipo, Tipo::Lettura);
        assert_eq!(p[2].valore, "Chiusa");
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

    /// Un ponte vero sul backend di prova di Slint, con un `CasaService` che non si connette (i
    /// comandi tornano «non connesso» a un ascoltatore muto), la casa data per connessa e una
    /// luce spenta fra i preferiti.
    /// Lascia il ponte del thread anche se il test fallisce: distrutto dal thread_local a fine
    /// thread, il `CasaService` resterebbe appeso nella sua `join`.
    struct Spegni;
    impl Drop for Spegni {
        fn drop(&mut self) {
            spegni();
        }
    }

    fn ponte_di_prova() -> (Spegni, Rc<Ponte>, crate::Pillola, tempfile::TempDir) {
        use casa_core::config::ConfigStore;
        use casa_core::segreti::MemoriaSegreti;
        crate::test_ui::piattaforma_di_prova();
        let cartella = tempfile::tempdir().unwrap();
        let segreti: Arc<dyn Segreti> = Arc::new(MemoriaSegreti::new());
        let store = Arc::new(ConfigStore::apri(cartella.path()));
        let casa = Rc::new(CasaService::avvia(store, segreti.clone(), Arc::new(|_| {})));
        let pillola = crate::Pillola::new().unwrap();
        let cervello = Rc::new(RefCell::new(Cervello::new(Instant::now())));
        let p = collega(casa, segreti, &pillola, cervello, None, None);
        // Come col pannello aperto: i modelli della Casa seguono i dati.
        p.apri_modelli();
        p.ricevi(CasaEvento::Stato(StatoConnessione::Connesso));
        p.ricevi(CasaEvento::Dati(Arc::new(menu(vec![luce("light.a", false)], vec![]))));
        (Spegni, p, pillola, cartella)
    }

    /// Task 14g: col registro acceso il ponte conta gli eventi ricevuti, le ricostruzioni e le
    /// righe dei modelli davvero riscritte (dati uguali: nessuna).
    #[test]
    fn la_diagnosi_conta_dati_ricostruzioni_e_righe_riscritte() {
        use casa_core::config::ConfigStore;
        use casa_core::diagnosi::Diagnosi;
        use casa_core::segreti::MemoriaSegreti;
        crate::test_ui::piattaforma_di_prova();
        let cartella = tempfile::tempdir().unwrap();
        let segreti: Arc<dyn Segreti> = Arc::new(MemoriaSegreti::new());
        let store = Arc::new(ConfigStore::apri(cartella.path()));
        let diagnosi = Arc::new(Diagnosi::new(true));
        let casa = Rc::new(CasaService::avvia_con_diagnosi(store, segreti.clone(), Arc::new(|_| {}), diagnosi.clone()));
        let pillola = crate::Pillola::new().unwrap();
        let cervello = Rc::new(RefCell::new(Cervello::new(Instant::now())));
        let p = collega(casa, segreti, &pillola, cervello, None, None);
        let _spegni = Spegni;
        p.apri_modelli();
        p.ricevi(CasaEvento::Stato(StatoConnessione::Connesso));
        let dati = |accesa| CasaEvento::Dati(Arc::new(menu(vec![luce("light.a", accesa)], vec![])));
        p.ricevi(dati(false));
        let _ = diagnosi.riga(&[]);
        let conta = |c| diagnosi.valore(c);
        p.ricevi(dati(false));
        assert_eq!((conta(Contatore::ui_dati), conta(Contatore::ui_ricostruzioni), conta(Contatore::ui_righe)), (1, 1, 0));
        p.ricevi(dati(true));
        assert_eq!((conta(Contatore::ui_dati), conta(Contatore::ui_ricostruzioni), conta(Contatore::ui_righe)), (2, 2, 1));
        assert!(conta(Contatore::ui_umore) >= 2);
    }

    /// Giro di correzioni 1 del 14h: il registro diagnostico separa l'avvio. La prima consegna dei
    /// dati scrive i suoi valori (una volta); il tempo passato con «pensa» (prima connessione) si
    /// conta quando finisce.
    #[test]
    fn la_diagnosi_conta_la_prima_consegna_e_pensa() {
        use casa_core::config::ConfigStore;
        use casa_core::diagnosi::Diagnosi;
        use casa_core::segreti::MemoriaSegreti;
        crate::test_ui::piattaforma_di_prova();
        let cartella = tempfile::tempdir().unwrap();
        let segreti: Arc<dyn Segreti> = Arc::new(MemoriaSegreti::new());
        let store = Arc::new(ConfigStore::apri(cartella.path()));
        let diagnosi = Arc::new(Diagnosi::new(true));
        let casa = Rc::new(CasaService::avvia_con_diagnosi(store, segreti.clone(), Arc::new(|_| {}), diagnosi.clone()));
        let pillola = crate::Pillola::new().unwrap();
        let cervello = Rc::new(RefCell::new(Cervello::new(Instant::now())));
        let p = collega(casa, segreti, &pillola, cervello, None, None);
        let _spegni = Spegni;
        let valore = |n: &str| -> u64 {
            let riga = diagnosi.riga(&[]);
            riga.split(' ').find_map(|x| x.strip_prefix(&format!("{n}="))).unwrap().parse().unwrap()
        };
        p.ricevi(CasaEvento::Stato(StatoConnessione::InConnessione));
        std::thread::sleep(Duration::from_millis(40));
        p.ricevi(CasaEvento::Stato(StatoConnessione::Connesso));
        assert!(diagnosi.valore(Contatore::ui_pensa_ms) >= 40, "pensa: {}", diagnosi.valore(Contatore::ui_pensa_ms));
        assert_eq!(valore("avvio_consegna_ms"), 0);
        p.ricevi(CasaEvento::Dati(Arc::new(menu(vec![luce("light.a", false)], vec![]))));
        let prima = valore("avvio_consegna_ms");
        assert!(prima > 0);
        p.ricevi(CasaEvento::Dati(Arc::new(menu(vec![luce("light.a", true)], vec![]))));
        assert_eq!(valore("avvio_consegna_ms"), prima, "solo la prima");
    }

    /// Task 14h: a pannello chiuso i dati della casa non ricostruiscono i modelli della vista Casa
    /// (nessuno li guarda: la pillola mostra solo umore e voci in attesa); all'apertura si rifanno
    /// una volta coi dati di adesso, e da lì seguono i dati finché il pannello resta aperto.
    #[test]
    fn a_pannello_chiuso_i_modelli_della_casa_aspettano_l_apertura() {
        use casa_core::config::ConfigStore;
        use casa_core::diagnosi::Diagnosi;
        use casa_core::segreti::MemoriaSegreti;
        crate::test_ui::piattaforma_di_prova();
        let cartella = tempfile::tempdir().unwrap();
        let segreti: Arc<dyn Segreti> = Arc::new(MemoriaSegreti::new());
        let store = Arc::new(ConfigStore::apri(cartella.path()));
        let diagnosi = Arc::new(Diagnosi::new(true));
        let casa = Rc::new(CasaService::avvia_con_diagnosi(store, segreti.clone(), Arc::new(|_| {}), diagnosi.clone()));
        let pillola = crate::Pillola::new().unwrap();
        let cervello = Rc::new(RefCell::new(Cervello::new(Instant::now())));
        let p = collega(casa, segreti, &pillola, cervello, None, None);
        let _spegni = Spegni;
        let conta = |c| diagnosi.valore(c);
        let dati = |accesa| CasaEvento::Dati(Arc::new(menu(vec![luce("light.a", accesa)], vec![])));
        p.ricevi(CasaEvento::Stato(StatoConnessione::Connesso));
        for i in 0..5 {
            p.ricevi(dati(i % 2 == 0));
        }
        assert_eq!((conta(Contatore::ui_dati), conta(Contatore::ui_ricostruzioni)), (5, 0), "pannello chiuso: nulla da ricostruire");
        assert_eq!(p.preferiti.row_count(), 0);
        // Apertura: i modelli si rifanno una volta, coi dati di adesso (l'ultimo: accesa).
        p.apri_modelli();
        assert_eq!(conta(Contatore::ui_ricostruzioni), 1);
        assert_eq!(p.preferiti.row_data(0).unwrap().stato, "Acceso");
        p.ricevi(dati(false));
        assert_eq!(conta(Contatore::ui_ricostruzioni), 2, "aperto: seguono i dati");
        assert_eq!(p.preferiti.row_data(0).unwrap().stato, "Spento");
        // Chiuso di nuovo: i dati non ricostruiscono più nulla; una riapertura senza dati nuovi
        // non rifà nulla.
        p.pannello_chiuso();
        p.apri_modelli();
        assert_eq!(conta(Contatore::ui_ricostruzioni), 2, "dati uguali: niente da rifare");
        p.pannello_chiuso();
        p.ricevi(dati(true));
        assert_eq!(conta(Contatore::ui_ricostruzioni), 2);
        p.apri_modelli();
        assert_eq!(conta(Contatore::ui_ricostruzioni), 3);
        assert_eq!(p.preferiti.row_data(0).unwrap().stato, "Acceso");
        // Il pannello vero, appena creato e riempito, ha i dati di adesso.
        p.pannello_chiuso();
        p.ricevi(dati(false));
        let pannello = crate::Pannello::new().unwrap();
        p.riempi(&pannello);
        assert_eq!(pannello.get_preferiti().row_data(0).unwrap().stato, "Spento", "il pannello nuovo ha i dati di adesso");
    }

    fn stato_mostrato(p: &Ponte) -> String {
        p.preferiti.row_data(0).unwrap().stato.to_string()
    }

    /// Giro di correzioni 1, rilievo 1: due tocchi rapidi con `toggle` riportano la luce com'era.
    /// HA non ripubblica nulla (il `MenuData` è uguale), quindi nessuna conferma arriva: alla
    /// scadenza non deve comparire un falso «non ha confermato».
    #[test]
    fn doppio_tocco_senza_falso_errore() {
        let (_spegni, p, _pillola, _c) = ponte_di_prova();
        let t = Instant::now();
        p.tocca("light.a");
        assert_eq!(stato_mostrato(&p), "Acceso", "il primo tocco si vede subito");
        p.tocca("light.a");
        assert_eq!(stato_mostrato(&p), "Spento");
        p.ottimismo_scaduto(t + Duration::from_secs(6));
        assert_eq!(stato_mostrato(&p), "Spento", "nessun errore: la luce è com'era chiesto");
        assert!(p.errori.borrow().is_empty());
    }

    /// Rilievo 8: il cablaggio della scadenza nel ponte. Senza conferma, a 5 s la card torna vera
    /// con l'errore; la conferma che arriva prima chiude l'attesa senza errore.
    #[test]
    fn tocco_senza_conferma_torna_vero_con_l_errore() {
        let (_spegni, p, _pillola, _c) = ponte_di_prova();
        let t = Instant::now();
        p.tocca("light.a");
        p.ottimismo_scaduto(t + Duration::from_secs(4));
        assert_eq!(stato_mostrato(&p), "Acceso", "prima dei 5 s resta l'attesa");
        p.ottimismo_scaduto(t + Duration::from_secs(6));
        assert_eq!(stato_mostrato(&p), "Non riuscito");
        assert_eq!(p.errori.borrow().get("light.a").map(String::as_str), Some(ERRORE_SENZA_CONFERMA));
        // Un tocco nuovo toglie l'errore; la conferma lo chiude.
        p.tocca("light.a");
        assert_eq!(stato_mostrato(&p), "Acceso");
        p.ricevi(CasaEvento::Dati(Arc::new(menu(vec![luce("light.a", true)], vec![]))));
        p.ottimismo_scaduto(Instant::now() + Duration::from_secs(60));
        assert_eq!(stato_mostrato(&p), "Acceso");
        assert!(p.errori.borrow().is_empty());
    }

    /// Il numero dell'attesa armata dal tocco più recente su `id`.
    fn numero_attesa(p: &Ponte, id: &str) -> u64 {
        p.ottimismo.borrow().numero(id).expect("un'attesa armata")
    }

    /// Rilievo 8: un esito negativo toglie l'attesa e mostra l'errore.
    #[test]
    fn esito_negativo_toglie_l_attesa() {
        let (_spegni, p, _pillola, _c) = ponte_di_prova();
        p.tocca("light.a");
        let numero = numero_attesa(&p, "light.a");
        p.ricevi(CasaEvento::Esito { entity_id: "light.a".into(), numero, ok: false, errore: Some("guasto".into()) });
        assert_eq!(stato_mostrato(&p), "Non riuscito");
        assert!(p.ottimismo.borrow().atteso("light.a").is_none());
    }

    /// N3 (ri-revisione del 14d): ogni comando gira in un suo task, quindi gli esiti di due
    /// comandi sulla stessa entità possono arrivare scambiati. L'esito vale per il suo tocco, non
    /// per il primo in coda: l'errore del secondo comando, arrivato per primo, toglie l'attesa del
    /// secondo tocco; quello del primo, arrivato dopo, non tocca nulla.
    #[test]
    fn l_esito_torna_al_suo_tocco_anche_fuori_ordine() {
        let (_spegni, p, _pillola, _c) = ponte_di_prova();
        // Primo tocco: si accende (attesa n1), e la conferma arriva con i dati.
        p.tocca("light.a");
        let primo = numero_attesa(&p, "light.a");
        p.ricevi(CasaEvento::Dati(Arc::new(menu(vec![luce("light.a", true)], vec![]))));
        // Secondo tocco: si spegne (attesa n2); il suo comando fallisce e l'esito arriva per primo.
        p.tocca("light.a");
        let secondo = numero_attesa(&p, "light.a");
        assert_ne!(primo, secondo);
        p.ricevi(CasaEvento::Esito { entity_id: "light.a".into(), numero: secondo, ok: false, errore: Some("guasto".into()) });
        assert!(p.ottimismo.borrow().atteso("light.a").is_none(), "l'errore del secondo comando toglie la sua attesa");
        assert_eq!(stato_mostrato(&p), "Non riuscito");
        // Un terzo tocco arma un'attesa nuova; l'esito tardivo del primo comando non la tocca.
        p.tocca("light.a");
        let terzo = numero_attesa(&p, "light.a");
        p.ricevi(CasaEvento::Esito { entity_id: "light.a".into(), numero: primo, ok: false, errore: Some("tardi".into()) });
        assert_eq!(p.ottimismo.borrow().numero("light.a"), Some(terzo), "l'attesa del terzo tocco resta");
    }

    /// Task 14e: le stanze della vista Casa nell'ordine del mapper, con l'icona scelta o quella
    /// predefinita, «Altro» con la casa e in fondo le scene.
    #[test]
    fn stanze_della_vista_casa() {
        use casa_core::config::Config;
        let mut m = menu(vec![], vec![vec![luce("light.a", true), luce("light.b", false)], vec![interruttore("switch.c", true)]]);
        m.rooms[0].name = "Cucina".into();
        m.rooms[1].name = "Sala giochi".into();
        m.rooms.push(Room { area_id: String::new(), name: "Altro".into(), entities: vec![luce("light.z", false)] });
        let AppEntity::Scene(s) = scena("scene.notte") else { unreachable!() };
        m.scenes = vec![s];
        let mut config = Config::default();
        config.area_icons.insert("a1".into(), "🎮".into());
        let stanze = stanze_ui(&m, &config.area_icons);
        let nomi: Vec<&str> = stanze.iter().map(|s| s.nome.as_str()).collect();
        assert_eq!(nomi, ["Cucina", "Sala giochi", "Altro", "Scene"]);
        assert_eq!(stanze[0].icona as usize, crate::stanze::indice("🍳").unwrap(), "predefinita dal nome");
        assert_eq!(stanze[1].icona as usize, crate::stanze::indice("🎮").unwrap(), "quella scelta");
        assert_eq!((stanze[2].icona, stanze[3].icona), (-1, -2));
        assert_eq!(stanze.iter().map(|s| s.quanti).collect::<Vec<_>>(), [2, 1, 1, 1]);
        assert_eq!(dispositivi_stanza(&m, "a0").len(), 2);
        assert_eq!(dispositivi_stanza(&m, GRUPPO_SCENE)[0].entity_id(), "scene.notte");
        assert!(dispositivi_stanza(&m, "sparita").is_empty());
    }

    /// Task 14e: nella stanza aperta i dispositivi rispondono al tocco come i preferiti (stesso
    /// comando e stesso stato ottimistico); una stanza nascosta si chiude da sola.
    #[test]
    fn i_dispositivi_della_stanza_aperta_rispondono_come_i_preferiti() {
        let (_spegni, p, _pillola, _c) = ponte_di_prova();
        p.ricevi(CasaEvento::Dati(Arc::new(menu(vec![luce("light.a", false)], vec![vec![interruttore("switch.s", false)]]))));
        p.stanza(Some("a0".into()));
        assert_eq!(p.dispositivi.row_data(0).unwrap().stato, "Spento");
        p.tocca("switch.s");
        assert_eq!(p.dispositivi.row_data(0).unwrap().stato, "Acceso", "stato ottimistico anche nella stanza");
        assert_eq!(p.ottimismo.borrow().atteso("switch.s"), Some(&true));
        // La stanza non c'è più (tutto nascosto): si chiude.
        p.ricevi(CasaEvento::Dati(Arc::new(menu(vec![luce("light.a", false)], vec![]))));
        assert!(p.stanza_aperta.borrow().is_none());
        assert_eq!(p.dispositivi.row_count(), 0);
    }

    /// Task 14e: un pulsante di Impostazioni › Dispositivi cambia la configurazione (quella che il
    /// servizio salva) e rifà subito l'elenco; l'icona cambia anche le stanze del pannello.
    #[test]
    fn la_scheda_dispositivi_cambia_la_configurazione_e_l_elenco_subito() {
        use casa_core::types::{StanzaCatalogo, VoceCatalogo};
        let (_spegni, p, _pillola, _c) = ponte_di_prova();
        let voce = |id: &str| VoceCatalogo { entity_id: id.into(), name: id.into(), kind: "light".into() };
        let mut m = menu(vec![], vec![vec![luce("light.a", false)], vec![luce("light.b", false)]]);
        m.catalogo = Arc::new(vec![
            StanzaCatalogo { area_id: "a0".into(), name: "Stanza 0".into(), voci: vec![voce("light.a")] },
            StanzaCatalogo { area_id: "a1".into(), name: "Stanza 1".into(), voci: vec![voce("light.b")] },
        ]);
        p.ricevi(CasaEvento::Dati(Arc::new(m)));
        p.aggiorna_righe();
        let ids = |p: &Ponte| (0..p.righe.row_count()).map(|i| p.righe.row_data(i).unwrap().id.to_string()).collect::<Vec<_>>();
        assert_eq!(ids(&p), ["h:", "s:a0", "d:light.a", "s:a1", "d:light.b"]);
        p.azione_dispositivo("su", "s:a1", "a1", "a1");
        assert_eq!(p.casa.config().room_order, ["a1", "a0"]);
        assert_eq!(ids(&p), ["h:", "s:a1", "d:light.b", "s:a0", "d:light.a"], "l'elenco si rifà subito");
        p.azione_dispositivo("preferito", "d:light.a", "light.a", "a0");
        assert_eq!(p.casa.config().favorites, ["light.a"]);
        assert_eq!(ids(&p)[1], "p:light.a");
        p.azione_dispositivo("nascondi", "d:light.b", "light.b", "a1");
        assert_eq!(p.casa.config().hidden_entities, ["light.b"]);
        p.azione_dispositivo("comprimi", "s:a1", "a1", "a1");
        assert!(!ids(&p).contains(&"d:light.b".to_string()));
        p.scegli_icona("a0", 36);
        assert_eq!(p.casa.config().area_icons["a0"], crate::stanze::ICONE[36].0);
        assert_eq!(p.stanze.row_data(0).unwrap().icona, 36, "le stanze del pannello prendono l'icona subito");
    }

    /// Task 14f: aggiunta, modifica, ordine ed eliminazione dei collegamenti cambiano la
    /// configurazione (salvata dal servizio) e l'elenco del pannello subito; aprirne uno chiama il
    /// browser (qui finto, nessun browser vero) con l'indirizzo ricontrollato.
    #[test]
    fn collegamenti_salvati_e_aperti_col_browser_finto() {
        let (_spegni, p, _pillola, cartella) = ponte_di_prova();
        let aperti = Rc::new(RefCell::new(Vec::<String>::new()));
        let a = aperti.clone();
        p.su_apri_url(move |url| {
            a.borrow_mut().push(url.to_owned());
            Ok(())
        });
        let elenco = |p: &Ponte| {
            (0..p.collegamenti.row_count())
                .map(|i| {
                    let c = p.collegamenti.row_data(i).unwrap();
                    format!("{} {} {}", c.id, c.nome, c.corto)
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(p.salva_collegamento("", "Proxmox", "esempio.lan:8006"), "");
        assert_eq!(p.salva_collegamento("", "NAS", "http://nas.esempio.lan/admin?x=1"), "");
        assert_eq!(p.salva_collegamento("", "Male", "javascript:alert(1)"), "Solo indirizzi http:// o https://");
        assert_eq!(elenco(&p), ["proxmox Proxmox esempio.lan:8006", "nas NAS nas.esempio.lan/admin"]);
        assert_eq!(p.casa.config().collegamenti[0].url, "https://esempio.lan:8006/");
        p.sposta_collegamento("nas", -1);
        assert_eq!(elenco(&p), ["nas NAS nas.esempio.lan/admin", "proxmox Proxmox esempio.lan:8006"]);
        assert_eq!(p.salva_collegamento("proxmox", "Proxmox VE", "https://esempio.lan:8006/"), "");
        assert_eq!(elenco(&p)[1], "proxmox Proxmox VE esempio.lan:8006");
        assert!(p.apri_collegamento("proxmox"));
        assert_eq!(*aperti.borrow(), ["https://esempio.lan:8006/"]);
        // Invio ripetuto o doppio clic prima che il pannello si chiuda: una sola scheda.
        assert!(!p.apri_collegamento("proxmox"));
        assert!(!p.apri_collegamento("nas"));
        assert_eq!(aperti.borrow().len(), 1, "un solo tocco apre: {:?}", aperti.borrow());
        p.pannello_chiuso();
        p.elimina_collegamento("nas");
        assert_eq!(elenco(&p), ["proxmox Proxmox VE esempio.lan:8006"]);
        // Il servizio salva sul suo thread: il file arriva poco dopo.
        let file = cartella.path().join("assistente.json");
        let fine = Instant::now() + Duration::from_secs(5);
        loop {
            let testo = std::fs::read_to_string(&file).unwrap_or_default();
            if testo.contains("Proxmox VE") && !testo.contains("nas.esempio.lan") {
                break;
            }
            assert!(Instant::now() < fine, "configurazione non salvata: {testo}");
            std::thread::sleep(Duration::from_millis(20));
        }
        // Un indirizzo non ammesso scritto a mano nel file non si apre.
        p.casa.aggiorna_config(|c| c.collegamenti[0].url = "file:///C:/Windows/win.ini".into());
        assert!(!p.apri_collegamento("proxmox"));
        assert!(!p.apri_collegamento("sparito"));
        assert_eq!(aperti.borrow().len(), 1, "nulla di nuovo aperto");
        // Il browser non parte: niente chiusura del pannello, l'errore nei dettagli tecnici.
        p.casa.aggiorna_config(|c| c.collegamenti[0].url = "https://esempio.lan:8006".into());
        p.su_apri_url(|_| Err("ShellExecuteW: 2".into()));
        assert!(!p.apri_collegamento("proxmox"));
        assert!(p.errore_collegamento.borrow().as_deref().is_some_and(|e| e.contains("ShellExecuteW: 2")));
    }

    #[test]
    fn stato_in_italiano_per_cio_che_si_legge_soltanto() {
        let b = |id: &str, stato: &str| BaseEntity { state: stato.into(), ..base(id, true) };
        let tapparella = AppEntity::Cover(CoverEntity {
            base: b("cover.t", "open"),
            cover_state: "open".into(),
            position: Some(40.0),
            tilt: None,
            supports_position: true,
            supports_tilt: false,
            device_class: None,
        });
        assert_eq!(stato_in_lettura(&tapparella), "Aperta · 40%");
        let serratura = serratura("lock.porta");
        assert_eq!(stato_in_lettura(&serratura), "Chiusa");
        let termostato = AppEntity::Climate(ClimateEntity {
            base: b("climate.s", "heat"),
            hvac_mode: "heat".into(),
            hvac_action: None,
            hvac_modes: vec![],
            current_temp: Some(20.5),
            target_temp: None,
            target_temp_high: None,
            target_temp_low: None,
            min_temp: 7.0,
            max_temp: 35.0,
            temp_step: 0.5,
        });
        assert_eq!(stato_in_lettura(&termostato), "Riscaldamento · 20,5°");
        assert_eq!(preferiti_ui(&menu(vec![serratura], vec![]), 1)[0].valore, "Chiusa");
    }

    #[test]
    fn un_input_boolean_si_commuta_col_suo_servizio() {
        assert_eq!(comando_per(&interruttore("input_boolean.ospiti", true)), Some(("input_boolean", "toggle")));
        assert_eq!(comando_per(&interruttore("switch.presa", true)), Some(("switch", "toggle")));
    }

    #[test]
    fn lo_stato_ottimistico_cambia_icona_e_testo() {
        let mut o = Ottimismo::default();
        let spenta = p("light.a", Tipo::Interruttore, false, "off");
        assert_eq!(con_ottimismo(spenta.clone(), &o), spenta, "senza attesa resta vera");
        o.tocca("light.a", true, &false, Instant::now());
        let mostrata = con_ottimismo(spenta, &o);
        assert!(mostrata.acceso);
        assert_eq!(testo_stato(&mostrata), "Acceso");
        assert!(voce_ui(&mostrata, true).acceso);
        let altra = p("switch.b", Tipo::Interruttore, false, "off");
        assert!(!con_ottimismo(altra, &o).acceso, "solo il dispositivo toccato");
    }

    #[test]
    fn acceso_solo_per_luci_e_interruttori() {
        assert_eq!(acceso_di(&luce("light.a", true)), Some(true));
        assert_eq!(acceso_di(&interruttore("switch.b", false)), Some(false));
        assert_eq!(acceso_di(&scena("scene.c")), None);
        assert_eq!(acceso_di(&sensore("sensor.t", "21", Some("°C"))), None);
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
        assert_eq!(genere(&p("light.a", Tipo::Interruttore, true, "on")), GenereUi::Luce);
        assert_eq!(genere(&p("switch.a", Tipo::Interruttore, true, "on")), GenereUi::Presa);
        assert_eq!(genere(&p("scene.a", Tipo::Scena, false, "")), GenereUi::Scena);
        assert_eq!(genere(&p("sensor.t", Tipo::Lettura, false, "21.5 °C")), GenereUi::Termo);
        assert_eq!(genere(&p("sensor.u", Tipo::Lettura, false, "48 %")), GenereUi::Goccia);
        assert_eq!(genere(&p("sensor.x", Tipo::Lettura, false, "7")), GenereUi::Sensore);
        assert_eq!(genere(&p("lock.porta", Tipo::Lettura, false, "locked")), GenereUi::Casa);
    }

    #[test]
    fn riga_di_stato_della_casa() {
        let m = menu(
            vec![sensore("sensor.t", "21.5", Some("°C"))],
            vec![vec![luce("light.a", true), luce("light.b", true), luce("light.c", false)], vec![sensore("sensor.t", "21.5", Some("°C"))]],
        );
        assert_eq!(riga_stato(&StatoConnessione::Connesso, Some(&m)), "2 luci accese · 21,5 °C");
        let una = menu(vec![], vec![vec![luce("light.a", true)]]);
        assert_eq!(riga_stato(&StatoConnessione::Connesso, Some(&una)), "1 luce accesa");
        let spente = menu(vec![], vec![vec![luce("light.a", false)]]);
        assert_eq!(riga_stato(&StatoConnessione::Connesso, Some(&spente)), "Luci spente");
        assert_eq!(riga_stato(&StatoConnessione::Connesso, None), "Casa connessa");
        assert_eq!(riga_stato(&StatoConnessione::InConnessione, Some(&m)), "Connessione alla casa…");
        assert_eq!(riga_stato(&StatoConnessione::Disconnesso, Some(&m)), "Casa non connessa");
        assert_eq!(riga_stato(&StatoConnessione::TokenMancante, None), "Token di Home Assistant mancante");
        assert_eq!(riga_stato(&StatoConnessione::TokenNonValido, None), "Token di Home Assistant non valido");
        assert_eq!(riga_stato(&StatoConnessione::NonConfigurato, Some(&m)), "Casa non configurata");
    }

    #[test]
    fn errori_in_frasi_semplici() {
        let riga = |e: &str| riga_stato(&StatoConnessione::Errore(e.into()), None);
        assert_eq!(riga("connessione non riuscita: IO error: Impossibile stabilire la connessione"), "Casa non raggiungibile");
        assert_eq!(riga("Home Assistant non risponde"), "Casa non raggiungibile");
        assert_eq!(riga("connessione chiusa da Home Assistant"), "Casa non raggiungibile");
        assert_eq!(riga("portachiavi: accesso negato"), "Token salvato non leggibile");
    }

    #[test]
    fn riprova_solo_se_la_casa_non_risponde() {
        assert!(riprovabile(&StatoConnessione::Errore("giù".into())));
        assert!(riprovabile(&StatoConnessione::InConnessione), "niente pulsante che lampeggia fra un tentativo e l'altro");
        assert!(!riprovabile(&StatoConnessione::Connesso));
        assert!(!riprovabile(&StatoConnessione::Disconnesso));
        assert!(!riprovabile(&StatoConnessione::NonConfigurato));
        assert!(!riprovabile(&StatoConnessione::TokenNonValido));
    }

    #[test]
    fn numeri_di_home_assistant_con_la_virgola() {
        assert_eq!(numero_italiano("21.5"), "21,5");
        assert_eq!(numero_italiano("-3.25"), "-3,25");
        assert_eq!(numero_italiano("1013"), "1013");
        // Non numeri: restano come sono.
        assert_eq!(numero_italiano("on"), "on");
        assert_eq!(numero_italiano("2026-11-13"), "2026-11-13");
        assert_eq!(numero_italiano("1.2.3"), "1.2.3");
        assert_eq!(numero_italiano("192.0.2.1"), "192.0.2.1");
        assert_eq!(numero_italiano(".5"), ".5");
        assert_eq!(numero_italiano(""), "");
        // L'arco usa il valore grezzo: la virgola nel testo non lo cambia.
        let m = menu(vec![sensore("sensor.t", "20.0", Some("°C"))], vec![]);
        assert!((sensori_quadranti(&m, 8, 3)[0].1 - 0.5).abs() < 1e-6);
        assert_eq!(sensori_quadranti(&m, 8, 3)[0].0.valore, "20,0 °C");
    }

    #[test]
    fn sotto_il_titolo_la_parola_del_livello() {
        let v = attenzione::voce_di_prova(Livello::Attenzione, false, Instant::now());
        assert_eq!(voce_attesa_ui(&v, false).sotto, "Guarda presto · prova");
        assert_eq!(voce_attesa_ui(&v, true).sotto, "Guarda presto · prova · archiviazione non riuscita");
        // Il nome interno resta per il colore.
        assert_eq!(voce_attesa_ui(&v, false).livello, crate::LivelloUi::Attenzione);
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
