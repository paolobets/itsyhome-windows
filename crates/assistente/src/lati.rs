//! Che cosa mostra la pillola ai lati degli occhi e che cosa mostrano i tre quadranti
//! configurabili (M2, Task 4): un solo catalogo di fonti (`Lato`), le scelte da offrire nelle
//! Impostazioni, di che cosa c'è bisogno (per accendere solo i timer che servono), il testo da
//! mostrare e le misure della pillola a riposo. Tutto puro: niente interfaccia, niente timer.
//!
//! I timer e i dati vivi stanno in `sorgenti_lati`; le misure qui coincidono con quelle di
//! pillola.slint (lo controlla `test_ui::misure_dei_lati_slint_e_rust`).

use crate::claude;
use crate::finestre;
use crate::pc_stats::{tinta_batteria, Fonti, LetturaBatteria, TintaBatteria};
use crate::sistema::radio::StatoRete;
use api_claude::Importo;
use casa_core::config::PREFISSO_SENSORE;
use casa_core::types::{MenuData, SensoreNumerico};
use chrono::{DateTime, Local};
use claude_hook::Limite;
use std::collections::HashMap;

/// Una fonte per un lato della pillola o per un quadrante.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum Lato {
    #[default]
    Niente,
    Ora,
    Batteria,
    Cpu,
    Ram,
    /// L'icona della rete (Task 8, solo Windows): cavo o Wi-Fi con le tacche, mai il nome.
    Rete,
    /// La temperatura esterna, dal meteo di Home Assistant.
    Meteo,
    /// Un sensore di Home Assistant (`entity_id`).
    Sensore(String),
    /// Solo per i quadranti: il disco e la velocità della rete in arrivo e in uscita.
    Disco,
    ReteGiu,
    ReteSu,
    /// Solo per i quadranti e solo su Windows (M2, Task 10): l'utilizzo della GPU, 0..100 %.
    Gpu,
    /// Solo per i quadranti (M3, Task 12): il limite di 5 ore dell'abbonamento a Claude.
    Limite5h,
    /// Solo per i quadranti: il limite di 7 giorni dell'abbonamento a Claude.
    Limite7g,
    /// Solo per i quadranti: la spesa del mese delle API di Claude.
    SpesaApi,
}

impl Lato {
    /// Dalla stringa salvata nella configurazione; sconosciuta o vuota: `Niente`.
    pub fn da_str(s: &str) -> Lato {
        match s {
            "ora" => Lato::Ora,
            "batteria" => Lato::Batteria,
            "cpu" => Lato::Cpu,
            "ram" => Lato::Ram,
            "rete" => Lato::Rete,
            "meteo" => Lato::Meteo,
            "disco" => Lato::Disco,
            "rete_giu" => Lato::ReteGiu,
            "rete_su" => Lato::ReteSu,
            "gpu" => Lato::Gpu,
            "limite_5h" => Lato::Limite5h,
            "limite_7g" => Lato::Limite7g,
            "spesa_api" => Lato::SpesaApi,
            _ => match s.strip_prefix(PREFISSO_SENSORE) {
                Some(id) if !id.is_empty() => Lato::Sensore(id.to_owned()),
                _ => Lato::Niente,
            },
        }
    }

    /// La stringa da salvare (`""` per `Niente`).
    pub fn come_str(&self) -> String {
        match self {
            Lato::Niente => String::new(),
            Lato::Ora => "ora".into(),
            Lato::Batteria => "batteria".into(),
            Lato::Cpu => "cpu".into(),
            Lato::Ram => "ram".into(),
            Lato::Rete => "rete".into(),
            Lato::Meteo => "meteo".into(),
            Lato::Disco => "disco".into(),
            Lato::ReteGiu => "rete_giu".into(),
            Lato::ReteSu => "rete_su".into(),
            Lato::Gpu => "gpu".into(),
            Lato::Limite5h => "limite_5h".into(),
            Lato::Limite7g => "limite_7g".into(),
            Lato::SpesaApi => "spesa_api".into(),
            Lato::Sensore(id) => format!("{PREFISSO_SENSORE}{id}"),
        }
    }
}

/// L'unità è di temperatura («°C», «°F»).
fn di_temperatura(unita: &str) -> bool {
    matches!(unita.trim(), "°C" | "°F")
}

/// I sensori numerici della casa (`MenuData::sensori_numerici`, già per nome) che si possono
/// scegliere: per i lati solo le temperature (ci stanno in pochi caratteri), per i quadranti tutti
/// (temperatura, umidità, potenza, energia e produzione FV…). Anche quelli non disponibili adesso:
/// hanno nome e unità, il valore arriverà.
pub fn sensori_disponibili(menu: &MenuData, per_quadranti: bool) -> Vec<&SensoreNumerico> {
    menu.sensori_numerici.iter().filter(|s| per_quadranti || di_temperatura(&s.unita)).collect()
}

/// Il nome di un sensore nel selettore, col genere dall'unità: «Temperatura · Sala»,
/// «Potenza · Fotovoltaico», «Energia · Produzione»; senza un genere sicuro (un «%» può essere
/// umidità o batteria) il nome come lo dà HA.
fn nome_sensore(s: &SensoreNumerico) -> String {
    let genere = match s.unita.trim() {
        "°C" | "°F" => Some("Temperatura"),
        "W" | "kW" | "MW" => Some("Potenza"),
        "Wh" | "kWh" | "MWh" => Some("Energia"),
        _ => None,
    };
    match genere {
        Some(g) => format!("{g} · {}", s.nome),
        None => s.nome.clone(),
    }
}

