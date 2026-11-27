//! Ponte casa → interfaccia: dai dati di `casa-core` ai preferiti del pannello, e dai tocchi
//! sui preferiti ai comandi verso Home Assistant. Tiene anche le voci «In attesa» (notifiche di
//! Home Assistant, casa irraggiungibile, batteria, prova) e decide umore e aspetto della pillola.

use crate::attenzione::{
    self, avanzamento_mostrato, livello_massimo, notifica_sistema, prossimo_cambio_aspetto, AspettoPillola, Attese, Avanzamento, Fonte, Livello, SorveglianzaCasa,
    SorveglianzaBatteria, Voce, ID_BATTERIA, ID_CASA_OFFLINE, ID_CASA_TOKEN, PREFISSO_BT, PREFISSO_NOTIFICA, PREFISSO_SISTEMA,
    PULSANTE_ARCHIVIA, PULSANTE_APRI, PULSANTE_HO_CAPITO, PULSANTE_IMPOSTAZIONI, PULSANTE_RIPROVA,
};
use crate::claude::PREFISSO_CLAUDE;
use crate::lati::{self, Lato};
use crate::moduli::panoramica::ModoPanoramica;
use crate::orologio;
use crate::pc_stats::LetturaBatteria;
use crate::sorgenti_lati;
use crate::sistema::{autoavvio, servizi, volume};
#[cfg(windows)]
use crate::{connessioni, sistema::{batterie_bt, radio}};
#[cfg(windows)]
use crate::sistema::volume::{StatoVolume, Volume};
use crate::impostazioni::{self, Modulo, Tono};
use crate::ottimismo::Ottimismo;
use crate::pillola::{self, VistaPillola};
use crate::umore::{Cervello, Segnale, Umore};
use crate::dispositivi::{self, Riga, TipoRiga};
use crate::collegamenti;
use crate::moduli::registro::Registro;
use crate::{
    AmbienteUi, CardUi, CollegamentoUi, FiltroUi, FinestraImpostazioni, GenereUi, ModuloUi, MisuraUi, Pannello, PermessoUi, Pillola,
    RigaDispositivoUi, RigaMisuraUi, RigaModuloUi, SchedaBarraUi, TipoRigaUi, TonoUi, VoceAttesaUi,
};
use casa_core::config::Config;
use casa_core::diagnosi::Contatore;
use claude_hook::sessioni::Sessioni;
use casa_core::eventi::{Ascoltatore, CasaEvento, StatoConnessione};
use casa_core::segreti::Segreti;
use casa_core::servizio::CasaService;
use casa_core::types::{AppEntity, BaseEntity, MenuData, Meteo, TestConnectionResult};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(crate) mod con_claude;
mod con_moduli;
mod con_pagine;
use con_pagine::CampionePc;

/// Dove si apre il pannello da un clic sulla pillola (ridisegno F1, Task 7: la destinazione di ciò
/// che la pillola mostra).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DallaPillola {
    /// Su questa vista: la pagina di un modulo o la Coda (`coda`).
    Vista(&'static str),
    /// Sulla pagina Agenti, con la riga della sessione di questo permesso (l'id della voce)
    /// scelta e il permesso nel dettaglio: testo intero e sessione si leggono lì prima di
    /// concedere (ridisegno F2, Task 8, decisione di Paolo 5).
    Permesso(String),
    /// Ripiego: un permesso senza riga nella pagina Agenti (la sessione non c'è più, Claude
    /// scollegato) si apre sulla Coda, già aperto alla sua posizione (testo intero; dec. 11).
    PermessoInCoda(i32),
    /// Sulla Panoramica.
    Panoramica,
}

/// La sveglia dell'umore suona appena dopo la scadenza, mai un attimo prima.
const MARGINE_SVEGLIA: Duration = Duration::from_millis(20);

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
    /// La luminosità di una luce accesa, in percento (`LightEntity.brightness`, che il mapper dà
    /// già in percento e a luce spenta non dà): `None` per tutto il resto.
    pub luminosita: Option<u8>,
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

pub(crate) fn preferito(e: &AppEntity) -> Preferito {
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
        luminosita: match e {
            AppEntity::Light(l) if l.is_on => l.brightness.filter(|b| b.is_finite()).map(|b| b.round().clamp(0.0, 100.0) as u8).filter(|&b| b > 0),
            _ => None,
        },
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

/// Le luci accese e disponibili delle stanze **come si vedono adesso**, nell'ordine delle stanze:
/// con lo stato ottimistico dei tocchi (una luce appena spenta esce subito, una appena accesa
/// entra, prima di ogni conferma di Home Assistant). È la sola vista ottimistica: la carta di
/// Casa, la pagina, «Spegni tutte» e il conteggio partono da qui (ridisegno F3, C1).
pub fn luci_accese_ora(menu: &MenuData, ottimismo: &Ottimismo<bool>) -> Vec<Preferito> {
    luci_accese_in(menu, None, ottimismo)
}

/// Come `luci_accese_ora`, ma solo di una stanza (`Some(area_id)`; «Altro» è l'`area_id` vuoto).
pub fn luci_accese_in(menu: &MenuData, area: Option<&str>, ottimismo: &Ottimismo<bool>) -> Vec<Preferito> {
    menu.rooms
        .iter()
        .filter(|r| area.is_none_or(|a| r.area_id == a))
        .flat_map(|r| r.entities.iter())
        .filter(|e| matches!(e, AppEntity::Light(_)))
        .map(preferito)
        .map(|p| con_ottimismo(p, ottimismo))
        .filter(|p| p.acceso && p.disponibile)
        .collect()
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

/// La parte sinistra della riga alta del pannello (nella Panoramica): `(testo, nome per Narrator)`,
/// data e temperatura («VEN 13 NOV · 9°», senza meteo solo la data); per Narrator anche la
/// condizione. Una temperatura che manca non lascia «°»; una condizione sconosciuta non lascia il
/// vuoto.
pub fn riga_sinistra(data: &str, meteo: Option<&Meteo>) -> (String, String) {
    let temperatura = meteo.and_then(orologio::temperatura_breve);
    let condizione = meteo.map(|m| orologio::condizione_italiana(&m.condizione)).filter(|c| !c.is_empty());
    // Per chi ascolta: «9 gradi», senza il simbolo.
    let gradi = temperatura.as_deref().map(|t| format!("{} gradi", t.trim_end_matches('°')));
    let testo = [Some(data), temperatura.as_deref()].into_iter().flatten().collect::<Vec<_>>().join(" · ");
    let esteso = orologio::data_per_esteso(data).unwrap_or_else(|| data.to_owned());
    let minuscola = condizione.map(str::to_lowercase);
    let nome = [Some(esteso.as_str()), gradi.as_deref(), minuscola.as_deref()].into_iter().flatten().collect::<Vec<_>>().join(", ");
    (testo, nome)
}

/// «Riprova» ha senso mentre la casa non risponde (errore o tentativo in corso): non dopo un
/// «Disconnetti» voluto, senza ambiente o con un token da sistemare (lì servono le Impostazioni).
pub fn riprovabile(s: &StatoConnessione) -> bool {
    matches!(s, StatoConnessione::Errore(_) | StatoConnessione::InConnessione)
}

/// L'errore tecnico viene dal portachiavi (il token salvato non si legge), non dalla rete: la
/// chiave su cui `frase_errore` e `casa::frase_errore_breve` scelgono la frase.
pub fn errore_del_portachiavi(e: &str) -> bool {
    e.starts_with("portachiavi")
}

/// Una frase semplice al posto dell'errore tecnico di casa-core (il dettaglio resta per le
/// Impostazioni, sezione Informazioni).
pub fn frase_errore(e: &str) -> &'static str {
    if errore_del_portachiavi(e) {
        "Token salvato non leggibile"
    } else {
        "Casa non raggiungibile"
    }
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
        // Una luce che si mostra spenta non tiene la luminosità di quando era accesa.
        if !acceso {
            p.luminosita = None;
        }
    }
    p
}

/// L'errore di un tocco che Home Assistant non ha confermato in tempo.
pub const ERRORE_SENZA_CONFERMA: &str = "La casa non ha confermato il comando";

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

// ─── Lati della pillola (M2, Task 4) ─────────────────────────────────────────

/// Il nome di una scelta salvata che oggi non è fra quelle disponibili (un sensore sparito, il
/// meteo assente): resta nel selettore, così si vede che cosa è scelto.
fn voce_mancante(l: &Lato) -> String {
    let nome = match l {
        Lato::Sensore(id) => id.clone(),
        // Il nome che la scelta ha nel catalogo completo («Temperatura esterna», «Disco»…).
        altro => lati::scelte(true, &[], true, true)
            .into_iter()
            .find(|(l, _)| l == altro)
            .map_or_else(|| altro.come_str(), |(_, n)| n),
    };
    format!("{nome} (non disponibile)")
}

/// La posizione di `scelto` nel catalogo; se manca, la si aggiunge in fondo (vedi `voce_mancante`).
fn indice_in(catalogo: &mut Vec<(Lato, String)>, scelto: &Lato) -> i32 {
    match catalogo.iter().position(|(l, _)| l == scelto) {
        Some(i) => i as i32,
        None => {
            catalogo.push((scelto.clone(), voce_mancante(scelto)));
            (catalogo.len() - 1) as i32
        }
    }
}

/// Tutto ciò che Impostazioni › Isola mostra, già pronto.
#[derive(Debug, Default, PartialEq)]
pub struct Isola {
    pub lati: Vec<(Lato, String)>,
    pub sinistro: i32,
    pub destro: i32,
    /// `None` = automatico.
    pub meteo: Vec<(Option<String>, String)>,
    pub indice_meteo: i32,
}

