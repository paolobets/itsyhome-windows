//! Le sessioni di Claude Code come l'assistente le mostra (M3, Task 4): le voci «In attesa», il
//! colore degli occhi e il testo della pillola media «attività dal vivo». Logica pura: l'ora
//! arriva sempre dal chiamante e nessun testo di prompt, risposta o file passa di qui (le sessioni
//! portano solo cartella, stato e nome dello strumento).

use crate::attenzione::{Fonte, Livello, Voce};
use claude_hook::sessioni::{Sessione, Sessioni, Stato};
use std::time::{Duration, Instant};

/// Il prefisso degli id di tutte le voci di Claude Code.
pub const PREFISSO_CLAUDE: &str = "claude.";
/// La voce «porta occupata»: l'ascoltatore non è partito.
pub const ID_COLLEGAMENTO: &str = "claude.collegamento";

/// Lunghezza massima del nome di uno strumento sconosciuto nella pillola.
const MASSIMO_STRUMENTO: usize = 12;
/// Come si chiama una sessione senza cartella (il `cwd` mancava).
const SENZA_CARTELLA: &str = "Claude";

/// L'impronta di una sessione (FNV-1a a 64 bit, 16 cifre esadecimali): gli id delle voci stanno
/// nei log e nel pannello, il `session_id` di Claude Code no.
pub fn id_sessione(id: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in id.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// I tre id che una sessione può avere in «In attesa» (uno alla volta, a seconda dello stato).
pub fn id_voci(id: &str) -> [String; 3] {
    let i = id_sessione(id);
    [format!("{PREFISSO_CLAUDE}{i}.aspetta"), format!("{PREFISSO_CLAUDE}{i}.errore"), format!("{PREFISSO_CLAUDE}{i}.fatto")]
}

/// La frase per un errore dell'API di Claude (`StopFailure.error`).
pub fn frase_errore_claude(e: &str) -> &'static str {
    match e {
        "rate_limit" => "Limite raggiunto",
        "authentication_failed" => "Accesso da rifare",
        "billing_error" => "Problema di fatturazione",
        "server_error" => "Servizio non disponibile",
        "max_output_tokens" => "Risposta troppo lunga",
        _ => "Errore dell'API",
    }
}

/// La voce «In attesa» di una sessione, se ne merita una: chi aspetta e chi si è fermato per un
/// errore (*attenzione*), chi ha finito e non è stato ancora guardato (*informazione*). Chi lavora
/// e chi ha finito ed è stato visto non ne ha.
pub fn voce_sessione(s: &Sessione, ora: Instant) -> Option<Voce> {
    let [aspetta, errore, fatto] = id_voci(&s.id);
    let cartella = if s.cartella.is_empty() { SENZA_CARTELLA } else { s.cartella.as_str() };
    let (id, livello, titolo, dettaglio) = match (s.stato, s.vista) {
        (Stato::TiAspetta, _) => (
            aspetta,
            Livello::Attenzione,
            format!("Claude ti aspetta · {cartella}"),
            "Claude Code aspetta una tua risposta nel terminale.".to_owned(),
        ),
        (Stato::Errore, _) => (
            errore,
            Livello::Attenzione,
            format!("Claude si è fermato · {cartella}"),
            frase_errore_claude(s.errore.as_deref().unwrap_or("")).to_owned(),
        ),
        (Stato::Fatto, false) => (
            fatto,
            Livello::Informazione,
            format!("Claude ha finito · {cartella}"),
            "Ha finito di rispondere.".to_owned(),
        ),
        (Stato::Lavora, _) | (Stato::Fatto, true) => return None,
    };
    Some(Voce { id, livello, fonte: Fonte::Claude, titolo, dettaglio, pulsanti: vec![], creata: ora })
}

