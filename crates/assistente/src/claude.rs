//! Le sessioni di Claude Code come l'assistente le mostra (M3, Task 4): le voci «In attesa», il
//! colore degli occhi e il testo della pillola media «attività dal vivo». Logica pura: l'ora
//! arriva sempre dal chiamante e nessun testo di prompt, risposta o file passa di qui (le sessioni
//! portano solo cartella, stato e nome dello strumento).

use crate::attenzione::{Fonte, Livello, Pulsante, Voce};
use claude_hook::evento::EventoHook;
use api_claude::{Importo, Riepilogo};
use chrono::{DateTime, Datelike, Local, NaiveDate, TimeZone, Weekday};
use claude_hook::sessioni::{Sessione, Sessioni, Stato};
use claude_hook::Limite;
use gestione_protocollo::{testo_pulito, SchedaDati};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Il prefisso degli id di tutte le voci di Claude Code.
pub const PREFISSO_CLAUDE: &str = "claude.";
/// La voce «collegamento non disponibile»: il canale protetto (il tubo) non è partito.
pub const ID_COLLEGAMENTO: &str = "claude.collegamento";
/// La voce informativa «il collegamento va aggiornato» (le voci di `settings.json` non corrispondono più).
pub const ID_AGGIORNA: &str = "claude.aggiorna";

/// Il prefisso delle voci dei permessi (`claude.permesso.<id>`).
pub const PREFISSO_PERMESSO: &str = "claude.permesso.";
/// Esito dello spike del 18/11/2026 (spec §6.3): la risposta dell'hook `PermissionRequest` decide
/// davvero. `true`: la pillola offre «Nega» e «Consenti»; `false` (ramo B): solo «Apri il terminale».
pub const PERMESSI_RISOLVIBILI: bool = true;
pub const PULSANTE_NEGA: &str = "nega";
pub const PULSANTE_CONSENTI: &str = "consenti";
pub const PULSANTE_TERMINALE: &str = "terminale";
/// La voce informativa «non trovo il terminale»: un clic su «Apri il terminale» non ha trovato
/// nessuna finestra (o Windows non l'ha portata davanti). Sparisce da sola.
pub const ID_TERMINALE_MANCANTE: &str = "claude.terminale.mancante";
/// La voce informativa «la decisione è arrivata tardi»: Claude Code aveva già smesso di aspettare.
pub const ID_DECISIONE_TARDIVA: &str = "claude.decisione.tardiva";
/// Quanti caratteri del riassunto stanno nel titolo della voce di un permesso.
const MASSIMO_RIASSUNTO_TITOLO: usize = 60;

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

/// L'impronta di sessione dentro l'id di una voce `claude.<impronta>.<aspetta|errore|fatto>`;
/// `None` per le altre voci di Claude (permessi, collegamento, aggiornamento).
pub fn impronta_da_voce(voce: &str) -> Option<&str> {
    let resto = voce.strip_prefix(PREFISSO_CLAUDE)?;
    let (impronta, suffisso) = resto.split_once('.')?;
    let valida = impronta.len() == 16 && impronta.bytes().all(|b| b.is_ascii_hexdigit());
    (valida && matches!(suffisso, "aspetta" | "errore" | "fatto")).then_some(impronta)
}

/// I tre id che una sessione può avere in «In attesa» (uno alla volta, a seconda dello stato).
pub fn id_voci(id: &str) -> [String; 3] {
    let i = id_sessione(id);
    [format!("{PREFISSO_CLAUDE}{i}.aspetta"), format!("{PREFISSO_CLAUDE}{i}.errore"), format!("{PREFISSO_CLAUDE}{i}.fatto")]
}

/// L'id della voce del permesso `id`.
pub fn id_voce_permesso(id: u64) -> String {
    format!("{PREFISSO_PERMESSO}{id}")
}

/// Il numero di permesso dentro l'id di una voce; `None` se la voce non è un permesso.
pub fn id_permesso_da_voce(voce: &str) -> Option<u64> {
    voce.strip_prefix(PREFISSO_PERMESSO)?.parse().ok()
}

/// La voce del permesso `id` chiesto da una sessione: *bloccante*, col riassunto di cosa Claude sta
/// per fare (testo non fidato: ripulito da caratteri invisibili, di direzione e a capo) e i
/// pulsanti per decidere. Il titolo ne porta al più 60 caratteri, il dettaglio tutto.
pub fn voce_permesso(id: u64, e: &EventoHook, ora: Instant) -> Voce {
    let strumento = e.strumento.as_deref().map_or_else(|| "Strumento".to_owned(), |s| strumento_italiano(Some(s)));
    let riassunto = testo_pulito(e.riassunto.as_deref().unwrap_or(""));
    let cartella = testo_pulito(&e.cartella);
    let cartella = if cartella.is_empty() { SENZA_CARTELLA } else { cartella.as_str() };
    let titolo = if riassunto.is_empty() {
        format!("Claude chiede: {strumento}")
    } else if riassunto.chars().count() > MASSIMO_RIASSUNTO_TITOLO {
        let corto: String = riassunto.chars().take(MASSIMO_RIASSUNTO_TITOLO - 1).collect();
        format!("Claude chiede: {strumento} {corto}…")
    } else {
        format!("Claude chiede: {strumento} {riassunto}")
    };
    let dettaglio =
        if riassunto.is_empty() { format!("Sessione: {cartella}") } else { format!("{riassunto}\nSessione: {cartella}") };
    let pulsanti = if PERMESSI_RISOLVIBILI {
        vec![Pulsante::new(PULSANTE_NEGA, "Nega"), Pulsante::delicato(PULSANTE_CONSENTI, "Consenti")]
    } else {
        vec![Pulsante::new(PULSANTE_TERMINALE, "Apri il terminale")]
    };
    Voce { id: id_voce_permesso(id), livello: Livello::Bloccante, fonte: Fonte::Claude, titolo, dettaglio, pulsanti, creata: ora }
}

/// La voce «Claude ha già smesso di aspettare»: il gesto dell'utente è arrivato dopo che il tubo
/// aveva rinunciato (tempo scaduto), quindi non ha avuto effetto e decide il terminale.
pub fn voce_decisione_tardiva(ora: Instant) -> Voce {
    Voce {
        id: ID_DECISIONE_TARDIVA.into(),
        livello: Livello::Informazione,
        fonte: Fonte::Claude,
        titolo: "La risposta è arrivata troppo tardi".into(),
        dettaglio: "Claude ha già smesso di aspettare: rispondi nel terminale.".into(),
        pulsanti: vec![Pulsante::new(crate::attenzione::PULSANTE_HO_CAPITO, "Ho capito")],
        creata: ora,
    }
}

/// La voce «Windows non ha permesso…»: la finestra del terminale c'è, ma il sistema non l'ha
/// portata davanti. Stesso id della voce «non trovo» (una voce sola, stessa chiusura).
#[cfg(any(windows, test))]
pub fn voce_terminale_rifiutato(ora: Instant) -> Voce {
    Voce {
        id: ID_TERMINALE_MANCANTE.into(),
        livello: Livello::Informazione,
        fonte: Fonte::Claude,
        titolo: "Il terminale non è passato davanti".into(),
        dettaglio: "Windows non ha permesso di portare davanti il terminale: cliccalo nella barra.".into(),
        pulsanti: vec![Pulsante::new(crate::attenzione::PULSANTE_HO_CAPITO, "Ho capito")],
        creata: ora,
    }
}

