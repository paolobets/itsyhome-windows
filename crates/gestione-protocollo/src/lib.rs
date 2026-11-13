//! Protocollo tra l'assistente e la finestra di gestione (processo `itsyhome-gestione`).
//!
//! Una riga JSON per messaggio, su stdin (verso la finestra) e stdout (verso l'assistente):
//! - verso la finestra: `{"tipo":"apri","titolo":…,"html":…}`, `{"tipo":"chiudi"}`;
//! - verso l'assistente: `{"tipo":"pronta"}`, `{"tipo":"chiusa"}`, `{"tipo":"azione","id":…}`.
//!
//! Se lo stdout della finestra si chiude, l'assistente lo tratta come `chiusa`; se lo stdin della
//! finestra si chiude (l'assistente è uscito o è morto), la finestra esce da sola.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "lowercase")]
pub enum Messaggio {
    /// Mostra (o sostituisce) il contenuto: titolo fidato nella cornice, HTML ostile nell'area web.
    Apri { titolo: String, html: String },
    /// Chiude la finestra: il processo esce.
    Chiudi,
    /// Il contenuto dell'ultima `apri` è visibile.
    Pronta,
    /// La finestra si è chiusa (pulsante, Esc o `chiudi`): il processo sta uscendo.
    Chiusa,
    /// L'utente ha scelto un'azione della cornice.
    Azione { id: String },
}

/// Legge una riga del protocollo (senza il ritorno a capo finale, che comunque si tollera; si
/// tollera anche un BOM UTF-8 in testa, che alcuni scrittori mettono sulla prima riga).
pub fn messaggio_da_riga(s: &str) -> Result<Messaggio, String> {
    serde_json::from_str(s.trim_start_matches('\u{feff}').trim_end_matches(['\r', '\n'])).map_err(|e| format!("riga del protocollo non valida: {e}"))
}

/// Scrive un messaggio come una sola riga (senza `\n` finale): i ritorni a capo dentro le
/// stringhe escono come `\n` di JSON.
pub fn riga_da_messaggio(m: &Messaggio) -> String {
    // Un enum di sole stringhe si serializza sempre.
    serde_json::to_string(m).expect("messaggio serializzabile")
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn andata_e_ritorno_del_protocollo() {
        let messaggi = [
            Messaggio::Apri { titolo: "Posta «prova»".into(), html: "<p>riga 1\nriga 2</p>".into() },
            Messaggio::Chiudi,
            Messaggio::Pronta,
            Messaggio::Chiusa,
            Messaggio::Azione { id: "archivia".into() },
        ];
        for m in messaggi {
            let riga = riga_da_messaggio(&m);
            assert!(!riga.contains('\n'), "una riga sola: {riga}");
            assert_eq!(messaggio_da_riga(&riga), Ok(m.clone()));
            assert_eq!(messaggio_da_riga(&format!("{riga}\r\n")), Ok(m.clone()));
            assert_eq!(messaggio_da_riga(&format!("\u{feff}{riga}")), Ok(m));
        }
        // La forma esatta sul filo è quella del contratto.
        assert_eq!(riga_da_messaggio(&Messaggio::Chiudi), r#"{"tipo":"chiudi"}"#);
        assert_eq!(
            messaggio_da_riga(r#"{"tipo":"apri","titolo":"T","html":"<b>x</b>"}"#),
            Ok(Messaggio::Apri { titolo: "T".into(), html: "<b>x</b>".into() })
        );
        assert_eq!(messaggio_da_riga(r#"{"tipo":"azione","id":"a1"}"#), Ok(Messaggio::Azione { id: "a1".into() }));
    }

    #[test]
    fn riga_malformata_e_errore() {
        for riga in ["", "non json", "{}", r#"{"tipo":"vola"}"#, r#"{"tipo":"apri","titolo":"T"}"#, r#"{"tipo":"azione"}"#] {
            let esito = messaggio_da_riga(riga);
            assert!(matches!(&esito, Err(e) if !e.is_empty()), "{riga:?} → {esito:?}");
        }
    }
}
