//! Il riassunto di Agenti (Claude Code): la scala per stato della carta.
//!
//! La sessione di cui la carta parla è la più urgente (`Sessioni::elenco`: aspetta, errore, fatto
//! non visto, lavora; a parità la più recente). La stessa sessione riceve l'azione «Apri il
//! terminale» (`sessione_dell_azione`), così carta e azione non possono dire due cose diverse.

use super::{riga, riga_con_nome, Azione, Riassunto, Tono, R1};
use crate::attenzione::DatiPermesso;
use crate::claude::{frase_errore_claude, id_sessione, nome_visto, strumento_italiano};
use claude_hook::sessioni::{Sessione, Sessioni, Stato};
use claude_hook::Limite;

/// L'azione «Apri il terminale» della sessione di cui parla la carta.
pub const TERMINALE: &str = "agenti.terminale";
/// L'azione «Collega»: le Impostazioni sulla sezione Claude.
pub const COLLEGA: &str = "agenti.collega";

fn azione_terminale() -> Option<Azione> {
    Some(Azione { id: TERMINALE, testo: "Apri il terminale".into(), icona: "agenti" })
}

/// Un permesso in attesa come lo dà la pagina: (impronta della sessione, dati). Uno per sessione
/// con riga, dal più vecchio (`ponte::con_claude::permessi_dal_piu_vecchio`).
pub type PermessoInAttesa<'a> = (&'a str, &'a DatiPermesso);

/// La sessione di cui parla la carta: la prima dell'elenco per urgenza, se non è un «fatto» già
/// visto (quelle sono sessioni ferme, senza azione). Con un permesso in attesa la carta parla del
/// permesso, come la pagina, e non ha azione: nessuna sessione.
pub fn sessione_dell_azione<'a>(s: &'a Sessioni, permessi: &[PermessoInAttesa]) -> Option<&'a Sessione> {
    if !permessi.is_empty() {
        return None;
    }
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
        r1: R1::default(),
    }
}

/// Riassunto con il nome della sessione nella riga: il nome si accorcia, la riga intera resta per
/// Narrator; se il nome non ci sta proprio, la carta mostra `senza_nome`.
fn con_nome(tono: Tono, titolo: &str, (prima, nome, dopo): (&str, &str, &str), senza_nome: &str) -> Riassunto {
    let azione = azione_terminale();
    let intera = format!("{prima}{nome}{dopo}");
    let sec = riga_con_nome(prima, nome, dopo, true).unwrap_or_else(|| riga(senza_nome, true));
    Riassunto { tono, titolo: titolo.into(), titolo_numerico: false, sec, sec_intera: intera, azione, r1: R1::default() }
}

/// La carta con un permesso in attesa, come la pagina (una riga per sessione col permesso): con uno
/// solo «1 ti aspetta» e «{sessione}: {strumento}», con più «{n} ti aspettano» e «{n} permessi».
/// Il conteggio è delle **sessioni** con un permesso, non dei permessi: la pagina porta solo il più
/// vecchio di ogni sessione, e la carta dice quello che la pagina mostra. Nessuna azione: il
/// permesso si decide nella pagina (spec §7, M3).
fn col_permesso(s: &Sessioni, permessi: &[PermessoInAttesa]) -> Riassunto {
    let carta = |titolo: String, sec: String, sec_intera: String| Riassunto {
        tono: Tono::Aspetta,
        titolo,
        titolo_numerico: false,
        sec,
        sec_intera,
        azione: None,
        r1: R1::default(),
    };
    match permessi {
        [(impronta, d)] => {
            let nome = s
                .elenco()
                .into_iter()
                .find(|x| id_sessione(&x.id) == *impronta)
                .map_or_else(|| d.sessione.clone(), |x| nome_visto(&x.titolo, &x.cartella));
            let cosa = if d.strumento.is_empty() { "Permesso" } else { d.strumento.as_str() };
            let sec = riga_con_nome("", &nome, &format!(": {cosa}"), false).unwrap_or_else(|| riga(cosa, false));
            carta("1 ti aspetta".into(), sec, format!("{nome} chiede: {cosa}"))
        }
        _ => {
            let n = permessi.len();
            let testo = format!("{n} permessi");
            carta(format!("{n} ti aspettano"), riga(&testo, false), testo)
        }
    }
}

