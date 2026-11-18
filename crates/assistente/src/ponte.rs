//! Ponte casa → interfaccia: dai dati di `casa-core` ai preferiti del pannello, e dai tocchi
//! sui preferiti ai comandi verso Home Assistant. Tiene anche le voci «In attesa» (notifiche di
//! Home Assistant, casa irraggiungibile, batteria, prova) e decide umore e aspetto della pillola.

use crate::attenzione::{
    self, avanzamento_mostrato, livello_massimo, notifica_sistema, prossimo_cambio_aspetto, Attese, Avanzamento, Fonte, Livello, SorveglianzaCasa,
    SorveglianzaBatteria, Voce, ID_BATTERIA, ID_CASA_OFFLINE, ID_CASA_TOKEN, PREFISSO_BT, PREFISSO_NOTIFICA, PREFISSO_SISTEMA,
    PULSANTE_ARCHIVIA, PULSANTE_APRI, PULSANTE_HO_CAPITO, PULSANTE_IMPOSTAZIONI, PULSANTE_RIPROVA,
};
use crate::claude::PREFISSO_CLAUDE;
use crate::lati::{self, Lato};
use crate::layout::Layout;
use crate::orologio;
use crate::pc_stats::{Fonti, LetturaBatteria};
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
use crate::{
    AmbienteUi, CollegamentoUi, FinestraImpostazioni, FonteQuadranteUi, GenereUi, Pannello, Pillola, PreferitoUi, QuadranteUi,
    RigaDispositivoUi, RigaSessioneUi, SceltaQuadranteUi, SensoreUi, StanzaUi, TipoRigaUi, TonoUi, VoceAttesaUi,
};
use casa_core::config::Config;
use casa_core::diagnosi::Contatore;
use claude_hook::sessioni::Sessioni;
use casa_core::eventi::{Ascoltatore, CasaEvento, StatoConnessione};
use casa_core::segreti::Segreti;
use casa_core::servizio::CasaService;
use casa_core::types::{AppEntity, BaseEntity, MenuData, Meteo, SensorEntity, TestConnectionResult, GRUPPO_SCENE};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(crate) mod con_claude;

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