/// Le scelte del selettore, nell'ordine: «Niente», «Ora», «Batteria» (solo se il PC ne ha una),
/// «CPU», «RAM», poi per i quadranti disco, rete in arrivo e in uscita e (solo se `windows`, Task
/// 10) «GPU»; «Rete» solo se `windows` (Task 8) e solo per i lati; «Temperatura esterna» solo se
/// c'è un meteo; un sensore per voce (`sensori`, già filtrati da `sensori_disponibili`).
/// `windows`: le fonti che esistono solo su Windows ci sono (`cfg!(windows)`).
pub fn scelte(
    meteo: bool,
    sensori: &[&SensoreNumerico],
    windows: bool,
    batteria: bool,
    per_quadranti: bool,
) -> Vec<(Lato, String)> {
    let mut v: Vec<(Lato, String)> = vec![(Lato::Niente, "Niente".into()), (Lato::Ora, "Ora".into())];
    if batteria {
        v.push((Lato::Batteria, "Batteria".into()));
    }
    v.push((Lato::Cpu, "CPU".into()));
    v.push((Lato::Ram, "RAM".into()));
    if per_quadranti {
        v.push((Lato::Disco, "Disco".into()));
        v.push((Lato::ReteGiu, "Rete in arrivo".into()));
        v.push((Lato::ReteSu, "Rete in uscita".into()));
        if windows {
            v.push((Lato::Gpu, "GPU".into()));
        }
        v.push((Lato::Limite5h, "Limite 5 ore".into()));
        v.push((Lato::Limite7g, "Limite 7 giorni".into()));
        v.push((Lato::SpesaApi, "Spesa API del mese".into()));
    } else if windows {
        v.push((Lato::Rete, "Rete".into()));
    }
    if meteo {
        v.push((Lato::Meteo, "Temperatura esterna".into()));
    }
    v.extend(sensori.iter().map(|s| (Lato::Sensore(s.entity_id.clone()), nome_sensore(s))));
    v
}

/// Di che cosa c'è bisogno per i due lati scelti: ogni sorgente si accende solo se serve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Bisogni {
    pub ora: bool,
    pub batteria: bool,
    pub cpu_ram: bool,
    pub rete: bool,
    /// Meteo o un sensore: arrivano dal ponte a ogni `MenuData`.
    pub casa: bool,
}

pub fn bisogni(sinistro: &Lato, destro: &Lato) -> Bisogni {
    let mut b = Bisogni::default();
    for l in [sinistro, destro] {
        match l {
            Lato::Ora => b.ora = true,
            Lato::Batteria => b.batteria = true,
            Lato::Cpu | Lato::Ram => b.cpu_ram = true,
            Lato::Rete => b.rete = true,
            Lato::Meteo | Lato::Sensore(_) => b.casa = true,
            Lato::Niente
            | Lato::Disco
            | Lato::ReteGiu
            | Lato::ReteSu
            | Lato::Gpu
            | Lato::Limite5h
            | Lato::Limite7g
            | Lato::SpesaApi => {}
        }
    }
    b
}

/// I dati da cui nascono i lati.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DatiLati {
    /// «14:05»; vuota finché l'ora non serve.
    pub ora: String,
    pub batteria: Option<LetturaBatteria>,
    pub cpu: Option<f32>,
    pub ram: Option<f32>,
    /// La temperatura esterna già pronta («9°»).
    pub meteo: Option<String>,
    /// I sensori scelti, per `entity_id`, come arrivano dai dati della casa (valore e unità).
    pub sensori: HashMap<String, SensoreNumerico>,
    /// Come è collegato il PC (Task 8, solo Windows, solo se il lato «Rete» è scelto).
    pub rete: Option<StatoRete>,
}

/// Che cosa mostra un lato.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ContenutoLato {
    #[default]
    Vuoto,
    Testo(String),
    /// Il glifo della batteria con la percentuale accanto.
    Batteria { testo: String, frazione: f32, tinta: TintaBatteria },
    /// L'icona della rete, senza testo: mai il nome della rete (Task 8).
    Rete(StatoRete),
}

/// Un numero breve con la virgola italiana: un decimale sotto 100 («21,5», «20», «12,3»), intero
/// da 100 in su («1523»); mai «-0».
pub fn numero_breve(v: f64) -> String {
    let testo = if v.abs() >= 100.0 {
        format!("{:.0}", v.round())
    } else {
        let arrotondato = (v * 10.0).round() / 10.0;
        if arrotondato.fract() == 0.0 {
            format!("{arrotondato:.0}")
        } else {
            format!("{arrotondato:.1}").replace('.', ",")
        }
    };
    if testo == "-0" {
        "0".into()
    } else {
        testo
    }
}

/// Un valore con la sua unità: «1523 W», «12,5 kWh», «55%», «21,5 °C».
pub fn testo_con_unita(v: f64, unita: &str) -> String {
    match unita.trim() {
        "" => numero_breve(v),
        "%" => format!("{}%", numero_breve(v)),
        u => format!("{} {u}", numero_breve(v)),
    }
}

/// Il valore di un sensore in un lato: le temperature in gradi brevi («21,5°»), il resto col
/// numero e la sua unità («55%», «1523 W»).
fn testo_sensore(v: f64, unita: &str) -> String {
    if di_temperatura(unita) {
        format!("{}°", numero_breve(v))
    } else {
        testo_con_unita(v, unita)
    }
}

/// Il contenuto di un lato: «CPU 23%», «RAM 61%», «14:05», «80%» col glifo, «9°», «21,5°». Senza
/// dato meteo, sensore e batteria restano vuoti (mai «0°»); la CPU illeggibile è «CPU —».
pub fn contenuto(l: &Lato, d: &DatiLati) -> ContenutoLato {
    let percentuale = |sigla: &str, v: Option<f32>| match v {
        Some(v) => ContenutoLato::Testo(format!("{sigla} {}%", v.round() as i32)),
        None => ContenutoLato::Testo(format!("{sigla} —")),
    };
    match l {
        Lato::Ora if !d.ora.is_empty() => ContenutoLato::Testo(d.ora.clone()),
        Lato::Batteria => match &d.batteria {
            Some(b) => ContenutoLato::Batteria {
                testo: format!("{}%", b.percentuale.round() as i32),
                frazione: (b.percentuale / 100.0).clamp(0.0, 1.0),
                tinta: tinta_batteria(b),
            },
            None => ContenutoLato::Vuoto,
        },
        Lato::Cpu => percentuale("CPU", d.cpu),
        Lato::Ram => percentuale("RAM", d.ram),
        Lato::Meteo => d.meteo.clone().map_or(ContenutoLato::Vuoto, ContenutoLato::Testo),
        Lato::Rete => d.rete.map_or(ContenutoLato::Vuoto, ContenutoLato::Rete),
        Lato::Sensore(id) => d
            .sensori
            .get(id)
            .and_then(|s| s.valore.map(|v| testo_sensore(v, &s.unita)))
            .map_or(ContenutoLato::Vuoto, ContenutoLato::Testo),
        _ => ContenutoLato::Vuoto,
    }
}