/// Impostazioni › Isola dalla configurazione e dai dati della casa (anche senza: niente meteo né
/// sensori fra le scelte).
pub fn isola(config: &Config, menu: Option<&MenuData>, batteria: bool) -> Isola {
    let meteo_c = menu.is_some_and(|m| m.meteo.is_some());
    let sensori_lati = menu.map(lati::sensori_disponibili).unwrap_or_default();
    // La rete (Task 8) solo su Windows.
    let mut cat_lati = lati::scelte(meteo_c, &sensori_lati, cfg!(windows), batteria);
    let sinistro = indice_in(&mut cat_lati, &Lato::da_str(&config.lato_sinistro));
    let destro = indice_in(&mut cat_lati, &Lato::da_str(&config.lato_destro));

    let mut meteo: Vec<(Option<String>, String)> = vec![(None, "Automatico".into())];
    if let Some(m) = menu {
        meteo.extend(m.meteo_elenco.iter().map(|v| {
            let nome = if v.name.is_empty() { v.entity_id.clone() } else { v.name.clone() };
            (Some(v.entity_id.clone()), nome)
        }));
    }
    let indice_meteo = match &config.meteo_entita {
        None => 0,
        Some(id) => match meteo.iter().position(|(e, _)| e.as_deref() == Some(id.as_str())) {
            Some(i) => i as i32,
            None => {
                meteo.push((Some(id.clone()), format!("{id} (non disponibile)")));
                (meteo.len() - 1) as i32
            }
        },
    };
    Isola { lati: cat_lati, sinistro, destro, meteo, indice_meteo }
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
pub fn batteria(lettura: LetturaBatteria) {
    sorgenti_lati::batteria(lettura);
    if let Some(p) = ponte() {
        p.batteria(&lettura);
    }
}

/// Una voce «In attesa» per la riga del pannello.
/// Sotto il titolo il livello e il nome del modulo a cui la voce appartiene (dal registro).
/// `non_archiviata`: l'ultimo «Archivia» non è riuscito (lo dice la riga sotto il titolo).
fn voce_attesa_ui(v: &Voce, registro: &Registro, non_archiviata: bool) -> VoceAttesaUi {
    let mut sotto = format!("{} · {}", v.livello.parola(), registro.nome(v.fonte.modulo()));
    if non_archiviata {
        sotto.push_str(" · archiviazione non riuscita");
    }
    VoceAttesaUi {
        id: v.id.as_str().into(),
        livello: pillola::livello_ui(v.livello),
        titolo: v.titolo.as_str().into(),
        sotto: sotto.into(),
        modulo: registro.nome(v.fonte.modulo()).into(),
        dettaglio: v.dettaglio.as_str().into(),
        icona: pillola::icona_fonte(v.fonte),
        pulsanti: pillola::pulsanti_ui(&v.pulsanti),
        permesso: v.permesso.as_ref().map(permesso_ui).unwrap_or_default(),
    }
}

/// I dati di un permesso per il componente del guscio: «chiesto» è l'ora locale fissa della
/// richiesta («alle 14:32»), calcolata qui (nessun timer); vuota se l'ora non è valida.
fn permesso_ui(d: &attenzione::DatiPermesso) -> PermessoUi {
    let chiesto = chrono::DateTime::from_timestamp(d.alle, 0)
        .map(|t| format!("alle {}", t.with_timezone(&chrono::Local).format("%H:%M")))
        .unwrap_or_default();
    PermessoUi {
        sessione: d.sessione.as_str().into(),
        strumento: d.strumento.as_str().into(),
        campo: d.campo.as_str().into(),
        comando: d.comando.as_str().into(),
        cartella: d.cartella.as_str().into(),
        chiesto: chiesto.into(),
    }
}

/// Chi apre un indirizzo nel browser (vedi `Ponte::apri_url`).
type ApriUrl = dyn Fn(&str) -> Result<(), String>;

pub struct Ponte {
    casa: Rc<CasaService>,
    pillola: slint::Weak<Pillola>,
    /// Il pannello esiste solo da aperto: qui il riferimento debole, vuoto da chiuso.
    pannello: RefCell<slint::Weak<Pannello>>,
    /// La finestra di gestione con le schede delle API (Task 11): il processo nasce al primo «Dettagli».
    gestione: RefCell<Option<crate::gestione::Gestione>>,
    /// La finestra delle schede è aperta: una lettura nuova la rimpiazza.
    dettagli_aperti: Cell<bool>,
    /// La generazione del processo della finestra delle schede (`Gestione::generazione`): i messaggi
    /// di un altro processo si ignorano.
    dettagli_gen: Cell<u64>,
    /// Quando l'utente ha chiesto l'ultimo «Aggiorna» (per l'intervallo minimo).
    dettagli_ultimo_aggiorna: Cell<Option<std::time::Instant>>,
    /// Un avviso per la nota delle schede (una volta sola), come «Appena aggiornato…».
    dettagli_avviso: RefCell<Option<String>>,
    /// La vista su cui deve aprirsi il prossimo pannello (una volta sola): chi apre il pannello da
    /// fuori (la pillola che mostra Claude, o fuori da Windows un pulsante di una voce di Claude)
    /// la chiede qui.
    vista_richiesta: Cell<Option<&'static str>>,
    /// Il permesso (id della voce) su cui deve aprirsi la pagina Agenti del prossimo pannello: la
    /// prima scrittura della pagina sceglie la sua riga ed entra nel permesso del dettaglio (una
    /// volta sola, come `vista_richiesta`; ridisegno F2, Task 8).
    permesso_richiesto: RefCell<Option<String>>,
    cervello: Rc<RefCell<Cervello>>,
    /// `--umore`: umore fisso per prova, gli eventi non lo cambiano.
    umore_fisso: Option<Umore>,
    stato: RefCell<StatoConnessione>,
    menu: RefCell<Option<Arc<MenuData>>>,
    /// Comandi falliti, per entità: l'errore resta sulla voce fino al prossimo tocco o a un esito ok.
    errori: RefCell<HashMap<String, String>>,
    /// La pagina di F3 aperta (ridisegno F3, Task 4b e 7; pre-flight C5, su tutte le piattaforme):
    /// `Some("casa")`, `Some("collegamenti")` o `Some("pc")` a pagina aperta, `None` altrimenti
    /// (niente si calcola né si scrive). I modelli, uno per impianto (Comandi: filtri, card, scene;
    /// Misure: misure e righe), sopravvivono al pannello ma da chiusi sono vuoti. Vedi `con_pagine`.
    pagina_aperta: Cell<Option<&'static str>>,
    comandi_filtri: Rc<VecModel<FiltroUi>>,
    comandi_card: Rc<VecModel<CardUi>>,
    comandi_scene: Rc<VecModel<CardUi>>,
    /// La pagina PC sull'impianto Misure (ridisegno F3, Task 7): l'ultimo campione del PC (con la
    /// CPU pronta e le fonti lente lette), dimenticato alla chiusura del pannello (pre-flight C13:
    /// la pagina riaperta non mostra un dato vecchio come attuale); chi occupa il quarto posto,
    /// deciso una volta al primo campione con le fonti lente e fermo fino alla chiusura del pannello;
    /// i modelli delle misure e delle righe.
    pc_campione: RefCell<Option<CampionePc>>,
    pc_quarto: Cell<crate::moduli::pagina_pc::Quarto>,
    misure_valori: Rc<VecModel<MisuraUi>>,
    misure_righe: Rc<VecModel<RigaMisuraUi>>,
    /// Il filtro scelto della pagina Casa (`None`: quello iniziale), quello dei modelli scritti
    /// (un filtro diverso riporta il fuoco sulla prima card) e quello chiesto prima che la pagina si
    /// apra (la carta Casa con le luci accese: «Luci accese»), consumato dalla prima scrittura e
    /// dimenticato alla chiusura del pannello (pre-flight C9).
    filtro_casa: RefCell<Option<String>>,
    filtro_mostrato: RefCell<String>,
    filtro_richiesto: RefCell<Option<String>>,
    /// La guardia dei pulsanti che nascono sotto il clic (revisione finale di F3, I1; come
    /// `ARMAMENTO_CONSENTI` dei permessi: confronto di istanti, nessun timer). `azione_casa_dal`:
    /// «Spegni tutte» e «Spegni la stanza» da un clic valgono solo da quest'istante, spostato di
    /// `GUARDIA_CASA` in avanti ogni volta che nasce un'azione nuova nella testata della pagina Casa
    /// (`azione_casa_mostrata`, `None` a pagina appena aperta) o nella carta di Casa
    /// (`azione_carta_casa`): «Riprova» che diventa «Spegni tutte» alla connessione, un'azione che
    /// compare. `card_casa_dal`: il tocco di una card della Casa vale solo da quest'istante,
    /// spostato quando la pagina passa da uno stato senza card (il «Riprova» dello stato, il primo
    /// avvio) a quello pieno (`stato_casa_mostrato`).
    pub(crate) azione_casa_dal: Cell<Instant>,
    azione_casa_mostrata: RefCell<Option<SharedString>>,
    azione_carta_casa: Cell<&'static str>,
    pub(crate) card_casa_dal: Cell<Instant>,
    stato_casa_mostrato: Cell<Option<crate::TipoStatoPaginaUi>>,
    /// I riassunti dei moduli seguono i dati solo a pannello aperto (Task 14h): da chiuso nessuno li
    /// guarda (la pillola mostra umore e voci in attesa).
    modelli_vivi: Cell<bool>,
    /// Da quando l'umore è «pensa» (per il registro diagnostico: si conta quando finisce).
    pensa_da: Cell<Option<Instant>>,
    /// Impostazioni › Dispositivi: le righe e le stanze compresse (solo finché l'app gira).
    righe: Rc<VecModel<RigaDispositivoUi>>,
    /// I collegamenti veloci, per Impostazioni › Collegamenti (la pagina del pannello ha le sue card).
    collegamenti: Rc<VecModel<CollegamentoUi>>,
    /// Chi apre un collegamento nel browser (`sistema::browser::apri`; i test lo sostituiscono:
    /// nessun browser vero durante le prove).
    apri_url: RefCell<Rc<ApriUrl>>,
    /// Ultimo collegamento non aperto, col dettaglio tecnico, per Impostazioni › Informazioni.
    errore_collegamento: RefCell<Option<String>>,
    /// La frase dell'ultimo lancio non riuscito, nel sotto della sintesi della pagina Collegamenti
    /// (ridisegno F3, Task 5). Un'altra cosa da `errore_collegamento` (il dettaglio tecnico di
    /// Impostazioni › Informazioni): si azzera con un lancio riuscito e alla chiusura del pannello.
    errore_lancio: RefCell<Option<String>>,
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
    /// I moduli del pannello (ridisegno F1): ordine, ultimi riassunti e livelli delle voci, e il
    /// modello delle carte (sopravvive al pannello). Vedi `con_moduli`.
    registro: RefCell<Registro>,
    modello_moduli: Rc<VecModel<ModuloUi>>,
    /// Le schede della barra (ridisegno F1, Task 6): geometria, segni e nomi per Narrator, scritti
    /// da `scrivi_schede` (solo le righe cambiate).
    modello_schede: Rc<VecModel<SchedaBarraUi>>,
    /// Da chiuso è cambiato un dato dei moduli (sessioni, casa, voci): riassunti e livelli si
    /// ricalcolano all'apertura, una volta. Vero all'avvio (nessun riassunto ancora calcolato).
    moduli_vecchi: Cell<bool>,
    /// I lavori in corso: la linea della ritirata si riempie con quello più urgente.
    avanzamenti: RefCell<Vec<Avanzamento>>,
    /// Il timer di `--prova-avanzamento` (fermo se la prova non c'è).
    prova_avanzamento: slint::Timer,
    /// Da quanto la casa non è connessa; la sua sveglia suona solo allo scadere dei 5 minuti.
    sorveglianza: RefCell<SorveglianzaCasa>,
    /// La voce della batteria (due livelli) e il «Ho capito» che la zittisce fino alla ricarica.
    batteria: RefCell<SorveglianzaBatteria>,
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
    /// Chiude il pannello (lo collega `Apertura`): l'azione «Apri {nome}» di una carta, dopo che il
    /// browser è partito.
    chiudi_pannello: RefCell<Option<Box<dyn Fn()>>>,
    /// Acceso/spento mostrati subito dopo un tocco, finché Home Assistant non conferma.
    ottimismo: RefCell<Ottimismo<bool>>,
    /// Suona alla scadenza della prossima attesa di conferma (solo se ce ne sono).
    sveglia_ottimismo: slint::Timer,
    /// La pillola si vede (`Visibilita::Pillola`, la dicono i servizi di sistema): i lati
    /// campionano solo allora.
    pillola_mostrata: Cell<bool>,
    /// Claude Code (M3, Task 4 e 7A): le sessioni, il tubo protetto (solo se collegato), la
    /// sveglia singola alla prossima scadenza di una sessione, il timer da un secondo del testo di
    /// attività (acceso solo mentre serve: `attivita_serve`).
    claude_sessioni: RefCell<Sessioni>,
    claude_ascolto: RefCell<Option<claude_hook::Servitore>>,
    /// Limiti dell'abbonamento (5 ore, 7 giorni) dall'ultima barra che li portava (M3, Task 6) e
    /// contesto per sessione; mai scaduti per chi li legge (`limiti`).
    claude_limiti: RefCell<Option<(Option<claude_hook::Limite>, Option<claude_hook::Limite>)>>,
    claude_contesto: RefCell<HashMap<String, f32>>,
    /// La catena dei processi dell'aiutante per sessione (`session_id`), presa al primo evento:
    /// serve a portare davanti il terminale (Task 10). Al più 16, tolta con la sessione.
    claude_catene: RefCell<HashMap<String, Vec<(u32, String)>>>,
    /// Le sessioni il cui terminale non si raggiunge dalla catena (finestra non trovata alla
    /// prima richiesta, all'apertura della vista Claude o all'ultimo clic): il clic ripiega
    /// sull'ultimo terminale usato, e la riga lo dice. Solo sessioni vive (tolte con loro).
    claude_ripiego: RefCell<std::collections::HashSet<String>>,
    /// Toglie da sola la voce «Non trovo il terminale».
    sveglia_terminale: slint::Timer,
    /// La pagina Agenti del pannello (ridisegno F2, Task 6): si vede (pannello aperto su di lei) e
    /// il modello delle sue righe (`moduli::pagina_agenti::riga`). Chiusa, niente si calcola né si
    /// scrive.
    vista_claude: Cell<bool>,
    /// La Coda si vede (pannello aperto su di lei): entrarci riarma «Consenti».
    vista_coda: Cell<bool>,
    elenco_righe: Rc<VecModel<RigaModuloUi>>,
    /// L'ordine fermo delle righe (RC-53), per `session_id`: si decide all'apertura della vista e
    /// poi chi c'è tiene il suo posto, chi arriva va in fondo. Vuoto a vista chiusa.
    claude_ordine: RefCell<Vec<String>>,
    /// I permessi che Claude Code ha chiesto e aspettano una risposta (M3, Task 5): numero del
    /// permesso → chi l'ha chiesto e dove mandare la decisione. Al più 16.
    claude_permessi: RefCell<HashMap<u64, con_claude::Pendente>>,
    /// Da quando «Consenti» risponde dopo un cambio della voce mostrata.
    claude_armamento: RefCell<con_claude::Armamento>,
    /// L'anteprima che l'utente sta leggendo (M3, Task 7B): la conferma rilegge il file, rifà
    /// l'unione e scrive solo se la modifica è identica a questa. `None` finché non ha premuto
    /// «Collega Claude Code».
    claude_preparato: RefCell<Option<claude_hook::installa::Anteprima>>,
    /// Il numero del tubo in funzione: sale a ogni apertura e a ogni chiusura, così l'avviso di un
    /// servitore vecchio («canale perso») non ferma quello nuovo (M3, Task 7B).
    claude_generazione: Cell<u64>,
    sveglia_claude: slint::Timer,
    /// La sveglia singola della vista Claude (Task 9): all'azzeramento del primo limite mostrato,
    /// perché un anello scaduto sparisca in orario. Solo a vista aperta; `limiti_armati` è
    /// l'istante (secondi dall'epoca) per cui è armata.
    sveglia_limiti: slint::Timer,
    limiti_armati: Cell<Option<i64>>,
    /// Chiave da amministratore e letture delle API (M3, Task 8): il timer da 3 ore (esiste solo con
    /// la chiave), una lettura in corso, una prova in corso, l'ultimo esito (ora e riepilogo).
    sveglia_api: slint::Timer,
    /// Il segnale «lettura in corso» della generazione attuale (si cambia a ogni Salva/Togli) e il
    /// numero della generazione: un risultato di una generazione vecchia si scarta.
    api_in_corso: RefCell<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    api_generazione: Cell<u64>,
    api_prova_in_corso: Cell<bool>,
    api_ultimo: RefCell<Option<(String, Result<api_claude::Riepilogo, String>)>>,
    timer_attivita: slint::Timer,
    /// Impostazioni › Isola: i modelli dei selettori e i cataloghi da cui vengono (un indice
    /// scelto nell'interfaccia diventa la fonte da salvare).
    isola: RefCell<Isola>,
    voci_lati: Rc<VecModel<SharedString>>,
    voci_meteo: Rc<VecModel<SharedString>>,
    /// Il volume del PC (M2, Task 7, solo Windows): il thread `audio`, avviato alla prima
    /// richiesta (rotella sulla pillola o prima apertura del pannello) e poi fermo su `recv`;
    /// l'ultimo stato letto (`None`: nessuna uscita audio, o non ancora letto); la rotella
    /// accumulata; la sveglia singola che riporta gli occhi al posto della barra.
    #[cfg(windows)]
    audio: std::cell::OnceCell<volume::Audio>,
    #[cfg(windows)]
    volume: Cell<Option<StatoVolume>>,
    #[cfg(windows)]
    rotella: Cell<volume::Rotella>,
    #[cfg(windows)]
    sveglia_barra: slint::Timer,
    /// Wi-Fi, Bluetooth e rete (M2, Task 8, solo Windows; dal ridisegno F3, Task 7, lo stato delle
    /// righe della pagina PC vive qui, pre-flight C6). `radio`: il thread delle radio, vivo solo a
    /// vista PC aperta; `stati_radio`: l'ultimo elenco letto (`None`: non ancora letto o radio non
    /// più seguite; è lo stato vero a cui torna una riga dopo un rifiuto). `radio_volute`: il valore
    /// chiesto da un tocco (Wi-Fi, Bluetooth) finché non arriva lo stato vero. `errore_radio`: la
    /// riga (`pagina_pc::WIFI` o `BLUETOOTH`) il cui cambio Windows ha rifiutato. `cuffie`: la
    /// lettura delle batterie Bluetooth fatta entrando nella vista. `osserva_rete`: acceso solo se
    /// serve (`aggiorna_rete`); `nome_rete`: il nome della rete Wi-Fi, solo a vista PC aperta e mai
    /// nei log. `vista_pc`: la vista PC si vede (pannello aperto su di lei).
    #[cfg(windows)]
    radio: RefCell<Option<radio::Radio>>,
    #[cfg(windows)]
    stati_radio: RefCell<Option<Vec<radio::StatoRadio>>>,
    #[cfg(windows)]
    radio_volute: Cell<[Option<bool>; 2]>,
    #[cfg(windows)]
    errore_radio: Cell<Option<&'static str>>,
    #[cfg(windows)]
    cuffie: RefCell<Option<batterie_bt::RigaCuffie>>,
    #[cfg(windows)]
    osserva_rete: RefCell<Option<radio::OsservaRete>>,
    #[cfg(windows)]
    nome_rete: RefCell<Option<String>>,
    #[cfg(windows)]
    vista_pc: Cell<bool>,
    /// Le batterie dei dispositivi Bluetooth (M2, Task 9, solo Windows): il thread `batterie-bt`
    /// (che dorme; c'è solo con l'interruttore acceso).
    #[cfg(windows)]
    sorveglia_bt: RefCell<Option<batterie_bt::SorveglianzaBt>>,
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
    // L'ordine dei moduli scelto dall'utente (vuoto: quello del registro).
    let registro = casa.con_config(|c| Registro::nuovo(&c.moduli));
    let ponte = Rc::new(Ponte {
        casa,
        pillola: pillola.as_weak(),
        pannello: RefCell::new(slint::Weak::default()),
        gestione: RefCell::new(None),
        dettagli_aperti: Cell::new(false),
        dettagli_gen: Cell::new(0),
        dettagli_ultimo_aggiorna: Cell::new(None),
        dettagli_avviso: RefCell::new(None),
        vista_richiesta: Cell::new(None),
        permesso_richiesto: RefCell::new(None),
        cervello,
        umore_fisso,
        stato: RefCell::new(StatoConnessione::Disconnesso),
        menu: RefCell::new(None),
        dati_delle: RefCell::new(None),
        errori: RefCell::new(HashMap::new()),
        pagina_aperta: Cell::new(None),
        comandi_filtri: Rc::new(VecModel::default()),
        comandi_card: Rc::new(VecModel::default()),
        comandi_scene: Rc::new(VecModel::default()),
        pc_campione: RefCell::new(None),
        pc_quarto: Cell::new(crate::moduli::pagina_pc::Quarto::DaDecidere),
        misure_valori: Rc::new(VecModel::default()),
        misure_righe: Rc::new(VecModel::default()),
        filtro_casa: RefCell::new(None),
        filtro_mostrato: RefCell::new(String::new()),
        filtro_richiesto: RefCell::new(None),
        azione_casa_dal: Cell::new(Instant::now()),
        azione_casa_mostrata: RefCell::new(None),
        azione_carta_casa: Cell::new(""),
        card_casa_dal: Cell::new(Instant::now()),
        stato_casa_mostrato: Cell::new(None),
        modelli_vivi: Cell::new(false),
        pensa_da: Cell::new(None),
        righe: Rc::new(VecModel::default()),
        collegamenti: Rc::new(VecModel::default()),
        apri_url: RefCell::new(Rc::new(crate::sistema::browser::apri)),
        errore_collegamento: RefCell::new(None),
        errore_lancio: RefCell::new(None),
        collegamento_aperto: Cell::new(false),
        compresse: RefCell::new(std::collections::HashSet::new()),
        mostrato: RefCell::new(None),
        sveglia: slint::Timer::default(),
        attese: RefCell::new(Attese::default()),
        modello_attese: Rc::new(VecModel::default()),
        registro: RefCell::new(registro),
        modello_moduli: Rc::new(VecModel::default()),
        modello_schede: Rc::new(VecModel::default()),
        moduli_vecchi: Cell::new(true),
        avanzamenti: RefCell::new(Vec::new()),
        prova_avanzamento: slint::Timer::default(),
        sorveglianza: RefCell::new(SorveglianzaCasa::new(Instant::now())),
        batteria: RefCell::new(SorveglianzaBatteria::default()),
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
        chiudi_pannello: RefCell::new(None),
        ottimismo: RefCell::new(Ottimismo::default()),
        sveglia_ottimismo: slint::Timer::default(),
        traccia_tocco: std::env::var_os("ITSYHOME_TRACCIA_TOCCO").is_some_and(|v| v == "1"),
        pillola_mostrata: Cell::new(true),
        claude_sessioni: RefCell::new(Sessioni::new()),
        claude_ascolto: RefCell::new(None),
        claude_limiti: RefCell::new(None),
        claude_contesto: RefCell::new(HashMap::new()),
        claude_catene: RefCell::new(HashMap::new()),
        claude_ripiego: RefCell::new(std::collections::HashSet::new()),
        sveglia_terminale: slint::Timer::default(),
        vista_claude: Cell::new(false),
        vista_coda: Cell::new(false),
        elenco_righe: Rc::new(VecModel::default()),
        claude_ordine: RefCell::new(Vec::new()),
        claude_permessi: RefCell::new(HashMap::new()),
        claude_armamento: RefCell::new(con_claude::Armamento::nuovo()),
        claude_preparato: RefCell::new(None),
        claude_generazione: Cell::new(0),
        sveglia_claude: slint::Timer::default(),
        sveglia_limiti: slint::Timer::default(),
        limiti_armati: Cell::new(None),
        sveglia_api: slint::Timer::default(),
        api_in_corso: RefCell::new(Default::default()),
        api_generazione: Cell::new(0),
        api_prova_in_corso: Cell::new(false),
        api_ultimo: RefCell::new(None),
        timer_attivita: slint::Timer::default(),
        isola: RefCell::new(Isola::default()),
        voci_lati: Rc::new(VecModel::default()),
        voci_meteo: Rc::new(VecModel::default()),
        #[cfg(windows)]
        audio: std::cell::OnceCell::new(),
        #[cfg(windows)]
        volume: Cell::new(None),
        #[cfg(windows)]
        rotella: Cell::new(volume::Rotella::default()),
        #[cfg(windows)]
        sveglia_barra: slint::Timer::default(),
        #[cfg(windows)]
        radio: RefCell::new(None),
        #[cfg(windows)]
        stati_radio: RefCell::new(None),
        #[cfg(windows)]
        radio_volute: Cell::new([None; 2]),
        #[cfg(windows)]
        errore_radio: Cell::new(None),
        #[cfg(windows)]
        cuffie: RefCell::new(None),
        #[cfg(windows)]
        osserva_rete: RefCell::new(None),
        #[cfg(windows)]
        nome_rete: RefCell::new(None),
        #[cfg(windows)]
        vista_pc: Cell::new(false),
        #[cfg(windows)]
        sorveglia_bt: RefCell::new(None),
    });
    PONTE.with(|p| *p.borrow_mut() = Some(ponte.clone()));
    // I pulsanti della pillola allargata agiscono sulla voce che sta chiamando; «Apri» (un
    // consenso col testo tagliato) apre il pannello come un clic sulla pillola: per un permesso la
    // Panoramica, con la voce nella riga «In attesa».
    pillola.on_azione(|voce, pulsante| {
        if pulsante == PULSANTE_APRI {
            if let Some(pl) = self::ponte().and_then(|p| p.pillola.upgrade()) {
                pl.invoke_cliccata();
            }
            return;
        }
        // `voce`: quella che la pillola aveva resa insieme ai pulsanti; se nel frattempo è
        // cambiata il clic non vale (e non finisce su un'altra richiesta).
        if let Some(p) = self::ponte() {
            p.azione_pillola(&voce, &pulsante);
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
    // I lati della pillola scelti nelle Impostazioni › Isola (niente / niente: nessuna sorgente).
    let (sinistro, destro) = ponte.casa.con_config(|c| (Lato::da_str(&c.lato_sinistro), Lato::da_str(&c.lato_destro)));
    sorgenti_lati::collega(pillola, sinistro, destro);
    // La rotella sulla pillola cambia il volume del PC (M2, Task 7, solo Windows), se accesa.
    #[cfg(windows)]
    {
        pillola.set_rotella_attiva(ponte.casa.con_config(|c| c.volume_rotella));
        pillola.on_rotella(|delta| {
            if let Some(p) = self::ponte() {
                p.rotella_volume(delta);
            }
        });
        // Le cuffie Bluetooth scariche (Task 9): il thread c'è solo con l'interruttore acceso.
        if ponte.casa.con_config(|c| c.avvisi_bt) {
            ponte.accendi_bt();
        }
    }
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
        StatoConnessione::TokenMancante => "nessun token nell'archivio sicuro del sistema per la casa attiva".into(),
        _ => return None,
    };
    Some(format!("{} · {testo}", adesso_hh_mm()))
}

// Nei test, le scritture vere fatte da `aggiorna_modello` in questo thread (pre-flight C18: un
// controllo che confronta solo i valori non potrebbe fallire).
#[cfg(test)]
thread_local! {
    pub(crate) static RIGHE_SCRITTE: Cell<u64> = const { Cell::new(0) };
}

/// Riscrive solo le righe cambiate (una riga uguale non ridisegna nulla). Restituisce quante
/// righe ha riscritto (per il registro diagnostico).
fn aggiorna_modello<T: Clone + PartialEq + 'static>(modello: &VecModel<T>, voci: Vec<T>) -> u64 {
    if modello.row_count() != voci.len() {
        // Un modello svuotato conta come una riga riscritta (c'è stato un cambio).
        let n = voci.len().max(1) as u64;
        modello.set_vec(voci);
        #[cfg(test)]
        RIGHE_SCRITTE.with(|c| c.set(c.get() + n));
        return n;
    }
    let mut riscritte = 0;
    for (i, v) in voci.into_iter().enumerate() {
        if modello.row_data(i).as_ref() != Some(&v) {
            modello.set_row_data(i, v);
            #[cfg(test)]
            RIGHE_SCRITTE.with(|c| c.set(c.get() + 1));
            riscritte += 1;
        }
    }
    riscritte
}

// Nei test, le proprietà delle pagine scritte davvero da `scrivi_se_cambia` in questo thread
// (pre-flight C11 e C18: i test «scritta solo se cambia» contano le righe e le proprietà, un
// controllo che confronta solo i valori non potrebbe fallire).
#[cfg(test)]
thread_local! {
    pub(crate) static PROPRIETA_SCRITTE: Cell<u64> = const { Cell::new(0) };
}

/// Scrive una proprietà della pagina solo se il valore cambia (leggerezza: un valore uguale non
/// ridisegna nulla). Comune a tutte le pagine.
pub(crate) fn scrivi_se_cambia<T: PartialEq>(attuale: T, nuovo: T, scrivi: impl FnOnce(T)) {
    if attuale != nuovo {
        #[cfg(test)]
        PROPRIETA_SCRITTE.with(|c| c.set(c.get() + 1));
        scrivi(nuovo);
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
                // Con la connessione cambiano la pagina (le card si possono toccare, la sintesi e lo
                // stato) e la carta.
                self.casa_cambiata();
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
                // I lati della pillola leggono meteo e sensori scelti (nessuna lettura in più).
                sorgenti_lati::casa(&menu);
                *self.menu.borrow_mut() = Some(menu);
                *self.dati_delle.borrow_mut() = Some(adesso_hh_mm());
                self.casa_cambiata();
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
                // Un comando fallito rimette le luci come erano: la pagina e la carta lo dicono.
                self.casa_cambiata();
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
        self.aggiorna_lati();
        self.aggiorna_claude_pillola();
        match scadenza {
            Some(tra) => self.sveglia.start(slint::TimerMode::SingleShot, tra + MARGINE_SVEGLIA, || {
                if let Some(p) = ponte() {
                    p.aggiorna_umore();
                }
            }),
            None => self.sveglia.stop(),
        }
    }

    /// Le sorgenti dei lati seguono la pillola: accese solo se si vede (né ritirata né nascosta né
    /// allargata) e non dorme. Cambia qualcosa solo se cambia lo stato (vedi `sorgenti_lati`).
    fn aggiorna_lati(&self) {
        let (allargata, dorme) = match self.mostrato.borrow().as_ref() {
            Some(v) => (v.voce_allargata().is_some(), v.umore == Umore::Dorme),
            None => (false, false),
        };
        let visibile = self.pillola_mostrata.get() && !allargata;
        sorgenti_lati::aggiorna(sorgenti_lati::bisogni_scelti(), visibile, dorme);
        // La rete segue gli stessi bisogni (lato «Rete» scelto e pillola visibile e sveglia).
        #[cfg(windows)]
        self.aggiorna_rete(visibile && !dorme);
    }

    /// I servizi di sistema dicono se la pillola si vede (`Visibilita::Pillola`).
    pub fn pillola_visibile(&self, si: bool) {
        if self.pillola_mostrata.replace(si) != si {
            if si {
                self.claude_superficie_compare(Instant::now());
            }
            self.aggiorna_lati();
            self.aggiorna_claude_pillola();
        }
    }

    /// Impostazioni › Isola, se la finestra è aperta: riscrive solo ciò che cambia.
    fn riempi_isola(&self, f: &FinestraImpostazioni) {
        let config = self.casa.config();
        let nuova = isola(&config, self.menu.borrow().as_deref(), sorgenti_lati::ha_batteria());
        let nomi = |v: &[(Lato, String)]| v.iter().map(|(_, n)| SharedString::from(n.as_str())).collect::<Vec<_>>();
        aggiorna_modello(&self.voci_lati, nomi(&nuova.lati));
        aggiorna_modello(&self.voci_meteo, nuova.meteo.iter().map(|(_, n)| SharedString::from(n.as_str())).collect());
        if f.get_indice_lato_sinistro() != nuova.sinistro {
            f.set_indice_lato_sinistro(nuova.sinistro);
        }
        if f.get_indice_lato_destro() != nuova.destro {
            f.set_indice_lato_destro(nuova.destro);
        }
        if f.get_indice_meteo() != nuova.indice_meteo {
            f.set_indice_meteo(nuova.indice_meteo);
        }
        let panoramica = ModoPanoramica::da_config(&config.panoramica).indice();
        if f.get_panoramica_scelta() != panoramica {
            f.set_panoramica_scelta(panoramica);
        }
        *self.isola.borrow_mut() = nuova;
    }

    /// Un lato scelto nelle Impostazioni (0 sinistra, 1 destra; indice nel catalogo dei lati):
    /// salvato e subito nella pillola.
    fn scegli_lato(&self, quale: i32, indice: i32) {
        let Some(l) = self.isola.borrow().lati.get(indice.max(0) as usize).map(|(l, _)| l.clone()) else { return };
        self.casa.aggiorna_config(|c| {
            if quale == 0 {
                c.lato_sinistro = l.come_str();
            } else {
                c.lato_destro = l.come_str();
            }
        });
        let (sinistro, destro) = self.casa.con_config(|c| (Lato::da_str(&c.lato_sinistro), Lato::da_str(&c.lato_destro)));
        sorgenti_lati::scegli(sinistro, destro);
        if let Some(m) = self.menu.borrow().as_ref() {
            sorgenti_lati::casa(m);
        }
        self.aggiorna_lati();
        self.aggiorna_pannello();
    }

    /// La modalità della Panoramica scelta nelle Impostazioni (0 Griglia, 1 Orologio): salvata
    /// solo se cambia; vale dal prossimo pannello (e subito in quello aperto, se c'è).
    fn scegli_panoramica(&self, i: i32) {
        let scelta = ModoPanoramica::da_indice(i).come_str();
        if self.casa.con_config(|c| c.panoramica == scelta) {
            return;
        }
        self.casa.aggiorna_config(|c| c.panoramica = scelta.into());
        if let Some(p) = self.pannello.borrow().upgrade() {
            self.scrivi_panoramica(&p);
        }
    }

    /// La modalità salvata nel pannello (all'apertura e quando cambia), solo se diversa.
    fn scrivi_panoramica(&self, p: &Pannello) {
        let modo = self.casa.con_config(|c| ModoPanoramica::da_config(&c.panoramica)).indice();
        if p.get_panoramica() != modo {
            p.set_panoramica(modo);
        }
    }

    /// La fascia dell'Orologio dagli avvisi già scritti (`in-attesa`) e dai moduli che aspettano
    /// il primo riassunto; solo ciò che cambia.
    pub(super) fn scrivi_fascia(&self, p: &Pannello) {
        let dati = self.registro.borrow().in_attesa_di_dati();
        crate::pannello::scrivi_fascia(p, dati);
    }

    /// «Meteo da»: 0 automatico, poi le entità meteo.
    fn scegli_meteo(&self, indice: i32) {
        let Some(scelta) = self.isola.borrow().meteo.get(indice.max(0) as usize).map(|(e, _)| e.clone()) else { return };
        self.casa.aggiorna_config(|c| c.meteo_entita = scelta);
        self.aggiorna_pannello();
    }

    /// L'utente ha chiesto una connessione (Salva, Connetti, Riprova): il tentativo che parte
    /// adesso fa «pensa» (quelli automatici no, vedi `umore::Cervello`).
    fn azione_utente(&self) {
        self.cervello.borrow_mut().ricevi(Segnale::AzioneUtente, Instant::now());
    }

    /// «Riprova» (voce della casa irraggiungibile, testata della pagina Casa).
    pub fn riprova_casa(&self) {
        self.azione_utente();
        self.casa.connetti_attivo();
    }

    fn connesso(&self) -> bool {
        *self.stato.borrow() == StatoConnessione::Connesso
    }

    /// La riga alta del pannello aperto (data e meteo) e i dettagli della finestra delle
    /// Impostazioni.
    fn aggiorna_pannello(&self) {
        if let Some(p) = self.pannello.borrow().upgrade() {
            self.aggiorna_riga_sinistra(&p);
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
        if p.get_volume_rotella() != config.volume_rotella {
            p.set_volume_rotella(config.volume_rotella);
        }
        if p.get_avvisi_bt() != config.avvisi_bt {
            p.set_avvisi_bt(config.avvisi_bt);
        }
        p.set_config_non_salvata(self.errore_config.borrow().is_some());
        self.aggiorna_righe();
        self.riempi_isola(p);
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
        self.casa_cambiata();
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
        // L'icona non cambia i dati della casa: elenco e pagina (le icone dei filtri) si rifanno da qui.
        self.aggiorna_righe();
        self.casa_cambiata();
    }

    // ─── Collegamenti veloci (Task 14f) ──────────────────────────────────────

    /// I collegamenti della configurazione per le Impostazioni.
    fn aggiorna_collegamenti(&self) {
        let voci: Vec<CollegamentoUi> = self.casa.con_config(|c| {
            c.collegamenti
                .iter()
                .map(|c| CollegamentoUi {
                    id: c.id.as_str().into(),
                    nome: c.nome.as_str().into(),
                    url: c.url.as_str().into(),
                    corto: collegamenti::indirizzo_corto(&c.url).into(),
                    iniziale: collegamenti::iniziale(&c.nome).into(),
                    tono: collegamenti::tono(&c.nome),
                })
                .collect()
        });
        aggiorna_modello(&self.collegamenti, voci);
    }

    /// Dopo una modifica il fuoco delle Impostazioni va su un pulsante di una riga (`id`, azione
    /// «su», «giu», «modifica», «elimina») o su «Aggiungi un collegamento» (`id` vuoto, «aggiungi»).
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
                self.collegamenti_cambiati();
                self.fuoco_collegamento(&id, "modifica");
                String::new()
            }
            Err(e) => e,
        }
    }

    /// Elimina (la conferma l'ha già chiesta la finestra); il fuoco va sulla riga che prende il
    /// suo posto, o su «Aggiungi un collegamento» se non ne resta nessuna.
    fn elimina_collegamento(&self, id: &str) {
        let indice = self.casa.con_config(|c| c.collegamenti.iter().position(|x| x.id == id));
        self.casa.aggiorna_config(|c| {
            collegamenti::elimina(c, id);
        });
        self.aggiorna_collegamenti();
        self.collegamenti_cambiati();
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
        // L'ordine conta a parità di lanci (tutti a zero: il primo).
        self.collegamenti_cambiati();
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
                // Il browser è partito: l'errore di un lancio di prima non vale più.
                self.errore_lancio.borrow_mut().take();
                // Un lancio in più, salvato col resto della configurazione (solo qui, in locale;
                // il conteggio decide il «più usato» della carta e della sintesi).
                let ora = orologio::adesso_unix();
                self.casa.aggiorna_config(|c| {
                    collegamenti::conta_lancio(c, id, ora);
                });
                self.collegamenti_cambiati();
                return true;
            }
            Err(e) => e,
        };
        eprintln!("collegamento «{nome}» non aperto: {errore}");
        *self.errore_collegamento.borrow_mut() = Some(format!("{} · «{nome}» non aperto: {errore}", adesso_hh_mm()));
        // La frase nel sotto della sintesi della pagina Collegamenti (se è aperta): l'indicazione
        // di recupero prima del nome, perché l'ellissi del sotto tagli il nome e non lei; niente
        // glifi («›» è vietato nei testi: revisione finale di F3).
        *self.errore_lancio.borrow_mut() = Some(format!("Non si è aperto: dettagli in Impostazioni, scheda Informazioni · «{nome}»"));
        self.scrivi_pagina_collegamenti();
        if let Some(f) = self.finestra.borrow().upgrade() {
            self.riempi_impostazioni(&f);
        }
        false
    }

    /// `Apertura` dice come aprire la finestra delle Impostazioni (chiudendo il pannello).
    pub fn su_apri_impostazioni(&self, f: impl Fn() + 'static) {
        *self.apri_impostazioni.borrow_mut() = Some(Box::new(f));
    }

    /// Chi chiude il pannello al giro dopo (lo collega `Apertura`).
    pub fn su_chiudi_pannello(&self, f: impl Fn() + 'static) {
        *self.chiudi_pannello.borrow_mut() = Some(Box::new(f));
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
        f.set_fuori_da_windows(cfg!(not(windows)));
        // Impostazioni › Isola: «Volume con la rotella sulla pillola», solo dove c'è il volume.
        f.set_volume_rotella_disponibile(volume::disponibile());
        #[cfg(windows)]
        f.on_cambia_volume_rotella(|si| {
            if let Some(p) = ponte() {
                p.cambia_volume_rotella(si);
            }
        });
        // Impostazioni › Isola: «Avvisa quando le cuffie Bluetooth sono scariche», solo su Windows.
        f.set_avvisi_bt_disponibile(cfg!(windows));
        #[cfg(windows)]
        f.on_cambia_avvisi_bt(|si| {
            if let Some(p) = ponte() {
                p.cambia_avvisi_bt(si);
            }
        });
        // Fuori da Windows menu e icona non ci sono: «Esci» di Sistema chiude come `Comando::Esci`.
        f.on_esci(|| {
            let _ = slint::quit_event_loop();
        });
        f.set_righe_dispositivi(ModelRc::from(self.righe.clone()));
        // Impostazioni › Isola: lati della pillola e meteo.
        f.set_scelte_lati(ModelRc::from(self.voci_lati.clone()));
        f.set_scelte_meteo(ModelRc::from(self.voci_meteo.clone()));
        f.on_scegli_lato(|quale, indice| {
            if let Some(p) = ponte() {
                p.scegli_lato(quale, indice);
            }
        });
        f.on_scegli_meteo(|indice| {
            if let Some(p) = ponte() {
                p.scegli_meteo(indice);
            }
        });
        // Impostazioni › Isola › Panoramica: Griglia o Orologio.
        f.on_scegli_panoramica(|i| {
            if let Some(p) = ponte() {
                p.scegli_panoramica(i);
                if let Some(f) = p.finestra.borrow().upgrade() {
                    p.riempi_isola(&f);
                }
            }
        });
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
        // Impostazioni › Claude (M3, Task 7).
        self.collega_claude(f);
        // Impostazioni › Claude › Chiave da amministratore (M3, Task 8).
        self.collega_api(f);
        // Impostazioni › Claude › Avvisi (M3, Task 12).
        self.collega_soglie(f);
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
                "Casa rimossa, ma il token è rimasto nell'archivio sicuro del sistema".into()
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

    /// La parte sinistra della riga alta: data breve e meteo. Si riscrive solo se cambia (un giorno
    /// nuovo, un meteo nuovo); il resto del tempo non tocca niente e il pannello non ridisegna.
    pub fn aggiorna_riga_sinistra(&self, p: &Pannello) {
        let data = orologio::data_breve(chrono::Local::now().naive_local());
        let menu = self.menu.borrow();
        let meteo = menu.as_deref().and_then(|m| m.meteo.as_ref());
        let (testo, nome) = riga_sinistra(&data, meteo);
        if p.get_riga_sinistra() != testo.as_str() {
            p.set_riga_sinistra(testo.into());
        }
        if p.get_riga_sinistra_nome() != nome.as_str() {
            p.set_riga_sinistra_nome(nome.into());
        }
    }

    /// I riassunti dei moduli tornano a seguire i dati (pannello aperto). I modelli delle pagine no:
    /// si scrivono solo a pagina aperta (`vista_pagina`).
    pub fn apri_modelli(&self) {
        self.modelli_vivi.set(true);
    }

    /// Il pannello appena creato: i modelli delle pagine e delle voci in attesa, la riga alta, i
    /// tocchi e i pulsanti.
    pub fn riempi(&self, p: &Pannello) {
        self.apri_modelli();
        self.comandi_nel_pannello(p);
        self.misure_nel_pannello(p);
        p.set_elenco_righe(ModelRc::from(self.elenco_righe.clone()));
        p.set_attese(ModelRc::from(self.modello_attese.clone()));
        // La modalità della Panoramica (Impostazioni › Isola › Panoramica), prima delle voci.
        self.scrivi_panoramica(p);
        self.scrivi_moduli_in_attesa(p);
        // Un permesso aperto nella Coda, il suo «Consenti» che compare o la voce che ti aspetta
        // nella Panoramica: si arma di nuovo.
        p.on_consenti_compare(|| {
            if let Some(ponte) = ponte() {
                ponte.claude_superficie_compare(Instant::now());
            }
        });
        // Dove sta una voce fra quelle in attesa: il permesso aperto nella Coda segue il suo id.
        p.on_indice_voce(|id| ponte().map_or(-1, |ponte| crate::pannello::indice_voce(&*ponte.modello_attese, &id)));
        // Un pulsante della voce che ti aspetta: ignorato appena dopo un cambio della voce.
        p.on_azione_voce(|voce, pulsante| {
            if let Some(ponte) = ponte() {
                ponte.azione_voce(&voce, &pulsante, Instant::now());
            }
        });
        self.moduli_nel_pannello(p);
        // Il bottone d'azione di una carta: (id del modulo, id dell'azione).
        p.on_azione_modulo(|modulo, azione| {
            if let Some(ponte) = ponte() {
                ponte.azione_modulo(&modulo, &azione);
            }
        });
        self.aggiorna_riga_sinistra(p);
        p.on_azione(|voce, pulsante| {
            if let Some(ponte) = ponte() {
                ponte.azione(&voce, &pulsante);
            }
        });
        // Un'azione di una pagina (ridisegno F2, Task 6): il terminale di una sessione (pulsante del
        // dettaglio o Invio sulla riga), «Collega», «Dettagli», il permesso di una riga; in Casa
        // «Spegni tutte», «Spegni la stanza», «Riprova». Va chiamata solo da qui, cioè subito dopo
        // il gesto dell'utente.
        p.on_azione_pagina(|modulo, azione, riga| {
            if let Some(ponte) = ponte() {
                ponte.azione_pagina(&modulo, &azione, &riga);
            }
        });
        // Il volume del PC nella vista PC (M2, Task 7, solo Windows): il thread `audio` parte alla
        // prima apertura (lo stato arriva prima che si scelga la vista PC) e il pannello nuovo
        // prende l'ultimo stato letto.
        #[cfg(windows)]
        {
            self.avvia_volume();
            self.scrivi_volume(p);
            // La rotella sul pulsante del volume della barra segue la stessa impostazione di
            // quella sulla pillola.
            p.set_rotella_volume(self.casa.con_config(|c| c.volume_rotella));
            p.on_imposta_volume(|v| {
                if let Some(ponte) = ponte() {
                    ponte.imposta_volume(v);
                }
            });
            p.on_muto_volume(|si| {
                if let Some(ponte) = ponte() {
                    ponte.muto_volume(si);
                }
            });
        }
        // Wi-Fi e Bluetooth nella pagina PC (M2, Task 8, solo Windows): le radio nascono entrando
        // nella vista PC (`vista_pc`); qui i comandi delle righe.
        #[cfg(windows)]
        {
            p.on_commuta_radio(|tipo, acceso| {
                if let Some(ponte) = ponte() {
                    ponte.commuta_radio(connessioni::tipo_da(tipo), acceso);
                }
            });
            p.on_apri_pagina(|i| apri_pagina(connessioni::pagina_da(i)));
        }
    }

    /// Dove si apre il pannello da un clic sulla pillola (`attivita_visibile`: la pillola sta
    /// mostrando l'attività di Claude). Una voce allargata porta alla pagina del suo modulo (Claude
    /// → Agenti, Casa, PC); un permesso porta alla pagina Agenti con la riga della sua sessione e
    /// il permesso nel dettaglio (decisione di Paolo 5: lì si legge il comando e la sessione prima
    /// di decidere), e se la sua sessione non ha una riga nella Coda col testo intero; il puntino
    /// apre la Coda; l'attività di Claude, Agenti; a riposo la Panoramica.
    pub fn apertura_dalla_pillola(&self, attivita_visibile: bool) -> DallaPillola {
        let (allargata, puntino) = match self.mostrato.borrow().as_ref() {
            Some(v) => (v.voce_allargata().map(str::to_owned), v.aspetto == AspettoPillola::Puntino),
            None => (None, false),
        };
        if let Some(id) = allargata {
            let attese = self.attese.borrow();
            return match attese.voce(&id) {
                // Il permesso lo dice l'identità della voce, non i suoi pulsanti (senza risposta dalla
                // pillola il permesso ha solo «Apri il terminale»). Con la riga della sua sessione:
                // Agenti; senza (ripiego): la sua posizione nella Coda.
                Some(v) if v.fonte == Fonte::Claude && v.id.starts_with(crate::claude::PREFISSO_PERMESSO) => {
                    if self.permesso_ha_riga(&id) {
                        DallaPillola::Permesso(id)
                    } else {
                        match attese.ordinate().iter().position(|x| x.id == id) {
                            Some(i) => DallaPillola::PermessoInCoda(i as i32),
                            None => DallaPillola::Panoramica,
                        }
                    }
                }
                Some(v) => match v.fonte {
                    Fonte::Claude => DallaPillola::Vista("agenti"),
                    Fonte::Casa => DallaPillola::Vista("casa"),
                    Fonte::Pc => DallaPillola::Vista("pc"),
                    // Le voci di prova non hanno una pagina: la Coda le mostra.
                    Fonte::Prova => DallaPillola::Vista("coda"),
                },
                None => DallaPillola::Panoramica,
            };
        }
        // Il puntino viene prima dell'attività: dice che qualcosa aspetta (l'attività si vede
        // comunque nella pagina Agenti e nella sua carta).
        if puntino {
            DallaPillola::Vista("coda")
        } else if attivita_visibile {
            DallaPillola::Vista("agenti")
        } else {
            DallaPillola::Panoramica
        }
    }

    /// Il prossimo pannello si apre su questa vista (una volta sola).
    pub fn chiedi_vista(&self, vista: &'static str) {
        self.vista_richiesta.set(Some(vista));
    }

    /// Il prossimo pannello si apre sulla pagina Agenti con la riga del permesso `voce` (id della
    /// voce) scelta e il fuoco nel permesso del dettaglio, su «Nega» (una volta sola). La vista e
    /// la voce si chiedono insieme: una senza l'altra non c'è.
    pub fn chiedi_permesso(&self, voce: &str) {
        self.chiedi_vista("agenti");
        *self.permesso_richiesto.borrow_mut() = Some(voce.to_owned());
    }

    /// Il pannello si è aperto: le voci in attesa sono viste (un'attenzione non allarga più la
    /// pillola) e il pannello prende l'umore.
    pub fn pannello_aperto(&self, p: &Pannello) {
        *self.pannello.borrow_mut() = p.as_weak();
        if let Some(vista) = self.vista_richiesta.take() {
            p.set_vista(vista.into());
            self.vista_claude(vista == "agenti");
            self.vista_coda(vista == "coda");
            self.vista_pagina(vista);
        }
        self.attese.borrow_mut().segna_viste();
        // Il pannello compare: un permesso in attesa non si consente al primo istante, e una guardia
        // ancora in corso si vede.
        self.claude_pannello_compare(p, Instant::now());
        *self.mostrato.borrow_mut() = None; // il pannello nuovo non ha ancora l'umore
        self.aggiorna_umore();
    }

    /// Il pannello si è chiuso e la pillola (di nuovo attiva) sta per ricomparire: riprende
    /// l'umore di adesso.
    pub fn pannello_chiuso(&self) {
        // Una vista (o un permesso) chiesta a un pannello che non si è aperto non resta per il
        // prossimo.
        self.vista_richiesta.set(None);
        self.permesso_richiesto.borrow_mut().take();
        // Né un filtro chiesto dalla carta: la prossima Casa riparte dal filtro iniziale.
        self.filtro_richiesto.borrow_mut().take();
        // Né l'errore dell'ultimo lancio: come il testo di prima, muore col pannello.
        self.errore_lancio.borrow_mut().take();
        // La pagina aperta si chiude: i suoi modelli si svuotano (niente righe in memoria).
        self.vista_pagina("");
        // Il campione del PC e la scelta del quarto posto muoiono col pannello (pre-flight C13): il
        // prossimo pannello aspetta un campione fresco.
        self.dimentica_campione_pc();
        // Le radio si liberano e la rete torna a seguire solo la pillola.
        self.vista_pc(false);
        // La vista Claude non c'è più: il timer torna a seguire solo la pillola.
        self.vista_claude(false);
        // Né la Coda: la prossima apertura riarma «Consenti».
        self.vista_coda(false);
        *self.pannello.borrow_mut() = slint::Weak::default();
        self.modelli_vivi.set(false);
        self.collegamento_aperto.set(false);
        // L'elenco è stato guardato: i «fatto» di Claude Code sono visti.
        self.claude_pannello_chiuso();
        self.ridisegna_pillola();
    }

    /// La pillola (di nuovo attiva) sta per ricomparire: riprende l'aspetto di adesso anche se
    /// non è cambiato da quando si era nascosta.
    pub fn ridisegna_pillola(&self) {
        // La pillola ricompare (anche dalla ritirata): «Consenti» si arma dopo un po'.
        self.claude_superficie_compare(Instant::now());
        *self.mostrato.borrow_mut() = None;
        self.aggiorna_umore();
    }

    /// Colore e riempimento della linea della ritirata: il livello più urgente fra voci e
    /// avanzamento, e la frazione dell'avanzamento (1.0 senza).
    pub fn riempimento_linea(&self) -> (Option<Livello>, f32) {
        let avanzamenti = self.avanzamenti.borrow();
        attenzione::riempimento_linea(livello_massimo(&self.attese.borrow()), avanzamento_mostrato(&avanzamenti))
    }

    /// Mette (o aggiorna) un avanzamento: la linea della ritirata, se si vede, si riempie. Non fa
    /// svegliare altro: nessuna voce, nessun umore, nessuna notifica.
    pub fn metti_avanzamento(&self, a: Avanzamento) {
        {
            let mut tutti = self.avanzamenti.borrow_mut();
            match tutti.iter_mut().find(|x| x.id == a.id) {
                Some(x) => *x = a,
                None => tutti.push(a),
            }
        }
        servizi::colora_linea();
    }

    pub fn togli_avanzamento(&self, id: &str) {
        let prima = self.avanzamenti.borrow().len();
        self.avanzamenti.borrow_mut().retain(|a| a.id != id);
        if self.avanzamenti.borrow().len() != prima {
            servizi::colora_linea();
        }
    }

    /// Da quanto l'utente non tocca tastiera e mouse (controllo da 5 s dei servizi).
    pub fn inattivo(&self, durata: Duration) {
        self.cervello.borrow_mut().ricevi(Segnale::Inattivo(durata), Instant::now());
        self.aggiorna_umore();
    }

    /// Un tocco su una card della Casa: il comando se ce n'è uno (mai su entità non disponibili né
    /// con la casa non connessa: l'interfaccia già non lo permette, qui lo si garantisce).
    pub fn tocca(&self, entity_id: &str) {
        let ora = Instant::now();
        let Some((dominio, servizio, numero, acceso)) = self.avvia_tocco(entity_id, ora) else { return };
        // La pagina e la carta di Casa dicono subito com'è adesso, non com'era prima del tocco.
        self.casa_cambiata();
        if self.traccia_tocco && acceso {
            eprintln!("tocco {entity_id}: icona cambiata nel modello dopo {} µs", ora.elapsed().as_micros());
        }
        self.casa.comando(entity_id, dominio, servizio, numero);
    }

    /// La parte di un tocco che precede i modelli e il comando: stato ottimistico e numero del
    /// tocco (`None` se non c'è nulla da fare). Più tocchi insieme la fanno uno per uno e poi
    /// ricostruiscono i modelli una volta sola.
    fn avvia_tocco(&self, entity_id: &str, ora: Instant) -> Option<(&'static str, &'static str, u64, bool)> {
        if !self.connesso() {
            return None;
        }
        let (comando, acceso) = {
            let menu = self.menu.borrow();
            let menu = menu.as_ref()?;
            let e = entita(menu, entity_id);
            (e.as_ref().and_then(comando_per), e.as_ref().and_then(acceso_di))
        };
        let (dominio, servizio) = comando?;
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
        Some((dominio, servizio, numero, acceso.is_some()))
    }

    /// «Spegni tutte» (la testata della pagina Casa, la carta): un tocco, cioè lo stesso comando e
    /// lo stesso stato ottimistico degli interruttori, per ogni luce accesa che si vede (nessuna API
    /// nuova verso Home Assistant, nessuna conferma: spegnere luci non è un'azione delicata).
    pub fn spegni_tutte(&self) {
        self.spegni_luci(None);
    }

    /// «Spegni la stanza»: come `spegni_tutte`, sulle luci accese della stanza `area` («Altro» è
    /// l'`area_id` vuoto).
    pub fn spegni_stanza(&self, area: &str) {
        self.spegni_luci(Some(area));
    }

    /// Spegne le luci accese di una stanza (`Some(area)`) o di tutta la casa.
    fn spegni_luci(&self, area: Option<&str>) {
        let ora = Instant::now();
        // Le luci accese come si vedono adesso (C1): dal menu e dall'ottimismo, non da un modello
        // dell'interfaccia (che a pagina chiusa è vuoto e può restare indietro).
        let accese: Vec<String> = {
            let menu = self.menu.borrow();
            let ottimismo = self.ottimismo.borrow();
            menu.as_ref().map(|m| luci_accese_in(m, area, &ottimismo).into_iter().map(|p| p.entity_id).collect()).unwrap_or_default()
        };
        let partiti: Vec<_> = accese
            .iter()
            .filter_map(|id| self.avvia_tocco(id, ora).map(|(dominio, servizio, numero, _)| (id, dominio, servizio, numero)))
            .collect();
        // La pagina e la carta una volta sola (non una per luce), poi i comandi: il pulsante
        // sparisce subito, senza aspettare la conferma di Home Assistant.
        if !partiti.is_empty() {
            self.casa_cambiata();
        }
        for (id, dominio, servizio, numero) in partiti {
            self.casa.comando(id, dominio, servizio, numero);
        }
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
            self.casa_cambiata();
        }
        if !scadute.fallite.is_empty() {
            for id in scadute.fallite {
                eprintln!("comando su {id} non confermato entro {} s", crate::ottimismo::ATTESA_CONFERMA.as_secs());
                self.errori.borrow_mut().insert(id, ERRORE_SENZA_CONFERMA.into());
            }
            self.cervello.borrow_mut().ricevi(Segnale::ComandoFallito, ora);
            self.casa_cambiata();
            self.aggiorna_umore();
        }
        self.arma_sveglia_ottimismo();
    }

    // ─── Voci «In attesa» ────────────────────────────────────────────────────

    /// Mette (o aggiorna) una voce. Una voce nuova che lo merita manda la notifica di sistema,
    /// salvo `avvisa` falso (notifiche di HA già presenti alla connessione) o pannello aperto (la
    /// voce compare subito nella riga «In attesa»). Un bloccante nuovo fa uscire dalla ritirata.
    pub fn metti_voce(&self, v: Voce, avvisa: bool) {
        // Novità = voce mai vista o che sale di livello con lo stesso id (19% → 9%): la notifica e
        // il suono valgono come per una voce appena arrivata (`Attese::metti` ne azzera i 20 s).
        let nuova = self.attese.borrow().e_novita(&v);
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

    /// Umore, pillola, riga del pannello e Coda dopo un cambio delle voci.
    fn attese_cambiate(&self) {
        let livello = livello_massimo(&self.attese.borrow());
        self.cervello.borrow_mut().ricevi(Segnale::InAttesa(livello), Instant::now());
        let voci: Vec<VoceAttesaUi> = {
            let non_archiviate = self.non_archiviate.borrow();
            let registro = self.registro.borrow();
            self.attese
                .borrow()
                .ordinate()
                .into_iter()
                .map(|v| voce_attesa_ui(v, &registro, non_archiviate.contains(&v.id)))
                .collect()
        };
        aggiorna_modello(&self.modello_attese, voci);
        // Le voci alzano il tono delle carte dei loro moduli (e la luce dell'isola).
        self.moduli_livelli();
        // I permessi in attesa sono voci: la carta di Agenti li dice come la pagina («1 ti aspetta»).
        self.moduli_aggiorna("agenti");
        self.claude_riarma(Instant::now());
        servizi::colora_linea();
        if let Some(p) = self.pannello.borrow().upgrade() {
            self.scrivi_moduli_in_attesa(&p);
        }
        // Passa anche da `aggiorna_claude_pillola`, che riscrive la pagina Agenti aperta: la riga
        // col permesso tiene giusto l'indice della sua voce (`attesa`) anche quando le voci cambiano
        // posto senza che le sessioni cambino (`la_riga_col_permesso_segue_il_posto_della_voce`).
        self.aggiorna_umore();
    }

    /// Da quanti moduli vengono le voci in attesa (la testata della Coda) e quante ne dice «N in
    /// attesa ›» della riga alta (nessuna se la prima, bloccante, ti aspetta al posto
    /// dell'orologio): scritti solo se cambiano.
    fn scrivi_moduli_in_attesa(&self, p: &Pannello) {
        let (mut moduli, voci, voce_mostrata) = {
            let a = self.attese.borrow();
            let v = a.ordinate();
            let moduli: Vec<&'static str> = v.iter().map(|v| v.fonte.modulo()).collect();
            (moduli, v.len(), v.first().is_some_and(|x| x.livello == Livello::Bloccante))
        };
        moduli.sort_unstable();
        moduli.dedup();
        let n = moduli.len() as i32;
        if p.get_moduli_in_attesa() != n {
            p.set_moduli_in_attesa(n);
        }
        crate::pannello::scrivi_in_attesa(p, voci, voce_mostrata);
        self.scrivi_fascia(p);
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
        } else if voce == ID_BATTERIA && pulsante == PULSANTE_HO_CAPITO {
            // Zitta fino alla ricarica: la carta Batteria della pagina PC (tinta «Guarda» a 20 % o
            // meno) e la carta del PC nella Panoramica continuano a dire com'è la batteria.
            self.batteria.borrow_mut().zittisci();
            self.togli_voce(voce);
        } else if voce.starts_with(PREFISSO_BT) && pulsante == PULSANTE_HO_CAPITO {
            // Il dispositivo è già segnato come avvisato oggi: non torna fino a domani.
            self.togli_voce(voce);
        } else if voce.starts_with(PREFISSO_CLAUDE) {
            con_claude::azione(self, voce, pulsante);
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

    fn batteria(&self, lettura: &LetturaBatteria) {
        let presente = self.attese.borrow().voce(ID_BATTERIA).map(|v| v.livello);
        let voce = self.batteria.borrow_mut().osserva(lettura, presente, Instant::now());
        match voce {
            // Cambia solo il livello (titolo o dettaglio uguali non ridisegnano): metti_voce
            // avvisa solo se la voce è nuova.
            Some(v) => self.metti_voce(v, true),
            None => self.togli_voce(ID_BATTERIA),
        }
    }

    /// `--prova-avanzamento <s>`: un avanzamento da 0 a 1 in `s` secondi, aggiornato una volta al
    /// secondo; sparisce alla fine. Il timer vive finché dura la prova.
    pub fn prova_avanzamento(&self, secondi: u64) {
        const ID: &str = "prova.avanzamento";
        let inizio = Instant::now();
        let durata = Duration::from_secs(secondi);
        self.metti_avanzamento(Avanzamento { id: ID.into(), frazione: 0.0, livello: Livello::Informazione });
        self.prova_avanzamento.start(slint::TimerMode::Repeated, Duration::from_secs(1), move || {
            let Some(p) = ponte() else { return };
            let trascorso = inizio.elapsed();
            if trascorso >= durata {
                p.prova_avanzamento.stop();
                p.togli_avanzamento(ID);
            } else {
                let frazione = trascorso.as_secs_f32() / durata.as_secs_f32();
                p.metti_avanzamento(Avanzamento { id: ID.into(), frazione, livello: Livello::Informazione });
            }
        });
    }

    /// `--prova-avviso <livello>`.
    pub fn prova_avviso(&self, livello: Livello, lungo: bool) {
        self.metti_voce(attenzione::voce_di_prova(livello, lungo, Instant::now()), true);
    }
}

// ─── Wi-Fi, Bluetooth e rete (M2, Task 8, solo Windows) ──────────────────────

/// Gli stati delle radio letti dal thread `radio`: solo accodati al thread dell'interfaccia.
#[cfg(windows)]
fn consegna_radio(stati: Vec<radio::StatoRadio>) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(p) = ponte() {
            p.radio_ricevute(stati);
        }
    });
}