/// La voce «Non trovo il terminale di questa sessione» (informazione). Solo dove c'è un terminale
/// da portare davanti (Windows).
#[cfg(any(windows, test))]
pub fn voce_terminale_mancante(ora: Instant) -> Voce {
    Voce {
        id: ID_TERMINALE_MANCANTE.into(),
        livello: Livello::Informazione,
        fonte: Fonte::Claude,
        titolo: "Non trovo il terminale di questa sessione".into(),
        dettaglio: "Non è stata portata davanti nessuna finestra: aprila tu dalla barra.".into(),
        pulsanti: vec![Pulsante::new(crate::attenzione::PULSANTE_HO_CAPITO, "Ho capito")],
        creata: ora,
    }
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
    // Chi aspetta e chi ha finito possono portare davanti il suo terminale; l'errore no.
    let pulsanti = match s.stato {
        Stato::TiAspetta | Stato::Fatto => vec![Pulsante::new(PULSANTE_TERMINALE, "Apri il terminale")],
        _ => vec![],
    };
    Some(Voce { id, livello, fonte: Fonte::Claude, titolo, dettaglio, pulsanti, creata: ora })
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

/// Le due parti del testo della pillola media: a sinistra «Claude · Modifica» (la sola che può
/// tagliarsi), a destra la durata «3:12», sempre intera. Con più sessioni che lavorano
/// «Claude ×2» e la durata della più lunga. `None` se nessuna lavora.
pub fn parti_attivita(s: &Sessioni, ora: Instant) -> Option<(String, String)> {
    let l = lavorano(s, ora);
    match l.as_slice() {
        [] => None,
        [(una, durata)] => {
            Some((format!("Claude · {}", strumento_italiano(una.strumento.as_deref())), durata_breve(*durata)))
        }
        _ => {
            let piu_lunga = l.iter().map(|(_, d)| *d).max().unwrap_or(Duration::ZERO);
            Some((format!("Claude ×{}", l.len()), durata_breve(piu_lunga)))
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

// ─── La vista Claude del pannello (Task 9) ─────────────────────────────────────────────────

/// Quante sessioni mostra la vista (le altre si contano in «+N altre»).
pub const RIGHE_MASSIME: usize = 3;
/// La frase della spesa senza la chiave da amministratore.
pub const SPESA_SENZA_CHIAVE: &str = "Aggiungi una chiave da amministratore nelle Impostazioni";
/// La nota della spesa quando non c'è un dato da mostrare.
pub const SPESA_NESSUN_DATO: &str = "nessun dato";

/// Una riga della vista: una sessione come si legge. L'`id` è l'impronta (`id_sessione`), mai il
/// `session_id` di Claude Code; chi non lavora non ha strumento né durata (non cambierebbero a ogni
/// secondo: il timer della vista parte solo per chi lavora).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RigaSessione {
    pub id: String,
    pub cartella: String,
    pub stato_testo: &'static str,
    /// Il livello che dà il colore allo stato; `None` per chi lavora.
    pub livello: Option<Livello>,
    pub strumento: String,
    pub durata: String,
    /// «42%», se la barra di stato l'ha detto.
    pub contesto: Option<String>,
}

/// Le righe della vista (al più `RIGHE_MASSIME`, nell'ordine di urgenza) e quante sessioni restano
/// fuori. `contesto` è per `session_id`, in percentuale.
pub fn righe_sessioni(s: &Sessioni, contesto: &HashMap<String, f32>, ora: Instant) -> (Vec<RigaSessione>, usize) {
    let elenco = s.elenco();
    let altre = elenco.len().saturating_sub(RIGHE_MASSIME);
    let righe = elenco
        .into_iter()
        .take(RIGHE_MASSIME)
        .map(|x| {
            let cartella = testo_pulito(&x.cartella);
            let cartella = if cartella.is_empty() { SENZA_CARTELLA.to_owned() } else { cartella };
            let (stato_testo, livello) = match x.stato {
                Stato::Lavora => ("Lavora", None),
                Stato::TiAspetta => ("Ti aspetta", Some(Livello::Attenzione)),
                Stato::Fatto => ("Fatto", Some(Livello::Informazione)),
                Stato::Errore => ("Errore", Some(Livello::Attenzione)),
            };
            let lavora = x.stato == Stato::Lavora;
            RigaSessione {
                id: id_sessione(&x.id),
                cartella,
                stato_testo,
                livello,
                strumento: if lavora { strumento_italiano(x.strumento.as_deref()) } else { String::new() },
                durata: if lavora {
                    durata_breve(x.lavora_da.map_or(Duration::ZERO, |da| ora.saturating_duration_since(da)))
                } else {
                    String::new()
                },
                contesto: contesto.get(&x.id).map(|c| format!("{}%", c.round().clamp(0.0, 100.0) as u32)),
            }
        })
        .collect();
    (righe, altre)
}

/// Il valore e la nota della spesa API del mese. Senza chiave la frase della guida; con la chiave ma
/// senza un dato (nessuna lettura, lettura fallita, dati senza data) «nessun dato», mai «0 $»; con
/// la spesa il valore e «aggiornato alle 14:02» (l'ora dei dati dell'API, non della lettura, nel
/// fuso del sistema come le altre ore dell'app).
pub fn testo_spesa(r: Option<&Riepilogo>, chiave: bool) -> (String, String) {
    testo_spesa_in(r, chiave, &Local)
}

/// `testo_spesa` nel fuso `tz` (i test ne passano uno fisso).
pub fn testo_spesa_in<Tz: TimeZone>(r: Option<&Riepilogo>, chiave: bool, tz: &Tz) -> (String, String)
where
    Tz::Offset: std::fmt::Display,
{
    if !chiave {
        return (String::new(), SPESA_SENZA_CHIAVE.to_owned());
    }
    match r.and_then(|r| r.spesa_mese.map(|s| (s, r.aggiornato_a))) {
        Some((spesa, aggiornato)) => {
            let nota = aggiornato.map_or_else(String::new, |a| format!("aggiornato alle {}", a.with_timezone(tz).format("%H:%M")));
            (spesa.in_dollari(), nota)
        }
        None => (String::new(), SPESA_NESSUN_DATO.to_owned()),
    }
}

/// Un limite dell'abbonamento com'è nell'anello: la frazione dell'arco (0..1), «37%» e quando si
/// azzera.
#[derive(Debug, Clone, PartialEq)]
pub struct TestoLimite {
    pub frazione: f32,
    pub testo: String,
    pub azzera: String,
}

fn giorno_della_settimana(g: Weekday) -> &'static str {
    match g {
        Weekday::Mon => "lunedì",
        Weekday::Tue => "martedì",
        Weekday::Wed => "mercoledì",
        Weekday::Thu => "giovedì",
        Weekday::Fri => "venerdì",
        Weekday::Sat => "sabato",
        Weekday::Sun => "domenica",
    }
}

/// Il testo di un limite a `ora` (ora locale): «si azzera alle 14:30», «si azzera domani alle 09:05»
/// o col giorno della settimana per le finestre più lunghe. L'ora è un parametro: nessun orologio.
pub fn testo_limite(l: &Limite, ora: DateTime<Local>) -> TestoLimite {
    let pct = if l.percentuale.is_finite() { l.percentuale.clamp(0.0, 100.0) } else { 0.0 };
    let azzera = match Local.timestamp_opt(l.azzera_alle, 0).single() {
        None => String::new(),
        Some(quando) => {
            let giorni = quando.date_naive().signed_duration_since(ora.date_naive()).num_days();
            let alle = quando.format("%H:%M");
            match giorni {
                ..=0 => format!("si azzera alle {alle}"),
                1 => format!("si azzera domani alle {alle}"),
                2..=6 => format!("si azzera {} alle {alle}", giorno_della_settimana(quando.weekday())),
                // Da una settimana in là il solo giorno della settimana si leggerebbe come oggi.
                _ => format!("si azzera il {} alle {alle}", quando.format("%d/%m")),
            }
        }
    };
    TestoLimite { frazione: pct / 100.0, testo: format!("{}%", pct.round() as u32), azzera }
}

// ─── Avvisi a soglia: limiti dell'abbonamento e spesa delle API (Task 12) ──────────────────

/// La voce d'avviso del limite delle 5 ore e di quello dei 7 giorni.
pub const ID_LIMITE_5H: &str = "claude.limite.5h";
pub const ID_LIMITE_7G: &str = "claude.limite.7g";
/// La voce d'avviso della spesa del mese (80 % e 100 % della soglia: una voce che si aggiorna).
pub const ID_SPESA: &str = "claude.spesa";
/// La voce informativa della spesa insolita.
pub const ID_SPESA_INSOLITA: &str = "claude.spesa.insolita";

/// Le due finestre dei limiti dell'abbonamento.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finestra {
    CinqueOre,
    SetteGiorni,
}

impl Finestra {
    /// La chiave in `ConfigClaude::limiti_avvisati`.
    pub fn chiave(self) -> &'static str {
        match self {
            Finestra::CinqueOre => "5h",
            Finestra::SetteGiorni => "7g",
        }
    }

    pub fn id_voce(self) -> &'static str {
        match self {
            Finestra::CinqueOre => ID_LIMITE_5H,
            Finestra::SetteGiorni => ID_LIMITE_7G,
        }
    }

    fn nome(self) -> &'static str {
        match self {
            Finestra::CinqueOre => "Limite 5 ore",
            Finestra::SetteGiorni => "Limite settimanale",
        }
    }
}