/// I lati che in «dorme» restano vuoti, perché fermi sarebbero sbagliati (giro di correzioni 1):
/// ora, CPU e RAM, che cambiano col tempo, e la rete, perché in «dorme» `OsservaRete` è spento e
/// un Wi-Fi caduto non si vedrebbe (m1 della revisione finale di M2; il posto dell'icona resta,
/// `con_rete`, quindi la larghezza non cambia). Batteria, meteo e sensori restano.
pub fn a_tempo(l: &Lato) -> bool {
    matches!(l, Lato::Ora | Lato::Cpu | Lato::Ram | Lato::Rete)
}

/// Il campione per misurare un lato: la pillola si misura su questo e non sul valore vero, così
/// la larghezza cambia solo quando cambia la scelta, mai col valore («CPU 9%» → «CPU 10%» non
/// sposta nulla; giro di correzioni 1). È il valore più largo plausibile del tipo (il carattere è
/// mono: conta il numero di caratteri); un valore vero più largo, raro, allarga la pillola invece
/// di uscirne. `None`: il lato non c'è (o è solo un'icona, come la rete: il suo posto lo riserva
/// l'interfaccia). La batteria riserva anche il posto del glifo.
pub fn campione(l: &Lato, d: &DatiLati) -> Option<String> {
    Some(match l {
        Lato::Niente
        | Lato::Rete
        | Lato::Disco
        | Lato::ReteGiu
        | Lato::ReteSu
        | Lato::Gpu
        | Lato::Limite5h
        | Lato::Limite7g
        | Lato::SpesaApi => return None,
        Lato::Ora => "00:00".into(),
        Lato::Batteria => "100%".into(),
        Lato::Cpu => "CPU 100%".into(),
        Lato::Ram => "RAM 100%".into(),
        Lato::Meteo => CAMPIONE_GRADI.into(),
        // Finché il sensore non arriva la sua unità non si sa: per i lati si offrono le temperature.
        Lato::Sensore(id) => d.sensori.get(id).map_or_else(|| CAMPIONE_GRADI.into(), |s| campione_unita(&s.unita)),
    })
}

const CAMPIONE_GRADI: &str = "-10,5°";

/// Il valore più largo plausibile per un'unità, scritto come `testo_sensore`.
fn campione_unita(unita: &str) -> String {
    match unita.trim() {
        u if di_temperatura(u) => CAMPIONE_GRADI.into(),
        "%" => "100%".into(),
        // Una potenza può essere negativa (immissione in rete) e arrivare alle migliaia.
        "W" => "-9999 W".into(),
        u => testo_con_unita(-99.9, u),
    }
}

// ─── Misure della pillola a riposo (TwoCode) ────────────────────────────────
// Le calcola pillola.slint; qui le stesse, confrontate con Slint in `test_ui`.

/// Dal bordo della pillola al valore di un lato.
#[cfg_attr(not(test), allow(dead_code))]
pub const MARGINE_LATO: f32 = 14.0;
/// Mezzo spazio degli occhi con due lati: dal centro degli occhi al bordo della loro zona.
#[cfg_attr(not(test), allow(dead_code))]
pub const MEZZI_OCCHI: f32 = 19.0;
/// Fra gli occhi e il valore.
#[cfg_attr(not(test), allow(dead_code))]
pub const DISTANZA_LATO: f32 = 12.0;
/// Ingombro degli occhi a riposo: due occhi da 6 e lo spazio di 12 (`Faccia` in faccia.slint).
#[cfg_attr(not(test), allow(dead_code))]
pub const INGOMBRO_OCCHI: f32 = 24.0;

/// Larghezza della pillola a riposo con i lati larghi `l_sx` e `l_dx` (0 = vuoto). Due lati: occhi
/// al centro e la pillola simmetrica sul lato più largo; un lato solo: occhi e valore affiancati al
/// centro. Mai meno della pillola di M1 (124) né più della finestra (`LARGHEZZA_BLOCCANTE`).
#[cfg_attr(not(test), allow(dead_code))]
pub fn larghezza_riposo(l_sx: f32, l_dx: f32) -> f32 {
    let (sx, dx) = (l_sx.max(0.0), l_dx.max(0.0));
    let larga = match (sx > 0.0, dx > 0.0) {
        (false, false) => 0.0,
        (true, true) => 2.0 * (MARGINE_LATO + sx.max(dx) + DISTANZA_LATO + MEZZI_OCCHI),
        _ => MARGINE_LATO + INGOMBRO_OCCHI + DISTANZA_LATO + sx.max(dx) + MARGINE_LATO,
    };
    larga.clamp(finestre::LARGHEZZA_PILLOLA, finestre::LARGHEZZA_BLOCCANTE)
}

/// Centro degli occhi a riposo rispetto al centro della pillola: 0 con due lati o nessuno; con un
/// lato solo gli occhi stanno dalla parte opposta del valore, metà del valore e della distanza.
#[cfg_attr(not(test), allow(dead_code))]
pub fn scarto_occhi_riposo(l_sx: f32, l_dx: f32) -> f32 {
    let (sx, dx) = (l_sx.max(0.0), l_dx.max(0.0));
    match (sx > 0.0, dx > 0.0) {
        (true, false) => (DISTANZA_LATO + sx) / 2.0,
        (false, true) => -(DISTANZA_LATO + dx) / 2.0,
        _ => 0.0,
    }
}

// ─── Quadranti ──────────────────────────────────────────────────────────────

/// I tre quadranti di M1, quando l'utente non ne ha scelti.
pub const QUADRANTI_DI_M1: [Lato; 3] = [Lato::Cpu, Lato::Ram, Lato::Disco];

/// I tre quadranti dalla configurazione: vuota = quelli di M1; meno di tre = gli altri a niente.
pub fn quadranti(config: &[String]) -> [Lato; 3] {
    if config.is_empty() {
        return QUADRANTI_DI_M1;
    }
    let mut q: [Lato; 3] = Default::default();
    for (posto, s) in q.iter_mut().zip(config) {
        *posto = Lato::da_str(s);
    }
    q
}

/// Le fonti lente del PC che i quadranti mostrano (il campionatore del pannello legge solo quelle).
pub fn fonti_dei_quadranti(q: &[Lato]) -> Fonti {
    Fonti {
        disco: q.contains(&Lato::Disco),
        rete: q.iter().any(|l| matches!(l, Lato::ReteGiu | Lato::ReteSu)),
        gpu: q.contains(&Lato::Gpu),
    }
}

