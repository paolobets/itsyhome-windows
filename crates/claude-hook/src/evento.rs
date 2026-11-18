//! Un evento degli hook di Claude Code, ridotto ai soli campi che servono (spec M3 §6.3).
//!
//! Il corpo si legge in una struttura grezza con i soli campi ammessi: `prompt`,
//! `transcript_path`, `last_assistant_message` e simili non vengono mai deserializzati, quindi non
//! restano in memoria. Il solo contenuto di `tool_input` che si tiene è il riassunto di una
//! `PermissionRequest` (comando, percorso, indirizzo o ricerca), tagliato.

use serde::Deserialize;
use serde_json::value::RawValue;

use crate::ascoltatore::{sessione_valida, solo_testo};

/// Lunghezza massima della cartella.
const MASSIMO_CARTELLA: usize = 60;
/// Lunghezza massima del nome dello strumento.
const MASSIMO_STRUMENTO: usize = 40;
/// Lunghezza massima del riassunto.
const MASSIMO_RIASSUNTO: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoEvento {
    UserPromptSubmit,
    PreToolUse,
    PermissionRequest,
    Notification,
    Stop,
    StopFailure,
    SessionEnd,
}

/// L'evento ridotto: nessun testo di prompt, risposta o file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventoHook {
    pub sessione: String,
    pub tipo: TipoEvento,
    /// Ultimo componente di `cwd`.
    pub cartella: String,
    pub strumento: Option<String>,
    /// `notification_type` di una `Notification`.
    pub notifica: Option<String>,
    /// `error` di una `StopFailure`.
    pub errore: Option<String>,
    /// Solo per `PermissionRequest`: cosa lo strumento sta per fare.
    pub riassunto: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroreEvento {
    /// JSON illeggibile, `session_id` mancante o strano.
    NonValido,
    /// Nome di evento che non gestiamo.
    Sconosciuto,
}

#[derive(Deserialize)]
struct Grezzo {
    #[serde(default, deserialize_with = "solo_testo")]
    session_id: Option<String>,
    #[serde(default, deserialize_with = "solo_testo")]
    hook_event_name: Option<String>,
    #[serde(default, deserialize_with = "solo_testo")]
    cwd: Option<String>,
    #[serde(default, deserialize_with = "solo_testo")]
    tool_name: Option<String>,
    #[serde(default, deserialize_with = "solo_testo")]
    notification_type: Option<String>,
    #[serde(default, deserialize_with = "solo_testo")]
    error: Option<String>,
    /// Si analizza solo per `PermissionRequest`.
    #[serde(default)]
    tool_input: Option<Box<RawValue>>,
}