/// L'esito di un cambio (dal thread `radio`).
#[cfg(windows)]
fn consegna_esito(tipo: radio::TipoRadio, esito: radio::EsitoRadio) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(p) = ponte() {
            p.esito_radio(tipo, esito);
        }
    });
}

/// La rete letta dal thread `rete`.
#[cfg(windows)]
fn consegna_rete(stato: radio::StatoRete) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(p) = ponte() {
            p.rete_ricevuta(stato);
        }
    });
}

/// Il nome della rete (dal thread `rete`, solo a vista PC aperta): mai nei log.
#[cfg(windows)]
fn consegna_nome_rete(nome: Option<String>) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(p) = ponte() {
            p.nome_rete_ricevuto(nome);
        }
    });
}

/// Una pagina di Windows (righe della vista PC, o dopo un rifiuto): se non si apre, solo il log.
#[cfg(windows)]
fn apri_pagina(pagina: crate::sistema::browser::PaginaWindows) {
    if let Err(e) = crate::sistema::browser::apri_pagina(pagina) {
        eprintln!("pagina di Windows {pagina:?} non aperta: {e}");
    }
}

impl Ponte {
    /// La vista PC si vede (vero) o non più (falso: altra vista, pannello chiuso). Le radio
    /// nascono entrandoci e si liberano lasciandola; la rete si osserva anche per lei, col nome.
    /// Fuori da Windows non fa nulla.
    pub fn vista_pc(&self, si: bool) {
        #[cfg(windows)]
        {
            if self.vista_pc.replace(si) == si {
                return;
            }
            if si {
                *self.radio.borrow_mut() = Some(radio::Radio::avvia(consegna_radio));
                // Una lettura sola per ingresso, su un thread di lavoro: niente timer a vista aperta.
                batterie_bt::leggi_una_volta(|letture| {
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(p) = ponte() {
                            p.cuffie_lette(&letture);
                        }
                    });
                });
            } else {
                // `Drop` ferma il thread `radio`: toglie i gestori e libera le radio.
                self.radio.borrow_mut().take();
                self.dimentica_radio();
            }
            self.aggiorna_lati();
        }
        #[cfg(not(windows))]
        let _ = si;
    }
}