/// Ciò che Claude dà ai quadranti (M3, Task 12): i limiti dell'abbonamento già filtrati
/// (`Ponte::limiti`: mai scaduti né non finiti) e la spesa del mese dell'ultima lettura riuscita.
/// Tutto `None` finché non c'è un dato.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DatiClaude {
    pub cinque: Option<Limite>,
    pub sette: Option<Limite>,
    pub spesa: Option<Importo>,
    /// La soglia di spesa mensile (dollari): se c'è, è il fondo scala dell'arco della spesa.
    pub soglia_spesa: Option<f64>,
    /// C'è la chiave da amministratore (la lettura è in programma) ma non è ancora arrivata una
    /// spesa: il quadrante lo dice invece di restare vuoto.
    pub lettura_attesa: bool,
}

/// Valore, arco e riga piccola di un quadrante di Claude.
#[derive(Debug, Clone, PartialEq)]
pub struct QuadranteClaude {
    /// «37%», «413 $»; «—» senza dato, mai «0%» né «0 $».
    pub valore: String,
    /// 0..1; -1 = niente arco.
    pub frazione: f32,
    /// «si azzera alle 14:30», «soglia 250,00 $»; vuota senza dato.
    pub dettaglio: String,
}

/// Il quadrante di una fonte di Claude; `None` per ogni altra fonte. `massimo_utente`: il massimo
/// scritto per la spesa quando non c'è una soglia. `ora`: quando si legge «si azzera alle…».
pub fn quadrante_claude(
    l: &Lato,
    d: &DatiClaude,
    massimo_utente: Option<f64>,
    ora: DateTime<Local>,
) -> Option<QuadranteClaude> {
    let senza_dato = || QuadranteClaude { valore: "—".into(), frazione: -1.0, dettaglio: String::new() };
    let limite = |l: &Option<Limite>| match l.filter(|l| l.percentuale.is_finite()) {
        Some(l) => {
            let t = claude::testo_limite(&l, ora);
            QuadranteClaude { valore: t.testo, frazione: t.frazione, dettaglio: t.azzera }
        }
        None => senza_dato(),
    };
    Some(match l {
        Lato::Limite5h => limite(&d.cinque),
        Lato::Limite7g => limite(&d.sette),
        Lato::SpesaApi => match d.spesa {
            None => QuadranteClaude {
                dettaglio: if d.lettura_attesa { "prima lettura fra poco".into() } else { String::new() },
                ..senza_dato()
            },
            Some(s) => {
                let dollari = s.micro_centesimi as f64 / MICRO_PER_DOLLARO;
                let tetto = d.soglia_spesa.filter(|t| t.is_finite() && *t > 0.0).or(massimo_utente);
                QuadranteClaude {
                    // Breve come gli altri valori dei quadranti («413 $»): i centesimi stanno nella vista Claude.
                    valore: testo_con_unita(dollari, "$"),
                    frazione: frazione(l, None, tetto, dollari),
                    dettaglio: d
                        .soglia_spesa
                        .filter(|t| t.is_finite() && *t > 0.0)
                        .map_or_else(String::new, |t| format!("soglia {}", dollari_interi(t))),
                }
            }
        },
        _ => return None,
    })
}

/// Una soglia in dollari: senza centesimi se è intera («500 $»), altrimenti «250,50 $».
fn dollari_interi(d: f64) -> String {
    let testo = importo_da_dollari(d).in_dollari();
    testo.strip_suffix(",00 $").map_or_else(|| testo.clone(), |interi| format!("{interi} $"))
}

/// Quanti micro-centesimi (l'unità di `Importo`) fanno un dollaro.
pub const MICRO_PER_DOLLARO: f64 = 100_000_000.0;

/// Un importo da dollari scritti dall'utente (soglie).
pub fn importo_da_dollari(d: f64) -> Importo {
    Importo { micro_centesimi: (d * MICRO_PER_DOLLARO).round() as i128 }
}

/// Massimo predefinito dell'arco per i watt e per i chilowatt.
pub const MASSIMO_W: f64 = 3000.0;
pub const MASSIMO_KW: f64 = 6.0;

/// L'unità chiede un massimo all'utente: W, kW o un'unità che non ha una scala naturale.
pub fn chiede_massimo(unita: Option<&str>) -> bool {
    !matches!(unita.map(str::trim), Some("%" | "°C" | "°F"))
}

/// La scala dell'arco di un quadrante, `(minimo, massimo)`: percentuali del PC e `%` 0..100; `°C`
/// −10..40 (anche la temperatura esterna; `°F` lo stesso intervallo in Fahrenheit); `W` 0..3000 e
/// `kW` 0..6 se l'utente non sceglie un altro massimo; altre unità 0..massimo dell'utente o, finché
/// non lo imposta, 0..(valore × 2). (Rispetto al brief c'è `valore`: serve proprio a quest'ultimo
/// caso.)
pub fn scala(fonte: &Lato, unita: Option<&str>, massimo_utente: Option<f64>, valore: f64) -> (f64, f64) {
    let utente = massimo_utente.filter(|m| m.is_finite() && *m > 0.0);
    match fonte {
        Lato::Cpu | Lato::Ram | Lato::Disco | Lato::Batteria | Lato::Gpu | Lato::Limite5h | Lato::Limite7g => (0.0, 100.0),
        Lato::Meteo if unita.map(str::trim) == Some("°F") => (14.0, 104.0),
        Lato::Meteo => (-10.0, 40.0),
        _ => match unita.map(str::trim) {
            Some("%") => (0.0, 100.0),
            Some("°C") => (-10.0, 40.0),
            Some("°F") => (14.0, 104.0),
            Some("W") => (0.0, utente.unwrap_or(MASSIMO_W)),
            Some("kW") => (0.0, utente.unwrap_or(MASSIMO_KW)),
            _ => (0.0, utente.unwrap_or_else(|| if valore.abs() > 0.0 { valore.abs() * 2.0 } else { 1.0 })),
        },
    }
}

