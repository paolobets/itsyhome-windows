//! Il riassunto di Agenti (Claude Code): la scala per stato della carta.
//!
//! La sessione di cui la carta parla è la più urgente (`Sessioni::elenco`: aspetta, errore, fatto
//! non visto, lavora; a parità la più recente). La stessa sessione riceve l'azione «Apri il
//! terminale» (`sessione_dell_azione`), così carta e azione non possono dire due cose diverse.

use super::{riga, riga_con_nome, Azione, Riassunto, Tono};
use crate::claude::{frase_errore_claude, nome_visto, strumento_italiano};
use claude_hook::sessioni::{Sessione, Sessioni, Stato};
use claude_hook::Limite;

/// L'azione «Apri il terminale» della sessione di cui parla la carta.
pub const TERMINALE: &str = "agenti.terminale";
/// L'azione «Collega»: le Impostazioni sulla sezione Claude.
pub const COLLEGA: &str = "agenti.collega";

fn azione_terminale() -> Option<Azione> {
    Some(Azione { id: TERMINALE, testo: "Apri il terminale".into(), icona: "agenti" })
}

/// La sessione di cui parla la carta: la prima dell'elenco per urgenza, se non è un «fatto» già
/// visto (quelle sono sessioni ferme, senza azione).
pub fn sessione_dell_azione(s: &Sessioni) -> Option<&Sessione> {
    s.elenco().into_iter().next().filter(|x| !(x.stato == Stato::Fatto && x.vista))
}

/// Il limite più alto col nome della finestra («7 giorni 61 %»); `None` se non ce ne sono.
fn limite_piu_alto(limiti: (Option<&Limite>, Option<&Limite>)) -> Option<String> {
    [("5 ore", limiti.0), ("7 giorni", limiti.1)]
        .into_iter()
        .filter_map(|(nome, l)| l.filter(|l| l.percentuale.is_finite()).map(|l| (nome, l.percentuale)))
        .reduce(|a, b| if b.1 > a.1 { b } else { a })
        .map(|(nome, p)| format!("{nome} {} %", p.round().clamp(0.0, 100.0) as u32))
}

/// Riassunto con la riga già pronta (uguale per la carta e per Narrator).
fn fisso(tono: Tono, titolo: String, testo: &str, azione: Option<Azione>) -> Riassunto {
    Riassunto {
        tono,
        titolo,
        titolo_numerico: false,
        sec: riga(testo, azione.is_some()),
        sec_intera: testo.into(),
        azione,
    }
}

/// Riassunto con il nome della sessione nella riga: il nome si accorcia, la riga intera resta per
/// Narrator; se il nome non ci sta proprio, la carta mostra `senza_nome`.
fn con_nome(tono: Tono, titolo: &str, (prima, nome, dopo): (&str, &str, &str), senza_nome: &str) -> Riassunto {
    let azione = azione_terminale();
    let intera = format!("{prima}{nome}{dopo}");
    let sec = riga_con_nome(prima, nome, dopo, true).unwrap_or_else(|| riga(senza_nome, true));
    Riassunto { tono, titolo: titolo.into(), titolo_numerico: false, sec, sec_intera: intera, azione }
}

