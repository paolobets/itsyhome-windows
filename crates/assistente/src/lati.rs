//! Che cosa mostra la pillola ai lati degli occhi e che cosa mostrano i tre quadranti
//! configurabili (M2, Task 4): un solo catalogo di fonti (`Lato`), le scelte da offrire nelle
//! Impostazioni, di che cosa c'è bisogno (per accendere solo i timer che servono), il testo da
//! mostrare e le misure della pillola a riposo. Tutto puro: niente interfaccia, niente timer.
//!
//! I timer e i dati vivi stanno in `sorgenti_lati`; le misure qui coincidono con quelle di
//! pillola.slint (lo controlla `test_ui::misure_dei_lati_slint_e_rust`).

use crate::finestre;
use crate::pc_stats::{tinta_batteria, Fonti, LetturaBatteria, TintaBatteria};
use casa_core::config::PREFISSO_SENSORE;
use casa_core::types::{AppEntity, MenuData};
use std::collections::{HashMap, HashSet};

/// Una fonte per un lato della pillola o per un quadrante.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum Lato {
    #[default]
    Niente,
    Ora,
    Batteria,
    Cpu,
    Ram,
    /// La rete (Task 8): fino ad allora non si offre mai.
    Rete,
    /// La temperatura esterna, dal meteo di Home Assistant.
    Meteo,
    /// Un sensore di Home Assistant (`entity_id`).
    Sensore(String),
    /// Solo per i quadranti: il disco e la velocità della rete in arrivo e in uscita.
    Disco,
    ReteGiu,
    ReteSu,
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
            Lato::Sensore(id) => format!("{PREFISSO_SENSORE}{id}"),
        }
    }
}

/// Un sensore di Home Assistant che si può scegliere (dai dati della casa).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SensoreDisponibile {
    pub entity_id: String,
    pub nome: String,
    /// La `device_class` di HA («temperature», «humidity», «power»…).
    pub classe: Option<String>,
    pub unita: Option<String>,
}

/// I sensori disponibili fra preferiti e stanze, senza doppioni: per i lati solo le temperature
/// (ci stanno in pochi caratteri), per i quadranti tutti quelli che la casa manda.
pub fn sensori_disponibili(menu: &MenuData, per_quadranti: bool) -> Vec<SensoreDisponibile> {
    let mut visti = HashSet::new();
    menu.favorites
        .iter()
        .chain(menu.rooms.iter().flat_map(|r| r.entities.iter()))
        .filter_map(|e| match e {
            AppEntity::Sensor(s) if s.base.is_available => Some(s),
            _ => None,
        })
        .filter(|s| per_quadranti || s.device_class.as_deref() == Some("temperature"))
        .filter(|s| visti.insert(s.base.entity_id.clone()))
        .map(|s| SensoreDisponibile {
            entity_id: s.base.entity_id.clone(),
            nome: s.base.name.clone(),
            classe: s.device_class.clone(),
            unita: s.unit.clone(),
        })
        .collect()
}

/// Il nome di un sensore nel selettore: «Temperatura · Sala», «Umidità · Bagno»…
fn nome_sensore(s: &SensoreDisponibile) -> String {
    let genere = match s.classe.as_deref() {
        Some("temperature") => Some("Temperatura"),
        Some("humidity") => Some("Umidità"),
        Some("power") => Some("Potenza"),
        Some("energy") => Some("Energia"),
        _ => None,
    };
    match genere {
        Some(g) => format!("{g} · {}", s.nome),
        None => s.nome.clone(),
    }
}

/// Le scelte del selettore, nell'ordine: «Niente», «Ora», «Batteria» (solo se il PC ne ha una),
/// «CPU», «RAM», poi per i quadranti disco e rete in arrivo e in uscita; «Rete» solo se `rete` (dal
/// Task 8, mai fuori da Windows) e solo per i lati; «Temperatura esterna» solo se c'è un meteo; un
/// sensore per voce (`sensori`, già filtrati da `sensori_disponibili`).
pub fn scelte(
    meteo: bool,
    sensori: &[SensoreDisponibile],
    rete: bool,
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
    } else if rete {
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
            Lato::Niente | Lato::Disco | Lato::ReteGiu | Lato::ReteSu => {}
        }
    }
    b
}

/// I dati da cui nascono i lati (il campo `rete` lo aggiunge il Task 8).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DatiLati {
    /// «14:05»; vuota finché l'ora non serve.
    pub ora: String,
    pub batteria: Option<LetturaBatteria>,
    pub cpu: Option<f32>,
    pub ram: Option<f32>,
    /// La temperatura esterna già pronta («9°»).
    pub meteo: Option<String>,
    /// I valori grezzi dei sensori scelti, per `entity_id` («21.5»).
    pub sensori: HashMap<String, String>,
}

/// Che cosa mostra un lato.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ContenutoLato {
    #[default]
    Vuoto,
    Testo(String),
    /// Il glifo della batteria con la percentuale accanto.
    Batteria { testo: String, frazione: f32, tinta: TintaBatteria },
}