/// Il nome dello strumento in italiano per la pillola. Un nome che non conosciamo si mostra
/// ripulito (solo lettere, cifre e `_-.`) e tagliato a 12 caratteri.
pub fn strumento_italiano(s: Option<&str>) -> String {
    match s {
        None => "Pensa".into(),
        // «PowerShell» è la shell di Claude Code su Windows.
        Some("Bash" | "PowerShell") => "Comando".into(),
        Some("Edit" | "MultiEdit") => "Modifica".into(),
        Some("Write") => "Scrive".into(),
        Some("Read") => "Legge".into(),
        Some("Grep" | "Glob") => "Cerca".into(),
        Some("WebFetch" | "WebSearch") => "Web".into(),
        Some("Task" | "Agent") => "Agente".into(),
        Some(altro) => {
            let pulito: String =
                altro.chars().filter(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.')).take(MASSIMO_STRUMENTO).collect();
            if pulito.is_empty() {
                "Strumento".into()
            } else {
                pulito
            }
        }
    }
}

/// «0:07», «3:12», «1:02:05».
pub fn durata_breve(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s % 3600 / 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// Quelle che lavorano, con da quanto.
fn lavorano(s: &Sessioni, ora: Instant) -> Vec<(&Sessione, Duration)> {
    s.elenco()
        .into_iter()
        .filter(|x| x.stato == Stato::Lavora)
        .map(|x| (x, x.lavora_da.map_or(Duration::ZERO, |da| ora.saturating_duration_since(da))))
        .collect()
}

/// Il testo della pillola media: «Claude · Modifica · 3:12»; con più sessioni che lavorano
/// «Claude ×2 · 3:12» (la durata è quella della più lunga). `None` se nessuna lavora.
pub fn testo_attivita(s: &Sessioni, ora: Instant) -> Option<String> {
    let l = lavorano(s, ora);
    match l.as_slice() {
        [] => None,
        [(una, durata)] => {
            Some(format!("Claude · {} · {}", strumento_italiano(una.strumento.as_deref()), durata_breve(*durata)))
        }
        _ => {
            let piu_lunga = l.iter().map(|(_, d)| *d).max().unwrap_or(Duration::ZERO);
            Some(format!("Claude ×{} · {}", l.len(), durata_breve(piu_lunga)))
        }
    }
}

/// Quanto dura, a parole, per Narrator: «meno di un minuto», «3 minuti», «1 ora e 2 minuti».
fn durata_parlata(d: Duration) -> String {
    let minuti = d.as_secs() / 60;
    let minuti_testo = |m: u64| if m == 1 { "1 minuto".to_owned() } else { format!("{m} minuti") };
    match minuti {
        0 => "meno di un minuto".into(),
        1..=59 => minuti_testo(minuti),
        _ => {
            let (ore, resto) = (minuti / 60, minuti % 60);
            let ore_testo = if ore == 1 { "1 ora".to_owned() } else { format!("{ore} ore") };
            if resto == 0 {
                ore_testo
            } else {
                format!("{ore_testo} e {}", minuti_testo(resto))
            }
        }
    }
}

/// Il nome della pillola media per Narrator: «Claude, Modifica, 3 minuti»; con più sessioni
/// «Claude, 2 sessioni, 3 minuti». Cambia una volta al minuto, non a ogni secondo.
pub fn nome_attivita(s: &Sessioni, ora: Instant) -> Option<String> {
    let l = lavorano(s, ora);
    match l.as_slice() {
        [] => None,
        [(una, durata)] => {
            Some(format!("Claude, {}, {}", strumento_italiano(una.strumento.as_deref()), durata_parlata(*durata)))
        }
        _ => {
            let piu_lunga = l.iter().map(|(_, d)| *d).max().unwrap_or(Duration::ZERO);
            Some(format!("Claude, {} sessioni, {}", l.len(), durata_parlata(piu_lunga)))
        }
    }
}

/// Il colore degli occhi: chi aspetta o si è fermato (*attenzione*), poi chi ha finito e non è
/// stato guardato (*informazione*); altrimenti nessuno.
pub fn tinta_occhi(s: &Sessioni) -> Option<Livello> {
    let elenco = s.elenco();
    if elenco.iter().any(|x| matches!(x.stato, Stato::TiAspetta | Stato::Errore)) {
        Some(Livello::Attenzione)
    } else if elenco.iter().any(|x| x.stato == Stato::Fatto && !x.vista) {
        Some(Livello::Informazione)
    } else {
        None
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use claude_hook::evento::{EventoHook, TipoEvento};

    fn ev(id: &str, tipo: TipoEvento) -> EventoHook {
        EventoHook {
            sessione: id.into(),
            tipo,
            cartella: "progetto-a".into(),
            strumento: None,
            notifica: None,
            errore: None,
            riassunto: None,
        }
    }

    fn con(s: &mut Sessioni, e: EventoHook, ora: Instant) {
        s.ricevi(&e, ora);
    }

    fn prima(s: &Sessioni) -> &Sessione {
        s.elenco()[0]
    }

    #[test]
    fn voci_per_stato() {
        let t = Instant::now();
        let mut s = Sessioni::new();
        con(&mut s, ev("a", TipoEvento::PermissionRequest), t);
        let v = voce_sessione(prima(&s), t).expect("chi aspetta ha una voce");
        assert_eq!((v.livello, v.fonte), (Livello::Attenzione, Fonte::Claude));
        assert_eq!(v.titolo, "Claude ti aspetta · progetto-a");
        assert!(v.id.ends_with(".aspetta") && v.id.starts_with(PREFISSO_CLAUDE));

        let errore = EventoHook { errore: Some("rate_limit".into()), ..ev("a", TipoEvento::StopFailure) };
        con(&mut s, errore, t);
        let v = voce_sessione(prima(&s), t).unwrap();
        assert_eq!(v.livello, Livello::Attenzione);
        assert_eq!(v.titolo, "Claude si è fermato · progetto-a");
        assert_eq!(v.dettaglio, "Limite raggiunto");
        assert!(v.id.ends_with(".errore"));

        con(&mut s, ev("a", TipoEvento::Stop), t);
        let v = voce_sessione(prima(&s), t).unwrap();
        assert_eq!(v.livello, Livello::Informazione);
        assert_eq!(v.titolo, "Claude ha finito · progetto-a");
        assert!(v.id.ends_with(".fatto"));
        // Visto: la voce non c'è più.
        s.segna_viste();
        assert!(voce_sessione(prima(&s), t).is_none());

        con(&mut s, ev("a", TipoEvento::UserPromptSubmit), t);
        assert!(voce_sessione(prima(&s), t).is_none(), "chi lavora non ha voce");
    }

    #[test]
    fn senza_cartella_si_dice_claude() {
        let t = Instant::now();
        let mut s = Sessioni::new();
        con(&mut s, EventoHook { cartella: String::new(), ..ev("a", TipoEvento::Stop) }, t);
        assert_eq!(voce_sessione(prima(&s), t).unwrap().titolo, "Claude ha finito · Claude");
    }

    #[test]
    fn id_della_voce_senza_session_id() {
        let t = Instant::now();
        let id = "9f8e7d6c-1234-4abc-8def-0123456789ab";
        let mut s = Sessioni::new();
        con(&mut s, ev(id, TipoEvento::Stop), t);
        let v = voce_sessione(prima(&s), t).unwrap();
        assert!(!v.id.contains(id) && !v.id.contains("9f8e7d6c"), "{}", v.id);
        assert_eq!(id_sessione(id).len(), 16);
        assert_eq!(id_sessione(id), id_sessione(id), "stessa sessione, stessa impronta");
        assert_ne!(id_sessione(id), id_sessione("altra"));
        // Valori noti di FNV-1a a 64 bit: stringa vuota e «a».
        assert_eq!(id_sessione(""), "cbf29ce484222325");
        assert_eq!(id_sessione("a"), "af63dc4c8601ec8c");
        // I tre id di una sessione sono distinti e hanno tutti il prefisso.
        let tre = id_voci(id);
        assert!(tre.iter().all(|i| i.starts_with(PREFISSO_CLAUDE)));
        assert!(tre[0] != tre[1] && tre[1] != tre[2]);
    }

    #[test]
    fn testo_attivita_una_e_piu_sessioni() {
        let t = Instant::now();
        let mut s = Sessioni::new();
        assert_eq!(testo_attivita(&s, t), None);
        con(&mut s, ev("a", TipoEvento::UserPromptSubmit), t);
        let pre = EventoHook { strumento: Some("Edit".into()), ..ev("a", TipoEvento::PreToolUse) };
        con(&mut s, pre, t + Duration::from_secs(1));
        let ora = t + Duration::from_secs(192);
        assert_eq!(testo_attivita(&s, ora).as_deref(), Some("Claude · Modifica · 3:12"));
        assert_eq!(nome_attivita(&s, ora).as_deref(), Some("Claude, Modifica, 3 minuti"));
        // Una seconda sessione, partita dopo: il numero e la durata della più lunga.
        con(&mut s, ev("b", TipoEvento::UserPromptSubmit), t + Duration::from_secs(100));
        assert_eq!(testo_attivita(&s, ora).as_deref(), Some("Claude ×2 · 3:12"));
        assert_eq!(nome_attivita(&s, ora).as_deref(), Some("Claude, 2 sessioni, 3 minuti"));
        // Chi ha finito non conta.
        con(&mut s, ev("a", TipoEvento::Stop), ora);
        con(&mut s, ev("b", TipoEvento::Stop), ora);
        assert_eq!(testo_attivita(&s, ora), None);
        assert_eq!(nome_attivita(&s, ora), None);
    }

    #[test]
    fn durate() {
        assert_eq!(durata_breve(Duration::from_secs(7)), "0:07");
        assert_eq!(durata_breve(Duration::from_secs(192)), "3:12");
        assert_eq!(durata_breve(Duration::from_secs(3725)), "1:02:05");
        assert_eq!(durata_parlata(Duration::from_secs(30)), "meno di un minuto");
        assert_eq!(durata_parlata(Duration::from_secs(60)), "1 minuto");
        assert_eq!(durata_parlata(Duration::from_secs(3600)), "1 ora");
        assert_eq!(durata_parlata(Duration::from_secs(3725)), "1 ora e 2 minuti");
        assert_eq!(durata_parlata(Duration::from_secs(7500)), "2 ore e 5 minuti");
    }

    #[test]
    fn strumenti_in_italiano() {
        assert_eq!(strumento_italiano(Some("Bash")), "Comando");
        assert_eq!(strumento_italiano(Some("PowerShell")), "Comando");
        assert_eq!(strumento_italiano(Some("MultiEdit")), "Modifica");
        assert_eq!(strumento_italiano(Some("Write")), "Scrive");
        assert_eq!(strumento_italiano(Some("Read")), "Legge");
        assert_eq!(strumento_italiano(Some("Glob")), "Cerca");
        assert_eq!(strumento_italiano(Some("WebSearch")), "Web");
        assert_eq!(strumento_italiano(Some("Agent")), "Agente");
        assert_eq!(strumento_italiano(None), "Pensa");
        let lungo = strumento_italiano(Some("UnNomeLunghissimoDavvero"));
        assert_eq!(lungo.chars().count(), 12);
        assert_eq!(lungo, "UnNomeLunghi");
        // Il nome ripulito: niente caratteri strani o di controllo.
        assert_eq!(strumento_italiano(Some("mcp\u{202e}__x\n y")), "mcp__xy");
        assert_eq!(strumento_italiano(Some("\u{202e}\n")), "Strumento");
    }

    #[test]
    fn errori_in_frasi() {
        assert_eq!(frase_errore_claude("authentication_failed"), "Accesso da rifare");
        assert_eq!(frase_errore_claude("billing_error"), "Problema di fatturazione");
        assert_eq!(frase_errore_claude("server_error"), "Servizio non disponibile");
        assert_eq!(frase_errore_claude("max_output_tokens"), "Risposta troppo lunga");
        assert_eq!(frase_errore_claude("qualcosa"), "Errore dell'API");
    }

    #[test]
    fn colore_degli_occhi() {
        let t = Instant::now();
        let mut s = Sessioni::new();
        assert_eq!(tinta_occhi(&s), None);
        con(&mut s, ev("a", TipoEvento::UserPromptSubmit), t);
        assert_eq!(tinta_occhi(&s), None, "chi lavora non colora");
        con(&mut s, ev("b", TipoEvento::Stop), t);
        assert_eq!(tinta_occhi(&s), Some(Livello::Informazione));
        con(&mut s, ev("c", TipoEvento::PermissionRequest), t);
        assert_eq!(tinta_occhi(&s), Some(Livello::Attenzione), "il più urgente vince");
        let errore = EventoHook { errore: Some("server_error".into()), ..ev("c", TipoEvento::StopFailure) };
        con(&mut s, errore, t);
        assert_eq!(tinta_occhi(&s), Some(Livello::Attenzione));
        con(&mut s, ev("c", TipoEvento::SessionEnd), t);
        s.segna_viste();
        assert_eq!(tinta_occhi(&s), None, "il fatto visto non colora");
    }
}
