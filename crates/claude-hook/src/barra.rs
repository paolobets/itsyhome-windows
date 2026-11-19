//! La barra di stato di Claude Code (spec M3 §4, §6.5 e §10).
//!
//! Claude Code lancia il comando della barra a ogni aggiornamento e gli passa un JSON su stdin.
//! `isola-hook barra` lo legge, stampa una riga breve e manda all'assistente in esecuzione, per il
//! tubo protetto, solo `DatiBarra`: mai lo stdin grezzo (niente cartelle, trascrizione, workspace;
//! del nome della sessione passa il solo testo ripulito, come `titolo`).
//! Il comando deve essere veloce: 40 ms in tutto, la riga prima di ogni invio.

use std::io::{self, Read, Write};
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};

use crate::evento::{sessione_valida, solo_testo, ErroreEvento};
use crate::messaggio::Richiesta;
use crate::tubo::{self, Destinazione};

/// Stdin letto al più per tanto: oltre, il JSON è troncato e non si legge.
pub const MASSIMO_STDIN: usize = 64 * 1024;
/// Lunghezza massima del nome del modello.
const MASSIMO_MODELLO: usize = 40;
/// Lunghezza massima del titolo della sessione, «…» compreso.
const MASSIMO_TITOLO: usize = 60;
/// Il testo mostrato quando non si sa altro.
const RIGA_VUOTA: &str = "Claude";

/// Un limite dell'abbonamento: quanto è usato e quando si azzera (secondi dall'epoca).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limite {
    pub percentuale: f32,
    pub azzera_alle: i64,
}

/// I soli dati della barra che servono; è anche il corpo inviato all'assistente.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatiBarra {
    pub sessione: String,
    pub modello: Option<String>,
    pub contesto: Option<f32>,
    pub cinque_ore: Option<Limite>,
    pub sette_giorni: Option<Limite>,
    /// Il nome della sessione (`session_name` della barra: quello di `--name` o `/rename`, o il
    /// titolo che Claude Code genera da sé), ripulito e al più 60 caratteri. Assente se la sessione
    /// non ne ha ancora uno. Gli hook non lo ricevono: arriva solo da qui.
    pub titolo: Option<String>,
}

/// Un valore del tipo giusto o niente: un campo strano non fa fallire tutto il resto.
fn tollerante<'de, D: Deserializer<'de>, T: DeserializeOwned>(d: D) -> Result<Option<T>, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(v).ok())
}

#[derive(Deserialize, Default)]
struct Modello {
    #[serde(default, deserialize_with = "solo_testo")]
    display_name: Option<String>,
}

#[derive(Deserialize, Default)]
struct Contesto {
    #[serde(default, deserialize_with = "tollerante")]
    used_percentage: Option<f64>,
}

#[derive(Deserialize, Default)]
struct Finestra {
    #[serde(default, deserialize_with = "tollerante")]
    used_percentage: Option<f64>,
    #[serde(default, deserialize_with = "tollerante")]
    resets_at: Option<f64>,
}

#[derive(Deserialize, Default)]
struct Limiti {
    #[serde(default, deserialize_with = "tollerante")]
    five_hour: Option<Finestra>,
    #[serde(default, deserialize_with = "tollerante")]
    seven_day: Option<Finestra>,
}

#[derive(Deserialize)]
struct Grezzo {
    #[serde(default, deserialize_with = "solo_testo")]
    session_id: Option<String>,
    #[serde(default, deserialize_with = "tollerante")]
    model: Option<Modello>,
    #[serde(default, deserialize_with = "tollerante")]
    context_window: Option<Contesto>,
    #[serde(default, deserialize_with = "tollerante")]
    rate_limits: Option<Limiti>,
    /// Il nome di `--name` o `/rename`, o il titolo generato: assente se la sessione non ne ha.
    #[serde(default, deserialize_with = "solo_testo")]
    session_name: Option<String>,
}