#[cfg(windows)]
impl Ponte {
    /// `OsservaRete` acceso solo se serve (`radio::osserva_rete_acceso`): lato «Rete» scelto e
    /// pillola visibile e sveglia (`pillola_attiva`), o vista PC aperta. Il nome della rete solo a
    /// vista PC aperta.
    fn aggiorna_rete(&self, pillola_attiva: bool) {
        let vista_pc = self.vista_pc.get();
        let acceso = radio::osserva_rete_acceso(sorgenti_lati::bisogni_scelti().rete, pillola_attiva, vista_pc);
        let mut osserva = self.osserva_rete.borrow_mut();
        match (acceso, osserva.is_some()) {
            (true, false) => *osserva = Some(radio::OsservaRete::avvia(consegna_rete, consegna_nome_rete)),
            (false, true) => {
                // `Drop` ferma il thread `rete` e toglie il gestore.
                osserva.take();
            }
            _ => {}
        }
        if let Some(o) = osserva.as_ref() {
            o.chiedi_nome(vista_pc);
        }
    }

    /// La rete letta: il lato «Rete» della pillola (la pagina PC mostra il nome e il traffico).
    fn rete_ricevuta(&self, stato: radio::StatoRete) {
        if self.osserva_rete.borrow().is_none() {
            return;
        }
        sorgenti_lati::rete(stato);
    }