/// L'id è di una delle voci d'avviso a soglia (limiti, spesa, spesa insolita).
pub fn e_voce_di_avviso(id: &str) -> bool {
    matches!(id, ID_LIMITE_5H | ID_LIMITE_7G | ID_SPESA | ID_SPESA_INSOLITA)
}

/// «all'82%», «al 61%»: l'articolo segue come si legge il numero (otto…, undici → «all'»).
fn alla_percentuale(n: u32) -> String {
    let elisa = n == 8 || n == 11 || (80..=89).contains(&n);
    if elisa {
        format!("all'{n}%")
    } else {
        format!("al {n}%")
    }
}

fn pulsante_ho_capito() -> Vec<Pulsante> {
    vec![Pulsante::new(crate::attenzione::PULSANTE_HO_CAPITO, "Ho capito")]
}

/// Due fini di finestra a meno di questo scarto (secondi) sono la stessa finestra: `resets_at` può
/// oscillare di poco fra una barra e l'altra e non deve riavvisare né riscrivere la configurazione.
pub const TOLLERANZA_FINESTRA: i64 = 300;

pub fn stessa_finestra(a: i64, b: i64) -> bool {
    (a - b).abs() < TOLLERANZA_FINESTRA
}

/// L'avviso «il limite sta finendo»: la percentuale ha raggiunto la `soglia`, la finestra non si è
/// ancora azzerata (`azzera_alle > ora`, secondi Unix) e non è già stata avvisata (`avvisato` è la
/// fine della finestra salvata: finestra nuova = `azzera_alle` diverso). Una voce di *attenzione*.
pub fn avviso_limite(f: Finestra, l: &Limite, soglia: u8, avvisato: Option<i64>, ora: i64) -> Option<Voce> {
    if !l.percentuale.is_finite() || l.percentuale < f32::from(soglia) || l.azzera_alle <= ora || avvisato.is_some_and(|a| stessa_finestra(a, l.azzera_alle)) {
        return None;
    }
    let adesso = Local.timestamp_opt(ora, 0).single()?;
    let azzera = testo_limite(l, adesso).azzera;
    // «si azzera alle 14:30» → «Si azzera alle 14:30.»
    let mut lettere = azzera.chars();
    let dettaglio = match lettere.next() {
        Some(prima) => format!("{}{}.", prima.to_uppercase(), lettere.as_str()),
        None => String::new(),
    };
    Some(Voce {
        id: f.id_voce().into(),
        livello: Livello::Attenzione,
        fonte: Fonte::Claude,
        titolo: format!("{} {}", f.nome(), alla_percentuale(l.percentuale.round() as u32)),
        dettaglio,
        pulsanti: pulsante_ho_capito(),
        creata: Instant::now(),
    })
}

/// «novembre 2026» da «2026-11»; se il testo non è un mese, il testo com'è.
fn mese_a_parole(mese: &str) -> String {
    const MESI: [&str; 12] = [
        "gennaio",
        "febbraio",
        "marzo",
        "aprile",
        "maggio",
        "giugno",
        "luglio",
        "agosto",
        "settembre",
        "ottobre",
        "novembre",
        "dicembre",
    ];
    mese.split_once('-')
        .and_then(|(anno, m)| Some((anno, MESI.get(m.parse::<usize>().ok()?.checked_sub(1)?)?)))
        .map_or_else(|| mese.to_owned(), |(anno, nome)| format!("{nome} {anno}"))
}

/// L'avviso di spesa: all'80 % e al 100 % della `soglia` (dollari), una volta per gradino e per
/// `mese` («2026-11»); `gia` è l'ultimo gradino avvisato nel mese. Restituisce il gradino da
/// salvare e la voce di *attenzione*. Una soglia assente o non valida non avvisa mai.
pub fn avviso_spesa(spesa: &Importo, soglia: f64, mese: &str, gia: Option<u8>) -> Option<(u8, Voce)> {
    if !soglia.is_finite() || soglia <= 0.0 {
        return None;
    }
    let dollari = spesa.micro_centesimi as f64 / crate::lati::MICRO_PER_DOLLARO;
    let gradino: u8 = if dollari >= soglia {
        100
    } else if dollari * 100.0 >= soglia * 80.0 {
        80
    } else {
        return None;
    };
    if gia.is_some_and(|g| g >= gradino) {
        return None;
    }
    let tetto = crate::lati::importo_da_dollari(soglia).in_dollari();
    Some((
        gradino,
        Voce {
            id: ID_SPESA.into(),
            livello: Livello::Attenzione,
            fonte: Fonte::Claude,
            titolo: format!("Spesa API {} della soglia ({tetto})", alla_percentuale(u32::from(gradino))),
            dettaglio: format!("Spesa del mese ({}): {}.", mese_a_parole(mese), spesa.in_dollari()),
            pulsanti: pulsante_ho_capito(),
            creata: Instant::now(),
        },
    ))
}

/// La voce «spesa insolita»: ieri (`giorno`) è costato più di 3 volte la media dei 7 giorni prima.
/// Solo un'*informazione*.
pub fn voce_insolita(ieri: &Importo, giorno: NaiveDate) -> Voce {
    Voce {
        id: ID_SPESA_INSOLITA.into(),
        livello: Livello::Informazione,
        fonte: Fonte::Claude,
        titolo: format!("Spesa API insolita ieri: {}", ieri.in_dollari()),
        dettaglio: format!("Più di 3 volte la media dei 7 giorni prima ({}).", giorno.format("%d/%m")),
        pulsanti: pulsante_ho_capito(),
        creata: Instant::now(),
    }
}

/// «Avvisa al … % del limite» come scritto nelle Impostazioni: un intero da 50 a 100.
pub fn leggi_soglia_limite(testo: &str) -> Result<u8, &'static str> {
    match testo.trim().parse::<u8>() {
        Ok(v) if (50..=100).contains(&v) => Ok(v),
        _ => Err("Scrivi un numero intero da 50 a 100, per esempio 80."),
    }
}

/// La soglia di spesa mensile come scritta nelle Impostazioni: vuota = nessuna soglia, altrimenti
/// un importo positivo in dollari, con la virgola o il punto.
pub fn leggi_soglia_spesa(testo: &str) -> Result<Option<f64>, &'static str> {
    let t = testo.trim();
    if t.is_empty() {
        return Ok(None);
    }
    // Solo cifre con al più un separatore decimale (virgola o punto) e due decimali: niente
    // migliaia («1.000») né esponenti («1e3»).
    let forma = {
        let (intera, dec) = t.split_once([',', '.']).map_or((t, ""), |(i, d)| (i, d));
        !intera.is_empty()
            && intera.bytes().all(|b| b.is_ascii_digit())
            && dec.len() <= 2
            && dec.bytes().all(|b| b.is_ascii_digit())
            && !dec.contains([',', '.'])
            && (t.len() == intera.len() || (!dec.is_empty() && t.len() == intera.len() + 1 + dec.len()))
    };
    match t.replace(',', ".").parse::<f64>() {
        Ok(v) if forma && v.is_finite() && v > 0.0 && v <= 1_000_000_000.0 => Ok(Some(v)),
        _ => Err("Scrivi un importo maggiore di zero, per esempio 250 o 99,50, oppure lascia vuoto per non avere una soglia."),
    }
}

// ─── Dettaglio delle API nelle schede della finestra di gestione (Task 11) ─────────────────