fn percentuale(p: Option<f64>) -> Option<f32> {
    p.filter(|p| p.is_finite())
        .map(|p| p.clamp(0.0, 100.0) as f32)
}

fn limite(f: Option<Finestra>) -> Option<Limite> {
    let f = f?;
    let p = percentuale(f.used_percentage)?;
    let r = f.resets_at.filter(|r| r.is_finite() && r.abs() < 1e12)?;
    Some(Limite {
        percentuale: p,
        azzera_alle: r as i64,
    })
}

/// Il nome del modello senza caratteri di controllo né invisibili, al più 40 caratteri.
fn nome_pulito(s: &str) -> Option<String> {
    let t: String = s
        .chars()
        .filter(|c| {
            !c.is_control()
                && !matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
        })
        .take(MASSIMO_MODELLO)
        .collect();
    let t = t.trim().to_string();
    (!t.is_empty()).then_some(t)
}

/// Il nome della sessione ripulito: a capo e tab diventano spazi, gli altri caratteri di controllo,
/// di direzione e zero-width spariscono, spazi ai lati tolti; oltre 60 caratteri (non byte) se ne
/// tengono 59 e si chiude con «…». Vuoto dopo la pulizia: nessun titolo. Ripulire un testo già
/// pulito non lo cambia: la convalida si appoggia a questo.
fn titolo_pulito(s: &str) -> Option<String> {
    let t: String = s
        .chars()
        .filter_map(|c| match c {
            '\n' | '\r' | '\t' => Some(' '),
            c if c.is_control() => None,
            '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}' => None,
            c => Some(c),
        })
        .collect();
    let t = t.trim();
    if t.chars().count() <= MASSIMO_TITOLO {
        return (!t.is_empty()).then(|| t.to_owned());
    }
    let corto: String = t.chars().take(MASSIMO_TITOLO - 1).collect();
    Some(format!("{}\u{2026}", corto.trim_end()))
}

impl Limite {
    fn stretto(self) -> Limite {
        Limite {
            percentuale: self.percentuale.clamp(0.0, 100.0),
            ..self
        }
    }
}

impl DatiBarra {
    /// Le stesse regole per chi legge lo stdin di Claude e per l'assistente che riceve il
    /// corpo inoltrato: sessione valida, percentuali finite in 0..=100, nome ripulito.
    pub(crate) fn convalida(mut self) -> Result<DatiBarra, ErroreEvento> {
        if sessione_valida(Some(&self.sessione)).is_none() {
            return Err(ErroreEvento::NonValido);
        }
        // Il titolo, a differenza del modello, non si corregge: chi l'ha costruito l'ha già
        // ripulito, e uno troppo lungo o sporco (un cliente che non è l'aiutante) si rifiuta.
        if self.titolo.as_ref().is_some_and(|t| titolo_pulito(t).as_deref() != Some(t.as_str())) {
            return Err(ErroreEvento::NonValido);
        }
        self.modello = self.modello.as_deref().and_then(nome_pulito);
        self.contesto = self
            .contesto
            .filter(|c| c.is_finite())
            .map(|c| c.clamp(0.0, 100.0));
        self.cinque_ore = self
            .cinque_ore
            .filter(|l| l.percentuale.is_finite())
            .map(Limite::stretto);
        self.sette_giorni = self
            .sette_giorni
            .filter(|l| l.percentuale.is_finite())
            .map(Limite::stretto);
        Ok(self)
    }
}

/// Legge i dati dallo stdin di Claude Code (solo i campi dei fatti verificati). `null`, tipi
/// strani e valori non finiti diventano «assente»; `session_id` con le regole degli eventi.
pub fn dati_da_json(corpo: &[u8]) -> Result<DatiBarra, ErroreEvento> {
    // Un BOM UTF-8 iniziale (alcuni strumenti lo scrivono) non è JSON.
    let corpo = corpo.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(corpo);
    let g: Grezzo = serde_json::from_slice(corpo).map_err(|_| ErroreEvento::NonValido)?;
    let sessione = sessione_valida(g.session_id.as_deref())
        .ok_or(ErroreEvento::NonValido)?
        .to_string();
    let r = g.rate_limits.unwrap_or_default();
    DatiBarra {
        sessione,
        modello: g.model.and_then(|m| m.display_name),
        contesto: g
            .context_window
            .and_then(|c| percentuale(c.used_percentage)),
        cinque_ore: limite(r.five_hour),
        sette_giorni: limite(r.seven_day),
        titolo: g.session_name.as_deref().and_then(titolo_pulito),
    }
    .convalida()
}

