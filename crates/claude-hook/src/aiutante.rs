//! Il lavoro dell'aiutante `isola-hook.exe evento` (spec M3 §10), qui per poterlo provare.
//!
//! Legge lo stdin dell'hook (al più 256 KB), lo riduce a `RecordEvento` **qui**, apre il tubo,
//! verifica che il server sia isola (`tubo::collega`) e solo allora manda. Restituisce il testo da
//! stampare per Claude Code: esiste **solo** per un `PermissionRequest`, solo con una risposta di
//! isola verificata, e nasce solo da `messaggio::decisione_json`. Ogni altro caso → `None`: nessun
//! output, Claude Code prosegue come se l'hook non ci fosse.

use std::io::{Read, Write};
use std::time::{Duration, Instant};

use crate::evento::{evento_da_json, TipoEvento};
use crate::messaggio::{decisione_json, Richiesta, Risposta};
use crate::tubo::{self, Decisione, Destinazione, ATTESA_MASSIMA};

/// Stdin letto al più per tanto: oltre, il JSON è troncato e non vale.
pub const STDIN_MASSIMO: usize = 256 * 1024;
/// Tempo totale per un evento che non è un permesso (Claude Code aspetta i `PreToolUse`).
pub const TEMPO_EVENTO: Duration = Duration::from_millis(300);
/// Budget totale della barra di stato.
pub const TEMPO_BARRA: Duration = Duration::from_millis(40);

/// Il corpo di `isola-hook.exe` (e dell'esempio di prova `aiutante_prova`, che cambia solo la
/// destinazione): `evento` o `barra`, ogni altro comando non fa nulla. È **l'unico punto** in cui
/// `evento` scrive su stdout, e il testo viene solo da `decisione_json`.
pub fn principale(
    comando: Option<&str>,
    ingresso: impl Read,
    mut uscita: impl Write,
    destinazione: Option<&Destinazione>,
) {
    match comando {
        Some("evento") => {
            if let Some(testo) = evento(ingresso, destinazione) {
                let _ = uscita.write_all(testo.as_bytes());
                let _ = uscita.flush();
            }
        }
        Some("barra") => {
            let _ = crate::barra::esegui(ingresso, uscita, destinazione, TEMPO_BARRA);
        }
        _ => {}
    }
}

fn resto(inizio: Instant, tempo: Duration) -> Option<Duration> {
    let r = tempo.saturating_sub(inizio.elapsed());
    (!r.is_zero()).then_some(r)
}

/// `isola-hook evento`: vedi l'intestazione del modulo.
pub fn evento(ingresso: impl Read, destinazione: Option<&Destinazione>) -> Option<String> {
    let inizio = Instant::now();
    let mut corpo = Vec::new();
    ingresso
        .take(STDIN_MASSIMO as u64 + 1)
        .read_to_end(&mut corpo)
        .ok()?;
    if corpo.len() > STDIN_MASSIMO {
        return None;
    }
    let record = evento_da_json(&corpo).ok()?;
    let permesso = record.tipo == TipoEvento::PermissionRequest;
    let messaggio = Richiesta::Evento(record).in_byte()?;
    // La verifica del server avviene dentro `collega`, prima di qualunque scrittura.
    let canale = tubo::collega(destinazione?, resto(inizio, TEMPO_EVENTO)?)?;
    if !canale.manda(&messaggio, resto(inizio, TEMPO_EVENTO)?) {
        return None;
    }
    if !permesso {
        // Solo il «ricevuto» (isola ci riconosce mentre siamo ancora vivi); qualunque risposta si
        // ignora: fuori dal permesso non si stampa mai nulla.
        let _ = canale.ricevi(resto(inizio, TEMPO_EVENTO)?);
        return None;
    }
    let decisione = match Risposta::da_byte(&canale.ricevi(ATTESA_MASSIMA)?)? {
        Risposta::Consenti => Decisione::Consenti,
        Risposta::Nega => Decisione::Nega,
        Risposta::Nessuna | Risposta::Ricevuto => return None,
    };
    decisione_json(decisione)
}

#[cfg(all(test, windows))]
mod test {
    use super::*;
    use crate::tubo::sys::{self, Collegamento, EventoWin};
    use crate::tubo::{nome_di_prova, NOME_SERVER};
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::thread::JoinHandle;

    fn exe() -> PathBuf {
        std::env::current_exe().unwrap()
    }

    const PRE_TOOL: &str = r#"{"session_id":"s1","hook_event_name":"PreToolUse","cwd":"C:\\x\\progetto","tool_name":"Bash","tool_input":{"command":"ls"}}"#;
    const PERMESSO: &str = r#"{"session_id":"s1","hook_event_name":"PermissionRequest","cwd":"C:\\x\\progetto","tool_name":"Bash","tool_input":{"command":"ls"}}"#;

    /// Un server finto (questo processo, senza le regole di isola): registra ciò che legge e, se
    /// glielo si dice, risponde.
    struct ServerGrezzo {
        fermo: Arc<EventoWin>,
        thread: Option<JoinHandle<Option<Vec<u8>>>>,
    }

