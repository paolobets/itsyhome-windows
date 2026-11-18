//! La barra di stato di Claude Code (spec M3 §4 e §6.5).
//!
//! Claude Code lancia il comando della barra a ogni aggiornamento e gli passa un JSON su stdin.
//! `assistente --barra-stato <porta>` lo legge, stampa una riga breve e inoltra all'assistente in
//! esecuzione solo `DatiBarra`: mai lo stdin grezzo (niente cartelle, trascrizione, workspace).
//! Il comando deve essere veloce: connessione con scadenza di 30 ms, 40 ms in tutto.

use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};

use crate::ascoltatore::{sessione_valida, solo_testo};
use crate::evento::ErroreEvento;
use crate::PERCORSO_BARRA;

/// Stdin letto al più per tanto: oltre, il JSON è troncato e non si legge.
pub const MASSIMO_STDIN: usize = 64 * 1024;
/// Scadenza della connessione all'assistente.
const SCADENZA_CONNESSIONE: Duration = Duration::from_millis(30);
/// Lunghezza massima del nome del modello.
const MASSIMO_MODELLO: usize = 40;
/// Il testo mostrato quando non si sa altro.
const RIGA_VUOTA: &str = "Claude";

/// Un limite dell'abbonamento: quanto è usato e quando si azzera (secondi dall'epoca).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Limite {
    pub percentuale: f32,
    pub azzera_alle: i64,
}

/// I soli dati della barra che servono; è anche il corpo inviato all'assistente.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatiBarra {
    pub sessione: String,
    pub modello: Option<String>,
    pub contesto: Option<f32>,
    pub cinque_ore: Option<Limite>,
    pub sette_giorni: Option<Limite>,
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
}

fn percentuale(p: Option<f64>) -> Option<f32> {
    p.filter(|p| p.is_finite()).map(|p| p.clamp(0.0, 100.0) as f32)
}