/// «Opus 5.5 · contesto 42% · 5h 23% (↻ 14:30) · 7g 61%»: le parti mancanti si omettono, un
/// limite già azzerato (`azzera_alle <= ora`) non si mostra, senza nulla «Claude».
pub fn riga(d: &DatiBarra, ora: i64, ora_locale: impl Fn(i64) -> String) -> String {
    let arrotonda = |p: f32| p.round() as i64;
    let mut parti: Vec<String> = Vec::new();
    if let Some(m) = &d.modello {
        parti.push(m.clone());
    }
    if let Some(c) = d.contesto {
        parti.push(format!("contesto {}%", arrotonda(c)));
    }
    if let Some(l) = d.cinque_ore.filter(|l| l.azzera_alle > ora) {
        parti.push(format!(
            "5h {}% (\u{21bb} {})",
            arrotonda(l.percentuale),
            ora_locale(l.azzera_alle)
        ));
    }
    if let Some(l) = d.sette_giorni.filter(|l| l.azzera_alle > ora) {
        parti.push(format!("7g {}%", arrotonda(l.percentuale)));
    }
    if parti.is_empty() {
        RIGA_VUOTA.to_string()
    } else {
        parti.join(" \u{b7} ")
    }
}

fn ora_corrente() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn ora_locale_di_sistema(t: i64) -> String {
    use chrono::{DateTime, Local};
    DateTime::from_timestamp(t, 0)
        .map(|d| d.with_timezone(&Local).format("%H:%M").to_string())
        .unwrap_or_default()
}

/// Il comando della barra: legge lo stdin (al più 64 KB), stampa la riga e solo dopo, se c'è
/// isola (verificata, vedi `tubo::collega`), le manda `DatiBarra` entro `tempo` in tutto. Mai errori
/// verso Claude Code per un assistente spento: la riga è già stata scritta.
pub fn esegui(
    ingresso: impl Read,
    mut uscita: impl Write,
    destinazione: Option<&Destinazione>,
    tempo: Duration,
) -> io::Result<()> {
    let inizio = Instant::now();
    let mut corpo = Vec::new();
    // Un errore di lettura vale come stdin vuoto: si stampa comunque «Claude».
    let _ = ingresso.take(MASSIMO_STDIN as u64).read_to_end(&mut corpo);
    let dati = dati_da_json(&corpo).ok();
    let testo = match &dati {
        Some(d) => riga(d, ora_corrente(), ora_locale_di_sistema),
        None => RIGA_VUOTA.to_string(),
    };
    writeln!(uscita, "{testo}")?;
    uscita.flush()?;
    if let (Some(d), Some(dest)) = (dati, destinazione) {
        manda(d, dest, inizio, tempo);
    }
    Ok(())
}

fn resto(inizio: Instant, tempo: Duration) -> Option<Duration> {
    let r = tempo.saturating_sub(inizio.elapsed());
    (!r.is_zero()).then_some(r)
}