    impl ServerGrezzo {
        fn nuovo(nome: &str, risposta: Option<Risposta>) -> ServerGrezzo {
            let d = sys::descrittore_solo_utente().unwrap();
            let h = sys::crea_istanza(&sys::larga(nome), true, &d).unwrap();
            let fermo = Arc::new(EventoWin::nuovo().unwrap());
            let f2 = Arc::clone(&fermo);
            let thread = std::thread::spawn(move || {
                if !matches!(sys::connetti(&h, &f2), Collegamento::Collegato) {
                    return None;
                }
                let letto = sys::leggi(&h, 70_000, Some(Duration::from_secs(1)), None).ok();
                if let (Some(_), Some(r)) = (&letto, risposta) {
                    sys::scrivi(&h, &r.in_byte(), Duration::from_secs(1), None);
                    let _ = sys::leggi(&h, 1, Some(Duration::from_secs(1)), None);
                }
                letto
            });
            ServerGrezzo {
                fermo,
                thread: Some(thread),
            }
        }

        /// Ciò che il server ha letto (`None` = nessun byte).
        fn letto(mut self) -> Option<Vec<u8>> {
            self.fermo.segnala();
            self.thread.take().unwrap().join().unwrap()
        }
    }

    #[test]
    fn senza_isola_esce_in_silenzio() {
        let d = Destinazione {
            nome: nome_di_prova(),
            server_atteso: exe(),
        };
        let inizio = Instant::now();
        assert_eq!(evento(PERMESSO.as_bytes(), Some(&d)), None);
        assert_eq!(evento(PRE_TOOL.as_bytes(), None), None);
        assert!(
            inizio.elapsed() < Duration::from_millis(300),
            "{:?}",
            inizio.elapsed()
        );
    }

    #[test]
    fn server_falso_non_riceve_nulla() {
        // Il server del tubo è questo processo, non `assistente.exe`: nessun byte, nessun testo.
        let nome = nome_di_prova();
        let s = ServerGrezzo::nuovo(&nome, Some(Risposta::Consenti));
        let d = Destinazione {
            nome: nome.clone(),
            server_atteso: exe().with_file_name(NOME_SERVER),
        };
        assert_eq!(evento(PERMESSO.as_bytes(), Some(&d)), None);
        assert_eq!(s.letto(), None, "il server falso ha ricevuto qualcosa");
        // Controprova: lo stesso server, se fosse quello atteso, riceverebbe il record.
        let nome = nome_di_prova();
        let s = ServerGrezzo::nuovo(&nome, Some(Risposta::Consenti));
        let d = Destinazione {
            nome,
            server_atteso: exe(),
        };
        assert!(evento(PERMESSO.as_bytes(), Some(&d)).is_some());
        assert!(s.letto().is_some_and(|b| !b.is_empty()));
    }

    #[test]
    fn mai_decisioni_fuori_dal_permesso() {
        // Un server (verificato) che risponde «Consenti» a un PreToolUse: nessun testo.
        let nome = nome_di_prova();
        let s = ServerGrezzo::nuovo(&nome, Some(Risposta::Consenti));
        let d = Destinazione {
            nome,
            server_atteso: exe(),
        };
        assert_eq!(evento(PRE_TOOL.as_bytes(), Some(&d)), None);
        let letto = s.letto().expect("il record è arrivato");
        assert!(
            matches!(Richiesta::da_byte(&letto), Ok(Richiesta::Evento(e)) if e.tipo == TipoEvento::PreToolUse)
        );
        // Per un PermissionRequest la stessa risposta diventa la decisione.
        let nome = nome_di_prova();
        let s = ServerGrezzo::nuovo(&nome, Some(Risposta::Consenti));
        let d = Destinazione {
            nome,
            server_atteso: exe(),
        };
        assert_eq!(
            evento(PERMESSO.as_bytes(), Some(&d)),
            decisione_json(Decisione::Consenti)
        );
        s.letto();
        // «Nega» e «Nessuna».
        for (r, atteso) in [
            (Risposta::Nega, decisione_json(Decisione::Nega)),
            (Risposta::Nessuna, None),
            (Risposta::Ricevuto, None),
        ] {
            let nome = nome_di_prova();
            let s = ServerGrezzo::nuovo(&nome, Some(r));
            let d = Destinazione {
                nome,
                server_atteso: exe(),
            };
            assert_eq!(evento(PERMESSO.as_bytes(), Some(&d)), atteso, "{r:?}");
            s.letto();
        }
    }

    #[test]
    fn stdin_illeggibile_o_troppo_grande() {
        let nome = nome_di_prova();
        let s = ServerGrezzo::nuovo(&nome, Some(Risposta::Consenti));
        let d = Destinazione {
            nome,
            server_atteso: exe(),
        };
        let mut grande = PERMESSO.as_bytes().to_vec();
        grande.extend(std::iter::repeat_n(b' ', STDIN_MASSIMO));
        for stdin in [
            &grande[..],
            b"",
            b"[",
            br#"{"session_id":"s1","hook_event_name":"SubagentStart"}"#,
        ] {
            assert_eq!(evento(stdin, Some(&d)), None);
        }
        assert_eq!(s.letto(), None, "niente è stato mandato");
    }
}