    /// Il nome della rete Wi-Fi, solo a vista PC aperta (mai nella pillola né nei log): la riga
    /// del Wi-Fi.
    fn nome_rete_ricevuto(&self, nome: Option<String>) {
        if !self.vista_pc.get() {
            return;
        }
        *self.nome_rete.borrow_mut() = nome;
        self.scrivi_pagina_pc();
    }

    /// Lo stato vero delle radio: il valore voluto da un tocco lascia il posto a lui.
    fn radio_ricevute(&self, stati: Vec<radio::StatoRadio>) {
        // Arrivate dopo che la vista PC è stata lasciata: nessuno le guarda.
        if self.radio.borrow().is_none() {
            return;
        }
        *self.stati_radio.borrow_mut() = Some(stati);
        self.radio_volute.set([None; 2]);
        self.scrivi_pagina_pc();
    }

    /// Le radio non più seguite (vista PC lasciata, pannello chiuso): stato, valori voluti, rifiuto,
    /// cuffie e nome della rete si dimenticano; la pagina, se aperta, torna a «Lettura in corso».
    pub(super) fn dimentica_radio(&self) {
        *self.stati_radio.borrow_mut() = None;
        self.radio_volute.set([None; 2]);
        self.errore_radio.set(None);
        *self.cuffie.borrow_mut() = None;
        *self.nome_rete.borrow_mut() = None;
        self.scrivi_pagina_pc();
    }

    /// Un tocco su Wi-Fi o Bluetooth (clic, Invio, Narrator): la riga mostra subito il valore
    /// voluto e il rifiuto di prima sparisce; poi il comando al thread delle radio.
    fn commuta_radio(&self, tipo: radio::TipoRadio, acceso: bool) {
        self.radio_voluta(tipo, acceso);
        match self.radio.borrow().as_ref() {
            Some(r) => r.commuta(tipo, acceso, consegna_esito),
            None => self.esito_radio(tipo, radio::EsitoRadio::Errore),
        }
    }

    /// Il valore voluto di una radio, mostrato subito finché non arriva lo stato vero.
    pub(super) fn radio_voluta(&self, tipo: radio::TipoRadio, acceso: bool) {
        let mut volute = self.radio_volute.get();
        volute[indice_radio(tipo)] = Some(acceso);
        self.radio_volute.set(volute);
        self.errore_radio.set(None);
        self.scrivi_pagina_pc();
    }

    /// Un rifiuto: la pagina di Windows giusta si apre (vedi `esito_radio_senza_aprire`).
    fn esito_radio(&self, tipo: radio::TipoRadio, esito: radio::EsitoRadio) {
        if let Some(pagina) = self.esito_radio_senza_aprire(tipo, esito) {
            apri_pagina(pagina);
        }
    }

    /// L'esito di un cambio senza aprire nulla (i test lo chiamano così: nessuna pagina di Windows
    /// vera). Un rifiuto mette il testo sulla riga della radio che ha fallito e la riporta allo
    /// stato vero (l'ultimo letto); restituisce la pagina di Windows da aprire. Un esito arrivato
    /// dopo aver lasciato la vista PC (radio non più seguite) si ignora.
    pub(super) fn esito_radio_senza_aprire(&self, tipo: radio::TipoRadio, esito: radio::EsitoRadio) -> Option<crate::sistema::browser::PaginaWindows> {
        let seguite = self.stati_radio.borrow().is_some();
        let pagina = connessioni::esito(seguite, tipo, esito)?;
        let mut volute = self.radio_volute.get();
        volute[indice_radio(tipo)] = None;
        self.radio_volute.set(volute);
        self.errore_radio.set(Some(match tipo {
            radio::TipoRadio::WiFi => crate::moduli::pagina_pc::WIFI,
            radio::TipoRadio::Bluetooth => crate::moduli::pagina_pc::BLUETOOTH,
        }));
        self.scrivi_pagina_pc();
        Some(pagina)
    }

    /// Presente e acceso di una radio come li mostra la riga: `None` se non ancora lette; acceso è
    /// il valore voluto da un tocco, se c'è, finché non arriva lo stato vero.
    pub(super) fn radio_mostrata(&self, tipo: radio::TipoRadio) -> Option<(bool, bool)> {
        let stati = self.stati_radio.borrow();
        let stato = stati.as_ref()?.iter().find(|s| s.tipo == tipo);
        let voluto = self.radio_volute.get()[indice_radio(tipo)];
        Some(match stato {
            Some(s) => (true, voluto.unwrap_or(s.acceso)),
            None => (false, false),
        })
    }
}

/// Il posto di una radio in `radio_volute`.
#[cfg(windows)]
fn indice_radio(tipo: radio::TipoRadio) -> usize {
    match tipo {
        radio::TipoRadio::WiFi => 0,
        radio::TipoRadio::Bluetooth => 1,
    }
}

// ─── Le batterie dei dispositivi Bluetooth (M2, Task 9, solo Windows) ────────

/// La lettura periodica del thread `batterie-bt`: solo accodata al thread dell'interfaccia.
/// `false` se il ciclo eventi non c'è più (il thread allora finisce).
#[cfg(windows)]
fn consegna_bt(letture: Vec<batterie_bt::LetturaBt>) -> bool {
    slint::invoke_from_event_loop(move || {
        if let Some(p) = ponte() {
            p.bt_lette(&letture);
        }
    })
    .is_ok()
}

#[cfg(windows)]
impl Ponte {
    fn accendi_bt(&self) {
        let mut s = self.sorveglia_bt.borrow_mut();
        if s.is_none() {
            *s = Some(batterie_bt::SorveglianzaBt::avvia(consegna_bt));
        }
    }

    /// Impostazioni › Isola: l'avviso acceso o spento, salvato e subito in vigore. Spento il thread
    /// si ferma e le voci dei dispositivi scompaiono.
    fn cambia_avvisi_bt(&self, si: bool) {
        self.casa.aggiorna_config(|c| c.avvisi_bt = si);
        if si {
            self.accendi_bt();
        } else {
            // `Drop` fa finire il thread `batterie-bt`.
            self.sorveglia_bt.borrow_mut().take();
            self.togli_voci_bt(|_| true);
        }
    }

