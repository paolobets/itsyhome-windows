//! Il riassunto di Casa (Home Assistant): la scala per stato della carta.

use super::{misura, riga, Azione, Riassunto, Tono};
use crate::orologio::temperatura_breve;
use crate::ponte::{frase_errore, luci_accese, riprovabile};
use casa_core::eventi::StatoConnessione;
use casa_core::types::{AppEntity, MenuData};

/// «Spegni tutte»: le luci accese, come nella vista Casa filtrata.
pub const SPEGNI: &str = "casa.spegni";
/// «Riprova»: la connessione alla casa che non risponde.
pub const RIPROVA: &str = "casa.riprova";
/// «Collega»: le Impostazioni sulle case.
pub const COLLEGA: &str = "casa.collega";

fn azione(id: &'static str, testo: &str, icona: &'static str) -> Option<Azione> {
    Some(Azione { id, testo: testo.into(), icona })
}

fn carta(tono: Tono, titolo: String, sec: String, sec_intera: String, azione: Option<Azione>) -> Riassunto {
    Riassunto { tono, titolo, titolo_numerico: false, sec, sec_intera, azione }
}

/// Riassunto con la riga già pronta (uguale per la carta e per Narrator).
fn fisso(tono: Tono, titolo: impl Into<String>, testo: &str, azione: Option<Azione>) -> Riassunto {
    let sec = riga(testo, azione.is_some());
    carta(tono, titolo.into(), sec, testo.into(), azione)
}

