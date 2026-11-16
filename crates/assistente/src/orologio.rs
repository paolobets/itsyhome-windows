//! Ora e data del pannello, in italiano.

use casa_core::types::Meteo;
use chrono::{Datelike, Local, NaiveDateTime, Timelike};
use std::time::Duration;

const GIORNI: [&str; 7] = ["Lunedì", "Martedì", "Mercoledì", "Giovedì", "Venerdì", "Sabato", "Domenica"];
const MESI: [&str; 12] = [
    "gennaio", "febbraio", "marzo", "aprile", "maggio", "giugno",
    "luglio", "agosto", "settembre", "ottobre", "novembre", "dicembre",
];

const GIORNI_BREVI: [&str; 7] = ["LUN", "MAR", "MER", "GIO", "VEN", "SAB", "DOM"];
const MESI_BREVI: [&str; 12] = ["GEN", "FEB", "MAR", "APR", "MAG", "GIU", "LUG", "AGO", "SET", "OTT", "NOV", "DIC"];

/// `("10:24", "Venerdì 13 novembre")` per un istante locale.
pub fn ora_e_data(t: NaiveDateTime) -> (String, String) {
    let ora = format!("{:02}:{:02}", t.hour(), t.minute());
    let giorno = GIORNI[t.weekday().num_days_from_monday() as usize];
    let data = format!("{giorno} {} {}", t.day(), MESI[t.month0() as usize]);
    (ora, data)
}

/// Ora e data di adesso, nel fuso del sistema.
pub fn adesso() -> (String, String) {
    ora_e_data(Local::now().naive_local())
}

/// La data per la riga alta: `"VEN 13 NOV"`.
pub fn data_breve(t: NaiveDateTime) -> String {
    format!(
        "{} {} {}",
        GIORNI_BREVI[t.weekday().num_days_from_monday() as usize],
        t.day(),
        MESI_BREVI[t.month0() as usize]
    )
}

/// La data breve («VEN 13 NOV») per esteso e in minuscolo, per chi la ascolta («venerdì 13
/// novembre»). `None` se il testo non è una data breve.
pub fn data_per_esteso(breve: &str) -> Option<String> {
    let mut parti = breve.split(' ');
    let (giorno, numero, mese) = (parti.next()?, parti.next()?, parti.next()?);
    let g = GIORNI_BREVI.iter().position(|g| *g == giorno)?;
    let m = MESI_BREVI.iter().position(|m| *m == mese)?;
    Some(format!("{} {numero} {}", GIORNI[g].to_lowercase(), MESI[m]))
}

/// Margine dopo lo scatto del minuto: chi si sveglia al minuto si sveglia appena dopo, mai un
/// attimo prima.
const MARGINE_MINUTO: Duration = Duration::from_millis(20);

/// Fra quanto scatta il prossimo minuto, dato il secondo e il nanosecondo di adesso.
/// Un secondo intercalare (60) fa scattare subito il minuto dopo.
pub fn al_prossimo_minuto(secondo: u32, nano: u32) -> Duration {
    let passato = Duration::from_secs(u64::from(secondo.min(60))) + Duration::from_nanos(u64::from(nano.min(999_999_999)));
    Duration::from_secs(60).saturating_sub(passato) + MARGINE_MINUTO
}

/// La condizione di Home Assistant (`weather.*`) in italiano; quelle sconosciute: «».
pub fn condizione_italiana(c: &str) -> &'static str {
    match c {
        "sunny" | "clear-night" => "Sereno",
        "partlycloudy" => "Poco nuvoloso",
        "cloudy" => "Nuvoloso",
        "fog" => "Nebbia",
        "rainy" => "Pioggia",
        "pouring" => "Pioggia forte",
        "lightning" | "lightning-rainy" => "Temporale",
        "snowy" => "Neve",
        "snowy-rainy" => "Nevischio",
        "hail" => "Grandine",
        "windy" | "windy-variant" => "Vento",
        "exceptional" => "Eccezionale",
        _ => "",
    }
}

