//! Ciò che passa per il tubo (spec M3 §10): una richiesta dell'aiutante e la risposta di isola.
//!
//! La richiesta porta solo i campi già ridotti (`RecordEvento` = i campi di `EventoHook`, oppure
//! `DatiBarra`); isola la riconvalida con le stesse regole di chi l'ha costruita. La risposta dice
//! solo «ricevuto» o una decisione: il testo che l'aiutante può stampare per Claude Code nasce
//! **solo** da `decisione_json`.

use serde::{Deserialize, Serialize};

use crate::barra::DatiBarra;
use crate::evento::{ErroreEvento, EventoHook};
use crate::tubo::Decisione;

/// Un messaggio sul tubo, in byte: oltre, non si manda e non si legge.
pub const MESSAGGIO_MASSIMO: usize = 64 * 1024;

/// Il motivo che accompagna un «Nega».
const MESSAGGIO_NEGATO: &str = "Negato dall'utente dall'assistente ItsyHome.";

/// Il record di un evento: i soli campi di `EventoHook` (§6.3, §6.4), niente prompt, risposte,
/// percorsi completi o contenuti di file.
pub type RecordEvento = EventoHook;

/// Una richiesta dell'aiutante.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Richiesta {
    Evento(RecordEvento),
    Barra(DatiBarra),
}

impl Richiesta {
    /// Il messaggio da mandare; `None` se supererebbe `MESSAGGIO_MASSIMO`.
    pub fn in_byte(&self) -> Option<Vec<u8>> {
        serde_json::to_vec(self)
            .ok()
            .filter(|b| b.len() <= MESSAGGIO_MASSIMO)
    }

    /// Un messaggio ricevuto: JSON di una `Richiesta`, entro il limite, riconvalidato.
    pub fn da_byte(b: &[u8]) -> Result<Richiesta, ErroreEvento> {
        if b.len() > MESSAGGIO_MASSIMO {
            return Err(ErroreEvento::NonValido);
        }
        match serde_json::from_slice(b).map_err(|_| ErroreEvento::NonValido)? {
            Richiesta::Evento(e) => Ok(Richiesta::Evento(e.convalida()?)),
            Richiesta::Barra(d) => Ok(Richiesta::Barra(d.convalida()?)),
        }
    }
}

/// La risposta di isola. Per gli eventi diversi da `PermissionRequest` l'aiutante aspetta solo
/// `Ricevuto` (e comunque non stampa nulla).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Risposta {
    Ricevuto,
    Consenti,
    Nega,
    /// Nessuna decisione: decide il terminale.
    Nessuna,
}

impl Risposta {
    pub fn in_byte(self) -> Vec<u8> {
        serde_json::to_vec(&self).unwrap_or_default()
    }

    pub fn da_byte(b: &[u8]) -> Option<Risposta> {
        if b.len() > 64 {
            return None;
        }
        serde_json::from_slice(b).ok()
    }

    pub fn da_decisione(d: Decisione) -> Risposta {
        match d {
            Decisione::Consenti => Risposta::Consenti,
            Decisione::Nega => Risposta::Nega,
            Decisione::Nessuna => Risposta::Nessuna,
        }
    }
}