    fn togli_voci_bt(&self, da_togliere: impl Fn(&str) -> bool) {
        let ids: Vec<String> = self
            .attese
            .borrow()
            .ordinate()
            .iter()
            .filter(|v| v.id.starts_with(PREFISSO_BT) && da_togliere(&v.id))
            .map(|v| v.id.clone())
            .collect();
        for id in ids {
            self.togli_voce(&id);
        }
    }

    /// Una lettura periodica: via le voci non più valide, avviso (e data salvata) per chi è
    /// scarico e non è stato avvisato oggi. Dopo lo spegnimento dell'interruttore si ignora.
    fn bt_lette(&self, letture: &[batterie_bt::LetturaBt]) {
        if self.sorveglia_bt.borrow().is_none() {
            return;
        }
        let presenti: Vec<String> = self
            .attese
            .borrow()
            .ordinate()
            .iter()
            .filter(|v| v.id.starts_with(PREFISSO_BT))
            .map(|v| v.id.clone())
            .collect();
        let via = batterie_bt::da_togliere(letture, &presenti);
        self.togli_voci_bt(|id| via.iter().any(|x| x == id));
        let oggi = chrono::Local::now().format("%Y-%m-%d").to_string();
        let nuovi = self.casa.con_config(|c| batterie_bt::da_avvisare(letture, &c.bt_avvisati, &oggi));
        if nuovi.is_empty() {
            return;
        }
        let ora = Instant::now();
        for l in &nuovi {
            self.metti_voce(batterie_bt::voce_bt(l, ora), true);
        }
        self.casa.aggiorna_config(|c| {
            for l in &nuovi {
                c.bt_avvisati.insert(l.impronta.clone(), oggi.clone());
            }
            batterie_bt::pota_avvisati(&mut c.bt_avvisati, &oggi);
        });
    }

    /// La lettura fatta entrando nella vista PC: le cuffie nella riga del Bluetooth. Arrivata dopo
    /// aver lasciato la vista, nessuno la guarda.
    fn cuffie_lette(&self, letture: &[batterie_bt::LetturaBt]) {
        if !self.vista_pc.get() {
            return;
        }
        *self.cuffie.borrow_mut() = batterie_bt::riga_cuffie(letture);
        self.scrivi_pagina_pc();
    }
}

// ─── Il volume del PC (M2, Task 7, solo Windows) ─────────────────────────────

/// Uno stato letto dal thread `audio` o da `OnNotify` (thread di Windows): solo accodato al
/// thread dell'interfaccia, nessun altro lavoro qui.
#[cfg(windows)]
fn consegna_volume(s: Option<StatoVolume>) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(p) = ponte() {
            p.volume_ricevuto(s);
        }
    });
}

/// La barra del volume della pillola (`None`: di nuovo gli occhi), al punto percentuale intero;
/// ogni proprietà si riscrive solo se cambia.
#[cfg(windows)]
fn scrivi_barra(pl: &Pillola, s: Option<StatoVolume>) {
    let (valore, muto) = match s {
        Some(s) => (volume::percento(s.livello) as f32 / 100.0, s.muto),
        None => (-1.0, false),
    };
    if pl.get_volume_mostrato() != valore {
        pl.set_volume_mostrato(valore);
    }
    if pl.get_volume_muto() != muto {
        pl.set_volume_muto(muto);
    }
}

#[cfg(windows)]
impl Ponte {
    fn audio(&self) -> &volume::Audio {
        self.audio.get_or_init(|| volume::Audio::avvia(consegna_volume))
    }

    /// Il thread `audio` parte (se non è già partito): il primo stato arriva da solo.
    fn avvia_volume(&self) {
        self.audio();
    }

    /// La rotella sopra la pillola: il volume cambia e la barra compare al posto degli occhi per
    /// `BARRA_PER` dall'ultimo cambio. Il primo scatto avvia il thread `audio` e non fa altro
    /// (lo stato non c'è ancora).
    fn rotella_volume(&self, delta: f32) {
        let audio = self.audio();
        let mut rotella = self.rotella.get();
        let nuovo = volume::muovi(&mut rotella, self.volume.get(), delta, audio);
        self.rotella.set(rotella);
        let Some(s) = nuovo else { return };
        self.volume.set(Some(s));
        let Some(pl) = self.pillola.upgrade() else { return };
        scrivi_barra(&pl, Some(s));
        let debole = self.pillola.clone();
        self.sveglia_barra.start(slint::TimerMode::SingleShot, volume::BARRA_PER, move || {
            if let Some(pl) = debole.upgrade() {
                scrivi_barra(&pl, None);
            }
        });
    }

    /// Un nuovo stato da Windows (anche dai tasti del volume o da un altro dispositivo): la barra
    /// già visibile lo segue (ma non compare: la mostra solo la rotella) e il pannello aperto lo
    /// prende. Uguale all'ultimo: nulla.
    fn volume_ricevuto(&self, s: Option<StatoVolume>) {
        if self.volume.replace(s) == s {
            return;
        }
        if let Some(pl) = self.pillola.upgrade().filter(|pl| pl.get_volume_mostrato() >= 0.0) {
            if s.is_none() {
                self.sveglia_barra.stop();
            }
            scrivi_barra(&pl, s);
        }
        if let Some(p) = self.pannello.borrow().upgrade() {
            self.scrivi_volume(&p);
        }
    }

    /// Il volume del pannello (pulsante della barra, cursore della pagina PC): c'è solo con
    /// un'uscita audio; ogni proprietà si riscrive solo se cambia (il livello al punto intero). La
    /// pagina PC, se aperta, riscrive la riga del volume e l'azione «Muto» della testata.
    fn scrivi_volume(&self, p: &Pannello) {
        let s = self.volume.get();
        if p.get_volume_disponibile() != s.is_some() {
            p.set_volume_disponibile(s.is_some());
        }
        if let Some(s) = s {
            if volume::percento(p.get_volume()) != volume::percento(s.livello) {
                p.set_volume(volume::percento(s.livello) as f32 / 100.0);
            }
            if p.get_volume_muto() != s.muto {
                p.set_volume_muto(s.muto);
            }
        }
        self.scrivi_pagina_pc();
    }

    /// Un livello scelto col cursore o con la tastiera (il pannello lo mostra già).
    fn imposta_volume(&self, livello: f32) {
        self.imposta_volume_con(livello, self.audio());
    }

    /// `imposta_volume` con chi comanda il volume passato da fuori (un finto nei test: il volume
    /// vero del PC non si tocca).
    fn imposta_volume_con(&self, livello: f32, audio: &dyn Volume) {
        // Alzando il livello da muto l'audio torna, come con la rotella (una regola sola).
        let nuovo = volume::imposta_da_cursore(self.volume.get(), livello, audio);
        self.mostra_volume_comandato(nuovo);
    }

    /// Il muto chiesto dalla vista PC (altoparlante, Invio, Narrator): lo stato mostrato cambia
    /// subito, perché l'eco di Windows di un nostro comando si scarta (B1).
    fn muto_volume(&self, si: bool) {
        self.muto_volume_con(si, self.audio());
    }

    /// `muto_volume` con chi comanda il volume passato da fuori (un finto nei test).
    fn muto_volume_con(&self, si: bool, audio: &dyn Volume) {
        let nuovo = volume::muta(self.volume.get(), si, audio);
        self.mostra_volume_comandato(nuovo);
    }

    /// Lo stato ottimistico dopo un nostro comando (cursore o muto): lo prendono la riga della
    /// vista PC e, se è visibile, la barra della pillola. `None` (nessuno stato letto): si
    /// riscrive solo il pannello con quello che c'è.
    fn mostra_volume_comandato(&self, nuovo: Option<StatoVolume>) {
        if nuovo.is_some() {
            self.volume.set(nuovo);
            if let Some(pl) = self.pillola.upgrade().filter(|pl| pl.get_volume_mostrato() >= 0.0) {
                scrivi_barra(&pl, nuovo);
            }
        }
        if let Some(p) = self.pannello.borrow().upgrade() {
            self.scrivi_volume(&p);
        }
    }