/// Il titolo della testata della pagina (`r1.fatto`): lo stesso della sintesi di
/// `pagina_agenti::sintesi`, con la stessa precedenza (i permessi, poi chi lavora, poi il numero).
/// `permessi` sono le **sessioni** con un permesso, quelle che la pagina mostra con la sua riga.
fn fatto_della_testata(collegato: bool, s: &Sessioni, permessi: usize) -> String {
    let lavorano = s.lavorano();
    if !collegato {
        "Da collegare".into()
    } else if permessi == 1 {
        "1 ti aspetta".into()
    } else if permessi > 1 {
        format!("{permessi} ti aspettano")
    } else if lavorano > 0 {
        format!("{lavorano} al lavoro")
    } else {
        match s.elenco().len() {
            0 => "A riposo".into(),
            1 => "1 sessione".into(),
            n => format!("{n} sessioni"),
        }
    }
}

/// La carta di Agenti: `collegato` = Claude Code collegato (Impostazioni › Claude); `limiti` = 5
/// ore e 7 giorni non scaduti (`Ponte::limiti_a`); `permessi` = i permessi in attesa che la pagina
/// mostra, uno per sessione, dal più vecchio (vuoti se non collegato). Con un permesso la carta
/// dice quello, come la pagina; il titolo della testata (`r1`) è sempre quello della pagina.
pub fn riassunto(
    collegato: bool,
    s: &Sessioni,
    limiti: (Option<&Limite>, Option<&Limite>),
    permessi: &[PermessoInAttesa],
) -> Riassunto {
    let mut r = carta(collegato, s, limiti, permessi);
    r.r1 = R1 { fatto: fatto_della_testata(collegato, s, permessi.len()), valore: String::new() };
    r
}