fn limite(f: Option<Finestra>) -> Option<Limite> {
    let f = f?;
    let p = percentuale(f.used_percentage)?;
    let r = f.resets_at.filter(|r| r.is_finite() && r.abs() < 1e12)?;
    Some(Limite { percentuale: p, azzera_alle: r as i64 })
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

impl Limite {
    fn stretto(self) -> Limite {
        Limite { percentuale: self.percentuale.clamp(0.0, 100.0), ..self }
    }
}

impl DatiBarra {
    /// Le stesse regole per chi legge lo stdin di Claude e per l'assistente che riceve il
    /// corpo inoltrato: sessione valida, percentuali finite in 0..=100, nome ripulito.
    fn convalida(mut self) -> Result<DatiBarra, ErroreEvento> {
        if sessione_valida(Some(&self.sessione)).is_none() {
            return Err(ErroreEvento::NonValido);
        }
        self.modello = self.modello.as_deref().and_then(nome_pulito);
        self.contesto = self.contesto.filter(|c| c.is_finite()).map(|c| c.clamp(0.0, 100.0));
        self.cinque_ore = self.cinque_ore.filter(|l| l.percentuale.is_finite()).map(Limite::stretto);
        self.sette_giorni = self.sette_giorni.filter(|l| l.percentuale.is_finite()).map(Limite::stretto);
        Ok(self)
    }

    /// Il corpo inoltrato all'assistente (`DatiBarra` serializzato), riletto e riconvalidato.
    pub fn da_corpo_inoltrato(corpo: &[u8]) -> Result<DatiBarra, ErroreEvento> {
        let d: DatiBarra = serde_json::from_slice(corpo).map_err(|_| ErroreEvento::NonValido)?;
        d.convalida()
    }
}

/// Legge i dati dallo stdin di Claude Code (solo i campi dei fatti verificati). `null`, tipi
/// strani e valori non finiti diventano «assente»; `session_id` con le regole degli eventi.
pub fn dati_da_json(corpo: &[u8]) -> Result<DatiBarra, ErroreEvento> {
    let g: Grezzo = serde_json::from_slice(corpo).map_err(|_| ErroreEvento::NonValido)?;
    let sessione = sessione_valida(g.session_id.as_deref()).ok_or(ErroreEvento::NonValido)?.to_string();
    let r = g.rate_limits.unwrap_or_default();
    DatiBarra {
        sessione,
        modello: g.model.and_then(|m| m.display_name),
        contesto: g.context_window.and_then(|c| percentuale(c.used_percentage)),
        cinque_ore: limite(r.five_hour),
        sette_giorni: limite(r.seven_day),
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
        parti.push(format!("5h {}% (\u{21bb} {})", arrotonda(l.percentuale), ora_locale(l.azzera_alle)));
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

/// Il comando della barra: legge lo stdin (al più 64 KB), stampa la riga e solo dopo, se c'è il
/// codice e la porta, inoltra `DatiBarra` all'assistente entro `tempo` in tutto. Mai errori
/// verso Claude Code per un assistente spento: la riga è già stata scritta.
pub fn esegui(
    ingresso: impl Read,
    mut uscita: impl Write,
    porta: u16,
    codice: Option<&str>,
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
    if let (Some(d), Some(c)) = (dati, codice) {
        if porta != 0 {
            // Un errore di rete non è un errore della barra.
            let _ = invia(&d, porta, c, inizio, tempo);
        }
    }
    Ok(())
}

fn resto(inizio: Instant, tempo: Duration) -> Option<Duration> {
    let r = tempo.saturating_sub(inizio.elapsed());
    (!r.is_zero()).then_some(r)
}

fn invia(d: &DatiBarra, porta: u16, codice: &str, inizio: Instant, tempo: Duration) -> io::Result<()> {
    // Il codice finisce in un'intestazione: niente caratteri che la spezzino.
    if codice.is_empty() || codice.bytes().any(|b| !(0x21..0x7f).contains(&b)) {
        return Ok(());
    }
    let fuori = || io::Error::new(io::ErrorKind::TimedOut, "tempo esaurito");
    let corpo = serde_json::to_vec(d).map_err(io::Error::other)?;
    let scadenza = SCADENZA_CONNESSIONE.min(resto(inizio, tempo).ok_or_else(fuori)?);
    let indirizzo = SocketAddr::from((Ipv4Addr::LOCALHOST, porta));
    let mut flusso = TcpStream::connect_timeout(&indirizzo, scadenza)?;
    let _ = flusso.set_nodelay(true);
    flusso.set_write_timeout(Some(resto(inizio, tempo).ok_or_else(fuori)?))?;
    let testa = format!(
        "POST {PERCORSO_BARRA} HTTP/1.1\r\nHost: 127.0.0.1:{porta}\r\nContent-Type: application/json\r\n\
         Authorization: Bearer {codice}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        corpo.len()
    );
    let mut richiesta = testa.into_bytes();
    richiesta.extend_from_slice(&corpo);
    flusso.write_all(&richiesta)?;
    // Si aspetta la risposta (poche decine di byte) solo finché c'è tempo: chiudere subito
    // potrebbe azzerare la connessione prima che l'assistente abbia letto.
    if let Some(r) = resto(inizio, tempo) {
        flusso.set_read_timeout(Some(r))?;
        let mut b = [0u8; 64];
        let _ = flusso.read(&mut b);
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use std::net::TcpListener;
    use std::sync::mpsc::channel;

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
        assert_eq!(riga(&d, T, ora_fissa), "Opus 5.5 \u{b7} contesto 42% \u{b7} 5h 24% (\u{21bb} 14:30) \u{b7} 7g 61%");
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
        let d = DatiBarra { sessione: "s".into(), modello: None, contesto: None, cinque_ore: None, sette_giorni: None };
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
        assert_eq!(d.sette_giorni, Some(Limite { percentuale: 100.0, azzera_alle: 5 }));
        assert!(dati_da_json(br#"{"session_id":"a b"}"#).is_err());
        assert!(dati_da_json(br#"{"model":{}}"#).is_err());
        assert!(dati_da_json(b"[1]").is_err());
    }

    #[test]
    fn corpo_inoltrato_si_riconvalida() {
        let d = dati_da_json(completo().as_bytes()).unwrap();
        let corpo = serde_json::to_vec(&d).unwrap();
        assert_eq!(DatiBarra::da_corpo_inoltrato(&corpo).unwrap(), d);
        assert!(DatiBarra::da_corpo_inoltrato(br#"{"sessione":"a b"}"#).is_err());
        let sporco = br#"{"sessione":"s","modello":"X\u0007","contesto":500.0,"cinque_ore":null,"sette_giorni":null}"#;
        let r = DatiBarra::da_corpo_inoltrato(sporco).unwrap();
        assert_eq!(r.modello.as_deref(), Some("X"));
        assert_eq!(r.contesto, Some(100.0));
    }

    /// Un ascoltatore di prova: manda al test ciò che riceve (al più una connessione).
    fn ascolto() -> (u16, std::sync::mpsc::Receiver<String>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let porta = l.local_addr().unwrap().port();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = l.accept() {
                s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                let mut tutto = Vec::new();
                let mut b = [0u8; 4096];
                loop {
                    let n = s.read(&mut b).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    tutto.extend_from_slice(&b[..n]);
                    if let Some(i) = tutto.windows(4).position(|w| w == b"\r\n\r\n") {
                        let testa = String::from_utf8_lossy(&tutto[..i]).to_lowercase();
                        let lung: usize = testa
                            .lines()
                            .find_map(|r| r.strip_prefix("content-length:"))
                            .and_then(|v| v.trim().parse().ok())
                            .unwrap_or(0);
                        if tutto.len() >= i + 4 + lung {
                            break;
                        }
                    }
                }
                let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}");
                let _ = tx.send(String::from_utf8_lossy(&tutto).into_owned());
            }
        });
        (porta, rx)
    }

    #[test]
    fn campi_extra_non_inoltrati() {
        let (porta, rx) = ascolto();
        let mut uscita = Vec::new();
        esegui(completo().as_bytes(), &mut uscita, porta, Some("codice-di-prova"), Duration::from_millis(500)).unwrap();
        let ricevuto = rx.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(ricevuto.starts_with("POST /isola/v1/barra HTTP/1.1\r\n"), "{ricevuto}");
        assert!(ricevuto.contains("Authorization: Bearer codice-di-prova\r\n"));
        for vietato in ["segreto", "transcript_path", "cwd", "workspace", "session_name", "progetto"] {
            assert!(!ricevuto.contains(vietato), "inoltrato {vietato}: {ricevuto}");
        }
        let corpo = ricevuto.split("\r\n\r\n").nth(1).unwrap();
        let d = DatiBarra::da_corpo_inoltrato(corpo.as_bytes()).unwrap();
        assert_eq!(d.sessione, "sess-1");
        assert_eq!(d.cinque_ore.unwrap().azzera_alle, T + 600);
        // La riga stampata non porta il codice.
        let s = String::from_utf8(uscita).unwrap();
        assert!(s.starts_with("Opus 5.5"), "{s}");
        assert!(!s.contains("codice-di-prova"));
    }

    #[test]
    fn stdin_oltre_64k() {
        let spazzatura = vec![b'x'; 100 * 1024];
        let mut uscita = Vec::new();
        esegui(&spazzatura[..], &mut uscita, 0, None, Duration::from_millis(40)).unwrap();
        assert_eq!(String::from_utf8(uscita).unwrap(), "Claude\n");
        // Un JSON valido ma più grande del limite si tronca e non si legge.
        let mut grande = br#"{"session_id":"s","pad":""#.to_vec();
        grande.extend(std::iter::repeat_n(b'a', 100 * 1024));
        grande.extend_from_slice(br#""}"#);
        let mut uscita = Vec::new();
        esegui(&grande[..], &mut uscita, 0, None, Duration::from_millis(40)).unwrap();
        assert_eq!(String::from_utf8(uscita).unwrap(), "Claude\n");
    }

    #[test]
    fn assistente_spento_stampa_lo_stesso() {
        // Una porta che ha appena smesso di ascoltare.
        let porta = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let mut uscita = Vec::new();
        let t = Instant::now();
        esegui(completo().as_bytes(), &mut uscita, porta, Some("c"), Duration::from_millis(40)).unwrap();
        let durata = t.elapsed();
        assert!(String::from_utf8(uscita).unwrap().starts_with("Opus 5.5"));
        assert!(durata < Duration::from_millis(50), "{durata:?}");
    }

    #[test]
    fn senza_codice_non_invia() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.set_nonblocking(true).unwrap();
        let porta = l.local_addr().unwrap().port();
        let mut uscita = Vec::new();
        esegui(completo().as_bytes(), &mut uscita, porta, None, Duration::from_millis(40)).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        assert!(l.accept().is_err(), "nessuna connessione attesa");
        // Un codice con caratteri che spezzano l'intestazione vale come assente.
        esegui(completo().as_bytes(), Vec::new(), porta, Some("a\r\nX: y"), Duration::from_millis(40)).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        assert!(l.accept().is_err());
    }
}