    /// Impostazioni › Isola: la rotella sulla pillola (e sul pulsante del volume del pannello)
    /// accesa o spenta, salvata e subito in vigore.
    fn cambia_volume_rotella(&self, si: bool) {
        self.casa.aggiorna_config(|c| c.volume_rotella = si);
        if let Some(pl) = self.pillola.upgrade() {
            pl.set_rotella_attiva(si);
        }
        if let Some(p) = self.pannello.borrow().upgrade().filter(|p| p.get_rotella_volume() != si) {
            p.set_rotella_volume(si);
        }
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

    /// Una luce accesa con la luminosità in percento (come la dà il mapper).
    pub fn luce_con_luminosita(id: &str, percento: f32) -> AppEntity {
        let AppEntity::Light(mut l) = luce(id, true) else { unreachable!() };
        l.brightness = Some(percento);
        l.supports_brightness = true;
        AppEntity::Light(l)
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

    /// Un `MenuData` di prova; i sensori di preferiti e stanze sono anche fra i sensori numerici,
    /// come li mette il mapper di casa-core.
    pub fn menu(preferiti: Vec<AppEntity>, stanze: Vec<Vec<AppEntity>>) -> MenuData {
        let mut numerici: Vec<SensoreNumerico> = Vec::new();
        for e in preferiti.iter().chain(stanze.iter().flatten()) {
            if let AppEntity::Sensor(s) = e {
                if numerici.iter().all(|n| n.entity_id != s.base.entity_id) {
                    numerici.push(SensoreNumerico {
                        entity_id: s.base.entity_id.clone(),
                        nome: s.base.name.clone(),
                        valore: casa_core::ha::mapper::valore_numerico(&s.value).filter(|_| s.base.is_available),
                        unita: s.unit.clone().unwrap_or_default(),
                    });
                }
            }
        }
        MenuData {
            sensori_numerici: Arc::new(numerici),
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
            meteo: None,
            meteo_elenco: vec![],
        }
    }

    /// Il mapper mette le scene in `menu.scenes`, mai nelle stanze: il comando di un tocco su una
    /// scena (la fila «SCENE») le trova lì.
    #[test]
    fn il_comando_di_una_scena_si_trova_fra_le_scene() {
        let mut m = menu(vec![], vec![vec![luce("light.a", true)]]);
        let AppEntity::Scene(s) = scena("scene.cinema") else { unreachable!() };
        m.scenes = vec![s];
        assert_eq!(preferito(&entita(&m, "scene.cinema").expect("la scena")).tipo, Tipo::Scena);
        assert_eq!(entita(&m, "scene.cinema").as_ref().and_then(comando_per), Some(("scene", "turn_on")));
        assert!(entita(&m, "light.zzz").is_none());
    }

    #[test]
    fn sensore_mostra_valore_con_unita() {
        let p: Vec<Preferito> =
            [sensore("sensor.t", "21.5", Some("°C")), sensore("sensor.n", "7", None), serratura("lock.porta")].iter().map(preferito).collect();
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
        assert!(!preferito(&luce_non_disponibile("light.giardino")).disponibile);
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
            luminosita: None,
        }
    }

    /// Un ponte vero sul backend di prova di Slint, con un `CasaService` che non si connette (i
    /// comandi tornano «non connesso» a un ascoltatore muto), la casa data per connessa e una
    /// luce spenta fra i preferiti.
    /// Lascia il ponte del thread anche se il test fallisce: distrutto dal thread_local a fine
    /// thread, il `CasaService` resterebbe appeso nella sua `join`.
    pub(crate) struct Spegni;
    impl Drop for Spegni {
        fn drop(&mut self) {
            spegni();
        }
    }

    pub(crate) fn ponte_di_prova() -> (Spegni, Rc<Ponte>, crate::Pillola, tempfile::TempDir) {
        ponte_di_prova_con(None)
    }

    /// Come `ponte_di_prova`, con un `assistente.json` già scritto nella cartella (`config`).
    pub(crate) fn ponte_di_prova_con(config: Option<&str>) -> (Spegni, Rc<Ponte>, crate::Pillola, tempfile::TempDir) {
        use casa_core::config::ConfigStore;
        use casa_core::segreti::MemoriaSegreti;
        crate::test_ui::piattaforma_di_prova();
        let cartella = tempfile::tempdir().unwrap();
        if let Some(testo) = config {
            std::fs::write(cartella.path().join("assistente.json"), testo).unwrap();
        }
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
        // La pagina Casa aperta: i suoi modelli seguono i dati.
        p.vista_pagina("casa");
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

    /// Task 14h, ridisegno F3: a pagina Casa chiusa (pannello chiuso o su un'altra pagina) i dati
    /// della casa non ricostruiscono i modelli della pagina (nessuno li guarda); all'apertura si
    /// fanno una volta coi dati di adesso, e da lì seguono i dati finché la pagina resta aperta.
    #[test]
    fn a_pagina_chiusa_i_modelli_della_casa_aspettano_l_apertura() {
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
        assert_eq!((conta(Contatore::ui_dati), conta(Contatore::ui_ricostruzioni)), (5, 0), "pagina chiusa: nulla da ricostruire");
        assert_eq!(p.comandi_card.row_count(), 0);
        // Il pannello aperto su un'altra pagina: ancora nulla.
        p.apri_modelli();
        p.ricevi(dati(true));
        assert_eq!(conta(Contatore::ui_ricostruzioni), 0, "pannello aperto sulla Panoramica: nulla");
        // La pagina si apre: i modelli si fanno una volta, coi dati di adesso (l'ultimo: accesa).
        p.vista_pagina("casa");
        assert_eq!(conta(Contatore::ui_ricostruzioni), 1);
        assert_eq!(p.comandi_card.row_data(0).unwrap().stato, "Accesa");
        p.ricevi(dati(false));
        assert_eq!(conta(Contatore::ui_ricostruzioni), 2, "aperta: seguono i dati");
        assert_eq!(p.comandi_card.row_data(0).unwrap().stato, "Spenta");
        // Chiusa di nuovo (il pannello): i modelli si svuotano e i dati non ricostruiscono nulla.
        p.pannello_chiuso();
        assert_eq!(p.comandi_card.row_count(), 0, "nessuna riga in memoria");
        p.ricevi(dati(true));
        assert_eq!(conta(Contatore::ui_ricostruzioni), 2);
        // Il pannello vero, appena creato, riempito e aperto sulla Casa, ha i dati di adesso.
        let pannello = crate::Pannello::new().unwrap();
        p.riempi(&pannello);
        p.chiedi_vista("casa");
        p.pannello_aperto(&pannello);
        assert_eq!(conta(Contatore::ui_ricostruzioni), 3);
        assert_eq!(pannello.get_comandi_card().row_data(0).unwrap().stato, "Accesa", "il pannello nuovo ha i dati di adesso");
    }

    /// R1-13: a casa non configurata la pagina Casa è un invito («Collega la casa», «Collega»);
    /// non connessa non vuol dire non configurata (coi dati la pagina resta); configurata e
    /// connessa l'invito sparisce.
    #[test]
    fn casa_non_configurata_e_un_invito() {
        use crate::TipoStatoPaginaUi;
        let (_spegni, p, _pillola, _c) = ponte_di_prova();
        let pannello = crate::Pannello::new().unwrap();
        p.pannello_aperto(&pannello);
        p.riempi(&pannello);
        p.vista_pagina("casa");
        let invito = |pannello: &crate::Pannello| pannello.get_comandi_stato().frase == "Collega la casa";
        p.ricevi(CasaEvento::Stato(StatoConnessione::NonConfigurato));
        assert!(invito(&pannello));
        assert_eq!(pannello.get_comandi_stato().azione_id, crate::moduli::casa::COLLEGA);
        p.ricevi(CasaEvento::Stato(StatoConnessione::Disconnesso));
        assert!(!invito(&pannello), "non connessa non vuol dire non configurata");
        p.ricevi(CasaEvento::Stato(StatoConnessione::NonConfigurato));
        p.ricevi(CasaEvento::Stato(StatoConnessione::Connesso));
        assert!(!invito(&pannello));
        // Connessa, con i dati (qui senza stanze: nessun dispositivo da mostrare).
        assert_eq!(pannello.get_comandi_stato().tipo, TipoStatoPaginaUi::Vuoto);
        assert_eq!(pannello.get_comandi_stato().frase, "Nessun dispositivo da mostrare");
    }

    /// Il ponte di prova con la pagina Casa aperta (i suoi modelli seguono i dati): la luce
    /// preferita è la prima card.
    fn ponte_con_la_casa() -> (Spegni, Rc<Ponte>, crate::Pillola, tempfile::TempDir) {
        let (spegni, p, pillola, cartella) = ponte_di_prova();
        p.vista_pagina("casa");
        (spegni, p, pillola, cartella)
    }

    fn stato_mostrato(p: &Ponte) -> String {
        p.comandi_card.row_data(0).unwrap().stato.to_string()
    }

    /// Giro di correzioni 1, rilievo 1: due tocchi rapidi con `toggle` riportano la luce com'era.
    /// HA non ripubblica nulla (il `MenuData` è uguale), quindi nessuna conferma arriva: alla
    /// scadenza non deve comparire un falso «non ha confermato».
    #[test]
    fn doppio_tocco_senza_falso_errore() {
        let (_spegni, p, _pillola, _c) = ponte_con_la_casa();
        let t = Instant::now();
        p.tocca("light.a");
        assert_eq!(stato_mostrato(&p), "Accesa", "il primo tocco si vede subito");
        p.tocca("light.a");
        assert_eq!(stato_mostrato(&p), "Spenta");
        p.ottimismo_scaduto(t + Duration::from_secs(6));
        assert_eq!(stato_mostrato(&p), "Spenta", "nessun errore: la luce è com'era chiesto");
        assert!(p.errori.borrow().is_empty());
    }

    /// Rilievo 8: il cablaggio della scadenza nel ponte. Senza conferma, a 5 s la card torna vera
    /// con l'errore; la conferma che arriva prima chiude l'attesa senza errore.
    #[test]
    fn tocco_senza_conferma_torna_vero_con_l_errore() {
        let (_spegni, p, _pillola, _c) = ponte_con_la_casa();
        let t = Instant::now();
        p.tocca("light.a");
        p.ottimismo_scaduto(t + Duration::from_secs(4));
        assert_eq!(stato_mostrato(&p), "Accesa", "prima dei 5 s resta l'attesa");
        p.ottimismo_scaduto(t + Duration::from_secs(6));
        assert_eq!(stato_mostrato(&p), "Non riuscito");
        assert_eq!(p.errori.borrow().get("light.a").map(String::as_str), Some(ERRORE_SENZA_CONFERMA));
        // Un tocco nuovo toglie l'errore; la conferma lo chiude.
        p.tocca("light.a");
        assert_eq!(stato_mostrato(&p), "Accesa");
        p.ricevi(CasaEvento::Dati(Arc::new(menu(vec![luce("light.a", true)], vec![]))));
        p.ottimismo_scaduto(Instant::now() + Duration::from_secs(60));
        assert_eq!(stato_mostrato(&p), "Accesa");
        assert!(p.errori.borrow().is_empty());
    }

    /// Le luci accese e disponibili delle stanze, nell'ordine delle stanze.
    #[test]
    fn luci_accese_dai_dati() {
        let m = menu(
            vec![luce("light.f", true)],
            vec![
                vec![luce("light.a", true), luce("light.b", false), interruttore("switch.c", true)],
                vec![luce("light.d", true), luce_non_disponibile("light.e")],
            ],
        );
        let ids: Vec<String> = luci_accese_ora(&m, &Ottimismo::default()).into_iter().map(|p| p.entity_id).collect();
        assert_eq!(ids, ["light.a", "light.d"], "né spente, né non disponibili, né interruttori");
    }

    /// Le card della pagina Casa (aperta) sul filtro «Luci accese».
    fn id_luci(p: &Ponte) -> Vec<String> {
        p.comandi_card.iter().map(|l| l.id.to_string()).collect()
    }

    /// La pagina Casa aperta sul filtro «Luci accese».
    fn ponte_sulle_luci() -> (Spegni, Rc<Ponte>, crate::Pillola, tempfile::TempDir) {
        let (spegni, p, pillola, cartella) = ponte_con_la_casa();
        p.filtro_pagina("casa", crate::moduli::pagina_casa::FILTRO_LUCI);
        (spegni, p, pillola, cartella)
    }

    /// «Luci accese» segue i dati: una luce spenta (qui o altrove) esce dall'elenco, una accesa
    /// entra, e a zero resta l'elenco vuoto (con la sua nota).
    #[test]
    fn la_casa_filtrata_segue_gli_aggiornamenti() {
        let (_spegni, p, _pillola, _c) = ponte_sulle_luci();
        let pannello = crate::Pannello::new().unwrap();
        p.riempi(&pannello);
        assert!(id_luci(&p).is_empty(), "ponte_di_prova: l'unica luce è spenta");
        let dati = |a: bool, b: bool, c: bool| {
            CasaEvento::Dati(Arc::new(menu(vec![], vec![vec![luce("light.a", a), luce("light.b", b)], vec![luce("light.c", c)]])))
        };
        p.ricevi(dati(true, true, false));
        assert_eq!(id_luci(&p), ["light.a", "light.b"]);
        assert_eq!(pannello.get_comandi_card().row_count(), 2, "il pannello vede lo stesso modello");
        assert_eq!(p.comandi_card.row_data(0).unwrap().stato, "Accesa");
        // Spenta altrove: sparisce; una nuova accesa: compare.
        p.ricevi(dati(true, false, true));
        assert_eq!(id_luci(&p), ["light.a", "light.c"]);
        // Spenta da qui: esce subito (stato ottimistico), prima di ogni conferma.
        p.tocca("light.a");
        assert_eq!(id_luci(&p), ["light.c"]);
        // Il comando non confermato in tempo la riporta, con l'errore.
        p.ottimismo_scaduto(Instant::now() + Duration::from_secs(6));
        assert_eq!(id_luci(&p), ["light.a", "light.c"]);
        let a = p.comandi_card.row_data(0).unwrap();
        assert!(a.errore && a.stato == "Non riuscito", "{a:?}");
        // Tutte spente: elenco vuoto.
        p.ricevi(dati(false, false, false));
        assert!(id_luci(&p).is_empty());
    }

    /// «Spegni tutte»: lo stesso percorso di un tocco (stato ottimistico e comando col suo numero)
    /// per ogni luce accesa che si vede, e nessun altro dispositivo.
    #[test]
    fn spegni_tutte_tocca_ogni_luce_accesa() {
        let (_spegni, p, _pillola, _c) = ponte_sulle_luci();
        p.ricevi(CasaEvento::Dati(Arc::new(menu(
            vec![],
            vec![vec![luce("light.a", true), luce("light.b", false), interruttore("switch.c", true)], vec![luce("light.d", true)]],
        ))));
        assert_eq!(id_luci(&p), ["light.a", "light.d"]);
        p.spegni_tutte();
        assert!(id_luci(&p).is_empty(), "tutte spente subito");
        let (na, nd) = (numero_attesa(&p, "light.a"), numero_attesa(&p, "light.d"));
        assert_ne!(na, nd, "un comando per luce, ciascuno col suo numero");
        assert!(p.ottimismo.borrow().numero("light.b").is_none(), "la luce già spenta non riceve comandi");
        assert!(p.ottimismo.borrow().numero("switch.c").is_none(), "gli interruttori non sono luci");
        // Una seconda pressione non riaccende niente: non c'è più nulla di acceso da spegnere.
        p.spegni_tutte();
        assert_eq!(numero_attesa(&p, "light.a"), na);
        // Una conferma negativa di una luce la riporta fra le accese, con l'errore.
        p.ricevi(CasaEvento::Esito { entity_id: "light.d".into(), numero: nd, ok: false, errore: Some("guasto".into()) });
        assert_eq!(id_luci(&p), ["light.d"]);
        // Senza connessione non parte nulla.
        p.ricevi(CasaEvento::Stato(StatoConnessione::Disconnesso));
        p.spegni_tutte();
        assert!(p.ottimismo.borrow().numero("light.d").is_none(), "casa non connessa: nessun comando");
    }

    /// Le righe della carta di Casa nel modello dei moduli: (titolo, c'è l'azione «Spegni tutte»).
    fn carta_casa(p: &Ponte) -> (String, bool) {
        let riga = p.modello_moduli.iter().find(|m| m.id == "casa").expect("la carta di Casa");
        (riga.titolo.to_string(), riga.con_azione)
    }

    /// Review Focus 3 (C1): «Spegni tutte» spegne subito la vista, prima di ogni conferma di HA.
    /// Con due luci accese, dopo il tocco le luci accese di adesso sono zero e la carta di Casa non
    /// ha più l'azione, mentre il menu (i dati veri) ne ha ancora due; una seconda pressione non
    /// manda comandi. La conferma negativa di una luce la riporta, con la sua azione.
    #[test]
    fn spegni_tutte_conta_con_l_ottimismo() {
        use crate::moduli::casa;
        let (_spegni, p, _pillola, _c) = ponte_di_prova();
        p.ricevi(CasaEvento::Dati(Arc::new(menu(vec![], vec![vec![luce("light.a", true), luce("light.b", true), luce("light.c", false)]]))));
        let accese = |p: &Ponte| luci_accese_ora(p.menu.borrow().as_ref().unwrap(), &p.ottimismo.borrow()).len();
        assert_eq!(accese(&p), 2);
        assert_eq!(carta_casa(&p), ("2 luci".into(), true));

        p.spegni_tutte();
        // I dati veri dicono ancora due accese: la conferma di HA non è arrivata.
        let veri = luci_accese_ora(p.menu.borrow().as_ref().unwrap(), &Ottimismo::default()).len();
        assert_eq!(veri, 2, "il menu è quello di prima");
        assert_eq!(accese(&p), 0, "la vista ottimistica: nessuna luce accesa");
        assert_eq!(carta_casa(&p), ("Luci spente".into(), false), "la carta non ha più «Spegni tutte»");
        let r = casa::riassunto(&p.stato.borrow(), p.menu.borrow().as_deref(), accese(&p), None);
        assert!(r.azione.is_none() && r.r1.fatto == "Luci spente", "{r:?}");

        // Un secondo «Spegni tutte» (il clic nello stesso punto) non manda comandi.
        let (na, nb) = (numero_attesa(&p, "light.a"), numero_attesa(&p, "light.b"));
        assert_ne!(na, nb);
        p.spegni_tutte();
        assert_eq!((numero_attesa(&p, "light.a"), numero_attesa(&p, "light.b")), (na, nb), "nessun tocco nuovo");
        assert!(p.ottimismo.borrow().numero("light.c").is_none(), "la luce già spenta non riceve comandi");

        // Una conferma negativa riporta la luce: la vista e la carta tornano a dirlo.
        p.ricevi(CasaEvento::Esito { entity_id: "light.b".into(), numero: nb, ok: false, errore: Some("guasto".into()) });
        assert_eq!(accese(&p), 1);
        assert_eq!(carta_casa(&p), ("1 luce".into(), true), "la carta segue anche un comando fallito");
        // Il comando non confermato in tempo riporta anche l'altra.
        p.ottimismo_scaduto(Instant::now() + Duration::from_secs(6));
        assert_eq!(accese(&p), 2);
        assert_eq!(carta_casa(&p), ("2 luci".into(), true));
    }

    /// Una luce che si accende con un tocco entra subito nella vista (stesso stato ottimistico), e
    /// «Spegni tutte» la spegne anche a pannello chiuso (non legge più il modello dell'interfaccia).
    #[test]
    fn la_vista_ottimistica_vale_anche_per_chi_si_accende_e_a_pannello_chiuso() {
        let (_spegni, p, _pillola, _c) = ponte_di_prova();
        p.ricevi(CasaEvento::Dati(Arc::new(menu(vec![], vec![vec![luce("light.a", false), luce("light.b", true)]]))));
        p.pannello_chiuso();
        p.tocca("light.a");
        let ids = |p: &Ponte| -> Vec<String> {
            luci_accese_ora(p.menu.borrow().as_ref().unwrap(), &p.ottimismo.borrow()).into_iter().map(|l| l.entity_id).collect()
        };
        assert_eq!(ids(&p), ["light.a", "light.b"], "la luce appena accesa si conta subito");
        p.spegni_tutte();
        assert!(ids(&p).is_empty(), "il pannello è chiuso e i modelli sono vuoti: il conto non dipende da loro");
        assert!(p.ottimismo.borrow().numero("light.b").is_some());
    }

    /// La luminosità arriva dal mapper (già in percento) e una luce che si mostra spenta non la tiene.
    #[test]
    fn luminosita_della_luce_accesa() {
        let m = menu(vec![], vec![vec![luce_con_luminosita("light.a", 79.6), luce("light.b", true), luce_con_luminosita("light.c", 100.0)]]);
        let l: Vec<Option<u8>> = luci_accese_ora(&m, &Ottimismo::default()).iter().map(|p| p.luminosita).collect();
        assert_eq!(l, [Some(80), None, Some(100)]);
        let mut o = Ottimismo::<bool>::default();
        o.tocca("light.a", false, &true, Instant::now());
        let a = con_ottimismo(preferito(&m.rooms[0].entities[0]), &o);
        assert_eq!((a.acceso, a.luminosita), (false, None), "spenta: niente luminosità");
        assert_eq!(preferito(&interruttore("switch.s", true)).luminosita, None);
    }

    /// La pagina Casa è del pannello aperto: chiuso, i modelli non tengono righe; riaperta, tornano.
    #[test]
    fn le_luci_non_restano_in_memoria_a_pannello_chiuso() {
        let (_spegni, p, _pillola, _c) = ponte_sulle_luci();
        p.ricevi(CasaEvento::Dati(Arc::new(menu(vec![], vec![vec![luce("light.a", true)]]))));
        assert_eq!(id_luci(&p), ["light.a"]);
        p.pannello_chiuso();
        assert!(id_luci(&p).is_empty());
        assert_eq!((p.comandi_filtri.row_count(), p.comandi_scene.row_count()), (0, 0));
        p.vista_pagina("casa");
        p.filtro_pagina("casa", crate::moduli::pagina_casa::FILTRO_LUCI);
        assert_eq!(id_luci(&p), ["light.a"], "alla riapertura l'elenco si rifà");
    }

    /// Il numero dell'attesa armata dal tocco più recente su `id`.
    fn numero_attesa(p: &Ponte, id: &str) -> u64 {
        p.ottimismo.borrow().numero(id).expect("un'attesa armata")
    }

    /// Rilievo 8: un esito negativo toglie l'attesa e mostra l'errore.
    #[test]
    fn esito_negativo_toglie_l_attesa() {
        let (_spegni, p, _pillola, _c) = ponte_con_la_casa();
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
        let (_spegni, p, _pillola, _c) = ponte_con_la_casa();
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

    /// Task 14e, ridisegno F3: le card di una stanza rispondono al tocco come i preferiti (stesso
    /// comando e stesso stato ottimistico); una stanza nascosta torna al filtro iniziale.
    #[test]
    fn le_card_della_stanza_rispondono_come_i_preferiti() {
        let (_spegni, p, _pillola, _c) = ponte_con_la_casa();
        p.ricevi(CasaEvento::Dati(Arc::new(menu(vec![luce("light.a", false)], vec![vec![interruttore("switch.s", false)]]))));
        p.filtro_pagina("casa", "a0");
        assert_eq!(stato_mostrato(&p), "Spento");
        p.tocca("switch.s");
        assert_eq!(stato_mostrato(&p), "Acceso", "stato ottimistico anche nella stanza");
        assert_eq!(p.ottimismo.borrow().atteso("switch.s"), Some(&true));
        // La stanza non c'è più (tutto nascosto): il filtro iniziale, i preferiti.
        p.ricevi(CasaEvento::Dati(Arc::new(menu(vec![luce("light.a", false)], vec![]))));
        assert_eq!(p.filtro_casa.borrow().as_deref(), Some(crate::moduli::pagina_casa::FILTRO_PREFERITI));
        assert_eq!(p.comandi_card.row_data(0).map(|c| c.id.to_string()).as_deref(), Some("light.a"));
    }

    /// Task 14e: un pulsante di Impostazioni › Dispositivi cambia la configurazione (quella che il
    /// servizio salva) e rifà subito l'elenco; l'icona cambia anche i filtri della pagina Casa.
    #[test]
    fn la_scheda_dispositivi_cambia_la_configurazione_e_l_elenco_subito() {
        use casa_core::types::{StanzaCatalogo, VoceCatalogo};
        let (_spegni, p, _pillola, _c) = ponte_con_la_casa();
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
        let filtro_a0 = p.comandi_filtri.iter().find(|f| f.id == "a0").expect("il filtro della stanza a0");
        assert_eq!(filtro_a0.icona, 36, "i filtri della pagina prendono l'icona subito");
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

    /// Ridisegno F1, Task 5: ogni apertura riuscita porta `lanci` a +1 (e scrive l'ora) nella
    /// configurazione, in memoria e sul disco; se il browser non parte il conteggio non sale.
    #[test]
    fn aprire_conta_e_salva() {
        let (_spegni, p, _pillola, cartella) = ponte_di_prova();
        p.su_apri_url(|_| Ok(()));
        assert_eq!(p.salva_collegamento("", "NAS", "nas.esempio.lan"), "");
        assert_eq!(p.casa.config().collegamenti[0].lanci, 0);
        let prima = crate::orologio::adesso_unix();
        assert!(p.apri_collegamento("nas"));
        let c = p.casa.config().collegamenti[0].clone();
        assert_eq!(c.lanci, 1);
        assert!(c.ultimo_lancio >= prima, "l'ora dell'apertura: {} contro {prima}", c.ultimo_lancio);
        // Il servizio salva sul suo thread: il file arriva poco dopo, con il conteggio.
        let file = cartella.path().join("assistente.json");
        let fine = Instant::now() + Duration::from_secs(5);
        loop {
            let testo = std::fs::read_to_string(&file).unwrap_or_default();
            if testo.contains("\"lanci\": 1") && testo.contains("\"ultimoLancio\"") {
                break;
            }
            assert!(Instant::now() < fine, "conteggio non salvato: {testo}");
            std::thread::sleep(Duration::from_millis(20));
        }
        // Un secondo giro (pannello richiuso) conta ancora.
        p.pannello_chiuso();
        assert!(p.apri_collegamento("nas"));
        assert_eq!(p.casa.config().collegamenti[0].lanci, 2);
        // Il browser non parte: il conteggio non sale.
        p.pannello_chiuso();
        p.su_apri_url(|_| Err("ShellExecuteW: 2".into()));
        assert!(!p.apri_collegamento("nas"));
        assert_eq!(p.casa.config().collegamenti[0].lanci, 2);
        // Un id che non c'è non conta e non scrive nulla.
        assert!(!p.apri_collegamento("sparito"));
        assert_eq!(p.casa.config().collegamenti.len(), 1);
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
        assert_eq!(preferito(&serratura).valore, "Chiusa");
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
        let card = crate::moduli::pagina_casa::card(&mostrata, true);
        assert_eq!((card.stato.as_str(), card.acceso), ("Accesa", true));
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
    fn errori_in_frasi_semplici() {
        let riga = frase_errore;
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
    }

    /// Il percorso del Ponte: la voce del 19% nasce informazione, «Ho capito» la toglie e la
    /// zittisce (18% niente), la ricarica la riattiva. (Niente 9%: l'attenzione manderebbe un
    /// toast vero; la salita di livello è coperta in `attenzione.rs`.)
    #[test]
    fn ho_capito_sulla_batteria_toglie_e_zittisce() {
        use crate::pc_stats::{LetturaBatteria, StatoBatteria};
        let (_spegni, p, _pillola, _cartella) = ponte_di_prova();
        let lettura = |percentuale, stato| LetturaBatteria { percentuale, stato };
        let livello = || p.attese.borrow().voce(ID_BATTERIA).map(|v| v.livello);
        p.batteria(&lettura(19.0, StatoBatteria::Scarica));
        assert_eq!(livello(), Some(Livello::Informazione));

        p.azione(ID_BATTERIA, PULSANTE_HO_CAPITO);
        assert_eq!(livello(), None, "«Ho capito» toglie la voce");
        p.batteria(&lettura(18.0, StatoBatteria::Scarica));
        assert_eq!(livello(), None, "zittita fino alla ricarica");

        p.batteria(&lettura(18.0, StatoBatteria::InCarica));
        p.batteria(&lettura(19.0, StatoBatteria::Scarica));
        assert_eq!(livello(), Some(Livello::Informazione), "dopo la ricarica torna");
    }

    /// «Ho capito» sulla voce di un dispositivo Bluetooth la toglie (senza toast: `avvisa` falso).
    #[test]
    fn ho_capito_sulla_voce_bluetooth_la_toglie() {
        use crate::sistema::batterie_bt::{voce_bt, LetturaBt};
        let (_spegni, p, _pillola, _cartella) = ponte_di_prova();
        let l = LetturaBt { impronta: "0123456789abcdef".into(), nome: "Cuffie finte".into(), percentuale: 18, connesso: true };
        let voce = voce_bt(&l, Instant::now());
        let id = voce.id.clone();
        p.metti_voce(voce, false);
        assert!(p.attese.borrow().contiene(&id));
        p.azione(&id, PULSANTE_HO_CAPITO);
        assert!(!p.attese.borrow().contiene(&id));
    }

    #[test]
    fn sotto_il_titolo_la_parola_del_livello() {
        let r = Registro::nuovo(&[]);
        let v = attenzione::voce_di_prova(Livello::Attenzione, false, Instant::now());
        assert_eq!(voce_attesa_ui(&v, &r, false).sotto, "Guarda presto · Prova");
        assert_eq!(voce_attesa_ui(&v, &r, true).sotto, "Guarda presto · Prova · archiviazione non riuscita");
        // Il nome interno resta per il colore.
        assert_eq!(voce_attesa_ui(&v, &r, false).livello, crate::LivelloUi::Attenzione);
    }

    /// Ridisegno F1, Task 3: sotto la voce il nome del modulo a cui appartiene («Agenti», non più
    /// «Claude Code»), mai l'identificativo.
    #[test]
    fn sotto_il_titolo_il_nome_per_l_utente_non_l_identificativo() {
        use crate::attenzione::Fonte;
        let r = Registro::nuovo(&[]);
        let nomi: Vec<_> = [Fonte::Casa, Fonte::Pc, Fonte::Claude, Fonte::Prova].iter().map(|f| r.nome(f.modulo())).collect();
        assert_eq!(nomi, ["Casa", "PC", "Agenti", "Prova"]);
        let mut v = attenzione::voce_di_prova(Livello::Informazione, false, Instant::now());
        v.fonte = Fonte::Pc;
        assert_eq!(voce_attesa_ui(&v, &r, false).sotto, "Quando vuoi · PC");
        v.fonte = Fonte::Claude;
        v.livello = Livello::Bloccante;
        assert_eq!(voce_attesa_ui(&v, &r, false).sotto, "Aspetta te · Agenti");
    }

    /// Ridisegno F2, Task 2: la voce del pannello porta i dati divisi del permesso, con l'ora fissa
    /// della richiesta («alle 14:32»); una voce senza dati ha il `PermessoUi` vuoto.
    #[test]
    fn voce_attesa_ui_col_permesso() {
        let r = Registro::nuovo(&[]);
        let e = claude_hook::evento::EventoHook {
            sessione: "a".into(),
            tipo: claude_hook::evento::TipoEvento::PermissionRequest,
            cartella: "progetto".into(),
            strumento: Some("PowerShell".into()),
            notifica: None,
            errore: None,
            riassunto: Some("npm test".into()),
        };
        let alle = 1_790_000_000;
        let v = crate::claude::voce_permesso(1, &e, "Sistemare il restyle", Instant::now(), alle);
        let dati = v.permesso.clone().unwrap();
        let ui = voce_attesa_ui(&v, &r, false).permesso;
        assert_eq!(ui.sessione, dati.sessione.as_str());
        assert_eq!(ui.strumento, dati.strumento.as_str());
        assert_eq!(ui.comando, dati.comando.as_str());
        assert_eq!(ui.cartella, dati.cartella.as_str());
        assert_eq!((ui.sessione.as_str(), ui.cartella.as_str()), ("Sistemare il restyle", "progetto"));
        // L'ora locale della richiesta, fissa: «alle HH:MM».
        let ora = chrono::DateTime::from_timestamp(alle, 0).unwrap().with_timezone(&chrono::Local).format("%H:%M");
        assert_eq!(ui.chiesto, format!("alle {ora}").as_str());
        assert!(ui.chiesto.starts_with("alle "));
        // Una voce senza dati di permesso (la voce di prova, le altre fonti): tutto vuoto.
        let prova = attenzione::voce_di_prova(Livello::Bloccante, true, Instant::now());
        assert_eq!(voce_attesa_ui(&prova, &r, false).permesso, crate::PermessoUi::default());
    }

    fn nuvoloso(temperatura: Option<f64>) -> Meteo {
        Meteo { entity_id: "weather.casa".into(), condizione: "cloudy".into(), temperatura, unita: "°C".into() }
    }

    /// La riga alta della Panoramica: data e temperatura; per Narrator anche la condizione.
    #[test]
    fn riga_alta_con_e_senza_meteo() {
        let m = nuvoloso(Some(9.0));
        let (testo, nome) = riga_sinistra("VEN 13 NOV", Some(&m));
        assert_eq!(testo, "VEN 13 NOV · 9°");
        assert_eq!(nome, "venerdì 13 novembre, 9 gradi, nuvoloso");
        let (testo, nome) = riga_sinistra("VEN 13 NOV", None);
        assert_eq!((testo.as_str(), nome.as_str()), ("VEN 13 NOV", "venerdì 13 novembre"));
    }

    #[test]
    fn meteo_senza_temperatura_niente_gradi() {
        let m = nuvoloso(None);
        let (testo, nome) = riga_sinistra("VEN 13 NOV", Some(&m));
        assert_eq!(testo, "VEN 13 NOV");
        assert!(!testo.contains('°') && !nome.contains("gradi"), "{nome}");
        // Una condizione sconosciuta non lascia il vuoto nel nome.
        let strano = Meteo { condizione: "boh".into(), ..m };
        assert_eq!(riga_sinistra("VEN 13 NOV", Some(&strano)).1, "venerdì 13 novembre");
        let solo_gradi = Meteo { temperatura: Some(-3.0), ..strano };
        assert_eq!(riga_sinistra("VEN 13 NOV", Some(&solo_gradi)).0, "VEN 13 NOV · -3°");
    }

    // ─── M2, Task 4: Impostazioni › Isola ───

    fn potenza(id: &str, valore: &str) -> AppEntity {
        AppEntity::Sensor(SensorEntity {
            base: BaseEntity { state: valore.into(), ..base(id, true) },
            value: valore.into(),
            unit: Some("W".into()),
            device_class: Some("power".into()),
        })
    }

    #[test]
    fn isola_scelte_e_indici() {
        let mut m = menu(vec![sensore("sensor.t", "15", Some("°C")), potenza("sensor.fv", "1500")], vec![]);
        m.meteo = Some(nuvoloso(Some(9.0)));
        m.meteo_elenco = vec![VoceCatalogo { entity_id: "weather.casa".into(), name: "Casa".into(), kind: "weather".into() }];
        let config = Config {
            lato_sinistro: "cpu".into(),
            lato_destro: "sensore:sensor.via".into(),
            meteo_entita: Some("weather.casa".into()),
            ..Config::default()
        };
        let i = isola(&config, Some(&m), false);
        let nomi: Vec<&str> = i.lati.iter().map(|(_, n)| n.as_str()).collect();
        // «Rete» (Task 8) solo su Windows.
        let mut attesi = vec!["Niente", "Ora", "CPU", "RAM"];
        if cfg!(windows) {
            attesi.push("Rete");
        }
        attesi.extend(["Temperatura esterna", "Temperatura · Nome sensor.t", "sensor.via (non disponibile)"]);
        assert_eq!(nomi, attesi, "per i lati solo le temperature; la scelta sparita resta in fondo");
        assert_eq!((i.sinistro, i.destro), (2, attesi.len() as i32 - 1));
        assert_eq!(i.meteo.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>(), ["Automatico", "Casa"]);
        assert_eq!(i.indice_meteo, 1);
        // Senza dati della casa: niente meteo né sensori fra le scelte.
        let vuota = isola(&Config::default(), None, true);
        assert!(vuota.lati.iter().any(|(l, _)| *l == Lato::Batteria), "col PC a batteria");
        assert!(!vuota.lati.iter().any(|(l, _)| *l == Lato::Meteo));
    }

    /// Scegliere un lato dalle Impostazioni lo salva e lo porta subito nella pillola.
    #[test]
    fn scelte_dell_isola_salvate() {
        let (_spegni, p, pillola, _dir) = ponte_di_prova();
        *p.isola.borrow_mut() = isola(&p.casa.config(), None, false);
        let cpu = p.isola.borrow().lati.iter().position(|(l, _)| *l == Lato::Cpu).unwrap() as i32;
        p.scegli_lato(1, cpu);
        assert_eq!(p.casa.config().lato_destro, "cpu");
        assert_eq!(pillola.get_lato_destro().testo, "CPU —", "subito nella pillola, la CPU arriva dopo");
        sorgenti_lati::spegni();
    }

    /// Ridisegno F1, Task 9: Impostazioni › Panoramica. Di serie la Griglia; «Orologio» (1) si
    /// salva come «orologio» senza toccare nient'altro della configurazione, la finestra delle
    /// Impostazioni lo mostra scelto e il pannello che si apre dopo è in Orologio. La fascia del
    /// pannello nuovo dice «Niente ti aspetta» (i moduli hanno già il loro primo riassunto).
    #[test]
    fn panoramica_scelta_salvata() {
        let (_spegni, p, _pillola, _dir) = ponte_di_prova();
        p.casa.aggiorna_config(|c| c.lato_destro = "cpu".into());
        let prima = p.casa.config();
        let pannello = crate::Pannello::new().unwrap();
        p.riempi(&pannello);
        assert_eq!(pannello.get_panoramica(), 0, "di serie la Griglia");
        let f = crate::FinestraImpostazioni::new().unwrap();
        p.riempi_isola(&f);
        assert_eq!(f.get_panoramica_scelta(), 0);

        p.scegli_panoramica(1);
        let dopo = p.casa.config();
        assert_eq!(dopo.panoramica, "orologio");
        assert_eq!(Config { panoramica: String::new(), ..dopo.clone() }, prima, "nient'altro cambia");
        p.riempi_isola(&f);
        assert_eq!(f.get_panoramica_scelta(), 1, "le Impostazioni la mostrano scelta");
        let nuovo = crate::Pannello::new().unwrap();
        p.riempi(&nuovo);
        assert_eq!(nuovo.get_panoramica(), 1, "il pannello seguente si apre in Orologio");
        assert_eq!(nuovo.get_fascia_testo(), "Niente ti aspetta");

        // Di nuovo la Griglia: «griglia» salvato, il pannello seguente in Griglia.
        p.scegli_panoramica(0);
        assert_eq!(p.casa.config().panoramica, "griglia");
        let terzo = crate::Pannello::new().unwrap();
        p.riempi(&terzo);
        assert_eq!(terzo.get_panoramica(), 0);
    }

    /// Il finto del volume per i test del ponte: annota i comandi, il volume vero del PC non si
    /// tocca.
    #[cfg(windows)]
    #[derive(Default)]
    struct VolumeFinto {
        comandi: RefCell<Vec<String>>,
    }

    #[cfg(windows)]
    impl Volume for VolumeFinto {
        fn imposta(&self, livello: f32) {
            self.comandi.borrow_mut().push(format!("imposta {}", volume::percento(livello)));
        }

        fn muto(&self, si: bool) {
            self.comandi.borrow_mut().push(format!("muto {si}"));
        }
    }

    /// B1 della revisione finale di M2: l'altoparlante della vista PC mette il muto e l'icona lo
    /// mostra subito (l'eco di Windows si scarta, quindi deve farlo il ponte). Poi il muto si
    /// toglie con l'altoparlante e con il cursore alzato; la rotella legge lo stesso stato.
    /// Prima della correzione il ponte mandava solo il comando: `volume` restava «non muto».
    #[cfg(windows)]
    #[test]
    fn il_muto_della_vista_pc_si_vede_e_si_toglie() {
        let (_spegni, p, _pillola, _dir) = ponte_di_prova();
        let pannello = crate::Pannello::new().unwrap();
        p.pannello_aperto(&pannello);
        p.volume.set(Some(StatoVolume { livello: 0.40, muto: false }));
        p.scrivi_volume(&pannello);
        assert!(pannello.get_volume_disponibile() && !pannello.get_volume_muto());

        let finto = VolumeFinto::default();
        p.muto_volume_con(true, &finto);
        assert_eq!(*finto.comandi.borrow(), ["muto true"]);
        assert_eq!(p.volume.get(), Some(StatoVolume { livello: 0.40, muto: true }));
        assert!(pannello.get_volume_muto(), "l'icona mostra subito il muto");

        // Di nuovo l'altoparlante (il pannello chiede `!volume-muto`): il muto si toglie.
        let finto = VolumeFinto::default();
        p.muto_volume_con(!pannello.get_volume_muto(), &finto);
        assert_eq!(*finto.comandi.borrow(), ["muto false"]);
        assert!(!pannello.get_volume_muto());

        // Muto di nuovo, poi il cursore alzato lo toglie.
        p.muto_volume_con(true, &VolumeFinto::default());
        let finto = VolumeFinto::default();
        p.imposta_volume_con(0.42, &finto);
        assert_eq!(*finto.comandi.borrow(), ["imposta 42", "muto false"]);
        assert!(!pannello.get_volume_muto());
        assert_eq!(volume::percento(pannello.get_volume()), 42);

        // Muto di nuovo: la rotella in su (la stessa regola, sullo stato del ponte) lo toglie.
        p.muto_volume_con(true, &VolumeFinto::default());
        let finto = VolumeFinto::default();
        let su = volume::muovi(&mut volume::Rotella::default(), p.volume.get(), 60.0, &finto).expect("uno scatto");
        assert!(!su.muto);
        assert_eq!(*finto.comandi.borrow(), ["imposta 44", "muto false"]);
    }
}