/// La frase della carta per un errore della casa: quella di `frase_errore` (intera per Narrator),
/// più corta nella carta, dove accanto c'è «Riprova».
fn frase_breve(frase: &'static str) -> &'static str {
    match frase {
        "Casa non raggiungibile" => "Non raggiungibile",
        "Token salvato non leggibile" => "Token illeggibile",
        altra => altra,
    }
}

/// La temperatura di dentro: il primo sensore di temperatura disponibile fra i preferiti, poi
/// nelle stanze (lo stesso di `riga_stato`), con un decimale al più e la virgola («21,5°»).
fn dentro(menu: &MenuData) -> Option<String> {
    let valore = menu.favorites.iter().chain(menu.rooms.iter().flat_map(|r| r.entities.iter())).find_map(|e| match e {
        AppEntity::Sensor(s) if s.base.is_available && s.device_class.as_deref() == Some("temperature") => {
            s.value.trim().parse::<f64>().ok().filter(|v| v.is_finite())
        }
        _ => None,
    })?;
    let decimi = (valore * 10.0).round() / 10.0;
    let testo = if decimi.fract() == 0.0 { format!("{}", decimi as i64) } else { format!("{decimi:.1}") };
    Some(format!("{}°", testo.replace('.', ",")))
}

/// La riga delle temperature, con quelle che ci sono: «21,5° dentro, 9° fuori»; se tutte e due
/// non stanno accanto al bottone, nella carta resta quella di dentro. (riga della carta, riga
/// intera per Narrator); `None` senza temperature.
fn temperature(menu: &MenuData, con_azione: bool) -> Option<(String, String)> {
    let fuori = menu.meteo.as_ref().and_then(temperatura_breve);
    match (dentro(menu), fuori) {
        (Some(d), Some(f)) => {
            let tutte = format!("{d} dentro, {f} fuori");
            let spazio = if con_azione { misura::SPAZIO_CON_AZIONE } else { misura::SPAZIO_PIENO } - misura::MARGINE;
            let carta = if misura::larghezza(&tutte, misura::Stile::Riga) <= spazio { tutte.clone() } else { format!("{d} dentro") };
            Some((carta, tutte))
        }
        (Some(d), None) => Some((format!("{d} dentro"), format!("{d} dentro"))),
        (None, Some(f)) => Some((format!("{f} fuori"), format!("{f} fuori"))),
        (None, None) => None,
    }
}

/// La carta di Casa: lo stato della connessione, gli ultimi dati arrivati (`menu`) e l'ora in cui
/// sono arrivati (`dati_delle`, «hh:mm»).
pub fn riassunto(stato: &StatoConnessione, menu: Option<&MenuData>, dati_delle: Option<&str>) -> Riassunto {
    let collega = || azione(COLLEGA, "Collega", "collegamenti");
    match (stato, menu) {
        (StatoConnessione::NonConfigurato, _) => fisso(Tono::Vuoto, "Da collegare", "Collega la casa", collega()),
        (StatoConnessione::InConnessione, _) | (StatoConnessione::Connesso, None) => {
            fisso(Tono::Calmo, "Mi collego", "—", None)
        }
        (StatoConnessione::Connesso, Some(m)) => {
            let accese = luci_accese(m).len();
            let spegni = (accese > 0).then(|| azione(SPEGNI, "Spegni tutte", "luce")).flatten();
            let (temperature, intere) = temperature(m, spegni.is_some())
                .unwrap_or_else(|| ("Nessun sensore".into(), "Nessun sensore".into()));
            let titolo = match accese {
                0 => "Luci spente".to_owned(),
                1 => "1 luce".to_owned(),
                n => format!("{n} luci"),
            };
            let mut r = fisso(Tono::Calmo, titolo, &temperature, spegni);
            r.sec_intera = intere;
            r
        }
        (StatoConnessione::Errore(_) | StatoConnessione::Disconnesso | StatoConnessione::TokenNonValido | StatoConnessione::TokenMancante, _) => {
            let azione = if riprovabile(stato) { azione(RIPROVA, "Riprova", "collegamenti") } else { collega() };
            let (breve, intera): (String, String) = match stato {
                StatoConnessione::Errore(e) => {
                    let frase = frase_errore(e);
                    (frase_breve(frase).into(), frase.into())
                }
                StatoConnessione::TokenNonValido => ("Token non valido".into(), "Token non valido".into()),
                StatoConnessione::TokenMancante => ("Token mancante".into(), "Token mancante".into()),
                _ => {
                    let eta = dati_delle.map_or_else(|| "Nessun dato".to_owned(), |ora| format!("Dati delle {ora}"));
                    (eta.clone(), eta)
                }
            };
            let sec = riga(&breve, azione.is_some());
            carta(Tono::Fermo, "Scollegata".into(), sec, intera, azione)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::moduli::controlla;
    use crate::ponte::test::{luce, menu, sensore};
    use casa_core::types::Meteo;

    fn vista(r: &Riassunto) -> (Tono, &str, &str, Option<&str>) {
        (r.tono, r.titolo.as_str(), r.sec.as_str(), r.azione.as_ref().map(|a| a.testo.as_str()))
    }

    fn meteo(t: f64) -> Meteo {
        Meteo { entity_id: "weather.casa".into(), condizione: "cloudy".into(), temperatura: Some(t), unita: "°C".into() }
    }

    /// Una casa con `accese` luci accese, una spenta e, se dati, il sensore di dentro e il meteo.
    fn casa(accese: usize, dentro: Option<&str>, fuori: Option<f64>) -> MenuData {
        let mut stanza: Vec<AppEntity> = (0..accese).map(|i| luce(&format!("light.l{i}"), true)).collect();
        stanza.push(luce("light.spenta", false));
        if let Some(d) = dentro {
            stanza.push(sensore("sensor.t", d, Some("°C")));
        }
        let mut m = menu(vec![], vec![stanza]);
        m.meteo = fuori.map(meteo);
        m
    }

    #[test]
    fn scala_per_stato() {
        use StatoConnessione as S;
        let r = riassunto(&S::NonConfigurato, None, None);
        assert_eq!(vista(&r), (Tono::Vuoto, "Da collegare", "Collega la casa", Some("Collega")));
        assert_eq!(r.azione.map(|a| a.id), Some(COLLEGA));
        assert_eq!(vista(&riassunto(&S::InConnessione, None, None)), (Tono::Calmo, "Mi collego", "—", None));

        // Non connessa: la frase dell'errore (corta nella carta, intera per Narrator) o l'età dei dati.
        let r = riassunto(&S::Errore("timeout".into()), None, Some("10:42"));
        assert_eq!(vista(&r), (Tono::Fermo, "Scollegata", "Non raggiungibile", Some("Riprova")));
        assert_eq!((r.sec_intera.as_str(), r.azione.map(|a| a.id)), ("Casa non raggiungibile", Some(RIPROVA)));
        let r = riassunto(&S::Errore("portachiavi: illeggibile".into()), None, None);
        assert_eq!((r.sec.as_str(), r.sec_intera.as_str()), ("Token illeggibile", "Token salvato non leggibile"));
        let r = riassunto(&S::Disconnesso, Some(&casa(2, None, None)), Some("10:42"));
        assert_eq!(vista(&r), (Tono::Fermo, "Scollegata", "Dati delle 10:42", Some("Collega")));
        assert_eq!(r.azione.map(|a| a.id), Some(COLLEGA));
        assert_eq!(vista(&riassunto(&S::TokenNonValido, None, None)), (Tono::Fermo, "Scollegata", "Token non valido", Some("Collega")));
        assert_eq!(vista(&riassunto(&S::TokenMancante, None, None)), (Tono::Fermo, "Scollegata", "Token mancante", Some("Collega")));

        // Connessa: le luci accese e le temperature che ci sono (accanto al bottone, se non ci
        // stanno tutte e due, quella di dentro).
        let m = casa(4, Some("9.04"), Some(9.4));
        let r = riassunto(&S::Connesso, Some(&m), Some("10:42"));
        assert_eq!(vista(&r), (Tono::Calmo, "4 luci", "9° dentro, 9° fuori", Some("Spegni tutte")));
        assert_eq!(r.azione.map(|a| a.id), Some(SPEGNI));
        let m = casa(4, Some("21.53"), Some(9.4));
        let r = riassunto(&S::Connesso, Some(&m), None);
        assert_eq!(r.sec, "21,5° dentro");
        assert_eq!(r.sec_intera, "21,5° dentro, 9° fuori", "Narrator le legge tutte e due");
        assert_eq!(riassunto(&S::Connesso, Some(&casa(0, Some("21.53"), Some(9.4))), None).sec, "21,5° dentro, 9° fuori");
        assert_eq!(riassunto(&S::Connesso, Some(&casa(1, Some("21"), None)), None).titolo, "1 luce");
        assert_eq!(riassunto(&S::Connesso, Some(&casa(1, Some("21"), None)), None).sec, "21° dentro");
        assert_eq!(riassunto(&S::Connesso, Some(&casa(1, None, Some(-3.0))), None).sec, "-3° fuori");
        assert_eq!(vista(&riassunto(&S::Connesso, Some(&casa(0, Some("19.0"), Some(9.0))), None)), (Tono::Calmo, "Luci spente", "19° dentro, 9° fuori", None));
        assert_eq!(vista(&riassunto(&S::Connesso, Some(&casa(0, None, None)), None)), (Tono::Calmo, "Luci spente", "Nessun sensore", None));
        assert_eq!(vista(&riassunto(&S::Connesso, Some(&casa(3, None, None)), None)), (Tono::Calmo, "3 luci", "Nessun sensore", Some("Spegni tutte")));
    }

    /// Coi dati peggiori (cento luci, temperature negative coi decimali, la frase d'errore più
    /// lunga di `frase_errore`) ogni gradino sta nella carta senza accorciare un testo fisso.
    #[test]
    fn ogni_gradino_sta_nei_limiti() {
        use StatoConnessione as S;
        let stati = [
            S::NonConfigurato,
            S::InConnessione,
            S::Connesso,
            S::Disconnesso,
            S::TokenNonValido,
            S::TokenMancante,
            S::Errore("timeout".into()),
            S::Errore("portachiavi: illeggibile".into()),
        ];
        let menu = [None, Some(casa(100, Some("-12.55"), Some(-12.4))), Some(casa(0, Some("-12.55"), Some(-12.4)))];
        for s in &stati {
            for m in &menu {
                let r = riassunto(s, m.as_ref(), Some("23:59"));
                assert_eq!(controlla("casa", &r), Ok(()), "{r:?}");
                assert!(!r.sec.ends_with('…'), "un testo fisso non si accorcia: {r:?}");
            }
        }
        let cento = riassunto(&S::Connesso, menu[1].as_ref(), None);
        assert_eq!((cento.titolo.as_str(), cento.sec.as_str()), ("100 luci", "-12,6° dentro"), "accanto al bottone resta dentro");
        let spente = riassunto(&S::Connesso, menu[2].as_ref(), None);
        assert_eq!(spente.sec, "-12,6° dentro, -12° fuori", "senza bottone ci stanno tutte e due");
    }

    /// Senza dati nessun «0»: «—» col motivo, o il motivo a parole.
    #[test]
    fn assenza_non_e_zero() {
        use StatoConnessione as S;
        for s in [S::NonConfigurato, S::InConnessione, S::Connesso, S::Disconnesso, S::TokenNonValido, S::TokenMancante, S::Errore("x".into())] {
            let r = riassunto(&s, None, None);
            assert!(!r.titolo.contains('0') && !r.sec.contains('0'), "{s:?}: {r:?}");
            assert!(!r.titolo.is_empty() && !r.sec.is_empty(), "{s:?}: mai un vuoto");
        }
        let r = riassunto(&S::Connesso, Some(&casa(0, None, None)), None);
        assert!(!r.titolo.contains('0') && !r.sec.contains('0'), "nessuna luce accesa: parole, non zero: {r:?}");
    }
}