/// La nota di una scheda che la chiave non riesce a leggere.
pub const SCHEDA_NON_DISPONIBILE: &str = "Non disponibile con questa chiave";
const RITARDO_DATI: &str = "dati con 4–24 ore di ritardo";
const NESSUN_VALORE: &str = "—";

/// «1.284.500»: le migliaia col punto, come si scrive in italiano.
fn numero(n: u64) -> String {
    let cifre = n.to_string();
    let mut fuori = String::new();
    for (i, c) in cifre.chars().enumerate() {
        if i > 0 && (cifre.len() - i).is_multiple_of(3) {
            fuori.push('.');
        }
        fuori.push(c);
    }
    fuori
}

fn data_in<Tz: TimeZone>(d: Option<DateTime<chrono::Utc>>, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    d.map_or_else(|| NESSUN_VALORE.to_owned(), |d| d.with_timezone(tz).format("%d/%m/%Y").to_string())
}

fn stato_chiave(s: &str) -> String {
    match s {
        "active" => "Attiva".into(),
        "inactive" => "Disattivata".into(),
        "archived" => "Archiviata".into(),
        "expired" => "Scaduta".into(),
        "" => NESSUN_VALORE.into(),
        altro => altro.to_owned(),
    }
}

fn ruolo_membro(r: &str) -> String {
    match r {
        "admin" => "Amministratore".into(),
        "developer" => "Sviluppatore".into(),
        "billing" => "Fatturazione".into(),
        "user" => "Utente".into(),
        "claude_code_user" => "Utente di Claude Code".into(),
        "" => NESSUN_VALORE.into(),
        altro => altro.to_owned(),
    }
}

fn nome_limite(t: &str) -> String {
    match t {
        "requests_per_minute" => "Richieste al minuto".into(),
        "input_tokens_per_minute" => "Token in ingresso al minuto".into(),
        "output_tokens_per_minute" => "Token in uscita al minuto".into(),
        altro => altro.to_owned(),
    }
}

/// Il suggerimento della chiave: solo le ultime cifre, mai il prefisso né il valore.
fn suggerimento(s: &str) -> String {
    if s.is_empty() || s.contains("sk-ant") {
        NESSUN_VALORE.to_owned()
    } else {
        format!("…{s}")
    }
}

fn riga<const N: usize>(celle: [String; N]) -> Vec<String> {
    celle.into()
}

fn colonne(nomi: &[&str]) -> Vec<String> {
    nomi.iter().map(|n| (*n).to_owned()).collect()
}

/// Nome e azione della Console delle cinque schede, nell'ordine in cui si mostrano.
const SCHEDE: [(&str, &str); 5] = [
    ("Spesa", "console:spesa"),
    ("Token", "console:uso"),
    ("Chiavi", "console:chiavi"),
    ("Workspace e membri", "console:membri"),
    ("Limiti", "console:limiti"),
];

fn scheda_di(i: usize, nota: &str, colonne: Vec<String>, righe: Vec<Vec<String>>) -> SchedaDati {
    SchedaDati { nome: SCHEDE[i].0.into(), nota: nota.into(), colonne, righe, azione_console: SCHEDE[i].1.into() }
}

/// Le cinque schede senza dati, tutte con la stessa nota (nessuna lettura ancora, o non riuscita).
pub fn schede_vuote(nota: &str) -> Vec<SchedaDati> {
    (0..SCHEDE.len()).map(|i| scheda_di(i, nota, Vec::new(), Vec::new())).collect()
}

/// Le cinque schede del dettaglio (Spesa, Token, Chiavi, Workspace e membri, Limiti), nel fuso
/// del sistema. `adesso_locale` è l'ora della lettura («14:05»). Una scheda che la chiave non
/// legge ha la sua nota e nessuna riga; un riepilogo senza nessuna lettura riuscita dice perché.
/// Mai il valore di una chiave: solo il nome e le ultime cifre del suggerimento.
pub fn schede(r: &Riepilogo, adesso_locale: &str) -> Vec<SchedaDati> {
    schede_in(r, adesso_locale, &Local)
}