/// Un valore di Home Assistant come gradi brevi con la virgola italiana («21.5» → «21,5°»,
/// «20» → «20°»); `None` se non è un numero («unavailable»).
fn gradi(grezzo: &str) -> Option<String> {
    let v: f64 = grezzo.trim().replace(',', ".").parse().ok().filter(|v: &f64| v.is_finite())?;
    let arrotondato = (v * 10.0).round() / 10.0;
    let testo = if arrotondato.fract() == 0.0 {
        format!("{arrotondato:.0}")
    } else {
        format!("{arrotondato:.1}").replace('.', ",")
    };
    // Niente «-0°».
    Some(format!("{}°", if testo == "-0" { "0" } else { &testo }))
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
        Lato::Sensore(id) => d.sensori.get(id).and_then(|v| gradi(v)).map_or(ContenutoLato::Vuoto, ContenutoLato::Testo),
        _ => ContenutoLato::Vuoto,
    }
}

/// Il nome di un lato per Narrator, dentro il nome della pillola («CPU 23%», «batteria 80%»);
/// `None` se il lato è vuoto.
#[cfg_attr(not(test), allow(dead_code))] // il nome lo compone pillola.slint; qui per i test
pub fn nome_lato(c: &ContenutoLato) -> Option<String> {
    match c {
        ContenutoLato::Vuoto => None,
        ContenutoLato::Testo(t) => Some(t.clone()),
        ContenutoLato::Batteria { testo, .. } => Some(format!("batteria {testo}")),
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
    }
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
        Lato::Cpu | Lato::Ram | Lato::Disco | Lato::Batteria => (0.0, 100.0),
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
        ] {
            assert_eq!(Lato::da_str(&l.come_str()), l, "andata e ritorno di {l:?}");
        }
        assert_eq!(Lato::da_str("boh"), Lato::Niente);
        assert_eq!(Lato::da_str(""), Lato::Niente);
        assert_eq!(Lato::da_str("sensore:"), Lato::Niente, "un sensore senza id non è un sensore");
        assert_eq!(Lato::da_str("sensore:sensor.sala_t"), Lato::Sensore("sensor.sala_t".into()));
        assert_eq!(Lato::Niente.come_str(), "");
    }

    fn sensore(id: &str, nome: &str, classe: &str) -> SensoreDisponibile {
        SensoreDisponibile { entity_id: id.into(), nome: nome.into(), classe: Some(classe.into()), unita: None }
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
    }

    #[test]
    fn scelte_con_tutto_nell_ordine() {
        let sala = sensore("sensor.sala_t", "Sala", "temperature");
        let v = scelte(true, std::slice::from_ref(&sala), true, true, false);
        let nomi: Vec<&str> = v.iter().map(|(_, n)| n.as_str()).collect();
        assert_eq!(nomi, ["Niente", "Ora", "Batteria", "CPU", "RAM", "Rete", "Temperatura esterna", "Temperatura · Sala"]);
        // Per i quadranti: disco e rete in arrivo e in uscita, niente «Rete» (il nome della rete).
        let fv = SensoreDisponibile { unita: Some("W".into()), ..sensore("sensor.fv", "Fotovoltaico", "power") };
        let q = scelte(true, &[sala, fv], true, false, true);
        let nomi: Vec<&str> = q.iter().map(|(_, n)| n.as_str()).collect();
        assert_eq!(
            nomi,
            ["Niente", "Ora", "CPU", "RAM", "Disco", "Rete in arrivo", "Rete in uscita", "Temperatura esterna", "Temperatura · Sala", "Potenza · Fotovoltaico"]
        );
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
        d.sensori.insert("sensor.sala_t".into(), "21.5".into());
        d.sensori.insert("sensor.studio_t".into(), "20".into());
        let testo = |s: &str| ContenutoLato::Testo(s.into());
        assert_eq!(contenuto(&Lato::Cpu, &d), testo("CPU 23%"));
        assert_eq!(contenuto(&Lato::Ram, &d), testo("RAM 61%"));
        assert_eq!(contenuto(&Lato::Sensore("sensor.sala_t".into()), &d), testo("21,5°"));
        assert_eq!(contenuto(&Lato::Sensore("sensor.studio_t".into()), &d), testo("20°"));
        assert_eq!(contenuto(&Lato::Meteo, &d), testo("9°"));
        assert_eq!(contenuto(&Lato::Ora, &d), testo("14:05"));
        d.batteria = Some(LetturaBatteria { percentuale: 80.4, stato: StatoBatteria::Scarica });
        assert_eq!(
            contenuto(&Lato::Batteria, &d),
            ContenutoLato::Batteria { testo: "80%".into(), frazione: 0.804, tinta: TintaBatteria::Normale }
        );
        assert_eq!(nome_lato(&contenuto(&Lato::Batteria, &d)).as_deref(), Some("batteria 80%"));
        assert_eq!(nome_lato(&contenuto(&Lato::Cpu, &d)).as_deref(), Some("CPU 23%"));
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
        d.sensori.insert("sensor.x".into(), "unavailable".into());
        assert_eq!(contenuto(&Lato::Sensore("sensor.x".into()), &d), ContenutoLato::Vuoto);
        assert_eq!(nome_lato(&ContenutoLato::Vuoto), None);
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
        assert_eq!(fonti_dei_quadranti(&QUADRANTI_DI_M1), Fonti { disco: true, rete: false });
        assert_eq!(fonti_dei_quadranti(&[Lato::ReteSu]), Fonti { disco: false, rete: true });
    }

    #[test]
    fn scala_per_unita() {
        let s = Lato::Sensore("sensor.x".into());
        assert_eq!(scala(&Lato::Cpu, None, None, 50.0), (0.0, 100.0));
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