/// L'unico testo che l'aiutante può stampare per Claude Code: la decisione di un
/// `PermissionRequest`. `Nessuna` → `None` (nessun output = decide il terminale). Testo scritto a
/// mano: l'ordine dei campi è quello della documentazione di Claude Code.
pub fn decisione_json(d: Decisione) -> Option<String> {
    let decisione = match d {
        Decisione::Consenti => r#"{"behavior":"allow"}"#.to_owned(),
        Decisione::Nega => {
            let messaggio = serde_json::Value::from(MESSAGGIO_NEGATO);
            format!(r#"{{"behavior":"deny","message":{messaggio}}}"#)
        }
        Decisione::Nessuna => return None,
    };
    Some(format!(
        r#"{{"hookSpecificOutput":{{"hookEventName":"PermissionRequest","decision":{decisione}}}}}"#
    ))
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::evento::{evento_da_json, TipoEvento};

    #[test]
    fn decisione_json_unica_uscita() {
        assert_eq!(
            decisione_json(Decisione::Consenti).as_deref(),
            Some(
                r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}"#
            )
        );
        assert_eq!(
            decisione_json(Decisione::Nega).as_deref(),
            Some(
                r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"deny","message":"Negato dall'utente dall'assistente ItsyHome."}}}"#
            )
        );
        assert_eq!(decisione_json(Decisione::Nessuna), None);
        // Mai le chiavi che comandano altri eventi.
        for d in [Decisione::Consenti, Decisione::Nega] {
            let t = decisione_json(d).unwrap();
            for vietato in [
                "permissionDecision",
                "additionalContext",
                "\"block\"",
                "systemMessage",
                "continue",
            ] {
                assert!(!t.contains(vietato), "{vietato} in {t}");
            }
            let v: serde_json::Value = serde_json::from_str(&t).unwrap();
            assert_eq!(
                v["hookSpecificOutput"]["hookEventName"],
                "PermissionRequest"
            );
        }
    }

    #[test]
    fn record_senza_campi_estranei() {
        let stdin = r#"{"session_id":"s1","hook_event_name":"PermissionRequest","cwd":"C:\\Users\\segreto\\progetto",
            "transcript_path":"C:\\TRASCRIZIONE.jsonl","prompt":"TESTO-PROMPT","permission_mode":"default",
            "tool_name":"Write","tool_input":{"file_path":"src/a.rs","content":"CONTENUTO-DEL-FILE"}}"#;
        let r = Richiesta::Evento(evento_da_json(stdin.as_bytes()).unwrap());
        let b = r.in_byte().unwrap();
        let t = String::from_utf8(b.clone()).unwrap();
        for vietato in [
            "segreto",
            "TRASCRIZIONE",
            "transcript",
            "TESTO-PROMPT",
            "prompt",
            "CONTENUTO-DEL-FILE",
            "content",
            "permission_mode",
            "Users",
        ] {
            assert!(!t.contains(vietato), "{vietato} nel record: {t}");
        }
        assert!(t.contains("src/a.rs") && t.contains("progetto"), "{t}");
        // Riletto da isola: identico.
        assert_eq!(Richiesta::da_byte(&b).unwrap(), r);
    }

    #[test]
    fn richiesta_riconvalidata() {
        let buona = |extra: &str| {
            format!(
                r#"{{"Evento":{{"sessione":"s1","tipo":"PreToolUse","cartella":"a","strumento":null,"notifica":null,"errore":null,"riassunto":null{extra}}}}}"#
            )
        };
        assert!(Richiesta::da_byte(buona("").as_bytes()).is_ok());
        // Campi estranei, sessione strana, cartella con separatori, riassunto fuori dal permesso.
        assert!(Richiesta::da_byte(buona(r#","prompt":"x""#).as_bytes()).is_err());
        let storta = buona("").replace("\"s1\"", "\"a b\"");
        assert!(Richiesta::da_byte(storta.as_bytes()).is_err());
        let cartella = buona("").replace("\"cartella\":\"a\"", "\"cartella\":\"C:\\\\x\\\\a\"");
        assert!(Richiesta::da_byte(cartella.as_bytes()).is_err());
        let riassunto = buona("").replace("\"riassunto\":null", "\"riassunto\":\"rm -rf\"");
        assert!(Richiesta::da_byte(riassunto.as_bytes()).is_err());
        let lungo = buona("").replace(
            "\"strumento\":null",
            &format!("\"strumento\":\"{}\"", "t".repeat(41)),
        );
        assert!(Richiesta::da_byte(lungo.as_bytes()).is_err());
        assert!(
            Richiesta::da_byte(br#"{"Evento":{"sessione":"s","tipo":"SubagentStart"}}"#).is_err()
        );
        assert!(Richiesta::da_byte(&vec![b' '; MESSAGGIO_MASSIMO + 1]).is_err());
        // La barra con le sue regole.
        assert!(Richiesta::da_byte(br#"{"Barra":{"sessione":"a b","modello":null,"contesto":null,"cinque_ore":null,"sette_giorni":null}}"#).is_err());
        let e = EventoHook {
            sessione: "s".into(),
            tipo: TipoEvento::PermissionRequest,
            cartella: "a".into(),
            strumento: Some("Bash".into()),
            notifica: None,
            errore: None,
            riassunto: Some("x".repeat(2000)),
        };
        assert!(Richiesta::da_byte(&Richiesta::Evento(e).in_byte().unwrap()).is_ok());
    }

    #[test]
    fn risposta_andata_e_ritorno() {
        for r in [
            Risposta::Ricevuto,
            Risposta::Consenti,
            Risposta::Nega,
            Risposta::Nessuna,
        ] {
            assert_eq!(Risposta::da_byte(&r.in_byte()), Some(r));
        }
        assert_eq!(Risposta::da_byte(b"{\"hookSpecificOutput\":{}}"), None);
        assert_eq!(Risposta::da_byte(&[b'"'; 100]), None);
    }
}