/// Un errore del tubo non è un errore della barra: si rinuncia in silenzio.
fn manda(d: DatiBarra, dest: &Destinazione, inizio: Instant, tempo: Duration) {
    let Some(messaggio) = Richiesta::Barra(d).in_byte() else {
        return;
    };
    let Some(r) = resto(inizio, tempo) else {
        return;
    };
    let Some(canale) = tubo::collega(dest, r) else {
        return;
    };
    let Some(r) = resto(inizio, tempo) else {
        return;
    };
    if !canale.manda(&messaggio, r) {
        return;
    }
    // Si aspetta il «ricevuto» finché c'è tempo: isola controlla chi siamo prima di leggere, e un
    // aiutante già uscito non si potrebbe più riconoscere.
    if let Some(r) = resto(inizio, tempo) {
        let _ = canale.ricevi(r);
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const T: i64 = 1_800_000_000;

    fn completo() -> String {
        format!(
            r#"{{"session_id":"sess-1","cwd":"C:\\Users\\segreto\\progetto","transcript_path":"C:\\segreto\\t.jsonl",
            "session_name":"nome-segreto","workspace":{{"current_dir":"C:\\segreto"}},
            "model":{{"id":"x","display_name":"Opus 5.5"}},
            "context_window":{{"used_percentage":42.4}},
            "rate_limits":{{"five_hour":{{"used_percentage":23.5,"resets_at":{}}},"seven_day":{{"used_percentage":61,"resets_at":{}}}}}}}"#,
            T + 600,
            T + 86_400
        )
    }

    fn ora_fissa(t: i64) -> String {
        assert_eq!(t, T + 600);
        "14:30".into()
    }

    #[test]
    fn riga_completa() {
        let d = dati_da_json(completo().as_bytes()).unwrap();
        assert_eq!(
            riga(&d, T, ora_fissa),
            "Opus 5.5 \u{b7} contesto 42% \u{b7} 5h 24% (\u{21bb} 14:30) \u{b7} 7g 61%"
        );
    }

    #[test]
    fn riga_senza_abbonamento() {
        let d = dati_da_json(
            br#"{"session_id":"s","model":{"display_name":"Opus 5.5"},"context_window":{"used_percentage":42.4}}"#,
        )
        .unwrap();
        assert_eq!(riga(&d, T, ora_fissa), "Opus 5.5 \u{b7} contesto 42%");
    }

    #[test]
    fn percentuale_null_ignorata() {
        let d = dati_da_json(
            br#"{"session_id":"s","model":{"display_name":"Opus 5.5"},"context_window":{"used_percentage":null}}"#,
        )
        .unwrap();
        assert_eq!(d.contesto, None);
        assert_eq!(riga(&d, T, ora_fissa), "Opus 5.5");
    }

    #[test]
    fn limite_scaduto_non_mostrato() {
        let d = dati_da_json(completo().as_bytes()).unwrap();
        // Il limite delle 5 ore si azzera a T+600: dopo, non si mostra; l'altro sì.
        let r = riga(&d, T + 601, |_| "x".into());
        assert!(!r.contains("5h"), "{r}");
        assert!(r.contains("7g 61%"), "{r}");
        // Esattamente all'azzeramento vale come scaduto.
        assert!(!riga(&d, T + 600, |_| "x".into()).contains("5h"));
    }

    #[test]
    fn senza_nulla_claude_e_valori_strani_scartati() {
        let d = DatiBarra {
            sessione: "s".into(),
            modello: None,
            contesto: None,
            cinque_ore: None,
            sette_giorni: None,
            titolo: None,
        };
        assert_eq!(riga(&d, T, ora_fissa), "Claude");
        let d = dati_da_json(
            br#"{"session_id":"s","model":{"display_name":"A\u001b[31mB\u202e"},"context_window":{"used_percentage":"molto"},
            "rate_limits":{"five_hour":{"used_percentage":150,"resets_at":"domani"},"seven_day":{"used_percentage":1e30,"resets_at":5}}}"#,
        )
        .unwrap();
        assert_eq!(d.modello.as_deref(), Some("A[31mB"));
        assert_eq!(d.contesto, None);
        assert_eq!(d.cinque_ore, None);
        // Percentuale enorme: si stringe a 100.
        assert_eq!(
            d.sette_giorni,
            Some(Limite {
                percentuale: 100.0,
                azzera_alle: 5
            })
        );
        assert!(dati_da_json(br#"{"session_id":"a b"}"#).is_err());
        assert!(dati_da_json(br#"{"model":{}}"#).is_err());
        assert!(dati_da_json(b"[1]").is_err());
    }

    /// I dati arrivati per il tubo, riletti e riconvalidati da isola (`Richiesta::da_byte`).
    fn inoltrati(corpo: &[u8]) -> Result<DatiBarra, ErroreEvento> {
        let mut m = br#"{"Barra":"#.to_vec();
        m.extend_from_slice(corpo);
        m.push(b'}');
        match Richiesta::da_byte(&m)? {
            Richiesta::Barra(d) => Ok(d),
            Richiesta::Evento(_) => Err(ErroreEvento::NonValido),
        }
    }

    #[test]
    fn corpo_inoltrato_si_riconvalida() {
        let d = dati_da_json(completo().as_bytes()).unwrap();
        let corpo = serde_json::to_vec(&d).unwrap();
        assert_eq!(inoltrati(&corpo).unwrap(), d);
        assert!(inoltrati(br#"{"sessione":"a b"}"#).is_err());
        let sporco = br#"{"sessione":"s","modello":"X\u0007","contesto":500.0,"cinque_ore":null,"sette_giorni":null}"#;
        let r = inoltrati(sporco).unwrap();
        assert_eq!(r.modello.as_deref(), Some("X"));
        assert_eq!(r.contesto, Some(100.0));
        // Campi in più: rifiutato.
        let extra = br#"{"sessione":"s","modello":null,"contesto":null,"cinque_ore":null,"sette_giorni":null,"cwd":"x"}"#;
        assert!(inoltrati(extra).is_err());
    }

    /// Un corpo inoltrato col solo `titolo` variabile.
    fn con_titolo(titolo: &str) -> Vec<u8> {
        format!(
            r#"{{"sessione":"s","modello":null,"contesto":null,"cinque_ore":null,"sette_giorni":null,"titolo":{titolo}}}"#
        )
        .into_bytes()
    }

    #[test]
    fn titolo_della_sessione_dalla_barra() {
        let d = dati_da_json(completo().as_bytes()).unwrap();
        assert_eq!(d.titolo.as_deref(), Some("nome-segreto"));
        let con = |nome: &str| {
            dati_da_json(format!(r#"{{"session_id":"s","session_name":{nome}}}"#).as_bytes()).unwrap().titolo
        };
        // Assente, null, di un altro tipo, vuoto o fatto di soli invisibili: nessun titolo.
        assert_eq!(dati_da_json(br#"{"session_id":"s"}"#).unwrap().titolo, None);
        let invisibili = format!("\"{}{}\"", '\u{200b}', '\u{202e}');
        for niente in ["null", "42", "[]", r#""""#, r#""  \t\n ""#, invisibili.as_str()] {
            assert_eq!(con(niente), None, "{niente}");
        }
        // Spazi ai lati tolti; controlli, direzione e zero-width via; a capo e tab diventano spazi.
        let sporco = format!("\"  Sistemare{} il\\nrestyle{} \"", '\u{202e}', '\u{200b}');
        assert_eq!(con(&sporco).as_deref(), Some("Sistemare il restyle"));
    }

    #[test]
    fn titolo_lungo_si_taglia_a_sessanta() {
        let con = |n: usize| {
            let nome = "è".repeat(n);
            dati_da_json(format!(r#"{{"session_id":"s","session_name":"{nome}"}}"#).as_bytes()).unwrap().titolo.unwrap()
        };
        // Fino a 60 caratteri resta intero (non 60 byte: «è» ne pesa due).
        assert_eq!(con(60), "è".repeat(60));
        // Oltre: 59 caratteri e «…», 60 in tutto, senza spezzare un carattere.
        let t = con(61);
        assert_eq!(t.chars().count(), 60);
        assert_eq!(t, format!("{}\u{2026}", "è".repeat(59)));
        assert_eq!(con(500).chars().count(), 60);
        // Lo spazio prima del taglio non resta appeso al «…».
        let t = dati_da_json(format!(r#"{{"session_id":"s","session_name":"{} {}"}}"#, "a".repeat(58), "b".repeat(10)).as_bytes())
            .unwrap()
            .titolo
            .unwrap();
        assert_eq!(t, format!("{}\u{2026}", "a".repeat(58)));
    }

    #[test]
    fn titolo_inoltrato_si_riconvalida_e_si_rifiuta_se_sporco() {
        // Il titolo ripulito passa e torna uguale.
        let mut d = dati_da_json(completo().as_bytes()).unwrap();
        d.titolo = Some("Sistemare il restyle".into());
        assert_eq!(inoltrati(&serde_json::to_vec(&d).unwrap()).unwrap(), d);
        assert_eq!(inoltrati(&con_titolo("null")).unwrap().titolo, None);
        // Il campo può mancare del tutto (un aiutante più vecchio).
        let senza = br#"{"sessione":"s","modello":null,"contesto":null,"cinque_ore":null,"sette_giorni":null}"#;
        assert_eq!(inoltrati(senza).unwrap().titolo, None);
        // Il 60 esatto passa, il 61 no.
        assert!(inoltrati(&con_titolo(&format!("\"{}\"", "a".repeat(60)))).is_ok());
        assert!(inoltrati(&con_titolo(&format!("\"{}\"", "a".repeat(61)))).is_err());
        // Sporco (controlli, direzione, zero-width), non rifilato o vuoto: rifiutato, non corretto.
        let bidi = format!("\"a{}b\"", '\u{202e}');
        let zero_width = format!("\"a{}b\"", '\u{200b}');
        for no in [r#""a\u0007b""#, bidi.as_str(), zero_width.as_str(), r#"" a""#, r#""a ""#, r#""""#, r#""a\nb""#, "42"] {
            assert!(inoltrati(&con_titolo(no)).is_err(), "{no}");
        }
    }

    #[test]
    fn bom_iniziale_ignorato() {
        let mut con_bom = vec![0xEF, 0xBB, 0xBF];
        con_bom.extend_from_slice(completo().as_bytes());
        assert_eq!(
            dati_da_json(&con_bom).unwrap(),
            dati_da_json(completo().as_bytes()).unwrap()
        );
    }

    #[test]
    fn stdin_oltre_64k() {
        let spazzatura = vec![b'x'; 100 * 1024];
        let mut uscita = Vec::new();
        esegui(
            &spazzatura[..],
            &mut uscita,
            None,
            Duration::from_millis(40),
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(uscita).unwrap(),
            "Claude
"
        );
        // Un JSON valido ma più grande del limite si tronca e non si legge.
        let mut grande = br#"{"session_id":"s","pad":""#.to_vec();
        grande.extend(std::iter::repeat_n(b'a', 100 * 1024));
        grande.extend_from_slice(br#""}"#);
        let mut uscita = Vec::new();
        esegui(&grande[..], &mut uscita, None, Duration::from_millis(40)).unwrap();
        assert_eq!(
            String::from_utf8(uscita).unwrap(),
            "Claude
"
        );
    }

    #[test]
    fn assistente_spento_stampa_lo_stesso() {
        // Un tubo che non c'è.
        let d = Destinazione {
            nome: crate::tubo::nome_di_prova(),
            server_atteso: std::env::current_exe().unwrap(),
        };
        let mut uscita = Vec::new();
        let t = Instant::now();
        esegui(
            completo().as_bytes(),
            &mut uscita,
            Some(&d),
            Duration::from_millis(40),
        )
        .unwrap();
        let durata = t.elapsed();
        assert!(String::from_utf8(uscita).unwrap().starts_with("Opus 5.5"));
        // Su Windows (dove gira davvero) il limite della barra; altrove senza tubo conta solo che
        // finisca presto (le macchine della CI hanno tempi variabili).
        let limite = if cfg!(windows) { 50 } else { 500 };
        assert!(durata < Duration::from_millis(limite), "{durata:?}");
    }

    /// Un gestore che tiene le barre ricevute.
    #[cfg(windows)]
    struct Raccolta(std::sync::Mutex<Vec<crate::Arrivo>>);
    #[cfg(windows)]
    impl crate::Gestore for Raccolta {
        fn evento(&self, _: crate::Arrivo) {}
        fn permesso(&self, _: crate::Arrivo) -> Option<crate::AttesaPermesso> {
            None
        }
        fn abbandonato(&self, _: u64) {}
        fn barra(&self, a: crate::Arrivo) {
            self.0.lock().unwrap().push(a);
        }
    }

    /// Contro il servitore vero (questo processo è sia isola sia l'aiutante): arrivano solo i
    /// `DatiBarra`, uguali a quelli letti, e la riga è stampata per prima.
    #[cfg(windows)]
    #[test]
    fn esegui_contro_il_servitore_vero() {
        use crate::{avvia, ConfigTubo, Contenuto, Gestore, ATTESA_MASSIMA};
        let exe = std::env::current_exe().unwrap();
        let nome = crate::tubo::nome_di_prova();
        let g = std::sync::Arc::new(Raccolta(Default::default()));
        let s = avvia(
            ConfigTubo {
                nome: nome.clone(),
                cliente_atteso: exe.clone(),
                attesa_massima: ATTESA_MASSIMA,
            },
            g.clone() as std::sync::Arc<dyn Gestore>,
        )
        .unwrap();
        let d = Destinazione {
            nome,
            server_atteso: exe,
        };
        let mut uscita = Vec::new();
        esegui(
            completo().as_bytes(),
            &mut uscita,
            Some(&d),
            Duration::from_millis(500),
        )
        .unwrap();
        let inizio = Instant::now();
        while g.0.lock().unwrap().is_empty() && inizio.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(10));
        }
        let ricevute = g.0.lock().unwrap();
        assert_eq!(ricevute.len(), 1, "il gestore doveva ricevere la barra");
        assert!(
            matches!(&ricevute[0].contenuto, Contenuto::Barra(x) if *x == dati_da_json(completo().as_bytes()).unwrap())
        );
        assert_eq!(s.rifiutate(), 0);
        let riga = String::from_utf8(uscita).unwrap();
        assert!(riga.starts_with("Opus 5.5"), "{riga}");
        // Con un server che non è quello atteso niente parte, la riga sì.
        drop(ricevute);
        let altra = Destinazione {
            nome: d.nome.clone(),
            server_atteso: d.server_atteso.with_file_name("assistente.exe"),
        };
        let mut uscita = Vec::new();
        esegui(
            completo().as_bytes(),
            &mut uscita,
            Some(&altra),
            Duration::from_millis(500),
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(g.0.lock().unwrap().len(), 1, "nessuna barra in più");
        assert!(String::from_utf8(uscita).unwrap().starts_with("Opus 5.5"));
    }

    #[test]
    fn i_dati_inoltrati_non_portano_campi_extra() {
        let d = dati_da_json(completo().as_bytes()).unwrap();
        let b = Richiesta::Barra(d).in_byte().unwrap();
        let t = String::from_utf8(b).unwrap();
        for vietato in [
            "C:\\\\segreto",
            "transcript",
            "cwd",
            "workspace",
            "session_name",
            "progetto",
        ] {
            assert!(!t.contains(vietato), "inoltrato {vietato}: {t}");
        }
        // Del `session_name` parte solo il valore, nel campo `titolo`: nessun altro campo nuovo.
        assert!(t.contains(r#""titolo":"nome-segreto""#), "{t}");
        let v: serde_json::Value = serde_json::from_str(&t).unwrap();
        let mut chiavi: Vec<&str> = v["Barra"].as_object().unwrap().keys().map(String::as_str).collect();
        chiavi.sort_unstable();
        assert_eq!(chiavi, ["cinque_ore", "contesto", "modello", "sessione", "sette_giorni", "titolo"]);
    }
}
