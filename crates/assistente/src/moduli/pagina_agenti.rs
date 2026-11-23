//! La pagina Agenti sull'impianto Elenco + dettaglio (ridisegno F2): righe, sintesi e stato della
//! pagina come funzioni pure. Il ponte (`con_claude`) le chiama coi dati che ha già (sessioni,
//! permessi in attesa, limiti, spesa) e scrive i modelli solo a pagina aperta, solo se cambiano.
//!
//! Il tono di una riga è quello vero della sessione: chi si è fermato per un errore è **Errore**
//! (rosa), non più blu come la parola «Errore» della vista di F1 sul livello «attenzione».

use super::agenti::{COLLEGA, TERMINALE};
use super::registro::tono_ui;
use super::Tono;
use crate::attenzione::DatiPermesso;
use crate::claude::{nome_visto, RigaSessione, TestoLimite};
use crate::{CampoDettaglioUi, LimiteUi, RigaModuloUi, SintesiUi, StatoPaginaUi, TipoStatoPaginaUi};
use claude_hook::sessioni::Stato;
use gestione_protocollo::testo_pulito;

/// L'azione «Dettagli» della sintesi: la spesa API nel dettaglio (solo con la chiave).
pub const DETTAGLI: &str = "agenti.dettagli";
/// L'azione «Apri il permesso» della riga col permesso (fino al Task 7: poi il dettaglio è il
/// permesso stesso).
pub const PERMESSO: &str = "agenti.permesso";

/// Il fornitore, come parola (mai come colore).
const FORNITORE: &str = "Claude";

fn campo(etichetta: &str, valore: &str) -> CampoDettaglioUi {
    CampoDettaglioUi { etichetta: etichetta.into(), valore: valore.into(), caldo: false }
}

/// «2 sessioni», «1 sessione».
fn sessioni(n: usize) -> String {
    if n == 1 { "1 sessione".into() } else { format!("{n} sessioni") }
}

/// Una riga della pagina: una sessione, col permesso in attesa se ne ha uno (`(id della voce,
/// indice nel modello delle attese, dati)`). `ripiego`: il terminale della sessione non si
/// raggiunge e il clic apre l'ultimo usato; `a_finestra`: il terminale è Windows Terminal o VS
/// Code, di cui si porta davanti la finestra e non la scheda (le note solo su Windows).
pub fn riga(r: &RigaSessione, permesso: Option<(&str, i32, &DatiPermesso)>, ripiego: bool, a_finestra: bool) -> RigaModuloUi {
    let nome = nome_visto(&r.titolo, &r.cartella);
    let con_titolo = !testo_pulito(&r.titolo).is_empty();
    let sotto = if con_titolo && r.cartella != nome { r.cartella.as_str() } else { "" };
    let contesto = match r.contesto {
        Some(c) => CampoDettaglioUi { caldo: r.contesto_caldo, ..campo("Contesto", &format!("{c} %")) },
        None => CampoDettaglioUi::default(),
    };
    let vuoto = CampoDettaglioUi::default;
    let terminale = (TERMINALE, "Apri il terminale");
    let (tono, cosa, valore, [d1, d2, d3], (azione_id, azione_testo)) = match permesso {
        // Il dettaglio della riga col permesso è il `Permesso` stesso: nessun campo.
        Some((_, _, d)) => {
            (Tono::Aspetta, format!("Chiede: {}", d.strumento), "", [vuoto(), vuoto(), vuoto()], (PERMESSO, "Apri il permesso"))
        }
        None => match r.stato {
            Stato::Lavora => (
                Tono::Attivo,
                r.strumento.clone(),
                r.durata.as_str(),
                [campo("Strumento", &r.strumento), campo("Da", &r.durata), contesto],
                terminale,
            ),
            Stato::TiAspetta => {
                (Tono::Guarda, "Tocca a te".into(), "", [campo("Stato", "Rispondi nel terminale"), vuoto(), contesto], terminale)
            }
            Stato::Errore => {
                let frase = r.errore.unwrap_or("");
                (Tono::Errore, frase.into(), "", [campo("Errore", frase), vuoto(), contesto], terminale)
            }
            Stato::Fatto if !r.vista => {
                (Tono::Fatto, "Ha finito".into(), "", [campo("Stato", "Ha finito"), vuoto(), contesto], terminale)
            }
            Stato::Fatto => (Tono::Calmo, "Fatto".into(), "", [campo("Stato", "Fatto"), vuoto(), contesto], terminale),
        },
    };
    // Per Narrator il nome intero e mai la durata (cambierebbe a ogni secondo); calmo non ha parola.
    let narratore = [nome.as_str(), FORNITORE, tono.per_narrator(), cosa.as_str()]
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    let nota = if permesso.is_some() || !cfg!(windows) {
        ""
    } else if ripiego {
        "Apre l'ultimo terminale usato"
    } else if a_finestra {
        "Apre la finestra del terminale, non la scheda"
    } else {
        ""
    };
    RigaModuloUi {
        id: r.id.as_str().into(),
        tono: tono_ui(tono),
        parola: tono.parola().into(),
        nome: nome.as_str().into(),
        chi: FORNITORE.into(),
        cosa: cosa.into(),
        valore: valore.into(),
        narratore: narratore.into(),
        sotto: sotto.into(),
        d1,
        d2,
        d3,
        nota: nota.into(),
        azione_id: azione_id.into(),
        azione_testo: azione_testo.into(),
        voce: permesso.map_or("", |(v, _, _)| v).into(),
        attesa: permesso.map_or(-1, |(_, i, _)| i),
    }
}