/// Al più `n` caratteri.
fn taglia(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn cartella_di(cwd: &str) -> String {
    let ultimo = cwd
        .split(['/', '\\'])
        .rfind(|p| !p.is_empty())
        .unwrap_or("");
    taglia(ultimo, MASSIMO_CARTELLA)
}

/// Il campo di `tool_input` che dice cosa farà lo strumento.
fn riassunto(strumento: &str, input: &RawValue) -> Option<String> {
    let campo = match strumento {
        // Su Windows la shell di Claude Code si chiama `PowerShell`.
        "Bash" | "PowerShell" => "command",
        "Edit" | "MultiEdit" | "Write" | "Read" | "NotebookEdit" => "file_path",
        "WebFetch" => "url",
        "WebSearch" => "query",
        _ => return None,
    };
    #[derive(Deserialize)]
    struct Uno {
        #[serde(default, deserialize_with = "solo_testo")]
        v: Option<String>,
    }
    // Un solo campo per volta: si rinomina senza costruire l'albero del resto.
    let mut mappa: std::collections::HashMap<String, Box<RawValue>> =
        serde_json::from_str(input.get()).ok()?;
    let valore = mappa.remove(campo)?;
    let testo: Option<String> = serde_json::from_str::<Uno>(&format!("{{\"v\":{}}}", valore.get()))
        .ok()?
        .v;
    testo.map(|t| taglia(&t, MASSIMO_RIASSUNTO))
}

/// Legge il corpo di una richiesta dell'ascoltatore.
pub fn evento_da_json(corpo: &[u8]) -> Result<EventoHook, ErroreEvento> {
    let g: Grezzo = serde_json::from_slice(corpo).map_err(|_| ErroreEvento::NonValido)?;
    let sessione = sessione_valida(g.session_id.as_deref())
        .ok_or(ErroreEvento::NonValido)?
        .to_owned();
    let tipo = match g.hook_event_name.as_deref().ok_or(ErroreEvento::NonValido)? {
        "UserPromptSubmit" => TipoEvento::UserPromptSubmit,
        "PreToolUse" => TipoEvento::PreToolUse,
        "PermissionRequest" => TipoEvento::PermissionRequest,
        "Notification" => TipoEvento::Notification,
        "Stop" => TipoEvento::Stop,
        "StopFailure" => TipoEvento::StopFailure,
        "SessionEnd" => TipoEvento::SessionEnd,
        _ => return Err(ErroreEvento::Sconosciuto),
    };
    let riassunto = match (tipo, g.tool_name.as_deref(), g.tool_input.as_deref()) {
        (TipoEvento::PermissionRequest, Some(s), Some(i)) => riassunto(s, i),
        _ => None,
    };
    Ok(EventoHook {
        sessione,
        tipo,
        cartella: g.cwd.as_deref().map(cartella_di).unwrap_or_default(),
        strumento: g.tool_name.as_deref().map(|s| taglia(s, MASSIMO_STRUMENTO)),
        notifica: g.notification_type,
        errore: g.error,
        riassunto,
    })
}

#[cfg(test)]
mod test {
    use super::*;

    fn leggi(j: &str) -> Result<EventoHook, ErroreEvento> {
        evento_da_json(j.as_bytes())
    }

    #[test]
    fn solo_i_campi_necessari() {
        let e = leggi(
            r#"{"session_id":"s1","hook_event_name":"PermissionRequest","cwd":"C:\\lavoro\\a",
            "tool_name":"Bash","tool_input":{"command":"cargo test","description":"TESTO-DESCRIZIONE"},
            "prompt":"TESTO-PROMPT","transcript_path":"TESTO-TRASCRIZIONE",
            "last_assistant_message":"TESTO-RISPOSTA"}"#,
        )
        .unwrap();
        let d = format!("{e:?}");
        for ammesso in ["cargo test", "s1"] {
            assert!(d.contains(ammesso));
        }
        for vietato in ["TESTO-PROMPT", "TESTO-TRASCRIZIONE", "TESTO-RISPOSTA", "TESTO-DESCRIZIONE"] {
            assert!(!d.contains(vietato), "{vietato} è rimasto nell'evento");
        }
    }

    #[test]
    fn tool_input_si_ignora_fuori_dal_permesso() {
        let e = leggi(
            r#"{"session_id":"s1","hook_event_name":"PreToolUse","tool_name":"Bash",
            "tool_input":{"command":"SEGRETO"}}"#,
        )
        .unwrap();
        assert_eq!(e.riassunto, None);
        assert!(!format!("{e:?}").contains("SEGRETO"));
    }

    #[test]
    fn cartella_e_l_ultimo_nome() {
        let c = |cwd: &str| {
            leggi(&format!(
                r#"{{"session_id":"s","hook_event_name":"Stop","cwd":{}}}"#,
                serde_json::to_string(cwd).unwrap()
            ))
            .unwrap()
            .cartella
        };
        assert_eq!(c("C:\\lavoro\\progetto-a"), "progetto-a");
        assert_eq!(c("/home/x/progetto-a/"), "progetto-a");
        assert_eq!(c(&"x".repeat(100)).chars().count(), 60);
    }

    #[test]
    fn riassunto_per_strumento() {
        let r = |strumento: &str, input: &str| {
            leggi(&format!(
                r#"{{"session_id":"s","hook_event_name":"PermissionRequest","tool_name":"{strumento}","tool_input":{input}}}"#
            ))
            .unwrap()
            .riassunto
        };
        assert_eq!(r("Bash", r#"{"command":"cargo test"}"#).as_deref(), Some("cargo test"));
        assert_eq!(r("PowerShell", r#"{"command":"Get-Date"}"#).as_deref(), Some("Get-Date"));
        assert_eq!(
            r("Write", r#"{"file_path":"src/a.rs","content":"corpo"}"#).as_deref(),
            Some("src/a.rs")
        );
        assert_eq!(r("WebFetch", r#"{"url":"https://x.it"}"#).as_deref(), Some("https://x.it"));
        assert_eq!(r("WebSearch", r#"{"query":"rust"}"#).as_deref(), Some("rust"));
        assert_eq!(r("Strano", r#"{"command":"x"}"#), None);
        assert_eq!(r("Bash", r#"{"altro":"x"}"#), None);
        let lungo = format!(r#"{{"command":"{}"}}"#, "a".repeat(3000));
        assert_eq!(r("Bash", &lungo).unwrap().chars().count(), 2000);
    }

    #[test]
    fn strumento_lungo_tagliato() {
        let e = leggi(&format!(
            r#"{{"session_id":"s","hook_event_name":"PreToolUse","tool_name":"{}"}}"#,
            "t".repeat(90)
        ))
        .unwrap();
        assert_eq!(e.strumento.unwrap().chars().count(), 40);
    }

    #[test]
    fn session_id_strano_rifiutato() {
        for id in ["", &"a".repeat(129), "a/b", "a b"] {
            let j = format!(r#"{{"session_id":"{id}","hook_event_name":"Stop"}}"#);
            assert_eq!(leggi(&j), Err(ErroreEvento::NonValido), "{id:?}");
        }
        assert_eq!(leggi(r#"{"hook_event_name":"Stop"}"#), Err(ErroreEvento::NonValido));
        assert_eq!(leggi("non json"), Err(ErroreEvento::NonValido));
        assert!(leggi(&format!(r#"{{"session_id":"{}","hook_event_name":"Stop"}}"#, "a".repeat(128))).is_ok());
    }

    #[test]
    fn evento_sconosciuto() {
        let j = r#"{"session_id":"s","hook_event_name":"SubagentStart"}"#;
        assert_eq!(leggi(j), Err(ErroreEvento::Sconosciuto));
    }

    #[test]
    fn notifica_ed_errore_letti() {
        let n = leggi(r#"{"session_id":"s","hook_event_name":"Notification","notification_type":"idle_prompt"}"#)
            .unwrap();
        assert_eq!(n.notifica.as_deref(), Some("idle_prompt"));
        let f = leggi(r#"{"session_id":"s","hook_event_name":"StopFailure","error":"rate_limit"}"#).unwrap();
        assert_eq!(f.errore.as_deref(), Some("rate_limit"));
    }
}