/// `schede` nel fuso `tz` (i test ne passano uno fisso).
pub fn schede_in<Tz: TimeZone>(r: &Riepilogo, adesso_locale: &str, tz: &Tz) -> Vec<SchedaDati>
where
    Tz::Offset: std::fmt::Display,
{
    use api_claude::Scheda;
    if let Some(e) = r.errore {
        return schede_vuote(&format!("Lettura delle {adesso_locale} non riuscita: {e}"));
    }
    let nota_ritardo = match r.aggiornato_a {
        Some(a) => {
            let a = a.with_timezone(tz);
            format!("Aggiornato al {} alle {} · {RITARDO_DATI}", a.format("%d/%m"), a.format("%H:%M"))
        }
        None => format!("Letto alle {adesso_locale} · {RITARDO_DATI}"),
    };
    let nota_diretta = format!("Letto alle {adesso_locale}");
    let dollari = |i: Option<api_claude::Importo>| i.map_or_else(|| SPESA_NESSUN_DATO.to_owned(), |i| i.in_dollari());

    let mut spesa = vec![
        riga(["Mese in corso".into(), String::new(), dollari(r.spesa_mese)]),
        riga(["Ieri".into(), String::new(), dollari(r.spesa_ieri)]),
    ];
    spesa.extend(r.per_modello.iter().map(|(n, i)| riga(["Modello".into(), n.clone(), i.in_dollari()])));
    spesa.extend(r.per_chiave.iter().map(|(n, i, _)| riga(["Chiave".into(), n.clone(), i.in_dollari()])));
    spesa.extend(r.per_workspace.iter().map(|(n, i)| riga(["Workspace".into(), n.clone(), i.in_dollari()])));
    spesa.extend(r.giorni.iter().rev().map(|(d, i)| riga(["Giorno".into(), d.format("%d/%m").to_string(), i.in_dollari()])));

    let token = r.token.map_or_else(Vec::new, |t| {
        vec![
            riga(["Ingresso".into(), numero(t.ingresso)]),
            riga(["Uscita".into(), numero(t.uscita)]),
            riga(["Cache letta".into(), numero(t.cache_lettura)]),
            riga(["Cache scritta".into(), numero(t.cache_scrittura)]),
            riga(["Chiamate del mese".into(), r.chiamate_mese.map_or_else(|| "non fornite".to_owned(), numero)]),
        ]
    });

    let chiavi = r
        .chiavi
        .iter()
        .map(|k| {
            let chiamate = k.chiamate;
            riga([
                k.nome.clone(),
                stato_chiave(&k.stato),
                data_in(k.creata, tz),
                data_in(k.scadenza, tz),
                suggerimento(&k.suggerimento),
                chiamate.map_or_else(|| NESSUN_VALORE.to_owned(), numero),
            ])
        })
        .collect();

    let mut membri: Vec<Vec<String>> =
        r.workspace.iter().map(|w| riga(["Workspace".into(), w.nome.clone(), String::new(), String::new()])).collect();
    membri.extend(
        r.membri.iter().map(|m| riga(["Membro".into(), m.nome.clone(), m.email.clone(), ruolo_membro(&m.ruolo)])),
    );

    let limiti = r
        .limiti
        .iter()
        .flat_map(|g| g.limiti.iter().map(|(t, v)| riga([g.nome.clone(), nome_limite(t), numero(*v)])))
        .collect();

    let dati = [
        (Scheda::Spesa, &nota_ritardo, colonne(&["Voce", "Dettaglio", "Spesa"]), spesa),
        (Scheda::Token, &nota_ritardo, colonne(&["Voce", "Token"]), token),
        (
            Scheda::Chiavi,
            &nota_diretta,
            colonne(&["Nome", "Stato", "Creata", "Scade", "Suggerimento", "Chiamate"]),
            chiavi,
        ),
        (Scheda::WorkspaceMembri, &nota_diretta, colonne(&["Tipo", "Nome", "Email", "Ruolo"]), membri),
        (Scheda::Limiti, &nota_diretta, colonne(&["Gruppo", "Limite", "Valore"]), limiti),
    ];
    dati.into_iter()
        .enumerate()
        .map(|(i, (quale, nota, colonne, mut righe))| {
            if r.non_disponibili.contains(&quale) {
                return scheda_di(i, SCHEDA_NON_DISPONIBILE, Vec::new(), Vec::new());
            }
            // Ultimo controllo: nessuna chiave per intero in nessuna cella, qualunque cosa dica l'API.
            for cella in righe.iter_mut().flatten() {
                if cella.contains("sk-ant-") {
                    *cella = NESSUN_VALORE.to_owned();
                }
            }
            scheda_di(i, nota, colonne, righe)
        })
        .collect()
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
    fn le_voci_di_chi_aspetta_e_di_chi_ha_finito_aprono_il_terminale() {
        let t = Instant::now();
        let apri = vec![Pulsante::new(PULSANTE_TERMINALE, "Apri il terminale")];
        let mut s = Sessioni::new();
        con(&mut s, ev("a", TipoEvento::PermissionRequest), t);
        assert_eq!(voce_sessione(prima(&s), t).unwrap().pulsanti, apri);
        con(&mut s, ev("a", TipoEvento::Stop), t);
        assert_eq!(voce_sessione(prima(&s), t).unwrap().pulsanti, apri);
        let errore = EventoHook { errore: Some("rate_limit".into()), ..ev("a", TipoEvento::StopFailure) };
        con(&mut s, errore, t);
        assert!(voce_sessione(prima(&s), t).unwrap().pulsanti.is_empty(), "l'errore non ha il pulsante");
    }

    #[test]
    fn la_voce_del_terminale_mancante_e_informativa_e_non_e_una_sessione() {
        let v = voce_terminale_mancante(Instant::now());
        assert_eq!(v.livello, Livello::Informazione);
        assert_eq!(v.titolo, "Non trovo il terminale di questa sessione");
        assert_eq!(impronta_da_voce(&v.id), None);
        assert!(v.id.starts_with(PREFISSO_CLAUDE));
    }

    #[test]
    fn l_impronta_dentro_l_id_della_voce() {
        let [aspetta, errore, fatto] = id_voci("a");
        for v in [&aspetta, &errore, &fatto] {
            assert_eq!(impronta_da_voce(v), Some(id_sessione("a").as_str()));
        }
        assert_eq!(impronta_da_voce(&id_voce_permesso(7)), None);
        assert_eq!(impronta_da_voce(ID_COLLEGAMENTO), None);
        assert_eq!(impronta_da_voce(ID_AGGIORNA), None);
        assert_eq!(impronta_da_voce("casa.offline"), None);
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
        assert_eq!(parti_attivita(&s, t), None);
        con(&mut s, ev("a", TipoEvento::UserPromptSubmit), t);
        let pre = EventoHook { strumento: Some("Edit".into()), ..ev("a", TipoEvento::PreToolUse) };
        con(&mut s, pre, t + Duration::from_secs(1));
        let ora = t + Duration::from_secs(192);
        assert_eq!(parti_attivita(&s, ora), Some(("Claude · Modifica".into(), "3:12".into())));
        assert_eq!(nome_attivita(&s, ora).as_deref(), Some("Claude, Modifica, 3 minuti"));
        // Una seconda sessione, partita dopo: il numero e la durata della più lunga.
        con(&mut s, ev("b", TipoEvento::UserPromptSubmit), t + Duration::from_secs(100));
        assert_eq!(parti_attivita(&s, ora), Some(("Claude ×2".into(), "3:12".into())));
        assert_eq!(nome_attivita(&s, ora).as_deref(), Some("Claude, 2 sessioni, 3 minuti"));
        // Chi ha finito non conta.
        con(&mut s, ev("a", TipoEvento::Stop), ora);
        con(&mut s, ev("b", TipoEvento::Stop), ora);
        assert_eq!(parti_attivita(&s, ora), None);
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

    fn permesso(command: &str) -> EventoHook {
        EventoHook {
            strumento: Some("PowerShell".into()),
            riassunto: Some(command.into()),
            ..ev("a", TipoEvento::PermissionRequest)
        }
    }

    #[test]
    fn riassunto_pulito() {
        let v = voce_permesso(7, &permesso("rm \u{202E}-rf\n/tmp\u{200B}/x"), Instant::now());
        assert_eq!(v.titolo, "Claude chiede: Comando rm -rf /tmp/x");
        assert_eq!(v.dettaglio, "rm -rf /tmp/x\nSessione: progetto-a");
        for t in [&v.titolo, &v.dettaglio] {
            assert!(!t.contains('\u{202E}') && !t.contains('\u{200B}'), "{t:?}");
        }
        assert!(!v.titolo.contains('\n'), "a capo diventati spazi");
        assert_eq!((v.livello, v.fonte, v.id.as_str()), (Livello::Bloccante, Fonte::Claude, "claude.permesso.7"));
        assert_eq!(id_permesso_da_voce(&v.id), Some(7));
        assert_eq!(id_permesso_da_voce("claude.permesso.x"), None);
        assert_eq!(id_permesso_da_voce("claude.0123.aspetta"), None);
        // Anche la cartella non fidata viene ripulita.
        let e = EventoHook { cartella: "pro\u{202E}getto".into(), ..permesso("ls") };
        assert_eq!(voce_permesso(1, &e, Instant::now()).dettaglio, "ls\nSessione: progetto");
    }

    #[test]
    fn titolo_permesso_tagliato_a_sessanta() {
        let lungo = "x".repeat(300);
        let v = voce_permesso(1, &permesso(&lungo), Instant::now());
        assert!(v.titolo.chars().count() <= "Claude chiede: Comando ".chars().count() + 60, "{}", v.titolo.chars().count());
        assert!(v.titolo.ends_with('…'));
        assert_eq!(v.dettaglio, format!("{lungo}\nSessione: progetto-a"), "il dettaglio ha il riassunto intero");
        // Esattamente 60: nessun taglio.
        let v = voce_permesso(1, &permesso(&"y".repeat(60)), Instant::now());
        assert!(!v.titolo.ends_with('…'));
        // Senza riassunto né strumento né cartella.
        let nudo = EventoHook { cartella: String::new(), ..ev("a", TipoEvento::PermissionRequest) };
        let v = voce_permesso(1, &nudo, Instant::now());
        assert_eq!(v.titolo, "Claude chiede: Strumento");
        assert_eq!(v.dettaglio, "Sessione: Claude");
    }

    #[test]
    fn pulsanti_del_permesso() {
        let v = voce_permesso(1, &permesso("ls"), Instant::now());
        assert_eq!(v.pulsanti, vec![Pulsante::new("nega", "Nega"), Pulsante::delicato("consenti", "Consenti")]);
        assert_eq!(PULSANTE_TERMINALE, "terminale");
    }

    #[test]
    fn errori_in_frasi() {
        assert_eq!(frase_errore_claude("authentication_failed"), "Accesso da rifare");
        assert_eq!(frase_errore_claude("billing_error"), "Problema di fatturazione");
        assert_eq!(frase_errore_claude("server_error"), "Servizio non disponibile");
        assert_eq!(frase_errore_claude("max_output_tokens"), "Risposta troppo lunga");
        assert_eq!(frase_errore_claude("qualcosa"), "Errore dell'API");
    }

    // ─── Vista Claude (Task 9) ───────────────────────────────────────────────

    fn cinque_sessioni(t: Instant) -> Sessioni {
        let mut s = Sessioni::new();
        // Dalla più vecchia: lavora, fatto, errore, ti aspetta, lavora.
        con(&mut s, EventoHook { cartella: "uno".into(), ..ev("a", TipoEvento::UserPromptSubmit) }, t);
        con(&mut s, EventoHook { cartella: "due".into(), ..ev("b", TipoEvento::Stop) }, t);
        let errore = EventoHook { cartella: "tre".into(), errore: Some("rate_limit".into()), ..ev("c", TipoEvento::StopFailure) };
        con(&mut s, errore, t);
        con(&mut s, EventoHook { cartella: "quattro".into(), ..ev("d", TipoEvento::PermissionRequest) }, t);
        con(&mut s, EventoHook { cartella: "cinque".into(), ..ev("e", TipoEvento::UserPromptSubmit) }, t);
        s
    }

    #[test]
    fn al_massimo_tre_righe() {
        let t = Instant::now();
        let s = cinque_sessioni(t);
        let (righe, altre) = righe_sessioni(&s, &HashMap::new(), t);
        assert_eq!(righe.len(), 3);
        assert_eq!(altre, 2);
        // Nell'ordine di urgenza dell'elenco: chi aspetta, chi si è fermato, chi ha finito.
        let attesi: Vec<_> = s.elenco().iter().take(3).map(|x| id_sessione(&x.id)).collect();
        assert_eq!(righe.iter().map(|r| r.id.clone()).collect::<Vec<_>>(), attesi);
        assert_eq!(righe[0].stato_testo, "Ti aspetta");
        assert_eq!(righe[1].stato_testo, "Errore");
        assert_eq!(righe[2].stato_testo, "Fatto");
        // Meno di tre: nessuna «altra».
        let mut poche = Sessioni::new();
        con(&mut poche, ev("a", TipoEvento::Stop), t);
        assert_eq!(righe_sessioni(&poche, &HashMap::new(), t).1, 0);
        assert_eq!(righe_sessioni(&Sessioni::new(), &HashMap::new(), t), (vec![], 0));
    }

    #[test]
    fn riga_con_livello_strumento_durata_e_contesto() {
        let t = Instant::now();
        let mut s = Sessioni::new();
        con(&mut s, ev("a", TipoEvento::UserPromptSubmit), t);
        let pre = EventoHook { strumento: Some("Edit".into()), ..ev("a", TipoEvento::PreToolUse) };
        con(&mut s, pre, t);
        let contesto: HashMap<String, f32> = [("a".to_owned(), 41.6)].into();
        let (righe, _) = righe_sessioni(&s, &contesto, t + Duration::from_secs(192));
        let r = &righe[0];
        assert_eq!((r.stato_testo, r.livello), ("Lavora", None));
        assert_eq!((r.cartella.as_str(), r.strumento.as_str(), r.durata.as_str()), ("progetto-a", "Modifica", "3:12"));
        assert_eq!(r.contesto.as_deref(), Some("42%"));
        assert_eq!(r.id, id_sessione("a"), "l'impronta, mai il session_id");
        // Chi non lavora: niente strumento né durata (non cambierebbe a ogni secondo), e il livello.
        con(&mut s, ev("a", TipoEvento::PermissionRequest), t);
        let (righe, _) = righe_sessioni(&s, &HashMap::new(), t + Duration::from_secs(300));
        let r = &righe[0];
        assert_eq!(
            (r.livello, r.strumento.as_str(), r.durata.as_str(), r.contesto.clone()),
            (Some(Livello::Attenzione), "", "", None)
        );
        con(&mut s, ev("a", TipoEvento::Stop), t);
        assert_eq!(righe_sessioni(&s, &HashMap::new(), t).0[0].livello, Some(Livello::Informazione));
    }

    #[test]
    fn riga_senza_cartella_si_dice_claude() {
        let t = Instant::now();
        let mut s = Sessioni::new();
        con(&mut s, EventoHook { cartella: String::new(), ..ev("a", TipoEvento::Stop) }, t);
        con(&mut s, EventoHook { cartella: "\u{202e}\n".into(), ..ev("b", TipoEvento::Stop) }, t);
        let (righe, _) = righe_sessioni(&s, &HashMap::new(), t);
        assert!(righe.iter().all(|r| r.cartella == "Claude"), "{righe:?}");
        let mut s = Sessioni::new();
        con(&mut s, EventoHook { cartella: "pro\u{202e}getto".into(), ..ev("a", TipoEvento::Stop) }, t);
        assert_eq!(righe_sessioni(&s, &HashMap::new(), t).0[0].cartella, "progetto");
    }

    fn riepilogo_con_spesa(micro: Option<&str>, aggiornato: Option<(u32, u32)>) -> Riepilogo {
        use chrono::TimeZone;
        Riepilogo {
            spesa_mese: micro.and_then(api_claude::importo_da),
            aggiornato_a: aggiornato.map(|(h, m)| chrono::Utc.with_ymd_and_hms(2026, 11, 18, h, m, 0).unwrap()),
            ..Default::default()
        }
    }

    #[test]
    fn spesa_senza_dati_e_senza_chiave() {
        // Con la chiave ma senza riepilogo: nessun dato.
        assert_eq!(testo_spesa(None, true), ("".to_owned(), "nessun dato".to_owned()));
        // Senza la chiave: la frase della guida, qualunque cosa ci fosse.
        let frase = "Aggiungi una chiave da amministratore nelle Impostazioni".to_owned();
        assert_eq!(testo_spesa(None, false), ("".to_owned(), frase.clone()));
        let r = riepilogo_con_spesa(Some("41280"), Some((14, 2)));
        assert_eq!(testo_spesa(Some(&r), false), ("".to_owned(), frase));
        // Letto ma senza spesa (dati senza data, mese non letto): nessun dato, mai «0».
        let vuoto = riepilogo_con_spesa(None, None);
        assert_eq!(testo_spesa(Some(&vuoto), true), ("".to_owned(), "nessun dato".to_owned()));
    }

    #[test]
    fn spesa_con_dati_e_ora_dell_aggiornamento() {
        let r = riepilogo_con_spesa(Some("41280"), Some((14, 2)));
        // Nel fuso scelto (qui +1 ora), non in UTC.
        let roma = chrono::FixedOffset::east_opt(3600).unwrap();
        assert_eq!(testo_spesa_in(Some(&r), true, &roma), ("412,80 $".to_owned(), "aggiornato alle 15:02".to_owned()));
        assert_eq!(testo_spesa_in(Some(&r), true, &chrono::Utc).1, "aggiornato alle 14:02");
        assert!(!testo_spesa(Some(&r), true).1.contains("UTC"));
        // PF18: un mese ancora a zero ma con dati aggiornati fino a fine periodo è «0,00 $».
        let zero = riepilogo_con_spesa(Some("0"), Some((10, 0)));
        assert_eq!(testo_spesa(Some(&zero), true).0, "0,00 $");
    }

    // ─── Avvisi a soglia (Task 12) ───────────────────────────────────────────

    fn lim(percentuale: f32, azzera_alle: i64) -> Limite {
        Limite { percentuale, azzera_alle }
    }

    /// Un'ora locale fissa: 18/11/2026 alle `h`:`m`.
    fn ora_locale(h: u32, m: u32) -> i64 {
        Local.with_ymd_and_hms(2026, 11, 18, h, m, 0).single().expect("ora locale valida").timestamp()
    }

    #[test]
    fn avviso_limite_una_volta_per_finestra() {
        let (ora, azzera) = (ora_locale(10, 0), ora_locale(14, 30));
        let v = avviso_limite(Finestra::CinqueOre, &lim(82.0, azzera), 80, None, ora).expect("82 >= 80: avvisa");
        assert_eq!(v.id, "claude.limite.5h");
        assert_eq!((v.livello, v.fonte), (Livello::Attenzione, Fonte::Claude));
        assert_eq!(v.titolo, "Limite 5 ore all'82%");
        assert_eq!(v.dettaglio, "Si azzera alle 14:30.");
        assert_eq!(v.pulsanti, vec![Pulsante::new(crate::attenzione::PULSANTE_HO_CAPITO, "Ho capito")]);
        // Già avvisata questa finestra (anche prima di un riavvio): niente.
        assert!(avviso_limite(Finestra::CinqueOre, &lim(95.0, azzera), 80, Some(azzera), ora).is_none());
        // Una finestra nuova (azzera_alle diverso): di nuovo.
        assert!(avviso_limite(Finestra::CinqueOre, &lim(82.0, azzera + 18_000), 80, Some(azzera), ora).is_some());
        // Sotto la soglia: niente; alla soglia: sì.
        assert!(avviso_limite(Finestra::CinqueOre, &lim(79.9, azzera), 80, None, ora).is_none());
        assert!(avviso_limite(Finestra::CinqueOre, &lim(80.0, azzera), 80, None, ora).is_some());
        // La finestra dei 7 giorni, con la sua voce e il suo nome; l'articolo segue il numero.
        let v = avviso_limite(Finestra::SetteGiorni, &lim(61.0, azzera), 50, None, ora).unwrap();
        assert_eq!((v.id.as_str(), v.titolo.as_str()), ("claude.limite.7g", "Limite settimanale al 61%"));
        let titolo = |p: f32| avviso_limite(Finestra::CinqueOre, &lim(p, azzera), 10, None, ora).unwrap().titolo;
        assert_eq!(titolo(80.0), "Limite 5 ore all'80%");
        assert_eq!(titolo(100.0), "Limite 5 ore al 100%");
        assert_eq!(titolo(11.0), "Limite 5 ore all'11%");
        assert_eq!(titolo(18.0), "Limite 5 ore al 18%");
        assert_eq!(titolo(85.4), "Limite 5 ore all'85%");
    }

    #[test]
    fn finestra_con_resets_at_che_oscilla_e_la_stessa() {
        let (ora, azzera) = (ora_locale(10, 0), ora_locale(14, 30));
        // Qualche secondo di scarto: stessa finestra, niente riavviso.
        assert!(avviso_limite(Finestra::CinqueOre, &lim(90.0, azzera + 7), 80, Some(azzera), ora).is_none());
        assert!(avviso_limite(Finestra::CinqueOre, &lim(90.0, azzera - 299), 80, Some(azzera), ora).is_none());
        // Cinque minuti o più: un'altra finestra.
        assert!(avviso_limite(Finestra::CinqueOre, &lim(90.0, azzera + 300), 80, Some(azzera), ora).is_some());
    }

    #[test]
    fn soglia_di_spesa_senza_migliaia_ne_esponenti() {
        assert_eq!(leggi_soglia_spesa("12,5"), Ok(Some(12.5)));
        assert_eq!(leggi_soglia_spesa("12.50"), Ok(Some(12.5)));
        assert_eq!(leggi_soglia_spesa(" 250 "), Ok(Some(250.0)));
        assert_eq!(leggi_soglia_spesa(""), Ok(None));
        for male in ["1.000", "1e3", "1.000,50", "12,345", "1,2,3", ",5", "5,", "+5", "0", "-1", "abc", "1_000"] {
            assert!(leggi_soglia_spesa(male).is_err(), "{male:?} si rifiuta");
        }
    }

    #[test]
    fn avviso_limite_scaduto_niente() {
        let (ora, azzera) = (ora_locale(10, 0), ora_locale(14, 30));
        assert!(avviso_limite(Finestra::CinqueOre, &lim(90.0, ora), 80, None, ora).is_none(), "azzera_alle == ora");
        assert!(avviso_limite(Finestra::SetteGiorni, &lim(90.0, azzera), 80, None, azzera + 1).is_none());
        // Una percentuale non finita non avvisa mai.
        assert!(avviso_limite(Finestra::CinqueOre, &lim(f32::NAN, azzera), 80, None, ora).is_none());
        assert!(avviso_limite(Finestra::CinqueOre, &lim(f32::INFINITY, azzera), 80, None, ora).is_none());
    }

    fn dollari(s: &str) -> api_claude::Importo {
        api_claude::importo_da(s).expect("importo valido")
    }

    #[test]
    fn spesa_ottanta_e_cento() {
        // 200 $ su 250 $ = 80 %: il primo gradino.
        let (gradino, v) = avviso_spesa(&dollari("20000"), 250.0, "2026-11", None).expect("80%");
        assert_eq!(gradino, 80);
        assert_eq!((v.id.as_str(), v.livello, v.fonte), ("claude.spesa", Livello::Attenzione, Fonte::Claude));
        assert_eq!(v.titolo, "Spesa API all'80% della soglia (250,00 $)");
        assert!(v.dettaglio.contains("200,00 $") && v.dettaglio.contains("novembre 2026"), "{}", v.dettaglio);
        assert_eq!(v.pulsanti, vec![Pulsante::new(crate::attenzione::PULSANTE_HO_CAPITO, "Ho capito")]);
        // Già avvisato a 80 e ancora a 80: niente.
        assert!(avviso_spesa(&dollari("20000"), 250.0, "2026-11", Some(80)).is_none());
        assert!(avviso_spesa(&dollari("22000"), 250.0, "2026-11", Some(80)).is_none());
        // 250 $ su 250 $ con già 80: il secondo gradino.
        let (gradino, v) = avviso_spesa(&dollari("25000"), 250.0, "2026-11", Some(80)).expect("100%");
        assert_eq!((gradino, v.titolo.as_str()), (100, "Spesa API al 100% della soglia (250,00 $)"));
        // Saltato l'80 si arriva dritti al 100, una voce sola; già avvisato a 100: mai più.
        assert_eq!(avviso_spesa(&dollari("30000"), 250.0, "2026-11", None).map(|(g, _)| g), Some(100));
        assert!(avviso_spesa(&dollari("30000"), 250.0, "2026-11", Some(100)).is_none());
        // Sotto l'80 % niente; nessuna soglia (o non valida): mai.
        assert!(avviso_spesa(&dollari("19900"), 250.0, "2026-11", None).is_none());
        assert!(avviso_spesa(&dollari("30000"), 0.0, "2026-11", None).is_none());
        assert!(avviso_spesa(&dollari("30000"), -5.0, "2026-11", None).is_none());
        assert!(avviso_spesa(&dollari("30000"), f64::NAN, "2026-11", None).is_none());
    }

    #[test]
    fn spesa_insolita_in_una_voce_informativa() {
        let giorno = chrono::NaiveDate::from_ymd_opt(2026, 11, 17).unwrap();
        let v = voce_insolita(&dollari("4000"), giorno);
        assert_eq!((v.id.as_str(), v.livello, v.fonte), ("claude.spesa.insolita", Livello::Informazione, Fonte::Claude));
        assert_eq!(v.titolo, "Spesa API insolita ieri: 40,00 $");
        assert_eq!(v.pulsanti, vec![Pulsante::new(crate::attenzione::PULSANTE_HO_CAPITO, "Ho capito")]);
        assert!(v.dettaglio.contains("17/11"), "{}", v.dettaglio);
    }

    // ─── Dettaglio API nelle schede (Task 11) ────────────────────────────────

    fn riepilogo_completo() -> Riepilogo {
        use api_claude::{importo_da, GruppoLimiti, InfoChiave, InfoMembro, InfoWorkspace, Token};
        use chrono::{NaiveDate, Utc};
        let euro = |s: &str| importo_da(s).unwrap();
        Riepilogo {
            aggiornato_a: Some(Utc.with_ymd_and_hms(2026, 11, 18, 14, 2, 0).unwrap()),
            spesa_mese: Some(euro("41280")),
            spesa_ieri: Some(euro("4000")),
            giorni: vec![(NaiveDate::from_ymd_opt(2026, 11, 17).unwrap(), euro("4000"))],
            per_modello: vec![("claude-sonnet-5-5".into(), euro("25000"))],
            per_chiave: vec![("Chiave di prova A".into(), euro("25000"), Some(90))],
            per_workspace: vec![("Workspace di prova".into(), euro("41280"))],
            token: Some(Token { ingresso: 1_284_500, uscita: 891_000, cache_lettura: 3_200_000, cache_scrittura: 120_000 }),
            chiamate_mese: Some(128),
            chiavi: vec![
                InfoChiave {
                    id: "apikey_FAKE0001".into(),
                    nome: "Chiave di prova A".into(),
                    stato: "active".into(),
                    scadenza: Some(Utc.with_ymd_and_hms(2027, 1, 10, 9, 0, 0).unwrap()),
                    creata: Some(Utc.with_ymd_and_hms(2026, 1, 10, 9, 0, 0).unwrap()),
                    suggerimento: "0001".into(),
                    chiamate: Some(90),
                },
                InfoChiave {
                    id: "apikey_FAKE0002".into(),
                    nome: "Chiave di prova B".into(),
                    stato: "inactive".into(),
                    scadenza: None,
                    creata: None,
                    // Un suggerimento ostile: il prefisso non deve uscire comunque.
                    suggerimento: "sk-ant-api03-SEGRETO".into(),
                    chiamate: None,
                },
            ],
            workspace: vec![InfoWorkspace { id: "w1".into(), nome: "Workspace di prova".into() }],
            membri: vec![InfoMembro { id: "u1".into(), email: "membro@example.com".into(), nome: "Membro di prova".into(), ruolo: "admin".into() }],
            limiti: vec![GruppoLimiti {
                nome: "Claude Sonnet di prova".into(),
                limiti: vec![("requests_per_minute".into(), 4000), ("input_tokens_per_minute".into(), 2_000_000)],
            }],
            ..Default::default()
        }
    }

    #[test]
    fn schede_dal_riepilogo() {
        let r = Riepilogo { non_disponibili: vec![api_claude::Scheda::Token], token: None, chiamate_mese: None, ..riepilogo_completo() };
        let s = schede_in(&r, "14:05", &chrono::Utc);
        let nomi: Vec<_> = s.iter().map(|x| x.nome.as_str()).collect();
        assert_eq!(nomi, ["Spesa", "Token", "Chiavi", "Workspace e membri", "Limiti"]);
        let console: Vec<_> = s.iter().map(|x| x.azione_console.as_str()).collect();
        assert_eq!(console, ["console:spesa", "console:uso", "console:chiavi", "console:membri", "console:limiti"]);
        // La scheda non disponibile lo dice e non ha righe.
        assert_eq!(s[1].nota, "Non disponibile con questa chiave");
        assert!(s[1].righe.is_empty());
        // La spesa dice da quando sono i dati e che arrivano in ritardo.
        assert_eq!(s[0].nota, "Aggiornato al 18/11 alle 14:02 · dati con 4–24 ore di ritardo");
        assert!(s[0].righe.contains(&vec!["Mese in corso".to_owned(), String::new(), "412,80 $".to_owned()]));
        // Le chiavi: nome, stato, creata, scade, suggerimento, chiamate del mese; mai il valore.
        assert_eq!(s[2].colonne, ["Nome", "Stato", "Creata", "Scade", "Suggerimento", "Chiamate"]);
        assert_eq!(s[2].righe[0], ["Chiave di prova A", "Attiva", "10/01/2026", "10/01/2027", "…0001", "90"]);
        assert_eq!(s[2].righe[1], ["Chiave di prova B", "Disattivata", "—", "—", "—", "—"]);
        assert_eq!(s[3].righe[1][3], "Amministratore");
        assert_eq!(s[4].righe[0], ["Claude Sonnet di prova", "Richieste al minuto", "4.000"]);
        for scheda in &s {
            assert!(scheda.colonne.len() <= 6, "{}", scheda.nome);
            for cella in scheda.righe.iter().flatten().chain(scheda.colonne.iter()).chain([&scheda.nota]) {
                assert!(!cella.contains("sk-ant-"), "{}: {cella}", scheda.nome);
            }
        }
    }

    #[test]
    fn le_chiamate_sono_per_chiave_e_non_per_nome() {
        let mut r = riepilogo_completo();
        // Due chiavi con lo stesso nome ma chiamate diverse: ognuna ha le sue.
        r.chiavi[1].nome = r.chiavi[0].nome.clone();
        r.chiavi[1].chiamate = Some(38);
        r.per_chiave = vec![(r.chiavi[0].nome.clone(), api_claude::importo_da("1").unwrap(), Some(90))];
        let s = schede_in(&r, "14:05", &chrono::Utc);
        assert_eq!((s[2].righe[0][5].as_str(), s[2].righe[1][5].as_str()), ("90", "38"));
    }

    #[test]
    fn schede_senza_lettura_o_con_errore() {
        let s = schede_vuote("Nessuna lettura ancora: premi Aggiorna");
        assert_eq!(s.len(), 5);
        assert!(s.iter().all(|x| x.righe.is_empty() && x.nota == "Nessuna lettura ancora: premi Aggiorna"));
        let r = Riepilogo { errore: Some(api_claude::ErroreApi::ChiaveNonValida), ..Default::default() };
        let s = schede_in(&r, "14:05", &chrono::Utc);
        assert!(s.iter().all(|x| x.righe.is_empty() && x.nota.starts_with("Lettura delle 14:05 non riuscita")), "{:?}", s[0].nota);
    }

    #[test]
    fn azioni_della_console_chiuse() {
        use crate::sistema::browser::PaginaConsole;
        assert_eq!(PaginaConsole::da_id("console:spesa"), Some(PaginaConsole::Spesa));
        assert_eq!(PaginaConsole::da_id("console:uso"), Some(PaginaConsole::Uso));
        assert_eq!(PaginaConsole::da_id("console:limiti"), Some(PaginaConsole::Limiti));
        assert_eq!(PaginaConsole::da_id("console:chiavi"), Some(PaginaConsole::Chiavi));
        assert_eq!(PaginaConsole::da_id("console:chiavi-admin"), Some(PaginaConsole::ChiaviAdmin));
        assert_eq!(PaginaConsole::da_id("console:membri"), Some(PaginaConsole::Membri));
        assert_eq!(PaginaConsole::da_id("console:organizzazione"), Some(PaginaConsole::Organizzazione));
        for no in ["console:altro", "https://evil.example/", "console:", "", "aggiorna", "CONSOLE:spesa", "console:spesa ", "console:spesa\n"] {
            assert_eq!(PaginaConsole::da_id(no), None, "{no:?}");
        }
        // Gli indirizzi sono fissi (verificati sul sito il 18/11/2026) e passano dal controllo di `apri`.
        let attesi = [
            (PaginaConsole::Spesa, "https://platform.claude.com/cost"),
            (PaginaConsole::Uso, "https://platform.claude.com/usage"),
            (PaginaConsole::Limiti, "https://platform.claude.com/usage/limits"),
            (PaginaConsole::Chiavi, "https://platform.claude.com/settings/keys"),
            (PaginaConsole::ChiaviAdmin, "https://platform.claude.com/settings/admin-keys"),
            (PaginaConsole::Membri, "https://platform.claude.com/settings/members"),
            (PaginaConsole::Organizzazione, "https://platform.claude.com/settings/organization"),
        ];
        for (p, url) in attesi {
            assert_eq!(p.url(), url);
            assert!(crate::sistema::browser::ammesso(p.url()));
        }
    }

    fn locale(g: u32, h: u32, m: u32) -> chrono::DateTime<chrono::Local> {
        use chrono::TimeZone;
        chrono::Local.with_ymd_and_hms(2026, 11, g, h, m, 0).unwrap()
    }

    #[test]
    fn testo_del_limite() {
        let ora = locale(18, 12, 0); // mercoledì
        let l = |g, h, m, p| Limite { percentuale: p, azzera_alle: locale(g, h, m).timestamp() };
        let t = testo_limite(&l(18, 14, 30, 36.6), ora);
        assert_eq!((t.testo.as_str(), t.azzera.as_str()), ("37%", "si azzera alle 14:30"));
        assert!((t.frazione - 0.366).abs() < 1e-6);
        assert_eq!(testo_limite(&l(19, 9, 5, 5.0), ora).azzera, "si azzera domani alle 09:05");
        assert_eq!(testo_limite(&l(21, 14, 30, 61.0), ora).azzera, "si azzera sabato alle 14:30");
        // Da sette giorni in là la data, mai un giorno della settimana che sembri oggi.
        assert_eq!(testo_limite(&l(25, 14, 30, 10.0), ora).azzera, "si azzera il 25/11 alle 14:30");
        assert_eq!(testo_limite(&l(24, 14, 30, 10.0), ora).azzera, "si azzera martedì alle 14:30");
        // Oltre il cento o sotto lo zero: l'arco resta nel suo giro e il testo nei limiti.
        let t = testo_limite(&l(18, 14, 30, 130.0), ora);
        assert_eq!((t.frazione, t.testo.as_str()), (1.0, "100%"));
        assert_eq!(testo_limite(&l(18, 14, 30, -3.0), ora).testo, "0%");
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
