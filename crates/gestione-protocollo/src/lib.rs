//! Protocollo tra l'assistente e la finestra di gestione (processo `itsyhome-gestione`).
//!
//! Una riga JSON per messaggio, su stdin (verso la finestra) e stdout (verso l'assistente):
//! - verso la finestra: `{"tipo":"apri","titolo":…,"html":…}`, `{"tipo":"apri_schede","titolo":…,"schede":[…]}`, `{"tipo":"chiudi"}`;
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
    /// Mostra (o sostituisce) le schede native del dettaglio delle API: nessun riquadro web. I
    /// testi sono NON fidati (vengono dalle API): la finestra li ripassa da `scheda_pulita`.
    #[serde(rename = "apri_schede")]
    ApriSchede { titolo: String, schede: Vec<SchedaDati> },
    /// Chiude la finestra: il processo esce.
    Chiudi,
    /// Il contenuto dell'ultima `apri` è visibile.
    Pronta,
    /// La finestra si è chiusa (pulsante, Esc o `chiudi`): il processo sta uscendo.
    Chiusa,
    /// L'utente ha scelto un'azione della cornice.
    Azione { id: String },
}

/// Una scheda del dettaglio: nome, nota (da quando sono i dati), tabella e la pagina della Console
/// dove si cambia qualcosa (`azione_console`, un id che torna indietro in `azione`).
///
/// `numeriche` (RC-43) dice, colonna per colonna, quali sono numeri o importi: la finestra le
/// allinea a destra (mancante = testo). `gruppi` (RC-54) mette un'intestazione sopra alcune righe
/// («Per modello», «Ultimi giorni»…): le righe prima del primo gruppo sono il riepilogo, in
/// evidenza. Entrambi mancano nelle righe di prima e valgono vuoti.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedaDati {
    pub nome: String,
    pub nota: String,
    pub colonne: Vec<String>,
    pub righe: Vec<Vec<String>>,
    pub azione_console: String,
    #[serde(default)]
    pub numeriche: Vec<bool>,
    #[serde(default)]
    pub gruppi: Vec<GruppoDati>,
}

/// Un'intestazione di gruppo: sta sopra la riga `riga` (indice in `righe`). Il titolo è un testo
/// della scheda come gli altri (non fidato, ripulito da `scheda_pulita`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GruppoDati {
    pub riga: usize,
    pub titolo: String,
}

/// Al più tante righe per scheda.
pub const RIGHE_MASSIME: usize = 200;
/// Al più tante colonne per scheda.
pub const COLONNE_MASSIME: usize = 6;
/// Al più tante schede in un messaggio.
pub const SCHEDE_MASSIME: usize = 8;
/// Al più tanti caratteri in una cella (puntini compresi).
pub const CELLA_MASSIMA: usize = 500;

/// Ciò che sostituisce una cella che porterebbe il prefisso di una chiave da amministratore o API.
const SEGNO_VUOTO: &str = "—";

/// Un testo della scheda pulito (`testo_pulito`), tagliato a `massimo` caratteri e, solo dopo la
/// pulizia (un `sk-` + carattere invisibile + `ant-` si ricompone), senza il prefisso di una chiave:
/// un testo che lo contiene diventa «—». Mai il valore di una chiave nella finestra, qualunque
/// cosa dica chi scrive.
fn testo_scheda(testo: &str, massimo: usize) -> String {
    let pulito = testo_pulito(testo);
    if pulito.to_ascii_lowercase().contains("sk-ant-") {
        return SEGNO_VUOTO.to_owned();
    }
    if pulito.chars().count() <= massimo {
        return pulito;
    }
    let mut corto: String = pulito.chars().take(massimo - 1).collect();
    corto.push('…');
    corto
}

/// La scheda con i testi ripuliti (`testo_scheda`) e le misure limitate: `RIGHE_MASSIME` righe di
/// al più `COLONNE_MASSIME` celle di al più `CELLA_MASSIMA` caratteri. `azione_console` è un id
/// interno, non si tocca. I gruppi tengono solo quelli che stanno sopra una riga rimasta, in ordine
/// di riga, uno per riga (il primo); le colonne numeriche al più `COLONNE_MASSIME`.
pub fn scheda_pulita(s: &SchedaDati) -> SchedaDati {
    let righe = s.righe.len().min(RIGHE_MASSIME);
    let mut gruppi: Vec<GruppoDati> = Vec::new();
    let mut ordinati: Vec<&GruppoDati> = s.gruppi.iter().filter(|g| g.riga < righe).collect();
    // Stabile: a parità di riga resta il primo arrivato.
    ordinati.sort_by_key(|g| g.riga);
    for g in ordinati {
        if gruppi.last().is_none_or(|u| u.riga != g.riga) {
            gruppi.push(GruppoDati { riga: g.riga, titolo: testo_scheda(&g.titolo, CELLA_MASSIMA) });
        }
    }
    SchedaDati {
        nome: testo_scheda(&s.nome, CELLA_MASSIMA),
        nota: testo_scheda(&s.nota, CELLA_MASSIMA),
        colonne: s.colonne.iter().take(COLONNE_MASSIME).map(|c| testo_scheda(c, CELLA_MASSIMA)).collect(),
        righe: s
            .righe
            .iter()
            .take(RIGHE_MASSIME)
            .map(|r| r.iter().take(COLONNE_MASSIME).map(|c| testo_scheda(c, CELLA_MASSIMA)).collect())
            .collect(),
        azione_console: s.azione_console.clone(),
        numeriche: s.numeriche.iter().take(COLONNE_MASSIME).copied().collect(),
        gruppi,
    }
}