/// La carta di Agenti: `collegato` = Claude Code collegato (Impostazioni › Claude); `limiti` = 5
/// ore e 7 giorni non scaduti (`Ponte::limiti_a`).
pub fn riassunto(collegato: bool, s: &Sessioni, limiti: (Option<&Limite>, Option<&Limite>)) -> Riassunto {
    if !collegato {
        let collega = Azione { id: COLLEGA, testo: "Collega".into(), icona: "collegamenti" };
        return fisso(Tono::Vuoto, "Da collegare".into(), "Collega Claude", Some(collega));
    }
    let limite = limite_piu_alto(limiti);
    let Some(una) = sessione_dell_azione(s) else {
        let n = s.elenco().len();
        return match n {
            0 => fisso(Tono::Calmo, "A riposo".into(), limite.as_deref().unwrap_or("Pronto"), None),
            1 => fisso(Tono::Calmo, "1 sessione".into(), limite.as_deref().unwrap_or("Nessun limite letto"), None),
            n => fisso(Tono::Calmo, format!("{n} sessioni"), limite.as_deref().unwrap_or("Nessun limite letto"), None),
        };
    };
    let nome = nome_visto(&una.titolo, &una.cartella);
    match una.stato {
        // Il tono della voce «Claude ti aspetta» (attenzione); un permesso (bloccante) lo alza.
        // Titolo «Tocca a te»: «Ti aspetta» è la parola del tono Aspetta (vocabolario chiuso).
        Stato::TiAspetta => con_nome(Tono::Guarda, "Tocca a te", ("", &nome, ""), ""),
        // «{nome}: {frase}» per Narrator; nella carta, accanto al bottone, anche la frase più corta
        // («Limite raggiunto») non lascia al nome lo spazio minimo: resta la frase.
        Stato::Errore => {
            let frase = frase_errore_claude(una.errore.as_deref().unwrap_or(""));
            let mut r = fisso(Tono::Errore, "Errore".into(), frase, azione_terminale());
            r.sec_intera = format!("{nome}: {frase}");
            r
        }
        Stato::Fatto => con_nome(Tono::Fatto, "Ha finito", ("", &nome, ""), ""),
        Stato::Lavora => {
            let titolo = format!("{} al lavoro", s.lavorano());
            match limite {
                Some(l) => fisso(Tono::Attivo, titolo, &l, azione_terminale()),
                None => {
                    let strumento = strumento_italiano(una.strumento.as_deref());
                    con_nome(Tono::Attivo, &titolo, ("", &nome, &format!(" · {strumento}")), &strumento)
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::moduli::controlla;
    use claude_hook::evento::{EventoHook, TipoEvento};
    use claude_hook::sessioni::MASSIMO_SESSIONI;
    use std::time::Instant;

    fn ev(id: &str, tipo: TipoEvento) -> EventoHook {
        EventoHook {
            sessione: id.into(),
            tipo,
            cartella: format!("prog-{id}"),
            strumento: (tipo == TipoEvento::PreToolUse).then(|| "Edit".into()),
            notifica: None,
            errore: (tipo == TipoEvento::StopFailure).then(|| "rate_limit".into()),
            riassunto: None,
        }
    }

    fn sessioni(eventi: &[(&str, TipoEvento)]) -> Sessioni {
        let mut s = Sessioni::new();
        let ora = Instant::now();
        for (id, tipo) in eventi {
            s.ricevi(&ev(id, *tipo), ora);
        }
        s
    }

    fn limite(percentuale: f32) -> Limite {
        Limite { percentuale, azzera_alle: i64::MAX }
    }

    /// (tono, titolo, riga, testo dell'azione)
    fn vista(r: &Riassunto) -> (Tono, &str, &str, Option<&str>) {
        (r.tono, r.titolo.as_str(), r.sec.as_str(), r.azione.as_ref().map(|a| a.testo.as_str()))
    }

    #[test]
    fn scala_per_stato() {
        let nessuno = (None, None);
        let (cinque, sette) = (limite(30.4), limite(61.2));
        let limiti = (Some(&cinque), Some(&sette));

        let vuote = Sessioni::new();
        let r = riassunto(false, &vuote, limiti);
        assert_eq!(vista(&r), (Tono::Vuoto, "Da collegare", "Collega Claude", Some("Collega")));
        assert_eq!(r.azione.as_ref().map(|a| a.id), Some(COLLEGA));

        // Al lavoro: il limite più alto col nome; senza limiti nome e strumento.
        let lavora = sessioni(&[("a", TipoEvento::PreToolUse), ("b", TipoEvento::UserPromptSubmit)]);
        let r = riassunto(true, &lavora, limiti);
        assert_eq!(vista(&r), (Tono::Attivo, "2 al lavoro", "7 giorni 61 %", Some("Apri il terminale")));
        assert_eq!(r.azione.as_ref().map(|a| a.id), Some(TERMINALE));
        let r = riassunto(true, &sessioni(&[("a", TipoEvento::PreToolUse)]), nessuno);
        assert_eq!(vista(&r), (Tono::Attivo, "1 al lavoro", "prog-a · Modifica", Some("Apri il terminale")));
        // A parità vince il limite delle 5 ore solo se è più alto.
        let alto = limite(80.0);
        assert_eq!(riassunto(true, &lavora, (Some(&alto), Some(&sette))).sec, "5 ore 80 %");

        // L'errore: «{nome}: {frase}» per Narrator; accanto al bottone anche la frase più corta
        // non lascia al nome lo spazio minimo, la carta dice la frase.
        let errore = sessioni(&[("a", TipoEvento::PreToolUse), ("b", TipoEvento::StopFailure)]);
        let r = riassunto(true, &errore, limiti);
        assert_eq!(vista(&r), (Tono::Errore, "Errore", "Limite raggiunto", Some("Apri il terminale")));
        assert_eq!(r.sec_intera, "prog-b: Limite raggiunto");

        let fatto = sessioni(&[("a", TipoEvento::PreToolUse), ("b", TipoEvento::Stop)]);
        let r = riassunto(true, &fatto, limiti);
        assert_eq!(vista(&r), (Tono::Fatto, "Ha finito", "prog-b", Some("Apri il terminale")));

        let aspetta = sessioni(&[("a", TipoEvento::Stop), ("b", TipoEvento::PermissionRequest)]);
        let r = riassunto(true, &aspetta, limiti);
        // «Tocca a te»: non «Ti aspetta», che è la parola del tono Aspetta (vocabolario chiuso).
        assert_eq!(vista(&r), (Tono::Guarda, "Tocca a te", "prog-b", Some("Apri il terminale")));

        // Ferme: i «fatto» già visti.
        let mut ferme = sessioni(&[("a", TipoEvento::Stop), ("b", TipoEvento::Stop)]);
        ferme.segna_viste();
        assert_eq!(vista(&riassunto(true, &ferme, limiti)), (Tono::Calmo, "2 sessioni", "7 giorni 61 %", None));
        assert_eq!(vista(&riassunto(true, &ferme, nessuno)), (Tono::Calmo, "2 sessioni", "Nessun limite letto", None));
        let mut una = sessioni(&[("a", TipoEvento::Stop)]);
        una.segna_viste();
        assert_eq!(vista(&riassunto(true, &una, nessuno)), (Tono::Calmo, "1 sessione", "Nessun limite letto", None));

        assert_eq!(vista(&riassunto(true, &vuote, limiti)), (Tono::Calmo, "A riposo", "7 giorni 61 %", None));
        assert_eq!(vista(&riassunto(true, &vuote, nessuno)), (Tono::Calmo, "A riposo", "Pronto", None));
    }

    /// L'azione va alla sessione di cui parla la carta: la più recente fra quelle che lavorano.
    #[test]
    fn l_azione_e_della_sessione_mostrata() {
        let s = sessioni(&[("a", TipoEvento::PreToolUse), ("b", TipoEvento::UserPromptSubmit)]);
        assert_eq!(sessione_dell_azione(&s).map(|x| x.id.as_str()), Some("b"));
        let s = sessioni(&[("a", TipoEvento::StopFailure), ("b", TipoEvento::UserPromptSubmit)]);
        assert_eq!(sessione_dell_azione(&s).map(|x| x.id.as_str()), Some("a"), "l'errore prima di chi lavora");
        let mut s = sessioni(&[("a", TipoEvento::Stop)]);
        s.segna_viste();
        assert!(sessione_dell_azione(&s).is_none(), "un fatto visto non ha azione");
    }

    /// Ogni gradino coi dati peggiori (sessioni al tetto, limiti al 100 %, nome di 60 «W», la
    /// frase d'errore più lunga) sta nei limiti della carta.
    #[test]
    fn ogni_gradino_sta_nei_limiti() {
        let pieno = limite(100.0);
        let limiti = (Some(&pieno), Some(&pieno));
        let ora = Instant::now();
        // Il tetto delle sessioni, tutte nello stesso stato, ognuna con un nome di 60 «W».
        let piene = |tipo| {
            let mut s = Sessioni::new();
            for i in 0..MASSIMO_SESSIONI {
                let mut e = ev(&format!("s{i}"), tipo);
                e.cartella = "W".repeat(60);
                e.errore = e.errore.map(|_| "billing_error".into());
                e.strumento = e.strumento.map(|_| "UnoStrumentoLungo".into());
                s.ricevi(&e, ora);
            }
            s
        };
        let mut casi: Vec<Sessioni> =
            [TipoEvento::PreToolUse, TipoEvento::StopFailure, TipoEvento::Stop, TipoEvento::PermissionRequest]
                .into_iter()
                .map(piene)
                .collect();
        let mut ferme = piene(TipoEvento::Stop);
        ferme.segna_viste();
        casi.push(ferme);
        casi.push(Sessioni::new());
        for s in &casi {
            for collegato in [true, false] {
                for l in [limiti, (None, None)] {
                    let r = riassunto(collegato, s, l);
                    assert_eq!(controlla("agenti", &r), Ok(()), "{r:?}");
                }
            }
        }
        assert_eq!(riassunto(true, &casi[0], (None, None)).titolo, format!("{MASSIMO_SESSIONI} al lavoro"));
    }

    /// Un nome lunghissimo (titolo della sessione) si accorcia con «…» nella carta e resta intero
    /// per Narrator; il Registro non scarta il riassunto per colpa sua.
    #[test]
    fn nomi_lunghi_accorciati_e_interi_per_narrator() {
        let nome = "W".repeat(60);
        let ora = Instant::now();
        for (tipo, dopo) in [
            (TipoEvento::Stop, String::new()),
            (TipoEvento::PermissionRequest, String::new()),
            (TipoEvento::PreToolUse, " · Modifica".to_owned()),
        ] {
            let mut s = Sessioni::new();
            s.ricevi(&ev("a", tipo), ora);
            s.imposta_titolo("a", Some(&nome));
            let r = riassunto(true, &s, (None, None));
            assert!(r.sec.starts_with('W') && r.sec.contains('…'), "{tipo:?}: {}", r.sec);
            assert!(r.sec.ends_with(&format!("…{dopo}")), "{tipo:?}: la parte fissa resta intera: {}", r.sec);
            assert_eq!(r.sec_intera, format!("{nome}{dopo}"), "{tipo:?}");
            assert_eq!(controlla("agenti", &r), Ok(()));
        }
        // L'errore: nella carta la frase (la più lunga accorciata), per Narrator nome e frase.
        let mut s = Sessioni::new();
        let mut e = ev("a", TipoEvento::StopFailure);
        e.errore = Some("billing_error".into());
        s.ricevi(&e, ora);
        s.imposta_titolo("a", Some(&nome));
        let r = riassunto(true, &s, (None, None));
        assert!(r.sec.starts_with("Problema di") && r.sec.ends_with('…'), "{}", r.sec);
        assert_eq!(r.sec_intera, format!("{nome}: Problema di fatturazione"));
        assert_eq!(controlla("agenti", &r), Ok(()));
    }
}