/// La carta senza il titolo della testata (`riassunto` lo aggiunge).
fn carta(collegato: bool, s: &Sessioni, limiti: (Option<&Limite>, Option<&Limite>), permessi: &[PermessoInAttesa]) -> Riassunto {
    if !collegato {
        let collega = Azione { id: COLLEGA, testo: "Collega".into(), icona: "collegamenti" };
        return fisso(Tono::Vuoto, "Da collegare".into(), "Collega Claude", Some(collega));
    }
    if !permessi.is_empty() {
        return col_permesso(s, permessi);
    }
    let limite = limite_piu_alto(limiti);
    let Some(una) = sessione_dell_azione(s, permessi) else {
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
    use crate::moduli::misura::{larghezza, Stile, MARGINE, MASSIMO_FATTO, SPAZIO_PIENO, SPAZIO_R1};
    use claude_hook::evento::{EventoHook, TipoEvento};
    use claude_hook::sessioni::MASSIMO_SESSIONI;
    use std::time::Instant;

    /// La carta senza permessi in attesa (la maggior parte dei casi); con i permessi si chiama
    /// `super::riassunto` per nome.
    fn riassunto(collegato: bool, s: &Sessioni, limiti: (Option<&Limite>, Option<&Limite>)) -> Riassunto {
        super::riassunto(collegato, s, limiti, &[])
    }

    fn dati(strumento: &str) -> DatiPermesso {
        DatiPermesso { sessione: "ignorato".into(), strumento: strumento.into(), ..Default::default() }
    }

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
        assert_eq!(r.r1.fatto, "Da collegare");

        // Al lavoro: il limite più alto col nome; senza limiti nome e strumento.
        let lavora = sessioni(&[("a", TipoEvento::PreToolUse), ("b", TipoEvento::UserPromptSubmit)]);
        let r = riassunto(true, &lavora, limiti);
        assert_eq!(vista(&r), (Tono::Attivo, "2 al lavoro", "7 giorni 61 %", Some("Apri il terminale")));
        assert_eq!(r.azione.as_ref().map(|a| a.id), Some(TERMINALE));
        assert_eq!(r.r1.fatto, "2 al lavoro");
        let r = riassunto(true, &sessioni(&[("a", TipoEvento::PreToolUse)]), nessuno);
        assert_eq!(r.r1.fatto, "1 al lavoro");
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
        assert_eq!(r.r1.fatto, "2 sessioni", "senza permesso la testata conta le sessioni");

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
        assert_eq!(riassunto(true, &vuote, nessuno).r1.fatto, "A riposo");
        assert_eq!(riassunto(true, &ferme, nessuno).r1.fatto, "2 sessioni");
        assert_eq!(riassunto(true, &una, nessuno).r1.fatto, "1 sessione");
    }

    /// L'azione va alla sessione di cui parla la carta: la più recente fra quelle che lavorano.
    #[test]
    fn l_azione_e_della_sessione_mostrata() {
        let s = sessioni(&[("a", TipoEvento::PreToolUse), ("b", TipoEvento::UserPromptSubmit)]);
        assert_eq!(sessione_dell_azione(&s, &[]).map(|x| x.id.as_str()), Some("b"));
        let s = sessioni(&[("a", TipoEvento::StopFailure), ("b", TipoEvento::UserPromptSubmit)]);
        assert_eq!(sessione_dell_azione(&s, &[]).map(|x| x.id.as_str()), Some("a"), "l'errore prima di chi lavora");
        let mut s = sessioni(&[("a", TipoEvento::Stop)]);
        s.segna_viste();
        assert!(sessione_dell_azione(&s, &[]).is_none(), "un fatto visto non ha azione");
        // Con un permesso in attesa la carta parla del permesso e non ha azione.
        let s = sessioni(&[("a", TipoEvento::UserPromptSubmit)]);
        let d = dati("Comando");
        assert!(sessione_dell_azione(&s, &[(id_sessione("a").as_str(), &d)]).is_none());
    }

    /// m2: con un permesso in attesa la carta dice quello che dice la pagina («1 ti aspetta», tono
    /// Aspetta, il nome e lo strumento), senza azione; senza permesso resta «Tocca a te».
    #[test]
    fn carta_col_permesso_come_la_pagina() {
        let s = sessioni(&[("a", TipoEvento::PermissionRequest)]);
        let (limiti, d) = ((None, None), dati("Comando"));
        let mia = id_sessione("a");
        let r = super::riassunto(true, &s, limiti, &[(mia.as_str(), &d)]);
        assert_eq!(vista(&r), (Tono::Aspetta, "1 ti aspetta", "prog-a: Comando", None));
        assert_eq!(r.sec_intera, "prog-a chiede: Comando");
        assert_eq!(r.r1.fatto, "1 ti aspetta");
        // Il nome è quello visto adesso (il titolo della scheda), non quello del permesso.
        let mut titolata = sessioni(&[("a", TipoEvento::PermissionRequest)]);
        titolata.imposta_titolo("a", Some("Restyle"));
        let r = super::riassunto(true, &titolata, limiti, &[(mia.as_str(), &d)]);
        assert_eq!((r.sec.as_str(), r.sec_intera.as_str()), ("Restyle: Comando", "Restyle chiede: Comando"));

        // Due sessioni con un permesso: il conteggio è delle sessioni.
        let due = sessioni(&[("a", TipoEvento::PermissionRequest), ("b", TipoEvento::PermissionRequest)]);
        let altra = id_sessione("b");
        let r = super::riassunto(true, &due, limiti, &[(mia.as_str(), &d), (altra.as_str(), &d)]);
        assert_eq!(vista(&r), (Tono::Aspetta, "2 ti aspettano", "2 permessi", None));
        assert_eq!((r.sec_intera.as_str(), r.r1.fatto.as_str()), ("2 permessi", "2 ti aspettano"));

        // La stessa sessione senza il permesso: «Tocca a te», con l'azione di sempre.
        let r = riassunto(true, &s, limiti);
        assert_eq!(vista(&r), (Tono::Guarda, "Tocca a te", "prog-a", Some("Apri il terminale")));
        // Scollegata i permessi non contano (la pagina non li porta): resta «Da collegare».
        let r = super::riassunto(false, &s, limiti, &[(mia.as_str(), &d)]);
        assert_eq!((r.tono, r.titolo.as_str(), r.r1.fatto.as_str()), (Tono::Vuoto, "Da collegare", "Da collegare"));

        // Titolo e riga stanno nella carta, anche con un nome lunghissimo e lo strumento più lungo.
        let mut lunga = sessioni(&[("a", TipoEvento::PermissionRequest)]);
        lunga.imposta_titolo("a", Some(&"W".repeat(60)));
        let lungo = dati("Ricerca nel codice");
        for permessi in [&[(mia.as_str(), &d)][..], &[(mia.as_str(), &lungo)]] {
            let r = super::riassunto(true, &lunga, limiti, permessi);
            assert!(larghezza(&r.titolo, Stile::Titolo) <= SPAZIO_PIENO - MARGINE, "{r:?}");
            assert!(larghezza(&r.sec, Stile::Riga) <= SPAZIO_PIENO - MARGINE, "{r:?}");
            assert!(r.sec.starts_with('W') && r.sec.contains('…'), "il nome si accorcia: {}", r.sec);
            assert_eq!(controlla("agenti", &r), Ok(()));
        }
    }

    /// Il titolo della testata (`r1.fatto`): gli stessi sette gradini della sintesi della pagina.
    #[test]
    fn r1_per_stato() {
        let nessuno = (None, None);
        let fatto = |collegato, s: &Sessioni, p: &[PermessoInAttesa]| super::riassunto(collegato, s, nessuno, p).r1;
        let vuote = Sessioni::new();
        assert_eq!(fatto(false, &vuote, &[]), R1 { fatto: "Da collegare".into(), valore: String::new() });
        assert_eq!(fatto(true, &vuote, &[]).fatto, "A riposo");
        let mut ferme = sessioni(&[("a", TipoEvento::Stop), ("b", TipoEvento::Stop)]);
        ferme.segna_viste();
        assert_eq!(fatto(true, &ferme, &[]).fatto, "2 sessioni");
        let mut una = sessioni(&[("a", TipoEvento::Stop)]);
        una.segna_viste();
        assert_eq!(fatto(true, &una, &[]).fatto, "1 sessione");
        let lavora = sessioni(&[("a", TipoEvento::PreToolUse), ("b", TipoEvento::UserPromptSubmit)]);
        assert_eq!(fatto(true, &lavora, &[]).fatto, "2 al lavoro");
        // Chi aspetta o ha un errore, senza permesso, non cambia il titolo della testata: conta il
        // numero (la carta dice «Errore», «Tocca a te»; la testata no).
        let errore = sessioni(&[("a", TipoEvento::StopFailure), ("b", TipoEvento::Stop)]);
        assert_eq!(fatto(true, &errore, &[]).fatto, "2 sessioni");
        let (d, a, b) = (dati("Comando"), id_sessione("a"), id_sessione("b"));
        assert_eq!(fatto(true, &lavora, &[(a.as_str(), &d)]).fatto, "1 ti aspetta", "i permessi vincono su chi lavora");
        assert_eq!(fatto(true, &lavora, &[(a.as_str(), &d), (b.as_str(), &d)]).fatto, "2 ti aspettano");
    }

    /// Senza sessioni il titolo della testata è «A riposo»: mai «0 sessioni».
    #[test]
    fn assenza_non_e_zero() {
        for collegato in [true, false] {
            let r = riassunto(collegato, &Sessioni::new(), (None, None));
            assert!(!r.r1.fatto.contains('0') && !r.r1.valore.contains('0'), "{collegato}: {r:?}");
            assert!(!r.titolo.contains('0') && !r.sec.contains('0'), "{collegato}: {r:?}");
            assert!(!r.r1.fatto.is_empty(), "mai un vuoto");
        }
    }

    /// Il titolo più lungo della scala sta nello spazio della testata.
    #[test]
    fn r1_sta_nella_testata() {
        let (d, nessuno) = (dati("Comando"), (None, None));
        let tutte: Vec<String> = (0..MASSIMO_SESSIONI).map(|i| id_sessione(&format!("s{i}"))).collect();
        let permessi: Vec<PermessoInAttesa> = tutte.iter().map(|i| (i.as_str(), &d)).collect();
        let mut s = Sessioni::new();
        for i in 0..MASSIMO_SESSIONI {
            s.ricevi(&ev(&format!("s{i}"), TipoEvento::PermissionRequest), Instant::now());
        }
        let r = super::riassunto(true, &s, nessuno, &permessi);
        assert_eq!(r.r1.fatto, format!("{MASSIMO_SESSIONI} ti aspettano"));
        assert!(larghezza(&r.r1.fatto, Stile::Titolo) <= SPAZIO_R1 - MARGINE);
        assert_eq!(controlla("agenti", &r), Ok(()));
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
                    assert!(!r.r1.fatto.is_empty() && r.r1.fatto.chars().count() <= MASSIMO_FATTO, "{r:?}");
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