/// Un limite a barra: `None` = nessun elemento (mai «0 %»). Caldo se la percentuale, come si
/// legge, arriva alla soglia dell'utente.
fn limite(nome: &str, l: Option<(TestoLimite, bool)>, con_fornitore: bool, soglia: u8) -> LimiteUi {
    let Some((t, ricordato)) = l else { return LimiteUi::default() };
    // Il numero di `testo_limite`, lo stesso della carta e della vista di oggi.
    let p = t.percento;
    let testo = format!("{p} %");
    let quale = format!("limite {nome}");
    let narratore = [FORNITORE, quale.as_str(), testo.as_str(), t.azzera.as_str()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    LimiteUi {
        presente: true,
        fornitore: if con_fornitore { FORNITORE } else { "" }.into(),
        nome: nome.into(),
        frazione: t.frazione,
        testo: testo.into(),
        caldo: p >= u32::from(soglia),
        vecchio: ricordato,
        narratore: narratore.into(),
    }
}

/// La riga di sintesi: titolo e sotto-testo dalle righe (i permessi vincono, poi chi lavora),
/// i limiti delle 5 ore e dei 7 giorni (`ricordato`: da prima del riavvio), la spesa già scritta
/// da `claude::testo_spesa` e «Dettagli» solo con la chiave da amministratore.
pub fn sintesi(
    collegato: bool,
    righe: &[RigaModuloUi],
    limiti: [Option<(TestoLimite, bool)>; 2],
    soglia: u8,
    spesa: (&str, &str),
    con_chiave: bool,
) -> SintesiUi {
    let n = righe.len();
    let permessi = righe.iter().filter(|r| !r.voce.is_empty()).count();
    // Una riga col permesso ha sempre il tono Aspetta: basta il tono.
    let lavorano = righe.iter().filter(|r| r.tono == tono_ui(Tono::Attivo)).count();
    let (tono, titolo, sotto) = if !collegato {
        (Tono::Vuoto, "Da collegare".to_owned(), String::new())
    } else if permessi > 0 {
        let titolo = if permessi == 1 { "1 ti aspetta".to_owned() } else { format!("{permessi} ti aspettano") };
        (Tono::Aspetta, titolo, sessioni(n))
    } else if lavorano > 0 {
        (Tono::Attivo, format!("{lavorano} al lavoro"), sessioni(n))
    } else if n == 0 {
        (Tono::Calmo, "A riposo".to_owned(), String::new())
    } else {
        // Il titolo è già il conteggio: niente sotto-testo doppio.
        (Tono::Calmo, sessioni(n), String::new())
    };
    let [cinque, sette] = limiti;
    let primo_c_e = cinque.is_some();
    let (azione_id, azione_testo) = if con_chiave { (DETTAGLI, "Dettagli") } else { ("", "") };
    SintesiUi {
        tono: tono_ui(tono),
        titolo: titolo.into(),
        sotto: sotto.into(),
        l1: limite("5 ore", cinque, true, soglia),
        // Il fornitore sul primo limite che c'è.
        l2: limite("7 giorni", sette, !primo_c_e, soglia),
        spesa: spesa.0.into(),
        spesa_nota: spesa.1.into(),
        azione_id: azione_id.into(),
        azione_testo: azione_testo.into(),
    }
}

/// Lo stato della pagina: scollegato (frase e «Collega»), nessuna sessione (frase senza azione:
/// non ce n'è una vera, pre-flight C19), altrimenti l'elenco.
pub fn stato(collegato: bool, righe: usize) -> StatoPaginaUi {
    let vuoto = |frase: &str, sotto: &str, (azione_id, azione_testo): (&str, &str)| StatoPaginaUi {
        tipo: TipoStatoPaginaUi::Vuoto,
        frase: frase.into(),
        sotto: sotto.into(),
        azione_id: azione_id.into(),
        azione_testo: azione_testo.into(),
    };
    if !collegato {
        vuoto("Collega Claude Code", "Qui vedrai sessioni, limiti e spesa.", (COLLEGA, "Collega"))
    } else if righe == 0 {
        vuoto("Nessuna sessione aperta", "Le sessioni di Claude Code compaiono qui mentre lavorano.", ("", ""))
    } else {
        StatoPaginaUi { tipo: TipoStatoPaginaUi::Pieno, ..Default::default() }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::claude::SPESA_SENZA_CHIAVE;
    use crate::TonoModuloUi;
    use claude_hook::sessioni::Stato;

    fn sessione(stato: Stato, vista: bool) -> RigaSessione {
        let lavora = stato == Stato::Lavora;
        RigaSessione {
            id: "0123456789abcdef".into(),
            cartella: "my-shop".into(),
            titolo: String::new(),
            stato,
            vista,
            errore: (stato == Stato::Errore).then_some("Limite raggiunto"),
            strumento: if lavora { "Modifica".into() } else { String::new() },
            durata: if lavora { "3:12".into() } else { String::new() },
            contesto: Some(42),
            contesto_caldo: false,
        }
    }

    fn dati() -> DatiPermesso {
        DatiPermesso {
            sessione: "my-shop".into(),
            strumento: "Comando".into(),
            campo: "TERMINALE".into(),
            comando: "npm test".into(),
            cartella: "my-shop".into(),
            alle: 0,
        }
    }

    fn campo(c: &CampoDettaglioUi) -> (&str, &str, bool) {
        (c.etichetta.as_str(), c.valore.as_str(), c.caldo)
    }

    /// Tono, parola, cosa, valore e azione per ognuna delle sei righe della scala.
    #[test]
    fn tono_per_stato() {
        let d = dati();
        let casi = [
            (riga(&sessione(Stato::TiAspetta, false), Some(("claude.permesso.7", 0, &d)), false, false),
                (TonoModuloUi::Aspetta, "Aspetta", "Chiede: Comando", "", PERMESSO, "Apri il permesso")),
            (riga(&sessione(Stato::Lavora, false), None, false, false),
                (TonoModuloUi::Attivo, "Al lavoro", "Modifica", "3:12", TERMINALE, "Apri il terminale")),
            (riga(&sessione(Stato::TiAspetta, false), None, false, false),
                (TonoModuloUi::Guarda, "Guarda", "Tocca a te", "", TERMINALE, "Apri il terminale")),
            (riga(&sessione(Stato::Errore, false), None, false, false),
                (TonoModuloUi::Errore, "Errore", "Limite raggiunto", "", TERMINALE, "Apri il terminale")),
            (riga(&sessione(Stato::Fatto, false), None, false, false),
                (TonoModuloUi::Fatto, "Fatto", "Ha finito", "", TERMINALE, "Apri il terminale")),
            (riga(&sessione(Stato::Fatto, true), None, false, false),
                (TonoModuloUi::Calmo, "", "Fatto", "", TERMINALE, "Apri il terminale")),
        ];
        for (r, atteso) in &casi {
            let visto = (r.tono, r.parola.as_str(), r.cosa.as_str(), r.valore.as_str(), r.azione_id.as_str(), r.azione_testo.as_str());
            assert_eq!(visto, *atteso);
            assert_eq!(r.chi.as_str(), "Claude", "il fornitore come parola, sempre");
            assert_eq!(r.id.as_str(), "0123456789abcdef");
        }
        // Il dettaglio di ogni riga: tre posti fissi; il contesto sempre nel terzo, «42 %».
        let dettagli: Vec<_> = casi.iter().map(|(r, _)| [campo(&r.d1), campo(&r.d2), campo(&r.d3)]).collect();
        let contesto = ("Contesto", "42 %", false);
        assert_eq!(dettagli[0], [("", "", false); 3], "il dettaglio della riga col permesso è il permesso");
        assert_eq!(dettagli[1], [("Strumento", "Modifica", false), ("Da", "3:12", false), contesto]);
        assert_eq!(dettagli[2], [("Stato", "Rispondi nel terminale", false), ("", "", false), contesto]);
        assert_eq!(dettagli[3], [("Errore", "Limite raggiunto", false), ("", "", false), contesto]);
        assert_eq!(dettagli[4], [("Stato", "Ha finito", false), ("", "", false), contesto]);
        assert_eq!(dettagli[5], [("Stato", "Fatto", false), ("", "", false), contesto]);
    }

    /// Il contesto è caldo come la riga di `claude` lo dice (oltre `CONTESTO_CALDO`); senza il dato
    /// il posto resta vuoto, mai «0 %».
    #[test]
    fn contesto_caldo_o_assente() {
        let caldo = RigaSessione { contesto: Some(91), contesto_caldo: true, ..sessione(Stato::Fatto, true) };
        assert_eq!(campo(&riga(&caldo, None, false, false).d3), ("Contesto", "91 %", true));
        let senza = RigaSessione { contesto: None, ..sessione(Stato::Fatto, true) };
        assert_eq!(campo(&riga(&senza, None, false, false).d3), ("", "", false));
    }

    /// F1 Task 10: la riga in errore diceva «Errore» col colore del livello attenzione (blu). Ora
    /// il tono è quello dell'errore (rosa), come la carta.
    #[test]
    fn errore_e_rosa_non_blu() {
        let r = riga(&sessione(Stato::Errore, false), None, false, false);
        assert_eq!((r.tono, r.parola.as_str()), (TonoModuloUi::Errore, "Errore"));
        // Confronto con le righe vere dei due toni blu, non con costanti: cade se la scala dà
        // all'errore lo stesso tono di «Tocca a te» o di «Ha finito».
        let tocca = riga(&sessione(Stato::TiAspetta, false), None, false, false);
        let finito = riga(&sessione(Stato::Fatto, false), None, false, false);
        assert_ne!(r.tono, tocca.tono, "non il tono blu di «guarda presto»");
        assert_ne!(r.tono, finito.tono, "non il blu del fatto");
    }

    #[test]
    fn riga_col_permesso() {
        let d = dati();
        let r = riga(&sessione(Stato::TiAspetta, false), Some(("claude.permesso.7", 3, &d)), false, false);
        assert_eq!((r.voce.as_str(), r.attesa), ("claude.permesso.7", 3));
        assert_eq!((r.tono, r.cosa.as_str()), (TonoModuloUi::Aspetta, "Chiede: Comando"));
        // Anche una sessione che lavora (il permesso arriva prima del suo stato) è un permesso.
        let r = riga(&sessione(Stato::Lavora, false), Some(("claude.permesso.8", 0, &d)), false, false);
        assert_eq!((r.tono, r.valore.as_str(), r.azione_id.as_str()), (TonoModuloUi::Aspetta, "", PERMESSO));
        // Senza permesso: nessuna voce, indice -1.
        let r = riga(&sessione(Stato::TiAspetta, false), None, false, false);
        assert_eq!((r.voce.as_str(), r.attesa), ("", -1));
    }

    #[test]
    fn nome_e_il_titolo_della_scheda() {
        let con = RigaSessione { titolo: "Sistemare il restyle".into(), ..sessione(Stato::Fatto, true) };
        let r = riga(&con, None, false, false);
        assert_eq!((r.nome.as_str(), r.sotto.as_str()), ("Sistemare il restyle", "my-shop"));
        let r = riga(&sessione(Stato::Fatto, true), None, false, false);
        assert_eq!((r.nome.as_str(), r.sotto.as_str()), ("my-shop", ""));
        // Un titolo di soli invisibili vale come assente.
        let invisibile = RigaSessione { titolo: "\u{200b}\u{202e}".into(), ..sessione(Stato::Fatto, true) };
        let r = riga(&invisibile, None, false, false);
        assert_eq!((r.nome.as_str(), r.sotto.as_str()), ("my-shop", ""));
        // Un titolo uguale alla cartella: la cartella non si ripete sotto.
        let uguale = RigaSessione { titolo: "my-shop".into(), ..sessione(Stato::Fatto, true) };
        let r = riga(&uguale, None, false, false);
        assert_eq!((r.nome.as_str(), r.sotto.as_str()), ("my-shop", ""));
    }

    /// «{nome intero}, Claude, {tono per Narrator}, {cosa}»: la durata no (cambia ogni secondo).
    #[test]
    fn narratore_della_riga() {
        let d = dati();
        let lavora = RigaSessione { titolo: "Sistemare il restyle".into(), ..sessione(Stato::Lavora, false) };
        let casi = [
            (riga(&lavora, None, false, false), "Sistemare il restyle, Claude, al lavoro, Modifica"),
            (riga(&sessione(Stato::TiAspetta, false), Some(("v", 0, &d)), false, false), "my-shop, Claude, ti aspetta, Chiede: Comando"),
            (riga(&sessione(Stato::TiAspetta, false), None, false, false), "my-shop, Claude, guarda presto, Tocca a te"),
            (riga(&sessione(Stato::Errore, false), None, false, false), "my-shop, Claude, errore, Limite raggiunto"),
            (riga(&sessione(Stato::Fatto, false), None, false, false), "my-shop, Claude, fatto, Ha finito"),
            // Calmo non ha parola: nessun pezzo vuoto.
            (riga(&sessione(Stato::Fatto, true), None, false, false), "my-shop, Claude, Fatto"),
        ];
        for (r, atteso) in casi {
            assert_eq!(r.narratore.as_str(), atteso);
        }
    }

    /// Review Focus 2: il nome si accorcia nella riga (Slint), mai per Narrator.
    #[test]
    fn nomi_lunghi_interi_per_narrator() {
        let lungo: String = "Riscrivere il modulo dei pagamenti con la nuova API e i test di ".chars().cycle().take(80).collect();
        assert_eq!(lungo.chars().count(), 80);
        let r = riga(&RigaSessione { titolo: lungo.clone(), ..sessione(Stato::Lavora, false) }, None, false, false);
        assert_eq!(r.nome.as_str(), lungo);
        assert!(r.narratore.starts_with(&format!("{lungo}, Claude, ")), "{}", r.narratore);
    }

    /// La nota per Narrator dice cosa fa davvero il terminale, solo su Windows.
    #[test]
    fn nota_del_terminale() {
        let s = sessione(Stato::Fatto, false);
        let nota = |ripiego, a_finestra| riga(&s, None, ripiego, a_finestra).nota.to_string();
        assert_eq!(nota(false, false), "");
        let su_windows = |t: &str| if cfg!(windows) { t.to_owned() } else { String::new() };
        assert_eq!(nota(true, false), su_windows("Apre l'ultimo terminale usato"));
        assert_eq!(nota(false, true), su_windows("Apre la finestra del terminale, non la scheda"));
        assert_eq!(nota(true, true), su_windows("Apre l'ultimo terminale usato"), "il ripiego vince");
    }

    fn sintesi_di(collegato: bool, righe: &[RigaModuloUi]) -> (TonoModuloUi, String, String) {
        let s = sintesi(collegato, righe, [None, None], 80, ("", SPESA_SENZA_CHIAVE), false);
        (s.tono, s.titolo.to_string(), s.sotto.to_string())
    }

    fn atteso(tono: TonoModuloUi, titolo: &str, sotto: &str) -> (TonoModuloUi, String, String) {
        (tono, titolo.into(), sotto.into())
    }

    #[test]
    fn sintesi_per_stato() {
        let d = dati();
        let lavora = riga(&sessione(Stato::Lavora, false), None, false, false);
        let fatto = riga(&sessione(Stato::Fatto, true), None, false, false);
        let permesso = riga(&sessione(Stato::TiAspetta, false), Some(("v", 0, &d)), false, false);
        assert_eq!(sintesi_di(false, &[]), atteso(TonoModuloUi::Vuoto, "Da collegare", ""));
        assert_eq!(sintesi_di(false, std::slice::from_ref(&lavora)), atteso(TonoModuloUi::Vuoto, "Da collegare", ""));
        assert_eq!(sintesi_di(true, &[]), atteso(TonoModuloUi::Calmo, "A riposo", ""));
        // Al lavoro: il conteggio delle sessioni sotto, al singolare con una.
        assert_eq!(sintesi_di(true, &[lavora.clone(), lavora.clone(), fatto.clone()]), atteso(TonoModuloUi::Attivo, "2 al lavoro", "3 sessioni"));
        assert_eq!(sintesi_di(true, std::slice::from_ref(&lavora)), atteso(TonoModuloUi::Attivo, "1 al lavoro", "1 sessione"));
        // I permessi vincono, al singolare e al plurale.
        assert_eq!(sintesi_di(true, &[permesso.clone(), lavora.clone()]), atteso(TonoModuloUi::Aspetta, "1 ti aspetta", "2 sessioni"));
        assert_eq!(sintesi_di(true, &[permesso.clone(), permesso.clone()]), atteso(TonoModuloUi::Aspetta, "2 ti aspettano", "2 sessioni"));
        // Calme: il titolo è già il conteggio, sotto niente (C17: né «1 sessioni» né doppioni).
        assert_eq!(sintesi_di(true, std::slice::from_ref(&fatto)), atteso(TonoModuloUi::Calmo, "1 sessione", ""));
        assert_eq!(sintesi_di(true, &[fatto.clone(), fatto.clone(), fatto]), atteso(TonoModuloUi::Calmo, "3 sessioni", ""));
    }

    fn limite(frazione: f32, azzera: &str) -> TestoLimite {
        // Come `testo_limite` (percentuale arrotondata dalla stessa fonte): i casi a metà sono in
        // `limiti_col_numero_della_carta`, che passa dalla funzione vera.
        let percento = (frazione * 100.0).round() as u32;
        TestoLimite { frazione, percento, testo: format!("{percento}%"), azzera: azzera.into() }
    }

    fn limiti(l: [Option<(TestoLimite, bool)>; 2], soglia: u8) -> [LimiteUi; 2] {
        let s = sintesi(true, &[], l, soglia, ("", ""), false);
        [s.l1, s.l2]
    }

    /// Revisione I1: una sola fonte del numero. Sui valori a metà (26,5, 29,5, 52,5, 58,5) la
    /// frazione in f32 rifatta per 100 arrotondava a un altro intero di quello della carta e della
    /// vista di oggi (`testo_limite`): «52 %» contro «53%», e `caldo` sbagliato con soglia 53.
    #[test]
    fn limiti_col_numero_della_carta() {
        let ora = chrono::Local::now();
        for (p, atteso) in [(26.5, 27), (29.5, 30), (52.5, 53), (58.5, 59)] {
            let t = crate::claude::testo_limite(&claude_hook::Limite { percentuale: p, azzera_alle: 0 }, ora);
            assert_eq!(t.testo, format!("{atteso}%"), "la carta e la vista di oggi");
            let soglia = atteso as u8;
            let [l, _] = limiti([Some((t, false)), None], soglia);
            assert_eq!((l.testo.to_string(), l.caldo), (format!("{atteso} %"), true), "{p} con soglia {soglia}");
        }
    }

    #[test]
    fn limiti_per_fornitore() {
        let [l1, l2] =
            limiti([Some((limite(0.61, "si azzera alle 14:30"), false)), Some((limite(0.34, "dalle 20:29"), true))], 60);
        assert!(l1.presente && l2.presente);
        assert_eq!((l1.fornitore.as_str(), l1.nome.as_str(), l1.testo.as_str()), ("Claude", "5 ore", "61 %"));
        assert_eq!((l2.fornitore.as_str(), l2.nome.as_str(), l2.testo.as_str()), ("", "7 giorni", "34 %"));
        assert!((l1.frazione - 0.61).abs() < 1e-6);
        assert_eq!((l1.caldo, l1.vecchio), (true, false), "61 oltre la soglia di 60");
        assert_eq!((l2.caldo, l2.vecchio), (false, true), "il ricordato è vecchio");
        assert_eq!(l1.narratore.as_str(), "Claude, limite 5 ore, 61 %, si azzera alle 14:30");
        assert_eq!(l2.narratore.as_str(), "Claude, limite 7 giorni, 34 %, dalle 20:29");
        // La soglia vale com'è scritta: 60 è caldo con soglia 60, 59 no.
        let [a, _] = limiti([Some((limite(0.60, ""), false)), None], 60);
        let [b, _] = limiti([Some((limite(0.594, ""), false)), None], 60);
        assert_eq!((a.testo.as_str(), a.caldo, b.testo.as_str(), b.caldo), ("60 %", true, "59 %", false));
        assert_eq!(a.narratore.as_str(), "Claude, limite 5 ore, 60 %", "senza l'ora, nessun pezzo vuoto");
        // Un limite assente non c'è: niente «0 %», niente barra vuota; il fornitore passa al primo
        // limite che c'è.
        let [l1, l2] = limiti([None, Some((limite(0.34, "si azzera domani alle 09:05"), false))], 80);
        assert_eq!(l1, LimiteUi::default());
        assert_eq!((l2.presente, l2.fornitore.as_str()), (true, "Claude"));
        assert_eq!(limiti([None, None], 80), [LimiteUi::default(), LimiteUi::default()]);
    }

    #[test]
    fn spesa_e_dettagli() {
        let senza = sintesi(true, &[], [None, None], 80, ("", SPESA_SENZA_CHIAVE), false);
        assert_eq!((senza.spesa.as_str(), senza.spesa_nota.as_str()), ("", SPESA_SENZA_CHIAVE));
        assert_eq!((senza.azione_id.as_str(), senza.azione_testo.as_str()), ("", ""), "senza chiave nessun «Dettagli»");
        let con = sintesi(true, &[], [None, None], 80, ("412,80 $", "aggiornato alle 14:02"), true);
        assert_eq!((con.spesa.as_str(), con.spesa_nota.as_str()), ("412,80 $", "aggiornato alle 14:02"));
        assert_eq!((con.azione_id.as_str(), con.azione_testo.as_str()), (DETTAGLI, "Dettagli"));
    }

    #[test]
    fn stato_della_pagina() {
        let s = stato(false, 3);
        assert_eq!(
            (s.tipo, s.frase.as_str(), s.sotto.as_str(), s.azione_id.as_str(), s.azione_testo.as_str()),
            (TipoStatoPaginaUi::Vuoto, "Collega Claude Code", "Qui vedrai sessioni, limiti e spesa.", COLLEGA, "Collega")
        );
        let s = stato(true, 0);
        assert_eq!(
            (s.tipo, s.frase.as_str(), s.sotto.as_str(), s.azione_id.as_str(), s.azione_testo.as_str()),
            (
                TipoStatoPaginaUi::Vuoto,
                "Nessuna sessione aperta",
                "Le sessioni di Claude Code compaiono qui mentre lavorano.",
                "",
                ""
            )
        );
        assert_eq!(stato(true, 2), StatoPaginaUi { tipo: TipoStatoPaginaUi::Pieno, ..Default::default() });
    }
}