/// La temperatura intera per la riga alta (`"9°"`); `None` se il meteo non la dà.
pub fn temperatura_breve(m: &Meteo) -> Option<String> {
    let t = m.temperatura.filter(|t| t.is_finite())?;
    // Per intero: niente «-0°» per -0,4.
    Some(format!("{}°", t.round() as i64))
}

#[cfg(test)]
mod test {
    use super::*;
    use chrono::NaiveDate;

    fn istante(a: i32, m: u32, g: u32, h: u32, min: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(a, m, g).unwrap().and_hms_opt(h, min, 0).unwrap()
    }

    #[test]
    fn venerdi_13_novembre() {
        assert_eq!(ora_e_data(istante(2026, 11, 13, 10, 24)), ("10:24".into(), "Venerdì 13 novembre".into()));
    }

    fn meteo(condizione: &str, temperatura: Option<f64>) -> Meteo {
        Meteo { entity_id: "weather.casa".into(), condizione: condizione.into(), temperatura, unita: "°C".into() }
    }

    #[test]
    fn data_breve_venerdi_13_novembre() {
        assert_eq!(data_breve(istante(2026, 11, 13, 10, 24)), "VEN 13 NOV");
        assert_eq!(data_breve(istante(2027, 1, 3, 0, 5)), "DOM 3 GEN");
        assert_eq!(data_per_esteso("VEN 13 NOV").as_deref(), Some("venerdì 13 novembre"));
        assert_eq!(data_per_esteso("DOM 3 GEN").as_deref(), Some("domenica 3 gennaio"));
        assert_eq!(data_per_esteso("boh"), None);
    }

    #[test]
    fn condizioni_in_italiano() {
        for (ha, it) in [
            ("sunny", "Sereno"), ("clear-night", "Sereno"), ("partlycloudy", "Poco nuvoloso"),
            ("cloudy", "Nuvoloso"), ("fog", "Nebbia"), ("rainy", "Pioggia"), ("pouring", "Pioggia forte"),
            ("lightning", "Temporale"), ("lightning-rainy", "Temporale"), ("snowy", "Neve"),
            ("snowy-rainy", "Nevischio"), ("hail", "Grandine"), ("windy", "Vento"),
            ("windy-variant", "Vento"), ("exceptional", "Eccezionale"), ("boh", ""), ("", ""),
        ] {
            assert_eq!(condizione_italiana(ha), it, "{ha}");
        }
    }

    #[test]
    fn temperatura_arrotondata() {
        assert_eq!(temperatura_breve(&meteo("cloudy", Some(9.4))).as_deref(), Some("9°"));
        assert_eq!(temperatura_breve(&meteo("cloudy", Some(8.5))).as_deref(), Some("9°"));
        assert_eq!(temperatura_breve(&meteo("cloudy", Some(-3.2))).as_deref(), Some("-3°"));
        assert_eq!(temperatura_breve(&meteo("cloudy", Some(-0.4))).as_deref(), Some("0°"));
        assert_eq!(temperatura_breve(&meteo("cloudy", None)), None);
        assert_eq!(temperatura_breve(&meteo("cloudy", Some(f64::NAN))), None);
    }

    #[test]
    fn mezzanotte_e_cifra_singola() {
        assert_eq!(ora_e_data(istante(2027, 1, 3, 0, 5)), ("00:05".into(), "Domenica 3 gennaio".into()));
    }

    #[test]
    fn prossimo_minuto() {
        assert_eq!(al_prossimo_minuto(15, 250_000_000), Duration::from_millis(44_750) + MARGINE_MINUTO);
        assert_eq!(al_prossimo_minuto(0, 0), Duration::from_secs(60) + MARGINE_MINUTO);
        assert_eq!(al_prossimo_minuto(59, 999_000_000), Duration::from_millis(1) + MARGINE_MINUTO);
        // Secondo intercalare: scatta subito il minuto dopo.
        assert_eq!(al_prossimo_minuto(60, 0), MARGINE_MINUTO);
    }
}