/// La parte sinistra della riga alta del pannello (nella Panoramica): `(testo, nome per Narrator)`.
/// Quadranti: data e temperatura («VEN 13 NOV · 9°», senza meteo solo la data). Orologio: la data è
/// già grande al centro, quindi solo il meteo («9° · Nuvoloso»; senza meteo niente). Una
/// temperatura che manca non lascia «°»; una condizione sconosciuta non lascia il vuoto.
pub fn riga_sinistra(layout: Layout, data: &str, meteo: Option<&Meteo>) -> (String, String) {
    let temperatura = meteo.and_then(orologio::temperatura_breve);
    let condizione = meteo.map(|m| orologio::condizione_italiana(&m.condizione)).filter(|c| !c.is_empty());
    // Per chi ascolta: «9 gradi», senza il simbolo.
    let gradi = temperatura.as_deref().map(|t| format!("{} gradi", t.trim_end_matches('°')));
    let dati = |sep: &str, voci: [Option<&str>; 2]| voci.into_iter().flatten().collect::<Vec<_>>().join(sep);
    match layout {
        Layout::Quadranti => {
            let testo = dati(" · ", [Some(data), temperatura.as_deref()]);
            let esteso = orologio::data_per_esteso(data).unwrap_or_else(|| data.to_owned());
            let minuscola = condizione.map(str::to_lowercase);
            let nome = [Some(esteso.as_str()), gradi.as_deref(), minuscola.as_deref()].into_iter().flatten().collect::<Vec<_>>().join(", ");
            (testo, nome)
        }
        Layout::Orologio => (
            dati(" · ", [temperatura.as_deref(), condizione]),
            dati(", ", [gradi.as_deref(), condizione.map(|c| c.to_lowercase()).as_deref()]),
        ),
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

// ─── Lati e quadranti (M2, Task 4) ───────────────────────────────────────────

/// Il genere (icona) di un sensore numerico dalla sua unità: termometro, goccia o sensore.
fn genere_di_unita(unita: &str) -> GenereUi {
    match unita.trim() {
        u if u.contains('°') => GenereUi::Termo,
        "%" => GenereUi::Goccia,
        _ => GenereUi::Sensore,
    }
}

/// I quadranti configurabili per il pannello (quelli a «niente» non ci sono). PC e ora li legge
/// l'interfaccia; meteo e sensori (da `MenuData::sensori_numerici`: anche W, kWh e FV, che stanze
/// e preferiti non hanno) hanno qui valore, arco (scala di `lati::scala`, col massimo
/// dell'utente) e nome. Un sensore che non c'è più resta, col trattino.
pub fn quadranti_ui(q: &[Lato], menu: Option<&MenuData>, massimi: &HashMap<String, f64>) -> Vec<QuadranteUi> {
    let pc = |fonte, etichetta: &str| QuadranteUi { fonte, etichetta: etichetta.into(), frazione: -1.0, ..Default::default() };
    q.iter()
        .filter_map(|l| {
            Some(match l {
                Lato::Niente | Lato::Rete => return None,
                Lato::Cpu => pc(FonteQuadranteUi::Cpu, "CPU"),
                Lato::Ram => pc(FonteQuadranteUi::Ram, "RAM"),
                Lato::Disco => pc(FonteQuadranteUi::Disco, "DISCO"),
                Lato::ReteGiu => pc(FonteQuadranteUi::ReteGiu, "RETE ↓"),
                Lato::ReteSu => pc(FonteQuadranteUi::ReteSu, "RETE ↑"),
                Lato::Gpu => pc(FonteQuadranteUi::Gpu, "GPU"),
                Lato::Batteria => pc(FonteQuadranteUi::Batteria, "BATTERIA"),
                Lato::Ora => pc(FonteQuadranteUi::Ora, "ORA"),
                Lato::Meteo => {
                    let meteo = menu.and_then(|m| m.meteo.as_ref());
                    let t = meteo.and_then(|m| m.temperatura).filter(|t| t.is_finite());
                    QuadranteUi {
                        fonte: FonteQuadranteUi::Meteo,
                        etichetta: "METEO".into(),
                        valore: meteo.and_then(orologio::temperatura_breve).unwrap_or_default().into(),
                        frazione: t.map_or(-1.0, |t| lati::frazione(l, meteo.map(|m| m.unita.as_str()), None, t)),
                        dettaglio: meteo.map(|m| orologio::condizione_italiana(&m.condizione)).unwrap_or_default().into(),
                        genere: GenereUi::Casa,
                    }
                }
                Lato::Sensore(id) => {
                    let sensore = menu.and_then(|m| m.sensori_numerici.iter().find(|n| n.entity_id == *id));
                    let (valore, frazione, nome, genere) = match sensore {
                        Some(s) => {
                            let unita = Some(s.unita.as_str()).filter(|u| !u.is_empty());
                            let (valore, frazione) = match s.valore {
                                Some(v) => (
                                    lati::testo_con_unita(v, &s.unita),
                                    lati::frazione(l, unita, massimi.get(&l.come_str()).copied(), v),
                                ),
                                None => ("Non disponibile".to_owned(), -1.0),
                            };
                            (valore, frazione, s.nome.clone(), genere_di_unita(&s.unita))
                        }
                        None => (String::new(), -1.0, "non disponibile".to_owned(), GenereUi::Sensore),
                    };
                    QuadranteUi {
                        fonte: FonteQuadranteUi::Sensore,
                        etichetta: "CASA".into(),
                        valore: valore.into(),
                        frazione,
                        dettaglio: nome.into(),
                        genere,
                    }
                }
            })
        })
        .collect()
}

/// Il nome di una scelta salvata che oggi non è fra quelle disponibili (un sensore sparito, il
/// meteo assente): resta nel selettore, così si vede che cosa è scelto.
fn voce_mancante(l: &Lato) -> String {
    let nome = match l {
        Lato::Sensore(id) => id.clone(),
        // Il nome che la scelta ha nel catalogo completo («Temperatura esterna», «Disco»…).
        altro => lati::scelte(true, &[], true, true, true)
            .into_iter()
            .chain(lati::scelte(true, &[], true, true, false))
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
    pub quadranti: Vec<(Lato, String)>,
    pub scelte_quadranti: Vec<SceltaQuadranteUi>,
}

/// Impostazioni › Isola dalla configurazione e dai dati della casa (anche senza: niente meteo né
/// sensori fra le scelte).
pub fn isola(config: &Config, menu: Option<&MenuData>, batteria: bool) -> Isola {
    let meteo_c = menu.is_some_and(|m| m.meteo.is_some());
    let sensori_lati = menu.map(|m| lati::sensori_disponibili(m, false)).unwrap_or_default();
    let sensori_q = menu.map(|m| lati::sensori_disponibili(m, true)).unwrap_or_default();
    // La rete (Task 8) solo su Windows.
    let mut cat_lati = lati::scelte(meteo_c, &sensori_lati, cfg!(windows), batteria, false);
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

    // La GPU (Task 10) solo su Windows.
    let mut cat_q = lati::scelte(meteo_c, &sensori_q, cfg!(windows), batteria, true);
    let mut scelte_quadranti = Vec::new();
    for l in lati::quadranti(&config.quadranti) {
        let indice = indice_in(&mut cat_q, &l);
        let unita: Option<Option<&str>> = match &l {
            Lato::Sensore(id) => {
                sensori_q.iter().find(|s| &s.entity_id == id).map(|s| Some(s.unita.as_str()).filter(|u| !u.is_empty()))
            }
            _ => None,
        };
        // Il massimo si chiede solo per un sensore che c'è e la cui unità non ha una scala.
        let con_massimo = unita.is_some_and(lati::chiede_massimo);
        let predefinito = match unita.flatten().map(str::trim) {
            Some("W") => lati::testo_massimo(lati::MASSIMO_W),
            Some("kW") => lati::testo_massimo(lati::MASSIMO_KW),
            _ => "automatico".into(),
        };
        let massimo = config.massimi_quadranti.get(&l.come_str()).map(|v| lati::testo_massimo(*v)).unwrap_or_default();
        scelte_quadranti.push(SceltaQuadranteUi {
            indice,
            con_massimo,
            massimo: massimo.into(),
            predefinito: if con_massimo { predefinito.into() } else { SharedString::default() },
        });
    }
    Isola { lati: cat_lati, sinistro, destro, meteo, indice_meteo, quadranti: cat_q, scelte_quadranti }
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
/// `non_archiviata`: l'ultimo «Archivia» non è riuscito (lo dice la riga sotto il titolo).
fn voce_attesa_ui(v: &Voce, non_archiviata: bool) -> VoceAttesaUi {
    let mut sotto = format!("{} · {}", v.livello.parola(), v.fonte.nome());
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
    /// fuori (fuori da Windows, un pulsante di una voce di Claude) la chiede qui.
    #[cfg(any(not(windows), test))]
    vista_richiesta: Cell<Option<&'static str>>,
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
    /// Acceso/spento mostrati subito dopo un tocco, finché Home Assistant non conferma.
    ottimismo: RefCell<Ottimismo<bool>>,
    /// Suona alla scadenza della prossima attesa di conferma (solo se ce ne sono).
    sveglia_ottimismo: slint::Timer,
    /// I quadranti configurabili per il pannello (sopravvive al pannello come gli altri modelli).
    quadranti: Rc<VecModel<QuadranteUi>>,
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
    /// La vista Claude del pannello (Task 9): si vede (pannello aperto su di lei) e il modello delle
    /// sue righe. Chiusa, niente si calcola né si scrive.
    vista_claude: Cell<bool>,
    claude_righe: Rc<VecModel<RigaSessioneUi>>,
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
    voci_quadranti: Rc<VecModel<SharedString>>,
    scelte_quadranti: Rc<VecModel<SceltaQuadranteUi>>,
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
    /// Wi-Fi, Bluetooth e rete (M2, Task 8, solo Windows). `radio`: il thread delle radio, vivo
    /// solo a vista PC aperta; `stati_radio`: l'ultimo elenco letto (lo stato vero a cui torna un
    /// riquadro dopo un rifiuto). `osserva_rete`: acceso solo se serve (`aggiorna_rete`);
    /// `stato_rete`: l'ultima lettura; `nome_rete`: il nome della rete Wi-Fi, solo a vista PC
    /// aperta e mai nei log. `vista_pc`: la vista PC si vede (pannello aperto su di lei).
    #[cfg(windows)]
    radio: RefCell<Option<radio::Radio>>,
    #[cfg(windows)]
    stati_radio: RefCell<Vec<radio::StatoRadio>>,
    #[cfg(windows)]
    osserva_rete: RefCell<Option<radio::OsservaRete>>,
    #[cfg(windows)]
    stato_rete: Cell<Option<radio::StatoRete>>,
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
    let ponte = Rc::new(Ponte {
        casa,
        pillola: pillola.as_weak(),
        pannello: RefCell::new(slint::Weak::default()),
        gestione: RefCell::new(None),
        dettagli_aperti: Cell::new(false),
        dettagli_gen: Cell::new(0),
        dettagli_ultimo_aggiorna: Cell::new(None),
        dettagli_avviso: RefCell::new(None),
        #[cfg(any(not(windows), test))]
        vista_richiesta: Cell::new(None),
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
        ottimismo: RefCell::new(Ottimismo::default()),
        sveglia_ottimismo: slint::Timer::default(),
        traccia_tocco: std::env::var_os("ITSYHOME_TRACCIA_TOCCO").is_some_and(|v| v == "1"),
        quadranti: Rc::new(VecModel::default()),
        pillola_mostrata: Cell::new(true),
        claude_sessioni: RefCell::new(Sessioni::new()),
        claude_ascolto: RefCell::new(None),
        claude_limiti: RefCell::new(None),
        claude_contesto: RefCell::new(HashMap::new()),
        claude_catene: RefCell::new(HashMap::new()),
        claude_ripiego: RefCell::new(std::collections::HashSet::new()),
        sveglia_terminale: slint::Timer::default(),
        vista_claude: Cell::new(false),
        claude_righe: Rc::new(VecModel::default()),
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
        voci_quadranti: Rc::new(VecModel::default()),
        scelte_quadranti: Rc::new(VecModel::default()),
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
        stati_radio: RefCell::new(Vec::new()),
        #[cfg(windows)]
        osserva_rete: RefCell::new(None),
        #[cfg(windows)]
        stato_rete: Cell::new(None),
        #[cfg(windows)]
        nome_rete: RefCell::new(None),
        #[cfg(windows)]
        vista_pc: Cell::new(false),
        #[cfg(windows)]
        sorveglia_bt: RefCell::new(None),
    });
    PONTE.with(|p| *p.borrow_mut() = Some(ponte.clone()));
    // I pulsanti della pillola allargata agiscono sulla voce che sta chiamando; «Apri» (un
    // consenso col testo tagliato) apre il pannello su quella voce, come un clic sulla pillola.
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
                // I lati della pillola leggono meteo e sensori scelti (nessuna lettura in più).
                sorgenti_lati::casa(&menu);
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

    /// Le fonti lente del PC che i quadranti scelti mostrano (per il campionatore del pannello).
    pub fn fonti_dei_quadranti(&self) -> Fonti {
        let q = self.casa.con_config(|c| lati::quadranti(&c.quadranti));
        lati::fonti_dei_quadranti(&q)
    }

    /// I quadranti configurabili dalla configurazione e dagli ultimi dati (solo a pannello aperto,
    /// con gli altri modelli della casa).
    fn ricostruisci_quadranti(&self) {
        let (q, massimi) = self.casa.con_config(|c| (lati::quadranti(&c.quadranti), c.massimi_quadranti.clone()));
        let voci = quadranti_ui(&q, self.menu.borrow().as_deref(), &massimi);
        let n = aggiorna_modello(&self.quadranti, voci);
        self.casa.diagnosi().conta_n(Contatore::ui_righe, n);
    }

    /// Impostazioni › Isola, se la finestra è aperta: riscrive solo ciò che cambia.
    fn riempi_isola(&self, f: &FinestraImpostazioni) {
        let config = self.casa.config();
        let nuova = isola(&config, self.menu.borrow().as_deref(), sorgenti_lati::ha_batteria());
        let nomi = |v: &[(Lato, String)]| v.iter().map(|(_, n)| SharedString::from(n.as_str())).collect::<Vec<_>>();
        aggiorna_modello(&self.voci_lati, nomi(&nuova.lati));
        aggiorna_modello(&self.voci_quadranti, nomi(&nuova.quadranti));
        aggiorna_modello(&self.voci_meteo, nuova.meteo.iter().map(|(_, n)| SharedString::from(n.as_str())).collect());
        aggiorna_modello(&self.scelte_quadranti, nuova.scelte_quadranti.clone());
        if f.get_indice_lato_sinistro() != nuova.sinistro {
            f.set_indice_lato_sinistro(nuova.sinistro);
        }
        if f.get_indice_lato_destro() != nuova.destro {
            f.set_indice_lato_destro(nuova.destro);
        }
        if f.get_indice_meteo() != nuova.indice_meteo {
            f.set_indice_meteo(nuova.indice_meteo);
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

    /// «Meteo da»: 0 automatico, poi le entità meteo.
    fn scegli_meteo(&self, indice: i32) {
        let Some(scelta) = self.isola.borrow().meteo.get(indice.max(0) as usize).map(|(e, _)| e.clone()) else { return };
        self.casa.aggiorna_config(|c| c.meteo_entita = scelta);
        self.aggiorna_pannello();
    }

    /// Un quadrante scelto (posizione 0..2, indice nel catalogo dei quadranti): si salvano sempre
    /// tutti e tre (quelli di M1 diventano espliciti).
    fn scegli_quadrante(&self, posizione: i32, indice: i32) {
        let Some(l) = self.isola.borrow().quadranti.get(indice.max(0) as usize).map(|(l, _)| l.clone()) else { return };
        let posizione = posizione.clamp(0, 2) as usize;
        self.casa.aggiorna_config(|c| {
            let mut q = lati::quadranti(&c.quadranti);
            q[posizione] = l;
            c.quadranti = q.iter().map(Lato::come_str).collect();
        });
        self.ricostruisci_preferiti();
        self.aggiorna_pannello();
    }

    /// Il massimo dell'arco scritto per un quadrante: vuoto = il predefinito. Restituisce l'errore
    /// da mostrare sotto il campo (vuoto se salvato).
    fn cambia_massimo(&self, posizione: i32, testo: &str) -> String {
        let q = self.casa.con_config(|c| lati::quadranti(&c.quadranti));
        let Some(l) = q.get(posizione.clamp(0, 2) as usize) else { return String::new() };
        let chiave = l.come_str();
        match lati::leggi_massimo(testo) {
            Ok(v) => {
                self.casa.aggiorna_config(|c| match v {
                    Some(v) => {
                        c.massimi_quadranti.insert(chiave, v);
                    }
                    None => {
                        c.massimi_quadranti.remove(&chiave);
                    }
                });
                self.ricostruisci_preferiti();
                String::new()
            }
            Err(e) => e.into(),
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
        // I quadranti anche senza dati: quelli del PC si vedono subito.
        self.ricostruisci_quadranti();
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
        // Impostazioni › Isola: lati della pillola, meteo e quadranti.
        f.set_scelte_lati(ModelRc::from(self.voci_lati.clone()));
        f.set_scelte_meteo(ModelRc::from(self.voci_meteo.clone()));
        f.set_scelte_quadranti(ModelRc::from(self.voci_quadranti.clone()));
        f.set_quadranti_scelti(ModelRc::from(self.scelte_quadranti.clone()));
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
        f.on_scegli_quadrante(|posizione, indice| {
            if let Some(p) = ponte() {
                p.scegli_quadrante(posizione, indice);
            }
        });
        f.on_cambia_massimo(|posizione, testo| ponte().map(|p| p.cambia_massimo(posizione, &testo)).unwrap_or_default().into());
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

    /// La parte sinistra della riga alta: data breve e meteo secondo il layout. Si riscrive solo
    /// se cambia (un minuto nuovo, un meteo nuovo, un altro layout); il resto del tempo non tocca
    /// niente e il pannello non ridisegna.
    pub fn aggiorna_riga_sinistra(&self, p: &Pannello) {
        let data = orologio::data_breve(chrono::Local::now().naive_local());
        let menu = self.menu.borrow();
        let meteo = menu.as_deref().and_then(|m| m.meteo.as_ref());
        let (testo, nome) = riga_sinistra(Layout::da_indice(p.get_layout()), &data, meteo);
        if p.get_riga_sinistra() != testo.as_str() {
            p.set_riga_sinistra(testo.into());
        }
        if p.get_riga_sinistra_nome() != nome.as_str() {
            p.set_riga_sinistra_nome(nome.into());
        }
    }

    fn riempi_stato(&self, p: &Pannello) {
        self.aggiorna_riga_sinistra(p);
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
        } else {
            // I quadranti del PC si mostrano anche senza alcun dato della casa.
            self.ricostruisci_quadranti();
        }
    }

    /// Il pannello appena creato: modelli dei preferiti, dei sensori e delle voci in attesa,
    /// stato della casa, tocchi e pulsanti.
    pub fn riempi(&self, p: &Pannello) {
        self.apri_modelli();
        p.set_preferiti(ModelRc::from(self.preferiti.clone()));
        p.set_sensori(ModelRc::from(self.sensori.clone()));
        p.set_quadranti(ModelRc::from(self.quadranti.clone()));
        p.set_stanze(ModelRc::from(self.stanze.clone()));
        p.set_dispositivi(ModelRc::from(self.dispositivi.clone()));
        p.set_collegamenti(ModelRc::from(self.collegamenti.clone()));
        p.set_claude_righe(ModelRc::from(self.claude_righe.clone()));
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
        // Clic o Invio su una riga della vista Claude (Task 10): porta davanti il terminale. Va
        // chiamato solo da qui, cioè subito dopo il gesto dell'utente.
        p.on_apri_sessione(|impronta| {
            if let Some(ponte) = ponte() {
                ponte.claude_apri_terminale(&impronta);
            }
        });
        // «Dettagli» nella vista Claude (Task 11): le schede delle API nella finestra di gestione.
        p.on_apri_dettagli_claude(|| {
            if let Some(ponte) = ponte() {
                ponte.claude_apri_dettagli();
            }
        });
        p.on_riprova_casa(|| {
            if let Some(ponte) = ponte() {
                ponte.riprova_casa();
            }
        });
        // Il volume del PC nella vista PC (M2, Task 7, solo Windows): il thread `audio` parte alla
        // prima apertura (lo stato arriva prima che si scelga la vista PC) e il pannello nuovo
        // prende l'ultimo stato letto.
        #[cfg(windows)]
        {
            self.avvia_volume();
            self.scrivi_volume(p);
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
        // Wi-Fi, Bluetooth e rete nella vista PC (M2, Task 8, solo Windows): le radio nascono
        // entrando nella vista PC (`vista_pc`); qui l'ultima rete letta e i comandi.
        #[cfg(windows)]
        {
            p.set_connessioni_disponibili(true);
            connessioni::scrivi_rete(p, self.stato_rete.get());
            p.on_commuta_radio(|tipo, acceso| {
                if let Some(ponte) = ponte() {
                    ponte.commuta_radio(connessioni::tipo_da(tipo), acceso);
                }
            });
            p.on_apri_pagina(|i| apri_pagina(connessioni::pagina_da(i)));
        }
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
        #[cfg(any(not(windows), test))]
        if let Some(vista) = self.vista_richiesta.take() {
            p.set_vista(vista.into());
            self.vista_claude(vista == "claude");
        }
        self.attese.borrow_mut().segna_viste();
        // Il pannello compare: un permesso in attesa non si consente al primo istante.
        self.claude_superficie_compare(Instant::now());
        *self.mostrato.borrow_mut() = None; // il pannello nuovo non ha ancora l'umore
        self.aggiorna_umore();
    }

    /// Il pannello si è chiuso e la pillola (di nuovo attiva) sta per ricomparire: riprende
    /// l'umore di adesso.
    pub fn pannello_chiuso(&self) {
        // Le radio si liberano e la rete torna a seguire solo la pillola.
        self.vista_pc(false);
        // La vista Claude non c'è più: il timer torna a seguire solo la pillola.
        self.vista_claude(false);
        *self.pannello.borrow_mut() = slint::Weak::default();
        self.modelli_vivi.set(false);
        self.collegamento_aperto.set(false);
        // L'elenco è stato guardato: i «fatto» di Claude Code sono visti.
        self.claude_pannello_chiuso();
        // Il pannello si riapre sui preferiti.
        if self.stanza_aperta.borrow_mut().take().is_some() {
            self.dispositivi.set_vec(Vec::new());
        }
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
        self.claude_riarma(Instant::now());
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
        } else if voce == ID_BATTERIA && pulsante == PULSANTE_HO_CAPITO {
            // Zitta fino alla ricarica: il glifo nella vista PC continua a dire com'è la batteria.
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
                self.stati_radio.borrow_mut().clear();
                *self.nome_rete.borrow_mut() = None;
                if let Some(p) = self.pannello.borrow().upgrade() {
                    connessioni::dimentica_radio(&p);
                    connessioni::scrivi_nome(&p, None);
                    batterie_bt::scrivi_cuffie(&p, None);
                }
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

    fn rete_ricevuta(&self, stato: radio::StatoRete) {
        if self.osserva_rete.borrow().is_none() {
            return;
        }
        self.stato_rete.set(Some(stato));
        sorgenti_lati::rete(stato);
        if let Some(p) = self.pannello.borrow().upgrade() {
            connessioni::scrivi_rete(&p, Some(stato));
        }
    }

    fn nome_rete_ricevuto(&self, nome: Option<String>) {
        if !self.vista_pc.get() {
            return;
        }
        if let Some(p) = self.pannello.borrow().upgrade() {
            connessioni::scrivi_nome(&p, nome.as_deref());
        }
        *self.nome_rete.borrow_mut() = nome;
    }

    fn radio_ricevute(&self, stati: Vec<radio::StatoRadio>) {
        // Arrivate dopo che la vista PC è stata lasciata: nessuno le guarda.
        if self.radio.borrow().is_none() {
            return;
        }
        if let Some(p) = self.pannello.borrow().upgrade() {
            connessioni::scrivi_radio(&p, &stati);
        }
        *self.stati_radio.borrow_mut() = stati;
    }

    fn commuta_radio(&self, tipo: radio::TipoRadio, acceso: bool) {
        match self.radio.borrow().as_ref() {
            Some(r) => r.commuta(tipo, acceso, consegna_esito),
            None => self.esito_radio(tipo, radio::EsitoRadio::Errore),
        }
    }

    /// Un rifiuto: riga in ambra, riquadro di nuovo allo stato vero, pagina di Windows aperta.
    /// Un esito arrivato dopo aver lasciato la vista PC (radio non più seguite) si ignora.
    fn esito_radio(&self, tipo: radio::TipoRadio, esito: radio::EsitoRadio) {
        let Some(p) = self.pannello.borrow().upgrade() else { return };
        let seguite = self.radio.borrow().is_some();
        let stati = self.stati_radio.borrow();
        let pagina = connessioni::esito(&p, seguite.then_some(stati.as_slice()), tipo, esito);
        if let Some(pagina) = pagina {
            apri_pagina(pagina);
        }
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

    /// La lettura fatta entrando nella vista PC: la riga «CUFFIE». Arrivata dopo aver lasciato la
    /// vista, nessuno la guarda.
    fn cuffie_lette(&self, letture: &[batterie_bt::LetturaBt]) {
        if !self.vista_pc.get() {
            return;
        }
        if let Some(p) = self.pannello.borrow().upgrade() {
            batterie_bt::scrivi_cuffie(&p, batterie_bt::riga_cuffie(letture).as_ref());
        }
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

    /// La riga del volume della vista PC: c'è solo con un'uscita audio; ogni proprietà si
    /// riscrive solo se cambia (il livello al punto intero).
    fn scrivi_volume(&self, p: &Pannello) {
        let s = self.volume.get();
        if p.get_volume_disponibile() != s.is_some() {
            p.set_volume_disponibile(s.is_some());
        }
        let Some(s) = s else { return };
        if volume::percento(p.get_volume()) != volume::percento(s.livello) {
            p.set_volume(volume::percento(s.livello) as f32 / 100.0);
        }
        if p.get_volume_muto() != s.muto {
            p.set_volume_muto(s.muto);
        }
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

    /// Impostazioni › Isola: la rotella sulla pillola accesa o spenta, salvata e subito in vigore.
    fn cambia_volume_rotella(&self, si: bool) {
        self.casa.aggiorna_config(|c| c.volume_rotella = si);
        if let Some(pl) = self.pillola.upgrade() {
            pl.set_rotella_attiva(si);
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
    pub(crate) struct Spegni;
    impl Drop for Spegni {
        fn drop(&mut self) {
            spegni();
        }
    }

    pub(crate) fn ponte_di_prova() -> (Spegni, Rc<Ponte>, crate::Pillola, tempfile::TempDir) {
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
        let v = attenzione::voce_di_prova(Livello::Attenzione, false, Instant::now());
        assert_eq!(voce_attesa_ui(&v, false).sotto, "Guarda presto · Prova");
        assert_eq!(voce_attesa_ui(&v, true).sotto, "Guarda presto · Prova · archiviazione non riuscita");
        // Il nome interno resta per il colore.
        assert_eq!(voce_attesa_ui(&v, false).livello, crate::LivelloUi::Attenzione);
    }

    #[test]
    fn sotto_il_titolo_il_nome_per_l_utente_non_l_identificativo() {
        use crate::attenzione::Fonte;
        let nomi: Vec<_> = [Fonte::Casa, Fonte::Pc, Fonte::Claude, Fonte::Prova].iter().map(|f| f.nome()).collect();
        assert_eq!(nomi, ["Casa", "PC", "Claude Code", "Prova"]);
        let mut v = attenzione::voce_di_prova(Livello::Informazione, false, Instant::now());
        v.fonte = Fonte::Pc;
        assert_eq!(voce_attesa_ui(&v, false).sotto, "Quando vuoi · PC");
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

    fn nuvoloso(temperatura: Option<f64>) -> Meteo {
        Meteo { entity_id: "weather.casa".into(), condizione: "cloudy".into(), temperatura, unita: "°C".into() }
    }

    #[test]
    fn riga_alta_quadranti_con_e_senza_meteo() {
        let m = nuvoloso(Some(9.0));
        let (testo, nome) = riga_sinistra(Layout::Quadranti, "VEN 13 NOV", Some(&m));
        assert_eq!(testo, "VEN 13 NOV · 9°");
        assert_eq!(nome, "venerdì 13 novembre, 9 gradi, nuvoloso");
        let (testo, nome) = riga_sinistra(Layout::Quadranti, "VEN 13 NOV", None);
        assert_eq!((testo.as_str(), nome.as_str()), ("VEN 13 NOV", "venerdì 13 novembre"));
    }

    #[test]
    fn riga_alta_orologio_solo_meteo() {
        let m = nuvoloso(Some(9.0));
        let (testo, nome) = riga_sinistra(Layout::Orologio, "VEN 13 NOV", Some(&m));
        assert_eq!((testo.as_str(), nome.as_str()), ("9° · Nuvoloso", "9 gradi, nuvoloso"));
        assert_eq!(riga_sinistra(Layout::Orologio, "VEN 13 NOV", None), (String::new(), String::new()));
    }

    #[test]
    fn meteo_senza_temperatura_niente_gradi() {
        let m = nuvoloso(None);
        let (testo, nome) = riga_sinistra(Layout::Quadranti, "VEN 13 NOV", Some(&m));
        assert_eq!(testo, "VEN 13 NOV");
        assert!(!testo.contains('°') && !nome.contains("gradi"), "{nome}");
        // Nell'Orologio resta la sola condizione; condizione sconosciuta e niente temperatura: nulla.
        assert_eq!(riga_sinistra(Layout::Orologio, "VEN 13 NOV", Some(&m)).0, "Nuvoloso");
        let strano = Meteo { condizione: "boh".into(), ..m };
        assert_eq!(riga_sinistra(Layout::Orologio, "VEN 13 NOV", Some(&strano)).0, "");
        let solo_gradi = Meteo { temperatura: Some(-3.0), ..strano };
        assert_eq!(riga_sinistra(Layout::Orologio, "VEN 13 NOV", Some(&solo_gradi)).0, "-3°");
    }

    // ─── M2, Task 4: quadranti configurabili e Impostazioni › Isola ───

    fn potenza(id: &str, valore: &str) -> AppEntity {
        AppEntity::Sensor(SensorEntity {
            base: BaseEntity { state: valore.into(), ..base(id, true) },
            value: valore.into(),
            unit: Some("W".into()),
            device_class: Some("power".into()),
        })
    }

    #[test]
    fn quadranti_configurabili_dai_dati() {
        let m = menu(vec![sensore("sensor.t", "15", Some("°C")), potenza("sensor.fv", "1500")], vec![]);
        let q = [Lato::Cpu, Lato::Niente, Lato::Sensore("sensor.t".into())];
        let v = quadranti_ui(&q, Some(&m), &HashMap::new());
        assert_eq!(v.len(), 2, "«niente» non si mostra");
        assert_eq!((v[0].fonte, v[0].etichetta.as_str()), (FonteQuadranteUi::Cpu, "CPU"));
        assert_eq!(v[1].fonte, FonteQuadranteUi::Sensore);
        assert_eq!(v[1].valore, "15 °C");
        assert!((v[1].frazione - 0.5).abs() < 1e-6, "15 °C a metà fra −10 e 40");
        assert_eq!(v[1].dettaglio, "Nome sensor.t");
        // Watt: 3000 di serie, poi il massimo dell'utente.
        let fv = [Lato::Sensore("sensor.fv".into())];
        assert!((quadranti_ui(&fv, Some(&m), &HashMap::new())[0].frazione - 0.5).abs() < 1e-6);
        let massimi = HashMap::from([("sensore:sensor.fv".to_owned(), 6000.0)]);
        assert!((quadranti_ui(&fv, Some(&m), &massimi)[0].frazione - 0.25).abs() < 1e-6);
        // Un sensore sparito resta col trattino; senza dati i quadranti del PC ci sono lo stesso.
        let sparito = quadranti_ui(&[Lato::Sensore("sensor.via".into())], Some(&m), &HashMap::new());
        assert_eq!((sparito[0].valore.as_str(), sparito[0].frazione), ("", -1.0));
        assert_eq!(quadranti_ui(&lati::QUADRANTI_DI_M1, None, &HashMap::new()).len(), 3);
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
            quadranti: vec!["ram".into(), "sensore:sensor.fv".into()],
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
        // Quadranti: RAM, il fotovoltaico (col massimo, predefinito 3000) e il terzo a niente.
        assert_eq!(i.scelte_quadranti.len(), 3);
        let q = &i.scelte_quadranti;
        assert_eq!(i.quadranti[q[0].indice as usize].0, Lato::Ram);
        assert_eq!(i.quadranti[q[1].indice as usize].0, Lato::Sensore("sensor.fv".into()));
        assert!(!q[0].con_massimo && q[1].con_massimo && !q[2].con_massimo);
        assert_eq!(q[1].predefinito, "3000");
        assert_eq!(i.quadranti[q[2].indice as usize].0, Lato::Niente);
        // Senza dati della casa: niente meteo né sensori fra le scelte, i quadranti di M1.
        let vuota = isola(&Config::default(), None, true);
        assert!(vuota.lati.iter().any(|(l, _)| *l == Lato::Batteria), "col PC a batteria");
        assert!(!vuota.lati.iter().any(|(l, _)| *l == Lato::Meteo));
        let m1: Vec<&Lato> = vuota.scelte_quadranti.iter().map(|s| &vuota.quadranti[s.indice as usize].0).collect();
        assert_eq!(m1, [&Lato::Cpu, &Lato::Ram, &Lato::Disco]);
    }

    /// Scegliere un lato dalle Impostazioni lo salva e lo porta subito nella pillola; un quadrante
    /// salva tutti e tre.
    #[test]
    fn scelte_dell_isola_salvate() {
        let (_spegni, p, pillola, _dir) = ponte_di_prova();
        *p.isola.borrow_mut() = isola(&p.casa.config(), None, false);
        let cpu = p.isola.borrow().lati.iter().position(|(l, _)| *l == Lato::Cpu).unwrap() as i32;
        p.scegli_lato(1, cpu);
        assert_eq!(p.casa.config().lato_destro, "cpu");
        assert_eq!(pillola.get_lato_destro().testo, "CPU —", "subito nella pillola, la CPU arriva dopo");
        let disco = p.isola.borrow().quadranti.iter().position(|(l, _)| *l == Lato::Disco).unwrap() as i32;
        p.scegli_quadrante(0, disco);
        assert_eq!(p.casa.config().quadranti, ["disco", "ram", "disco"]);
        assert_eq!(p.cambia_massimo(0, "zero"), "Scrivi un numero maggiore di zero");
        assert_eq!(p.cambia_massimo(0, "4,5"), "");
        assert_eq!(p.casa.config().massimi_quadranti.get("disco"), Some(&4.5));
        assert_eq!(p.cambia_massimo(0, ""), "");
        assert!(p.casa.config().massimi_quadranti.is_empty(), "vuoto: torna il predefinito");
        sorgenti_lati::spegni();
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
