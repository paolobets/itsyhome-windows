//! Ora e data del pannello, in italiano.

use chrono::{Datelike, Local, NaiveDateTime, Timelike};

const GIORNI: [&str; 7] = ["Lunedì", "Martedì", "Mercoledì", "Giovedì", "Venerdì", "Sabato", "Domenica"];
const MESI: [&str; 12] = [
    "gennaio", "febbraio", "marzo", "aprile", "maggio", "giugno",
    "luglio", "agosto", "settembre", "ottobre", "novembre", "dicembre",
];

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

    #[test]
    fn mezzanotte_e_cifra_singola() {
        assert_eq!(ora_e_data(istante(2027, 1, 3, 0, 5)), ("00:05".into(), "Domenica 3 gennaio".into()));
    }
}