/// La frazione 0..1 dell'arco: fuori scala l'arco è pieno o vuoto (il testo dice il valore vero).
pub fn frazione(fonte: &Lato, unita: Option<&str>, massimo_utente: Option<f64>, valore: f64) -> f32 {
    let (minimo, massimo) = scala(fonte, unita, massimo_utente, valore);
    if massimo <= minimo || !valore.is_finite() {
        return 0.0;
    }
    ((valore - minimo) / (massimo - minimo)).clamp(0.0, 1.0) as f32
}

/// Un massimo scritto nelle Impostazioni («4500», «4,5»): `Ok(None)` se vuoto (torna il
/// predefinito), errore se non è un numero positivo.
pub fn leggi_massimo(testo: &str) -> Result<Option<f64>, &'static str> {
    let t = testo.trim();
    if t.is_empty() {
        return Ok(None);
    }
    match t.replace(',', ".").parse::<f64>() {
        Ok(v) if v.is_finite() && v > 0.0 => Ok(Some(v)),
        _ => Err("Scrivi un numero maggiore di zero"),
    }
}

/// Un massimo come lo si rilegge nel campo: senza decimali inutili, con la virgola.
pub fn testo_massimo(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{v:.0}")
    } else {
        format!("{v}").replace('.', ",")
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::pc_stats::StatoBatteria;
    use casa_core::config::Config;

    #[test]
    fn lato_da_e_verso_stringa() {
        for l in [
            Lato::Niente,
            Lato::Ora,
            Lato::Batteria,
            Lato::Cpu,
            Lato::Ram,
            Lato::Rete,
            Lato::Meteo,
            Lato::Sensore("sensor.sala_t".into()),
            Lato::Disco,
            Lato::ReteGiu,
            Lato::ReteSu,
            Lato::Gpu,
            Lato::Limite5h,
            Lato::Limite7g,
            Lato::SpesaApi,
        ] {
            assert_eq!(Lato::da_str(&l.come_str()), l, "andata e ritorno di {l:?}");
        }
        assert_eq!(Lato::da_str("boh"), Lato::Niente);
        assert_eq!(Lato::da_str(""), Lato::Niente);
        assert_eq!(Lato::da_str("sensore:"), Lato::Niente, "un sensore senza id non è un sensore");
        assert_eq!(Lato::da_str("sensore:sensor.sala_t"), Lato::Sensore("sensor.sala_t".into()));
        assert_eq!(Lato::Niente.come_str(), "");
    }

    fn sensore(id: &str, nome: &str, valore: Option<f64>, unita: &str) -> SensoreNumerico {
        SensoreNumerico { entity_id: id.into(), nome: nome.into(), valore, unita: unita.into() }
    }

    #[test]
    fn scelte_senza_meteo_ne_rete() {
        let s: Vec<Lato> = scelte(false, &[], false, false, false).into_iter().map(|(l, _)| l).collect();
        assert!(!s.contains(&Lato::Meteo) && !s.contains(&Lato::Rete));
        for l in [Lato::Niente, Lato::Ora, Lato::Cpu, Lato::Ram] {
            assert!(s.contains(&l), "manca {l:?}");
        }
        assert!(!s.contains(&Lato::Batteria), "senza batteria niente «Batteria»");
        assert!(!s.contains(&Lato::Disco), "il disco è solo per i quadranti");
        assert!(!s.contains(&Lato::Gpu), "la GPU è solo per i quadranti");
    }

    #[test]
    fn scelte_con_tutto_nell_ordine() {
        let sala = sensore("sensor.sala_t", "Sala", Some(21.5), "°C");
        let v = scelte(true, &[&sala], true, true, false);
        let nomi: Vec<&str> = v.iter().map(|(_, n)| n.as_str()).collect();
        assert_eq!(nomi, ["Niente", "Ora", "Batteria", "CPU", "RAM", "Rete", "Temperatura esterna", "Temperatura · Sala"]);
        // Per i quadranti: disco, rete in arrivo e in uscita e GPU (solo Windows), niente «Rete».
        let fv = sensore("sensor.fv", "Fotovoltaico", Some(1500.0), "W");
        let q = scelte(true, &[&sala, &fv], true, false, true);
        let nomi: Vec<&str> = q.iter().map(|(_, n)| n.as_str()).collect();
        assert_eq!(
            nomi,
            [
                "Niente",
                "Ora",
                "CPU",
                "RAM",
                "Disco",
                "Rete in arrivo",
                "Rete in uscita",
                "GPU",
                "Limite 5 ore",
                "Limite 7 giorni",
                "Spesa API del mese",
                "Temperatura esterna",
                "Temperatura · Sala",
                "Potenza · Fotovoltaico"
            ]
        );
        // Fuori da Windows la GPU non c'è (assente, non spenta).
        let q = scelte(false, &[], false, false, true);
        assert!(!q.iter().any(|(l, _)| *l == Lato::Gpu));
    }

    /// Giro di correzioni 1: il catalogo viene dai sensori numerici della casa. Per i quadranti
    /// tutti (W, kWh, %…, anche se non disponibili adesso), per i lati solo le temperature.
    #[test]
    fn sensori_numerici_fra_le_scelte() {
        let menu = MenuData {
            sensori_numerici: std::sync::Arc::new(vec![
                sensore("sensor.energia", "Produzione", Some(12.5), "kWh"),
                sensore("sensor.fv", "Fotovoltaico", None, "W"),
                sensore("sensor.sala_t", "Sala", Some(21.5), "°C"),
                sensore("sensor.bagno_u", "Umidità bagno", Some(55.0), "%"),
            ]),
            ..Default::default()
        };
        let nomi = |per_quadranti| -> Vec<String> {
            let s = sensori_disponibili(&menu, per_quadranti);
            scelte(false, &s, false, false, per_quadranti).into_iter().skip_while(|(l, _)| !matches!(l, Lato::Sensore(_))).map(|(_, n)| n).collect()
        };
        assert_eq!(nomi(true), ["Energia · Produzione", "Potenza · Fotovoltaico", "Temperatura · Sala", "Umidità bagno"]);
        assert_eq!(nomi(false), ["Temperatura · Sala"]);
    }

    #[test]
    fn bisogni_solo_di_cio_che_e_scelto() {
        assert_eq!(bisogni(&Lato::Niente, &Lato::Niente), Bisogni::default());
        let b = bisogni(&Lato::Cpu, &Lato::Ora);
        assert!(b.cpu_ram && b.ora);
        assert!(!b.batteria && !b.rete && !b.casa);
        let b = bisogni(&Lato::Sensore("sensor.x".into()), &Lato::Batteria);
        assert!(b.casa && b.batteria && !b.cpu_ram && !b.ora);
    }

    #[test]
    fn contenuto_dei_lati() {
        let mut d = DatiLati { cpu: Some(23.4), ram: Some(61.0), meteo: Some("9°".into()), ora: "14:05".into(), ..Default::default() };
        d.sensori.insert("sensor.sala_t".into(), sensore("sensor.sala_t", "Sala", Some(21.5), "°C"));
        d.sensori.insert("sensor.studio_t".into(), sensore("sensor.studio_t", "Studio", Some(20.0), "°C"));
        // Un sensore non di temperatura (scritto a mano nella configurazione): con la sua unità,
        // mai in gradi.
        d.sensori.insert("sensor.u".into(), sensore("sensor.u", "Umidità", Some(55.04), "%"));
        d.sensori.insert("sensor.fv".into(), sensore("sensor.fv", "FV", Some(1523.4), "W"));
        let testo = |s: &str| ContenutoLato::Testo(s.into());
        assert_eq!(contenuto(&Lato::Cpu, &d), testo("CPU 23%"));
        assert_eq!(contenuto(&Lato::Ram, &d), testo("RAM 61%"));
        assert_eq!(contenuto(&Lato::Sensore("sensor.sala_t".into()), &d), testo("21,5°"));
        assert_eq!(contenuto(&Lato::Sensore("sensor.studio_t".into()), &d), testo("20°"));
        assert_eq!(contenuto(&Lato::Sensore("sensor.u".into()), &d), testo("55%"));
        assert_eq!(contenuto(&Lato::Sensore("sensor.fv".into()), &d), testo("1523 W"));
        assert_eq!(contenuto(&Lato::Meteo, &d), testo("9°"));
        assert_eq!(contenuto(&Lato::Ora, &d), testo("14:05"));
        d.batteria = Some(LetturaBatteria { percentuale: 80.4, stato: StatoBatteria::Scarica });
        assert_eq!(
            contenuto(&Lato::Batteria, &d),
            ContenutoLato::Batteria { testo: "80%".into(), frazione: 0.804, tinta: TintaBatteria::Normale }
        );
    }

    #[test]
    fn il_lato_rete_e_solo_un_icona() {
        let mut d = DatiLati { rete: Some(StatoRete::WiFi { tacche: 2 }), ..Default::default() };
        assert_eq!(contenuto(&Lato::Rete, &d), ContenutoLato::Rete(StatoRete::WiFi { tacche: 2 }));
        // Nessun testo: niente campione da misurare (il posto dell'icona lo tiene l'interfaccia).
        assert_eq!(campione(&Lato::Rete, &d), None);
        assert!(a_tempo(&Lato::Rete), "in «dorme» l'osservatore è spento: l'icona si svuota");
        d.rete = None;
        assert_eq!(contenuto(&Lato::Rete, &d), ContenutoLato::Vuoto, "non ancora letta");
        assert!(bisogni(&Lato::Rete, &Lato::Niente).rete);
    }

    #[test]
    fn numeri_brevi() {
        assert_eq!(numero_breve(21.54), "21,5");
        assert_eq!(numero_breve(20.0), "20");
        assert_eq!(numero_breve(-0.04), "0", "mai «-0»");
        assert_eq!(numero_breve(1523.4), "1523");
        assert_eq!(numero_breve(-250.6), "-251");
        assert_eq!(testo_con_unita(12.5, "kWh"), "12,5 kWh");
        assert_eq!(testo_con_unita(48.0, "%"), "48%");
        assert_eq!(testo_con_unita(3.0, ""), "3");
    }

    /// Giro di correzioni 1: il campione dipende dalla scelta, mai dal valore; ed è largo almeno
    /// quanto i valori plausibili del suo tipo (carattere mono: conta il numero di caratteri).
    #[test]
    fn campione_per_tipo_e_mai_dal_valore() {
        let mut d = DatiLati::default();
        let caratteri = |s: &str| s.chars().count();
        for (l, atteso) in [(Lato::Cpu, "CPU 100%"), (Lato::Ram, "RAM 100%"), (Lato::Ora, "00:00"), (Lato::Batteria, "100%"), (Lato::Meteo, "-10,5°")] {
            assert_eq!(campione(&l, &d).as_deref(), Some(atteso));
        }
        assert_eq!(campione(&Lato::Niente, &d), None);
        for cpu in [0.0, 9.0, 10.0, 99.6, 100.0] {
            d.cpu = Some(cpu);
            let ContenutoLato::Testo(t) = contenuto(&Lato::Cpu, &d) else { panic!() };
            assert!(caratteri(&t) <= caratteri("CPU 100%"), "{t}");
            assert_eq!(campione(&Lato::Cpu, &d).as_deref(), Some("CPU 100%"));
        }
        // Un sensore: prima che arrivi le temperature, poi dalla sua unità.
        let id = Lato::Sensore("sensor.t".into());
        assert_eq!(campione(&id, &d).as_deref(), Some("-10,5°"));
        for v in [-10.5, -3.0, 21.5, 35.0] {
            d.sensori.insert("sensor.t".into(), sensore("sensor.t", "T", Some(v), "°C"));
            let ContenutoLato::Testo(t) = contenuto(&id, &d) else { panic!() };
            assert!(caratteri(&t) <= caratteri("-10,5°"), "{t}");
            assert_eq!(campione(&id, &d).as_deref(), Some("-10,5°"));
        }
        d.sensori.insert("sensor.t".into(), sensore("sensor.t", "FV", Some(-4500.0), "W"));
        let ContenutoLato::Testo(t) = contenuto(&id, &d) else { panic!() };
        assert!(caratteri(&t) <= caratteri(&campione(&id, &d).unwrap()), "{t}");
        // I lati a tempo, quelli che in «dorme» restano vuoti.
        assert!(a_tempo(&Lato::Ora) && a_tempo(&Lato::Cpu) && a_tempo(&Lato::Ram) && a_tempo(&Lato::Rete));
        assert!(!a_tempo(&Lato::Batteria) && !a_tempo(&Lato::Meteo) && !a_tempo(&id));
    }

    #[test]
    fn lato_senza_dato_resta_vuoto() {
        let mut d = DatiLati::default();
        assert_eq!(contenuto(&Lato::Meteo, &d), ContenutoLato::Vuoto);
        assert_eq!(contenuto(&Lato::Sensore("sensor.assente".into()), &d), ContenutoLato::Vuoto);
        assert_eq!(contenuto(&Lato::Batteria, &d), ContenutoLato::Vuoto);
        assert_eq!(contenuto(&Lato::Cpu, &d), ContenutoLato::Testo("CPU —".into()));
        assert_eq!(contenuto(&Lato::Ora, &d), ContenutoLato::Vuoto, "ora non ancora letta");
        assert_eq!(contenuto(&Lato::Niente, &d), ContenutoLato::Vuoto);
        // Un sensore non disponibile non diventa «0°».
        d.sensori.insert("sensor.x".into(), sensore("sensor.x", "X", None, "°C"));
        assert_eq!(contenuto(&Lato::Sensore("sensor.x".into()), &d), ContenutoLato::Vuoto);
    }

    #[test]
    fn larghezza_con_uno_e_due_lati() {
        assert_eq!(larghezza_riposo(0.0, 0.0), 124.0);
        assert_eq!(larghezza_riposo(40.0, 40.0), 2.0 * (14.0 + 40.0 + 12.0 + 19.0));
        // Due lati diversi: simmetrica sul più largo.
        assert_eq!(larghezza_riposo(20.0, 40.0), larghezza_riposo(40.0, 40.0));
        // Un lato: occhi e valore affiancati, mai meno di 124.
        assert_eq!(larghezza_riposo(0.0, 60.0), 14.0 + 24.0 + 12.0 + 60.0 + 14.0);
        assert_eq!(larghezza_riposo(10.0, 0.0), 124.0);
        assert!(larghezza_riposo(400.0, 0.0) <= 440.0);
        assert!(larghezza_riposo(400.0, 400.0) <= 440.0);
        // Gli occhi: al centro con due lati o nessuno; dal lato opposto al valore con uno.
        assert_eq!(scarto_occhi_riposo(0.0, 0.0), 0.0);
        assert_eq!(scarto_occhi_riposo(30.0, 50.0), 0.0);
        assert_eq!(scarto_occhi_riposo(0.0, 40.0), -26.0);
        assert_eq!(scarto_occhi_riposo(40.0, 0.0), 26.0);
    }

    #[test]
    fn quadranti_di_m1_predefiniti() {
        // Config senza la chiave (un file di M1): i tre quadranti di M1.
        assert_eq!(quadranti(&Config::default().quadranti), [Lato::Cpu, Lato::Ram, Lato::Disco]);
        let q = quadranti(&["sensore:sensor.fv".into(), "ram".into()]);
        assert_eq!(q, [Lato::Sensore("sensor.fv".into()), Lato::Ram, Lato::Niente]);
        assert_eq!(fonti_dei_quadranti(&q), Fonti::default(), "niente disco né rete");
        assert_eq!(fonti_dei_quadranti(&QUADRANTI_DI_M1), Fonti { disco: true, ..Fonti::default() });
        assert_eq!(fonti_dei_quadranti(&[Lato::ReteSu]), Fonti { rete: true, ..Fonti::default() });
        // Task 10: la GPU si legge solo se un quadrante la mostra.
        assert_eq!(fonti_dei_quadranti(&[Lato::Cpu, Lato::Gpu]), Fonti { gpu: true, ..Fonti::default() });
    }

    #[test]
    fn scala_per_unita() {
        let s = Lato::Sensore("sensor.x".into());
        assert_eq!(scala(&Lato::Cpu, None, None, 50.0), (0.0, 100.0));
        assert_eq!(scala(&Lato::Gpu, None, None, 50.0), (0.0, 100.0));
        assert_eq!(scala(&s, Some("%"), None, 50.0), (0.0, 100.0));
        assert_eq!(scala(&s, Some("°C"), None, 21.0), (-10.0, 40.0));
        assert_eq!(scala(&Lato::Meteo, None, None, 9.0), (-10.0, 40.0));
        assert_eq!(scala(&s, Some("W"), None, 800.0), (0.0, 3000.0), "W: massimo predefinito");
        assert_eq!(scala(&s, Some("W"), Some(4500.0), 800.0), (0.0, 4500.0), "W: massimo dell'utente");
        assert_eq!(scala(&s, Some("kW"), None, 2.0), (0.0, 6.0));
        assert_eq!(scala(&s, Some("lx"), None, 300.0), (0.0, 600.0), "unità ignota: valore × 2");
        assert_eq!(scala(&s, Some("lx"), Some(1000.0), 300.0), (0.0, 1000.0));
        assert_eq!(scala(&s, None, Some(-3.0), 5.0), (0.0, 10.0), "un massimo non valido non conta");
        // Fuori scala: arco pieno o vuoto.
        assert_eq!(frazione(&s, Some("W"), None, 9000.0), 1.0);
        assert_eq!(frazione(&s, Some("°C"), None, -30.0), 0.0);
        assert!((frazione(&s, Some("°C"), None, 15.0) - 0.5).abs() < 1e-6);
        // Chi chiede un massimo nelle Impostazioni.
        assert!(chiede_massimo(Some("W")) && chiede_massimo(Some("kW")) && chiede_massimo(None) && chiede_massimo(Some("lx")));
        assert!(!chiede_massimo(Some("%")) && !chiede_massimo(Some("°C")));
    }

    /// M3, Task 12: limiti dell'abbonamento e spesa API sono fonti dei soli quadranti.
    #[test]
    fn fonti_claude_andata_e_ritorno() {
        for (s, l) in [("limite_5h", Lato::Limite5h), ("limite_7g", Lato::Limite7g), ("spesa_api", Lato::SpesaApi)] {
            assert_eq!(Lato::da_str(s), l);
            assert_eq!(l.come_str(), s);
        }
        // Non fra le scelte dei lati della pillola; sì fra quelle dei quadranti, coi loro nomi.
        let lati: Vec<Lato> = scelte(true, &[], true, true, false).into_iter().map(|(l, _)| l).collect();
        assert!(![Lato::Limite5h, Lato::Limite7g, Lato::SpesaApi].iter().any(|l| lati.contains(l)));
        let q = scelte(true, &[], true, true, true);
        for (l, nome) in
            [(Lato::Limite5h, "Limite 5 ore"), (Lato::Limite7g, "Limite 7 giorni"), (Lato::SpesaApi, "Spesa API del mese")]
        {
            assert!(q.contains(&(l.clone(), nome.into())), "{nome}");
            // Mai testo né campione a lato della pillola, e nessuna sorgente da accendere.
            assert_eq!(contenuto(&l, &DatiLati::default()), ContenutoLato::Vuoto);
            assert_eq!(campione(&l, &DatiLati::default()), None);
            assert_eq!(bisogni(&l, &Lato::Niente), Bisogni::default());
        }
    }

    fn limite(percentuale: f32, azzera_alle: i64) -> Limite {
        Limite { percentuale, azzera_alle }
    }

    /// RC-51: nel primo minuto, con la chiave da amministratore ma senza ancora una lettura, il
    /// quadrante della spesa lo dice in una riga piccola invece di restare vuoto; senza la chiave
    /// la lettura non arriverà e la riga non c'è.
    #[test]
    fn spesa_nel_primo_minuto_dice_prima_lettura() {
        let ora = chrono::Local::now();
        let attesa = DatiClaude { lettura_attesa: true, ..Default::default() };
        let q = quadrante_claude(&Lato::SpesaApi, &attesa, None, ora).unwrap();
        assert_eq!((q.valore.as_str(), q.frazione, q.dettaglio.as_str()), ("—", -1.0, "prima lettura fra poco"));
        let senza_chiave = DatiClaude::default();
        assert_eq!(quadrante_claude(&Lato::SpesaApi, &senza_chiave, None, ora).unwrap().dettaglio, "");
        // Arrivato il dato, la riga è la soglia (o niente), mai «prima lettura».
        let spesa = api_claude::importo_da("41280").unwrap();
        let arrivata = DatiClaude { spesa: Some(spesa), lettura_attesa: true, ..Default::default() };
        assert_eq!(quadrante_claude(&Lato::SpesaApi, &arrivata, None, ora).unwrap().dettaglio, "");
        // Solo la spesa ha una prima lettura: i limiti restano vuoti.
        assert_eq!(quadrante_claude(&Lato::Limite5h, &attesa, None, ora).unwrap().dettaglio, "");
    }

    /// PF9: senza dato il quadrante dice «—», mai «0%» né «0 $», e non ha arco.
    #[test]
    fn quadrante_senza_dato_trattino() {
        let ora = chrono::Local::now();
        let nessuno = DatiClaude::default();
        for l in [Lato::Limite5h, Lato::Limite7g, Lato::SpesaApi] {
            let q = quadrante_claude(&l, &nessuno, None, ora).expect("è una fonte di Claude");
            assert_eq!((q.valore.as_str(), q.frazione, q.dettaglio.as_str()), ("—", -1.0, ""), "{l:?}");
        }
        assert_eq!(quadrante_claude(&Lato::Cpu, &nessuno, None, ora), None, "le altre fonti non sono sue");
        // Con il dato: percentuale e arco, e quando si azzera.
        let dati = DatiClaude { cinque: Some(limite(36.6, ora.timestamp() + 600)), ..Default::default() };
        let q = quadrante_claude(&Lato::Limite5h, &dati, None, ora).unwrap();
        assert_eq!(q.valore, "37%");
        assert!((q.frazione - 0.366).abs() < 1e-5);
        assert!(q.dettaglio.starts_with("si azzera"), "{}", q.dettaglio);
        assert_eq!(quadrante_claude(&Lato::Limite7g, &dati, None, ora).unwrap().valore, "—", "la finestra dei 7 giorni non c'è");
        // La spesa: «412,80 $»; col tetto è la frazione della soglia, senza è il massimo dell'utente o il doppio.
        let spesa = api_claude::importo_da("41280").unwrap();
        let dati = DatiClaude { spesa: Some(spesa), soglia_spesa: Some(1000.0), ..Default::default() };
        let q = quadrante_claude(&Lato::SpesaApi, &dati, Some(500.0), ora).unwrap();
        assert_eq!(q.valore, "413 $");
        // RC-51: la soglia intera senza centesimi, con i centesimi solo se ci sono.
        assert_eq!(q.dettaglio, "soglia 1000 $");
        let con_centesimi = DatiClaude { soglia_spesa: Some(250.5), ..dati.clone() };
        assert_eq!(quadrante_claude(&Lato::SpesaApi, &con_centesimi, None, ora).unwrap().dettaglio, "soglia 250,50 $");
        assert!((q.frazione - 0.4128).abs() < 1e-4, "la soglia vince sul massimo dell'utente");
        let dati = DatiClaude { soglia_spesa: None, ..dati };
        assert!((quadrante_claude(&Lato::SpesaApi, &dati, Some(500.0), ora).unwrap().frazione - 0.8256).abs() < 1e-4);
        assert!((quadrante_claude(&Lato::SpesaApi, &dati, None, ora).unwrap().frazione - 0.5).abs() < 1e-4, "0..(valore × 2)");
        // Un limite non finito è assente.
        let dati = DatiClaude { cinque: Some(limite(f32::NAN, ora.timestamp() + 600)), ..Default::default() };
        assert_eq!(quadrante_claude(&Lato::Limite5h, &dati, None, ora).unwrap().valore, "—");
    }

    #[test]
    fn scala_dei_limiti_e_della_spesa() {
        assert_eq!(scala(&Lato::Limite5h, None, None, 40.0), (0.0, 100.0));
        assert_eq!(scala(&Lato::Limite7g, None, Some(5.0), 40.0), (0.0, 100.0));
        assert_eq!(scala(&Lato::SpesaApi, None, Some(250.0), 40.0), (0.0, 250.0));
        assert_eq!(scala(&Lato::SpesaApi, None, None, 40.0), (0.0, 80.0));
        assert!(chiede_massimo(None), "la spesa senza soglia chiede un massimo");
    }

    #[test]
    fn massimo_scritto_a_mano() {
        assert_eq!(leggi_massimo(""), Ok(None));
        assert_eq!(leggi_massimo(" 4500 "), Ok(Some(4500.0)));
        assert_eq!(leggi_massimo("4,5"), Ok(Some(4.5)));
        assert!(leggi_massimo("zero").is_err() && leggi_massimo("-2").is_err() && leggi_massimo("0").is_err());
        assert_eq!(testo_massimo(4500.0), "4500");
        assert_eq!(testo_massimo(4.5), "4,5");
    }
}