/// Le schede di un messaggio, al più `SCHEDE_MASSIME`, ognuna ripulita.
pub fn schede_pulite(schede: &[SchedaDati]) -> Vec<SchedaDati> {
    schede.iter().take(SCHEDE_MASSIME).map(scheda_pulita).collect()
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

/// Un testo non fidato su una riga: via i caratteri invisibili (`carattere_invisibile`), i
/// controlli (a capo compresi) diventano spazi e gli spazi si comprimono. Nessun taglio: chi lo
/// usa decide la lunghezza (`titolo_pulito` a 200, i nomi dei collegamenti a 60).
pub fn testo_pulito(testo: &str) -> String {
    let parole: String = testo
        .chars()
        .filter(|c| !carattere_invisibile(*c))
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    parole.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Il titolo non fidato pronto per la cornice e per il titolo della finestra di sistema (barra
/// delle applicazioni, Alt+Tab): via i caratteri di controllo (a capo compresi) e quelli
/// invisibili (`carattere_invisibile`: direzione, larghezza zero…); spazi compressi; al più
/// `LUNGHEZZA_TITOLO` caratteri.
pub fn titolo_pulito(titolo: &str) -> String {
    let compatto = testo_pulito(titolo);
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

    fn scheda(nome: &str) -> SchedaDati {
        SchedaDati {
            nome: nome.into(),
            nota: "Aggiornato alle 14:02".into(),
            colonne: vec!["Voce".into(), "Valore".into()],
            righe: vec![vec!["Mese".into(), "412,80 $".into()]],
            azione_console: "console:spesa".into(),
            ..Default::default()
        }
    }

    #[test]
    fn apri_schede_andata_e_ritorno() {
        let m = Messaggio::ApriSchede { titolo: "API Claude".into(), schede: vec![scheda("Spesa"), scheda("Token")] };
        let riga = riga_da_messaggio(&m);
        assert!(!riga.contains('\n'), "una riga sola: {riga}");
        assert!(riga.starts_with(r#"{"tipo":"apri_schede","#), "{riga}");
        assert_eq!(messaggio_da_riga(&riga), Ok(m.clone()));
        assert_eq!(messaggio_da_riga(&format!("{riga}\r\n")), Ok(m));
    }

    #[test]
    fn la_scheda_si_ripulisce_e_si_limita() {
        let sporca = SchedaDati {
            nome: "Sp\u{202E}esa\n".into(),
            nota: "no\u{200B}ta\tlunga".into(),
            colonne: (0..9).map(|i| format!("c\u{202E}{i}")).collect(),
            righe: (0..300).map(|i| (0..9).map(|j| format!("r{i}\u{200B}c{j}\n")).collect()).collect(),
            azione_console: "console:spesa".into(),
            ..Default::default()
        };
        let p = scheda_pulita(&sporca);
        assert_eq!((p.nome.as_str(), p.nota.as_str()), ("Spesa", "nota lunga"));
        assert_eq!(p.colonne.len(), COLONNE_MASSIME);
        assert_eq!(p.colonne[0], "c0");
        assert_eq!(p.righe.len(), RIGHE_MASSIME);
        assert!(p.righe.iter().all(|r| r.len() == COLONNE_MASSIME));
        assert_eq!(p.righe[0][0], "r0c0");
        assert_eq!(p.azione_console, "console:spesa");
        // Una riga più corta resta com'è (non si riempie).
        let corta = SchedaDati { righe: vec![vec!["solo".into()]], ..scheda("X") };
        assert_eq!(scheda_pulita(&corta).righe, vec![vec!["solo".to_owned()]]);
    }

    #[test]
    fn nessuna_chiave_nelle_celle_nemmeno_con_invisibili() {
        let mut s = scheda("Spesa");
        s.nome = "sk-\u{200B}ant-api03-X".into();
        s.nota = "SK-ANT-admin01-Y".into();
        s.colonne = vec!["sk-\u{202E}ant-".into()];
        s.righe = vec![vec!["sk\u{200B}-ant-api03-ABC".into(), "ok sk-ant- dopo".into(), "sk-ant".into()]];
        let p = scheda_pulita(&s);
        assert_eq!((p.nome.as_str(), p.nota.as_str(), p.colonne[0].as_str()), ("—", "—", "—"));
        assert_eq!(p.righe[0], ["—", "—", "sk-ant"], "senza il trattino finale non è un prefisso di chiave");
    }

    #[test]
    fn celle_e_schede_hanno_un_tetto() {
        let mut s = scheda("Spesa");
        s.righe = vec![vec!["x".repeat(600), "y".repeat(CELLA_MASSIMA), "è".repeat(CELLA_MASSIMA + 1)]];
        let p = scheda_pulita(&s);
        assert_eq!(p.righe[0][0].chars().count(), CELLA_MASSIMA);
        assert!(p.righe[0][0].ends_with('…'));
        assert_eq!(p.righe[0][1], "y".repeat(CELLA_MASSIMA), "500 interi");
        assert_eq!(p.righe[0][2].chars().count(), CELLA_MASSIMA);
        let molte: Vec<SchedaDati> = (0..12).map(|i| scheda(&format!("S{i}"))).collect();
        let pulite = schede_pulite(&molte);
        assert_eq!(pulite.len(), SCHEDE_MASSIME);
        assert_eq!(pulite[7].nome, "S7");
    }

    /// RC-43 e RC-54: colonne numeriche e intestazioni di gruppo viaggiano sul filo; una riga di
    /// prima (senza i due campi) si legge ancora, con nessuna colonna numerica e nessun gruppo.
    #[test]
    fn numeriche_e_gruppi_andata_e_ritorno() {
        let s = SchedaDati {
            numeriche: vec![false, true],
            gruppi: vec![GruppoDati { riga: 1, titolo: "Per modello".into() }],
            ..scheda("Spesa")
        };
        let m = Messaggio::ApriSchede { titolo: "API Claude".into(), schede: vec![s] };
        assert_eq!(messaggio_da_riga(&riga_da_messaggio(&m)), Ok(m));
        let vecchia = r#"{"tipo":"apri_schede","titolo":"T","schede":[{"nome":"S","nota":"","colonne":[],"righe":[],"azione_console":"console:spesa"}]}"#;
        let Ok(Messaggio::ApriSchede { schede, .. }) = messaggio_da_riga(vecchia) else { panic!("riga di prima non letta") };
        assert!(schede[0].numeriche.is_empty() && schede[0].gruppi.is_empty());
    }

    /// I gruppi si ripuliscono come le celle e non indicano righe che non ci sono: un gruppo oltre
    /// l'ultima riga (anche dopo il taglio a `RIGHE_MASSIME`) sparisce, l'ordine si tiene e due
    /// gruppi sulla stessa riga diventano uno (il primo). Le colonne numeriche al più `COLONNE_MASSIME`.
    #[test]
    fn i_gruppi_si_ripuliscono_e_restano_nelle_righe() {
        let s = SchedaDati {
            righe: (0..300).map(|i| vec![format!("r{i}")]).collect(),
            numeriche: vec![true; 9],
            gruppi: vec![
                GruppoDati { riga: 5, titolo: "Ultimi\u{202E} giorni\n".into() },
                GruppoDati { riga: 0, titolo: "Per modello".into() },
                GruppoDati { riga: 5, titolo: "Doppio".into() },
                GruppoDati { riga: 199, titolo: "sk-ant-api03-X".into() },
                GruppoDati { riga: 200, titolo: "Fuori".into() },
            ],
            ..scheda("Spesa")
        };
        let p = scheda_pulita(&s);
        assert_eq!(p.numeriche.len(), COLONNE_MASSIME);
        let gruppi: Vec<(usize, &str)> = p.gruppi.iter().map(|g| (g.riga, g.titolo.as_str())).collect();
        assert_eq!(gruppi, [(0, "Per modello"), (5, "Ultimi giorni"), (199, "—")]);
    }

    #[test]
    fn riga_malformata_e_errore() {
        for riga in ["", "non json", "{}", r#"{"tipo":"vola"}"#, r#"{"tipo":"apri","titolo":"T"}"#, r#"{"tipo":"azione"}"#] {
            let esito = messaggio_da_riga(riga);
            assert!(matches!(&esito, Err(e) if !e.is_empty()), "{riga:?} → {esito:?}");
        }
    }
}
