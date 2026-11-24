//! Il riassunto di Casa (Home Assistant): la scala per stato della carta.

use super::{misura, riga, Azione, Riassunto, Tono, R1};
use crate::orologio::temperatura_breve;
use crate::ponte::{errore_del_portachiavi, frase_errore, riprovabile};
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
    Riassunto { tono, titolo, titolo_numerico: false, sec, sec_intera, azione, r1: R1::default() }
}

/// Riassunto con la riga già pronta (uguale per la carta e per Narrator).
fn fisso(tono: Tono, titolo: impl Into<String>, testo: &str, azione: Option<Azione>) -> Riassunto {
    let sec = riga(testo, azione.is_some());
    carta(tono, titolo.into(), sec, testo.into(), azione)
}

/// La frase della carta per un errore della casa: quella di `frase_errore` (intera per Narrator),
/// più corta nella carta, dove accanto c'è «Riprova». Sulla stessa chiave di `frase_errore`
/// (l'errore tecnico), non sul suo testo: se la frase cambia, non resta indietro.
pub fn frase_errore_breve(e: &str) -> &'static str {
    if errore_del_portachiavi(e) {
        "Token illeggibile"
    } else {
        "Non raggiungibile"
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
    Some(gradi(valore))
}

/// Una temperatura per la carta e per le card: un decimale al più e la virgola («21,5°», «9°»).
pub(super) fn gradi(valore: f64) -> String {
    let decimi = (valore * 10.0).round() / 10.0;
    let testo = if decimi.fract() == 0.0 { format!("{}", decimi as i64) } else { format!("{decimi:.1}") };
    format!("{}°", testo.replace('.', ","))
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

/// Il titolo della testata della pagina (`r1.fatto`): lo stato della connessione, poi le luci
/// accese a parole («Luci spente», mai «0 luci»). `accese` sono le luci come si vedono adesso,
/// con lo stato ottimistico dei comandi appena dati (`ponte::luci_accese_ora`). In connessione coi
/// dati di prima (casa-core lo dice a ogni tentativo di riconnessione) resta «Scollegata», come la
/// pagina che resta piena: il titolo non alterna «Scollegata» e «Mi collego» (Task 4b).
fn fatto_della_testata(stato: &StatoConnessione, menu: Option<&MenuData>, accese: usize) -> String {
    match (stato, menu) {
        (StatoConnessione::NonConfigurato, _) => "Da collegare".into(),
        (StatoConnessione::InConnessione, None) | (StatoConnessione::Connesso, None) => "Mi collego".into(),
        (StatoConnessione::Connesso, Some(_)) => match accese {
            0 => "Luci spente".into(),
            1 => "1 luce accesa".into(),
            n => format!("{n} luci accese"),
        },
        _ => "Scollegata".into(),
    }
}

/// La carta di Casa: lo stato della connessione, gli ultimi dati arrivati (`menu`), le luci accese
/// adesso (`accese`: con lo stato ottimistico, così «Spegni tutte» sparisce al tocco e non alla
/// conferma di Home Assistant) e l'ora in cui i dati sono arrivati (`dati_delle`, «hh:mm»).
pub fn riassunto(stato: &StatoConnessione, menu: Option<&MenuData>, accese: usize, dati_delle: Option<&str>) -> Riassunto {
    let mut r = carta_di(stato, menu, accese, dati_delle);
    r.r1 = R1 { fatto: fatto_della_testata(stato, menu, accese), valore: String::new() };
    r
}

/// La carta senza il titolo della testata (`riassunto` lo aggiunge).
fn carta_di(stato: &StatoConnessione, menu: Option<&MenuData>, accese: usize, dati_delle: Option<&str>) -> Riassunto {
    // In connessione coi dati di prima (casa-core lo dice a ogni tentativo di riconnessione: dopo
    // 1, 2, 4, 8, 16 s e poi ogni 30) la carta resta quella della casa che non risponde, con
    // «Riprova»: altrimenti il pulsante comparirebbe e sparirebbe sotto il puntatore a ogni
    // tentativo (giro di correzione 1 del Task 4b). Come il titolo della testata (`r1`).
    if *stato == StatoConnessione::InConnessione && menu.is_some() {
        return carta_di(&StatoConnessione::Errore(String::new()), menu, accese, dati_delle);
    }
    let collega = || azione(COLLEGA, "Collega", "collegamenti");
    match (stato, menu) {
        (StatoConnessione::NonConfigurato, _) => fisso(Tono::Vuoto, "Da collegare", "Collega la casa", collega()),
        (StatoConnessione::InConnessione, _) | (StatoConnessione::Connesso, None) => {
            fisso(Tono::Calmo, "Mi collego", "—", None)
        }
        (StatoConnessione::Connesso, Some(m)) => {
            // L'icona «spegni» (power), la stessa di «Spegni tutte» nella testata della pagina.
            let spegni = (accese > 0).then(|| azione(SPEGNI, "Spegni tutte", "spegni")).flatten();
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
                StatoConnessione::Errore(e) => (frase_errore_breve(e).into(), frase_errore(e).into()),
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
    use crate::ottimismo::Ottimismo;
    use crate::ponte::luci_accese_ora;
    use crate::ponte::test::{luce, menu, sensore};
    use casa_core::types::Meteo;

    /// Le luci accese come le vede il ponte senza tocchi in corso (`ponte::luci_accese_ora`).
    fn accese(m: Option<&MenuData>) -> usize {
        m.map_or(0, |m| luci_accese_ora(m, &Ottimismo::default()).len())
    }

    /// `super::riassunto` con le luci accese di chi non ha tocchi in corso: i test della scala
    /// non ripetono il conteggio a ogni riga (`accese_le_dice_il_chiamante` prova il parametro).
    fn riassunto(stato: &StatoConnessione, menu: Option<&MenuData>, dati_delle: Option<&str>) -> Riassunto {
        super::riassunto(stato, menu, accese(menu), dati_delle)
    }

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

    /// C1: le luci accese le dice il chiamante (con lo stato ottimistico dei tocchi), non il menu:
    /// il menu ne ha ancora quattro accese, ma due sono appena state spente e la carta non le conta
    /// (nessun «Spegni tutte» se non resta nulla da spegnere), prima di ogni conferma di HA.
    #[test]
    fn accese_le_dice_il_chiamante() {
        use StatoConnessione as S;
        let m = casa(4, Some("21"), None);
        let r = super::riassunto(&S::Connesso, Some(&m), 2, None);
        assert_eq!((r.titolo.as_str(), r.r1.fatto.as_str(), r.azione.as_ref().map(|a| a.id)), ("2 luci", "2 luci accese", Some(SPEGNI)));
        let r = super::riassunto(&S::Connesso, Some(&m), 1, None);
        assert_eq!((r.titolo.as_str(), r.r1.fatto.as_str()), ("1 luce", "1 luce accesa"));
        let r = super::riassunto(&S::Connesso, Some(&m), 0, None);
        assert_eq!((r.titolo.as_str(), r.r1.fatto.as_str(), r.azione.is_none()), ("Luci spente", "Luci spente", true));
        // Con un tocco in corso il conteggio è quello ottimistico, non quello del menu.
        let mut o = Ottimismo::<bool>::default();
        for id in ["light.l0", "light.l1", "light.l2", "light.l3"] {
            o.tocca(id, false, &true, std::time::Instant::now());
        }
        assert_eq!(luci_accese_ora(&m, &o).len(), 0);
        assert_eq!(super::riassunto(&S::Connesso, Some(&m), luci_accese_ora(&m, &o).len(), None).azione, None);
    }

    /// `frase_errore_breve` sceglie sulla stessa chiave di `frase_errore`: l'errore del portachiavi
    /// dà «Token illeggibile», ogni altro «Non raggiungibile», e la frase intera resta quella di
    /// `frase_errore` (la carta e Narrator non si scostano).
    #[test]
    fn frase_breve_sulla_chiave_della_frase_intera() {
        for (e, breve, intera) in [
            ("portachiavi: illeggibile", "Token illeggibile", "Token salvato non leggibile"),
            ("portachiavi", "Token illeggibile", "Token salvato non leggibile"),
            ("timeout", "Non raggiungibile", "Casa non raggiungibile"),
            ("", "Non raggiungibile", "Casa non raggiungibile"),
        ] {
            assert_eq!((frase_errore_breve(e), frase_errore(e)), (breve, intera), "{e:?}");
        }
    }

    /// Il titolo della testata (`r1.fatto`): lo stato della connessione, poi le luci accese a parole.
    #[test]
    fn r1_per_stato() {
        use StatoConnessione as S;
        let fatto = |s: &StatoConnessione, m: Option<&MenuData>| riassunto(s, m, Some("10:42")).r1;
        assert_eq!(fatto(&S::NonConfigurato, None).fatto, "Da collegare");
        assert_eq!(fatto(&S::InConnessione, None).fatto, "Mi collego");
        assert_eq!(fatto(&S::Connesso, None).fatto, "Mi collego", "connessa ma senza dati");
        assert_eq!(fatto(&S::InConnessione, Some(&casa(2, None, None))).fatto, "Scollegata", "riconnessione coi dati di prima");
        for s in [S::Disconnesso, S::TokenNonValido, S::TokenMancante, S::Errore("timeout".into())] {
            assert_eq!(fatto(&s, Some(&casa(2, None, None))).fatto, "Scollegata", "{s:?}");
        }
        assert_eq!(fatto(&S::Connesso, Some(&casa(0, None, None))).fatto, "Luci spente");
        assert_eq!(fatto(&S::Connesso, Some(&casa(1, None, None))).fatto, "1 luce accesa");
        assert_eq!(fatto(&S::Connesso, Some(&casa(4, Some("21"), Some(9.0)))).fatto, "4 luci accese");
        assert_eq!(fatto(&S::Connesso, Some(&casa(4, None, None))).valore, "");
    }

    /// Giro di correzione 1 del Task 4b: coi dati presenti, il riassunto in connessione (a ogni
    /// tentativo di riconnessione) è uguale a quello della casa che non risponde: tono, titolo,
    /// riga, azione «Riprova» e `r1`. Senza dati resta «Mi collego» senza azione.
    #[test]
    fn in_connessione_coi_dati_come_scollegata() {
        use StatoConnessione as S;
        for m in [casa(2, Some("21"), Some(9.0)), casa(0, None, None)] {
            let r = riassunto(&S::InConnessione, Some(&m), Some("10:42"));
            assert_eq!(r, riassunto(&S::Errore("timeout".into()), Some(&m), Some("10:42")));
            assert_eq!(vista(&r), (Tono::Fermo, "Scollegata", "Non raggiungibile", Some("Riprova")));
            assert_eq!(r.azione.map(|a| a.id), Some(RIPROVA));
        }
        assert_eq!(vista(&riassunto(&S::InConnessione, None, None)), (Tono::Calmo, "Mi collego", "—", None));
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
                assert!(!r.r1.fatto.is_empty() && !r.r1.fatto.ends_with('…'), "{r:?}");
            }
        }
        assert_eq!(riassunto(&S::Connesso, menu[1].as_ref(), None).r1.fatto, "100 luci accese");
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
            assert!(!r.r1.fatto.contains('0') && !r.r1.valore.contains('0'), "{s:?}: {r:?}");
            assert!(!r.r1.fatto.is_empty(), "{s:?}: mai un vuoto nel titolo della testata");
        }
        let r = riassunto(&S::Connesso, Some(&casa(0, None, None)), None);
        assert!(!r.titolo.contains('0') && !r.sec.contains('0'), "nessuna luce accesa: parole, non zero: {r:?}");
        assert!(!r.r1.fatto.contains('0') && !r.r1.valore.contains('0'), "nessuna luce accesa: «Luci spente»: {r:?}");
    }
}
