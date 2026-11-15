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
    /// Mostra (o sostituisce) il contenuto. Entrambi i campi sono NON fidati: il titolo (con la
    /// Mail, l'oggetto) va solo in testo semplice nella cornice; l'HTML nell'area web isolata.
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

/// Lunghezza massima del titolo, in caratteri (puntini compresi).
pub const LUNGHEZZA_TITOLO: usize = 200;

/// Un carattere che non si vede ma cambia come si legge il testo: direzione (LRE..RLO, LRI..PDI,
/// ALM), larghezza zero (ZWSP..RLM, WJ, operatori invisibili U+2061..2064, BOM, MVS), trattino
/// morbido, CGJ, riempitivi Hangul (usati per i titoli che sembrano vuoti), selettori di variante
/// e tag. Lo usano `titolo_pulito` e il controllo degli indirizzi dei collegamenti dell'assistente.
pub fn carattere_invisibile(c: char) -> bool {
    matches!(
        c,
        '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
            | '\u{200B}'..='\u{200F}'
            | '\u{2060}'..='\u{2064}'
            | '\u{FEFF}'
            | '\u{061C}'
            | '\u{180E}'
            | '\u{00AD}'
            | '\u{034F}'
            | '\u{115F}'
            | '\u{1160}'
            | '\u{3164}'
            | '\u{FFA0}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{E0000}'..='\u{E007F}'
    )
}

/// Il titolo non fidato pronto per la cornice e per il titolo della finestra di sistema (barra
/// delle applicazioni, Alt+Tab): via i caratteri di controllo (a capo compresi) e quelli
/// invisibili (`carattere_invisibile`: direzione, larghezza zero…); spazi compressi; al più
/// `LUNGHEZZA_TITOLO` caratteri.
pub fn titolo_pulito(titolo: &str) -> String {
    // Controlli (a capo, tabulazioni, campanello…): diventano spazi, poi compressi.
    let parole: String = titolo
        .chars()
        .filter(|c| !carattere_invisibile(*c))
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let compatto = parole.split_whitespace().collect::<Vec<_>>().join(" ");
    if compatto.chars().count() <= LUNGHEZZA_TITOLO {
        return compatto;
    }
    let mut corto: String = compatto.chars().take(LUNGHEZZA_TITOLO - 1).collect();
    corto.push('…');
    corto
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn titolo_senza_altri_invisibili() {
        // Operatori invisibili, trattino morbido, CGJ, riempitivi Hangul (titoli «vuoti»), MVS,
        // selettori di variante e tag: via anche loro.
        let sporco = "a\u{2061}b\u{2064}c\u{00AD}d\u{034F}e\u{115F}f\u{1160}g\u{3164}h\u{FFA0}i\u{180E}j\u{FE0F}k\u{E0041}l\u{E007F}m";
        assert_eq!(titolo_pulito(sporco), "abcdefghijklm");
        // Un titolo fatto solo di riempitivi resta vuoto.
        assert_eq!(titolo_pulito("\u{3164}\u{3164} \u{115F}"), "");
        // Accenti e lettere vere restano.
        assert_eq!(titolo_pulito("Perché è così · ok"), "Perché è così · ok");
    }

    #[test]
    fn titolo_al_confine_dei_200() {
        let duecento = "x".repeat(LUNGHEZZA_TITOLO);
        assert_eq!(titolo_pulito(&duecento), duecento, "200 caratteri: intero");
        let duecentouno = "x".repeat(LUNGHEZZA_TITOLO + 1);
        let tagliato = titolo_pulito(&duecentouno);
        assert_eq!(tagliato.chars().count(), LUNGHEZZA_TITOLO);
        assert_eq!(tagliato, format!("{}…", "x".repeat(LUNGHEZZA_TITOLO - 1)), "201: 199 + «…»");
    }

    #[test]
    fn titolo_ripulito_prima_della_cornice() {
        // Sostituzione di direzione: «moc.lapyap» non deve mostrarsi come «paypal.com».
        assert_eq!(titolo_pulito("Fattura \u{202E}moc.lapyap\u{202C} pagata"), "Fattura moc.lapyap pagata");
        assert_eq!(titolo_pulito("a\u{2066}b\u{2069}c\u{200B}d\u{200F}e\u{FEFF}f\u{061C}g"), "abcdefg");
        // A capo, tabulazioni e controlli: niente righe finte, spazi compressi.
        assert_eq!(titolo_pulito("  Oggetto\n\nReinserisci\til\u{0007}token  "), "Oggetto Reinserisci il token");
        // Lungo: al più 200 caratteri (non byte), con i puntini.
        let lungo = "è".repeat(500);
        let pulito = titolo_pulito(&lungo);
        assert_eq!(pulito.chars().count(), LUNGHEZZA_TITOLO);
        assert!(pulito.ends_with('…'));
        assert_eq!(titolo_pulito("Riepilogo del tuo ordine"), "Riepilogo del tuo ordine");
        assert_eq!(titolo_pulito("\u{202E}\n "), "");
    }

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
