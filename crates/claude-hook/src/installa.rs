//! Il collegamento a Claude Code nel suo `settings.json` (M3, Task 7 e 7B, spec §6.1 e §10).
//!
//! Qui solo funzioni pure sul JSON e la scrittura sicura del file: chi chiama (l'assistente) decide
//! *quando* scrivere, e scrive solo dopo che l'utente ha letto l'anteprima e confermato. Nel file
//! non entra nessun segreto né codice: gli hook lanciano l'aiutante `isola-hook.exe`, che parla con
//! isola per il tubo protetto.
//!
//! Note dalla documentazione di Claude Code (letta il 18/11/2026 su code.claude.com/docs/en/hooks
//! e /statusline; non da memoria):
//! - Hook `command` in **forma exec**: `{"type":"command","command":"<eseguibile>","args":[…]}`.
//!   «Quando `args` c'è, `command` è l'eseguibile lanciato direttamente con `args` come vettore di
//!   argomenti, senza shell.» Su Windows `command` deve essere un eseguibile vero (`.exe`); nessun
//!   problema di virgolette o spazi nel percorso. `args: []` basta a farne una forma exec.
//! - `async: true`: «gira in background senza bloccare»; `timeout` (secondi) «non è imposto» su un
//!   hook `async`. Per i sincroni il default è 600 s; noi scriviamo sempre un `timeout` esplicito.
//! - Mai `onFailure`, `headers`, `allowedEnvVars` (erano degli hook `http` del Task 7, che si
//!   sostituiscono: gli hook `http` non hanno TLS né verifica del server, spec §10).
//! - `statusLine`: oggetto `{"type":"command","command":"…"}` (+ `padding`, `refreshInterval`).
//!   Il comando passa da **Git Bash** se installato (altrimenti PowerShell): i `\` non quotati ci
//!   verrebbero mangiati, quindi percorso con `/`; con spazi serve il nome corto 8.3 (D5).
//!   «Qualunque file»: se ce n'è già una di altri non la tocchiamo.
//! - `disableAllHooks: true` spegne hook *e* barra di stato personalizzata (fuori dai gestiti):
//!   l'anteprima lo dice (`hook_disabilitati`).
//! - Claude Code ricarica le modifiche a `hooks` senza riavvio.

use crate::tubo::NOME_AIUTANTE;
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Come comincia l'URL delle vecchie voci HTTP del Task 7 (si riconoscono per toglierle).
const PREFISSO_URL_VECCHIO: &str = "http://127.0.0.1:";
/// Come comincia, dopo la porta, il loro percorso.
const PREFISSO_PERCORSO_VECCHIO: &str = "/isola/v";
/// Il vecchio comando della barra di stato (`assistente --barra-stato`, Task 6).
const MARCATORE_BARRA_VECCHIA: &str = "--barra-stato";
/// L'argomento con cui l'aiutante fa la barra di stato.
const ARGOMENTO_BARRA: &str = "barra";
/// L'unico argomento delle nostre voci di hook.
const ARGOMENTO_EVENTO: &str = "evento";

/// Gli eventi a cui ci iscriviamo. Né `SessionStart` (un hook che parte a ogni avvio rallenterebbe
/// l'apertura del terminale per nulla) né `PostToolUse` (rumore: uno per strumento).
pub const EVENTI: [&str; 7] = [
    "UserPromptSubmit",
    "PreToolUse",
    "PermissionRequest",
    "Notification",
    "Stop",
    "StopFailure",
    "SessionEnd",
];

/// Gli eventi asincroni: Claude Code non aspetta l'aiutante e non ne legge l'output. `PreToolUse`
/// e `PermissionRequest` restano sincroni (il primo deve arrivare prima del permesso).
pub const ASINCRONI: [&str; 5] = [
    "UserPromptSubmit",
    "Notification",
    "Stop",
    "StopFailure",
    "SessionEnd",
];

/// Secondi di attesa dell'hook: brevi, a parte il permesso che aspetta una decisione dell'utente.
const TIMEOUT_EVENTO: u32 = 5;
const TIMEOUT_PERMESSO: u32 = 120;

/// Che cosa installare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piano {
    /// Il percorso assoluto di `isola-hook.exe`.
    pub aiutante: PathBuf,
    /// Il comando della barra di stato; `None` se non si può scrivere in modo sicuro (nessun
    /// percorso corto per un percorso con spazi) o se non lo si vuole.
    pub comando_barra: Option<String>,
}

impl Piano {
    /// Il percorso dell'aiutante si può scrivere in `settings.json` (testo UTF-8, senza perdite).
    pub fn percorso_valido(&self) -> bool {
        self.aiutante.to_str().is_some_and(|s| !s.is_empty())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroreInstalla {
    /// Il file c'è ma non è un `settings.json` che si possa riscrivere senza perdere nulla.
    NonValido(String),
    Io(String),
}

impl std::fmt::Display for ErroreInstalla {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErroreInstalla::NonValido(m) | ErroreInstalla::Io(m) => f.write_str(m),
        }
    }
}

/// Com'è il file rispetto al piano.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stato {
    /// Nessuna voce di isola (né nuova né vecchia).
    Assente,
    /// Ognuno dei 7 eventi ha esattamente la voce attesa, la barra è a posto, nient'altro di nostro.
    Aggiornato,
    /// Voci di isola ci sono, ma mancano, sono di un altro percorso o forma, o sono vecchie (HTTP).
    DaAggiornare,
}

/// Che cosa si scriverebbe, per l'anteprima.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anteprima {
    /// Il JSON delle voci di isola come resteranno nel file (indentato).
    pub testo: String,
    /// Le voci di isola già nel file che la scrittura toglie o sostituisce (una frase ciascuna).
    pub sostituite: Vec<String>,
    /// La barra di stato è già di altri: la nostra non si scrive.
    pub barra_occupata: bool,
    /// `disableAllHooks` è vero: finché resta così gli hook e la barra non partono.
    pub hook_disabilitati: bool,
}

/// Dove sta il `settings.json` di Claude Code: `CLAUDE_CONFIG_DIR` se c'è (e non è vuota), altrimenti
/// `<home>/.claude`.
pub fn file_impostazioni(config_dir: Option<&OsStr>, home: &Path) -> PathBuf {
    match config_dir.filter(|d| !d.is_empty()) {
        Some(d) => Path::new(d).join("settings.json"),
        None => home.join(".claude").join("settings.json"),
    }
}

/// Legge il file. Assente o vuoto → `{}`; il BOM si tollera; tutto ciò che non è un oggetto JSON
/// stretto (commenti, virgole finali, `hooks` che non è un oggetto o con eventi che non sono
/// elenchi) è `NonValido`: così non si riscrive mai un file che non si è capito del tutto.
pub fn leggi(file: &Path) -> Result<Value, ErroreInstalla> {
    let byte = match std::fs::read(file) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(json!({})),
        Err(e) => {
            return Err(ErroreInstalla::Io(format!(
                "Non riesco a leggere {}: {e}",
                file.display()
            )))
        }
    };
    let testo = String::from_utf8(byte).map_err(|_| {
        ErroreInstalla::NonValido(format!("{} non è un testo UTF-8.", file.display()))
    })?;
    let testo = testo.strip_prefix('\u{feff}').unwrap_or(&testo);
    if testo.trim().is_empty() {
        return Ok(json!({}));
    }
    let v: Value = serde_json::from_str(testo).map_err(|e| {
        ErroreInstalla::NonValido(format!(
            "{} non è un JSON valido (niente commenti né virgole finali): {e}",
            file.display()
        ))
    })?;
    if !v.is_object() {
        return Err(ErroreInstalla::NonValido(format!(
            "{} non contiene un oggetto JSON.",
            file.display()
        )));
    }
    match v.get("hooks") {
        None => {}
        Some(Value::Object(eventi)) => {
            if let Some((nome, _)) = eventi.iter().find(|(_, g)| !g.is_array()) {
                return Err(ErroreInstalla::NonValido(format!(
                    "In «hooks», «{nome}» non è un elenco."
                )));
            }
        }
        Some(_) => {
            return Err(ErroreInstalla::NonValido(
                "«hooks» non è un oggetto.".into(),
            ))
        }
    }
    Ok(v)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Genere {
    /// `command` con `isola-hook.exe` e `args[0] == "evento"`.
    Aiutante,
    /// Una vecchia voce `http` del Task 7 verso `127.0.0.1` sul percorso `/isola/v…`.
    VecchiaHttp,
}

/// Il nome dell'ultimo componente di un percorso scritto con `\` o con `/`.
fn nome_file(percorso: &str) -> &str {
    percorso.rsplit(['\\', '/']).next().unwrap_or(percorso)
}

/// Una voce di hook è nostra: l'aiutante in forma exec, o una vecchia voce HTTP.
fn genere(e: &Value) -> Option<Genere> {
    match e.get("type")?.as_str()? {
        "command" => {
            let comando = e.get("command")?.as_str()?;
            let primo = e.get("args")?.as_array()?.first()?.as_str()?;
            (nome_file(comando).eq_ignore_ascii_case(NOME_AIUTANTE) && primo == ARGOMENTO_EVENTO)
                .then_some(Genere::Aiutante)
        }
        "http" => {
            let resto = e.get("url")?.as_str()?.strip_prefix(PREFISSO_URL_VECCHIO)?;
            let (porta, percorso) = resto.split_once('/')?;
            porta.parse::<u16>().ok()?;
            format!("/{percorso}")
                .starts_with(PREFISSO_PERCORSO_VECCHIO)
                .then_some(Genere::VecchiaHttp)
        }
        _ => None,
    }
}

/// La vecchia barra del Task 6: esattamente `<percorso>/assistente.exe --barra-stato <cifre>`.
fn barra_vecchia(c: &str) -> bool {
    let mut parti = c.split_whitespace();
    let (Some(p), Some(m), Some(porta), None) =
        (parti.next(), parti.next(), parti.next(), parti.next())
    else {
        return false;
    };
    nome_file(p).eq_ignore_ascii_case("assistente.exe")
        && m == MARCATORE_BARRA_VECCHIA
        && !porta.is_empty()
        && porta.bytes().all(|b| b.is_ascii_digit())
}

/// La barra di stato è nostra: `…/isola-hook.exe barra` o la vecchia barra (`barra_vecchia`).
fn barra_nostra(s: &Value) -> bool {
    s.get("command").and_then(Value::as_str).is_some_and(|c| {
        barra_vecchia(c) || {
            let mut parti = c.split_whitespace();
            parti
                .next()
                .is_some_and(|p| nome_file(p).eq_ignore_ascii_case(NOME_AIUTANTE))
                && parti.next() == Some(ARGOMENTO_BARRA)
        }
    })
}

/// Le voci di hook (dentro i gruppi di ogni evento) di un file, con il nome dell'evento.
fn tutte_le_voci(v: &Value) -> impl Iterator<Item = (&str, &Value)> {
    v.get("hooks")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|h| h.iter())
        .filter_map(|(ev, g)| g.as_array().map(|g| (ev.as_str(), g)))
        .flat_map(|(ev, g)| g.iter().map(move |g| (ev, g)))
        .filter_map(|(ev, g)| g.get("hooks").and_then(Value::as_array).map(|l| (ev, l)))
        .flat_map(|(ev, l)| l.iter().map(move |e| (ev, e)))
}

/// Nel file c'è qualcosa di nostro (voci o barra).
pub fn ha_nostre(v: &Value) -> bool {
    tutte_le_voci(v).any(|(_, e)| genere(e).is_some())
        || v.get("statusLine").is_some_and(barra_nostra)
}

fn voce(p: &Piano, evento: &str) -> Value {
    let mut m = Map::new();
    m.insert("type".into(), json!("command"));
    // Un percorso non UTF-8 non arriva qui: chi chiama lo rifiuta (`Piano::percorso_valido`).
    m.insert(
        "command".into(),
        json!(p.aiutante.to_str().unwrap_or_default()),
    );
    m.insert("args".into(), json!([ARGOMENTO_EVENTO]));
    if ASINCRONI.contains(&evento) {
        m.insert("async".into(), json!(true));
    }
    m.insert(
        "timeout".into(),
        json!(if evento == "PermissionRequest" {
            TIMEOUT_PERMESSO
        } else {
            TIMEOUT_EVENTO
        }),
    );
    Value::Object(m)
}

/// Il gruppo di un evento: una sola voce, nessun `matcher`.
fn gruppo(p: &Piano, evento: &str) -> Value {
    json!({ "hooks": [voce(p, evento)] })
}

fn barra(comando: &str) -> Value {
    json!({ "type": "command", "command": comando })
}

/// Com'è il file rispetto al piano (vedi `Stato`). Non dipende dall'ordine delle voci.
pub fn stato(v: &Value, p: &Piano) -> Stato {
    if !ha_nostre(v) {
        Stato::Assente
    } else if unisci(v.clone(), p) == *v {
        Stato::Aggiornato
    } else {
        Stato::DaAggiornare
    }
}

/// Toglie le nostre voci per cui `tieni(evento, voce)` è falso, e solo ciò che è rimasto vuoto per
/// causa nostra (gruppi, eventi, `hooks`). Il resto, vuoto o no, non si tocca e non cambia ordine.
fn rimuovi(mut v: Value, tieni: &mut dyn FnMut(&str, &Value) -> bool) -> Value {
    let Some(radice) = v.as_object_mut() else {
        return v;
    };
    let mut tolto_qualcosa = false;
    if let Some(hooks) = radice.get_mut("hooks").and_then(Value::as_object_mut) {
        let mut svuotati = Vec::new();
        for (evento, gruppi) in hooks.iter_mut() {
            let Some(gruppi) = gruppi.as_array_mut() else {
                continue;
            };
            let mut tolto_qui = false;
            gruppi.retain_mut(|g| {
                let Some(lista) = g.get_mut("hooks").and_then(Value::as_array_mut) else {
                    return true;
                };
                let prima = lista.len();
                lista.retain(|e| genere(e).is_none() || tieni(evento, e));
                let tolte = lista.len() != prima;
                tolto_qui |= tolte;
                !(tolte && lista.is_empty())
            });
            tolto_qualcosa |= tolto_qui;
            if tolto_qui && gruppi.is_empty() {
                svuotati.push(evento.clone());
            }
        }
        for evento in &svuotati {
            hooks.shift_remove(evento);
        }
        if tolto_qualcosa && hooks.is_empty() {
            radice.shift_remove("hooks");
        }
    }
    v
}

/// Toglie le nostre voci (nuove e vecchie HTTP) e la nostra barra, e solo ciò che è rimasto vuoto
/// per causa nostra. Il resto, vuoto o no, non si tocca.
pub fn togli(v: Value) -> Value {
    let mut v = rimuovi(v, &mut |_, _| false);
    if let Some(radice) = v.as_object_mut() {
        if radice.get("statusLine").is_some_and(barra_nostra) {
            radice.shift_remove("statusLine");
        }
    }
    v
}

/// Porta il file al piano: per ogni evento una sola voce, quella attesa (se c'è già resta dov'è,
/// altrimenti si aggiunge in coda), le vecchie o diverse si tolgono; la barra se manca o era nostra.
/// Tutto il resto resta com'è, nell'ordine.
pub fn unisci(v: Value, p: &Piano) -> Value {
    let mut visti: HashSet<String> = HashSet::new();
    let mut v = rimuovi(v, &mut |ev, e| {
        EVENTI.contains(&ev) && *e == voce(p, ev) && visti.insert(ev.to_owned())
    });
    let Some(radice) = v.as_object_mut() else {
        return v;
    };
    let hooks = radice
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()));
    if let Some(hooks) = hooks.as_object_mut() {
        for evento in EVENTI {
            if visti.contains(evento) {
                continue;
            }
            let elenco = hooks
                .entry(evento)
                .or_insert_with(|| Value::Array(Vec::new()));
            if let Some(elenco) = elenco.as_array_mut() {
                elenco.push(gruppo(p, evento));
            }
        }
    }
    let nostra = radice.get("statusLine").is_some_and(barra_nostra);
    match (nostra, p.comando_barra.as_deref()) {
        (true, Some(c)) => {
            radice.insert("statusLine".into(), barra(c));
        }
        (true, None) => {
            radice.shift_remove("statusLine");
        }
        (false, Some(c)) if !radice.contains_key("statusLine") => {
            radice.insert("statusLine".into(), barra(c));
        }
        _ => {}
    }
    v
}

/// Che cosa succederebbe nel file `v` con il piano `p`: com'è il risultato (le sole voci di isola),
/// che cosa di nostro già presente si toglie o sostituisce, e gli avvisi che l'utente deve leggere
/// prima di confermare. Due anteprime uguali = la stessa modifica (anche se il resto del file è
/// cambiato nel frattempo).
pub fn anteprima(v: &Value, p: &Piano) -> Anteprima {
    let barra_occupata = v.get("statusLine").is_some_and(|s| !barra_nostra(s));
    let dopo = unisci(v.clone(), p);
    // Le voci di isola come resteranno.
    let mut hooks = Map::new();
    for evento in EVENTI {
        let nostre: Vec<Value> = tutte_le_voci(&dopo)
            .filter(|(ev, e)| *ev == evento && genere(e).is_some())
            .map(|(_, e)| json!({ "hooks": [e] }))
            .collect();
        hooks.insert(evento.into(), Value::Array(nostre));
    }
    let mut mostrate = Map::new();
    mostrate.insert("hooks".into(), Value::Object(hooks));
    if let Some(s) = dopo.get("statusLine").filter(|s| barra_nostra(s)) {
        mostrate.insert("statusLine".into(), s.clone());
    }
    // Quelle di prima che non restano com'erano.
    let mut sostituite = Vec::new();
    let mut visti: HashSet<&str> = HashSet::new();
    for (evento, e) in tutte_le_voci(v) {
        let Some(g) = genere(e) else { continue };
        if EVENTI.contains(&evento) && *e == voce(p, evento) && visti.insert(evento) {
            continue;
        }
        sostituite.push(match g {
            Genere::VecchiaHttp => {
                format!("{evento}: voce HTTP di una versione precedente di isola (verrà tolta)")
            }
            Genere::Aiutante => format!(
                "{evento}: voce di isola con un altro percorso o duplicata (verrà sostituita)"
            ),
        });
    }
    if let Some(s) = v.get("statusLine").filter(|s| barra_nostra(s)) {
        match p.comando_barra.as_deref() {
            None => sostituite.push("statusLine: barra di stato di isola (verrà tolta)".into()),
            Some(c) if *s != barra(c) => sostituite
                .push("statusLine: barra di stato di isola precedente (verrà sostituita)".into()),
            Some(_) => {}
        }
    }
    Anteprima {
        testo: serde_json::to_string_pretty(&Value::Object(mostrate)).unwrap_or_default(),
        sostituite,
        barra_occupata,
        hook_disabilitati: v.get("disableAllHooks").and_then(Value::as_bool) == Some(true),
    }
}

/// Com'è andata `scrivi_se_invariato`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scrittura {
    Scritto,
    /// Il file non è più quello che si è letto: **nulla è stato scritto**.
    Cambiato,
}

/// Un nome di file temporaneo accanto a `destinazione`, unico per processo e istante.
fn nome_temporaneo(destinazione: &Path, tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let mut n = destinazione
        .file_name()
        .unwrap_or(OsStr::new("settings.json"))
        .to_os_string();
    n.push(format!(
        ".isola-{tag}-{}-{}-{t}.tmp",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    destinazione.with_file_name(n)
}

/// Scrive `nuovo` al posto del file **solo se il file è ancora uguale ad `atteso`** (il valore già
/// letto da chi chiama, da cui si decide anche la riserva): prima prepara il file temporaneo
/// (nome unico, `sync_all`), poi rilegge il file una volta e, se è cambiato, non scrive
/// (`Cambiato`). La riserva `<file>.isola-bak` si fa **solo** se `atteso` non ha niente di nostro
/// (è il file di prima di isola; le scritture successive non la toccano): si copia in un file
/// temporaneo e poi si rinomina, così una copia fallita non tronca una riserva esistente. Una
/// riserva già presente e non sovrascrivibile (sola lettura) non ferma la scrittura; una che non
/// si può fare e non c'è, sì. Infine `rename`.
///
/// Resta una finestra minima fra l'ultima rilettura e il `rename` (microsecondi): un'altra
/// applicazione che scrive proprio lì perderebbe la sua modifica. Non si può chiudere senza
/// blocchi sul file, che Claude Code non rispetta. Indentazione di 2 spazi, ritorno a capo
/// finale. Una scorciatoia (symlink) si segue: si riscrive il file vero.
pub fn scrivi_se_invariato(
    file: &Path,
    atteso: &Value,
    nuovo: &Value,
) -> Result<Scrittura, ErroreInstalla> {
    use std::io::Write;
    let io = |cosa: &str, e: std::io::Error| ErroreInstalla::Io(format!("{cosa}: {e}"));
    let destinazione = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    let mut testo =
        serde_json::to_string_pretty(nuovo).map_err(|e| ErroreInstalla::Io(e.to_string()))?;
    testo.push('\n');
    if let Some(cartella) = destinazione.parent() {
        std::fs::create_dir_all(cartella).map_err(|e| io("Non riesco a creare la cartella", e))?;
    }
    let tmp = nome_temporaneo(&destinazione, "nuovo");
    let preparato = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .and_then(|mut f| f.write_all(testo.as_bytes()).and_then(|_| f.sync_all()));
    if let Err(e) = preparato {
        let _ = std::fs::remove_file(&tmp);
        return Err(io("Non riesco a scrivere il file", e));
    }
    let esito = (|| -> Result<Scrittura, ErroreInstalla> {
        // Ancora quello che si è letto? Altrimenti non si scrive.
        let ora = leggi(&destinazione)?;
        if ora != *atteso {
            return Ok(Scrittura::Cambiato);
        }
        if destinazione.exists() && !ha_nostre(atteso) {
            let mut riserva_nome = destinazione
                .file_name()
                .unwrap_or(OsStr::new("settings.json"))
                .to_os_string();
            riserva_nome.push(".isola-bak");
            let riserva = destinazione.with_file_name(riserva_nome);
            let copia = nome_temporaneo(&destinazione, "bak");
            let fatta = std::fs::copy(&destinazione, &copia)
                .and_then(|_| std::fs::rename(&copia, &riserva));
            if let Err(e) = fatta {
                let _ = std::fs::remove_file(&copia);
                if !riserva.exists() {
                    return Err(io("Non riesco a fare la copia di riserva", e));
                }
            }
        }
        std::fs::rename(&tmp, &destinazione).map_err(|e| io("Non riesco a scrivere il file", e))?;
        Ok(Scrittura::Scritto)
    })();
    if esito != Ok(Scrittura::Scritto) {
        let _ = std::fs::remove_file(&tmp);
    }
    esito
}

/// Come `scrivi_se_invariato`, con il file com'è ora come valore atteso (per chi non ha una
/// lettura precedente da confrontare, come «Scollega»). Un file che cambia nel frattempo è un errore.
pub fn scrivi_sicuro(file: &Path, v: &Value) -> Result<(), ErroreInstalla> {
    let atteso = leggi(file)?;
    match scrivi_se_invariato(file, &atteso, v)? {
        Scrittura::Scritto => Ok(()),
        Scrittura::Cambiato => Err(ErroreInstalla::Io(
            "Il file è cambiato mentre lo scrivevo: non ho scritto nulla.".into(),
        )),
    }
}

/// I caratteri ammessi in un percorso scritto in un comando senza virgolette: la shell che lo
/// esegue (Git Bash o altra) non deve poterlo rompere.
fn percorso_sicuro(p: &str) -> bool {
    !p.is_empty()
        && p.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | ':' | '~' | '-' | '+')
        })
}

/// Il comando della barra di stato: il percorso dell'aiutante con `/` al posto di `\` (Claude Code
/// lo passa a una shell), e se non è «pulito» (spazi, apici…) il suo percorso corto 8.3 (`corto`);
/// se nemmeno quello lo è, `None`: meglio nessuna barra che un comando rotto.
pub fn comando_barra(aiutante: &Path, corto: impl Fn(&Path) -> Option<PathBuf>) -> Option<String> {
    let a_barre = |p: &Path| p.to_str().map(|s| s.replace('\\', "/"));
    let direttamente = a_barre(aiutante).filter(|s| percorso_sicuro(s));
    let percorso = match direttamente {
        Some(s) => s,
        None => corto(aiutante)
            .and_then(|c| a_barre(&c))
            .filter(|s| percorso_sicuro(s))?,
    };
    Some(format!("{percorso} {ARGOMENTO_BARRA}"))
}

#[cfg(test)]
mod test {
    use super::*;
    use tempfile::TempDir;

    fn piano() -> Piano {
        Piano {
            aiutante: PathBuf::from(r"C:\Users\u\isola\isola-hook.exe"),
            comando_barra: Some("C:/Users/u/isola/isola-hook.exe barra".into()),
        }
    }

    /// Un settings.json «vero»: permessi, ambiente, un hook di altri con `matcher`, un modello.
    fn fixture() -> Value {
        serde_json::from_str(
            r#"{
  "model": "opus",
  "permissions": { "allow": ["Bash(ls)"], "deny": ["Read(./.env)"] },
  "env": { "A": "1", "Z": "2" },
  "hooks": {
    "PreToolUse": [
      { "matcher": "Bash", "hooks": [ { "type": "command", "command": "echo ciao" } ] }
    ],
    "Stop": [
      { "hooks": [ { "type": "command", "command": "echo fine" } ] }
    ]
  },
  "effortLevel": "high"
}"#,
        )
        .unwrap()
    }

    /// Le voci del Task 7 (HTTP) come le scriveva allora, più la vecchia barra.
    fn con_vecchie_http(base: Value) -> Value {
        let mut v = base;
        let hooks = v
            .as_object_mut()
            .unwrap()
            .entry("hooks")
            .or_insert_with(|| json!({}));
        for e in EVENTI {
            let el = hooks
                .as_object_mut()
                .unwrap()
                .entry(e)
                .or_insert_with(|| json!([]));
            el.as_array_mut().unwrap().push(json!({ "hooks": [ {
                "type": "http",
                "url": "http://127.0.0.1:47613/isola/v1/evento",
                "headers": { "Authorization": "Bearer $ISOLA_HOOK_TOKEN" },
                "allowedEnvVars": ["ISOLA_HOOK_TOKEN"],
                "timeout": 5
            } ] }));
        }
        v["statusLine"] =
            json!({ "type": "command", "command": "C:/x/assistente.exe --barra-stato 47613" });
        v
    }

    fn chiavi(v: &Value) -> Vec<String> {
        v.as_object().unwrap().keys().cloned().collect()
    }

    fn testo(v: &Value) -> String {
        serde_json::to_string(v).unwrap()
    }

    #[test]
    fn voci_command_in_forma_exec() {
        let v = unisci(json!({}), &piano());
        for e in EVENTI {
            let voce = &v["hooks"][e][0]["hooks"][0];
            assert_eq!(voce["type"], "command", "{e}");
            assert_eq!(voce["command"], r"C:\Users\u\isola\isola-hook.exe", "{e}");
            assert_eq!(voce["args"], json!(["evento"]), "{e}");
            for vietato in ["headers", "allowedEnvVars", "url", "onFailure"] {
                assert!(voce.get(vietato).is_none(), "{e}: {vietato}");
            }
        }
        let t = testo(&v);
        assert!(
            !t.contains("http") && !t.contains("ISOLA_HOOK_TOKEN") && !t.contains("Bearer"),
            "{t}"
        );
    }

    #[test]
    fn async_solo_dove_serve() {
        let v = unisci(json!({}), &piano());
        let asincroni: Vec<&str> = EVENTI
            .into_iter()
            .filter(|e| v["hooks"][e][0]["hooks"][0]["async"] == json!(true))
            .collect();
        assert_eq!(
            asincroni,
            [
                "UserPromptSubmit",
                "Notification",
                "Stop",
                "StopFailure",
                "SessionEnd"
            ]
        );
        for e in ["PreToolUse", "PermissionRequest"] {
            assert!(
                v["hooks"][e][0]["hooks"][0].get("async").is_none(),
                "{e} è sincrono"
            );
        }
        assert_eq!(v["hooks"]["PreToolUse"][0]["hooks"][0]["timeout"], 5);
        assert_eq!(
            v["hooks"]["PermissionRequest"][0]["hooks"][0]["timeout"],
            120
        );
        for e in EVENTI {
            assert!(
                v["hooks"][e][0].get("matcher").is_none(),
                "{e}: nessun matcher"
            );
        }
        let hooks = v["hooks"].as_object().unwrap();
        assert_eq!(hooks.len(), 7);
        assert!(hooks.get("SessionStart").is_none() && hooks.get("PostToolUse").is_none());
    }

    #[test]
    fn vecchie_voci_http_tolte() {
        let originale = fixture();
        let vecchio = con_vecchie_http(originale.clone());
        assert!(ha_nostre(&vecchio));
        assert_eq!(stato(&vecchio, &piano()), Stato::DaAggiornare);
        // `unisci` le toglie e mette le nuove, senza toccare il resto.
        let nuovo = unisci(vecchio.clone(), &piano());
        let t = testo(&nuovo);
        assert!(
            !t.contains("127.0.0.1") && !t.contains("--barra-stato") && !t.contains("Bearer"),
            "{t}"
        );
        assert_eq!(stato(&nuovo, &piano()), Stato::Aggiornato);
        assert_eq!(
            nuovo["hooks"]["PreToolUse"][0],
            originale["hooks"]["PreToolUse"][0]
        );
        assert_eq!(
            nuovo["statusLine"]["command"],
            "C:/Users/u/isola/isola-hook.exe barra"
        );
        // `togli` ridà il file originale altrui.
        assert_eq!(testo(&togli(vecchio)), testo(&originale));
        // Una voce http di altri non è nostra.
        let altrui = json!({ "hooks": { "Stop": [ { "hooks": [ { "type": "http", "url": "http://localhost:9/isola/v1/evento" } ] } ] } });
        assert!(!ha_nostre(&altrui));
        assert_eq!(stato(&altrui, &piano()), Stato::Assente);
    }

    #[test]
    fn anteprima_mostra_anche_cio_che_si_sostituisce() {
        let a = anteprima(&con_vecchie_http(fixture()), &piano());
        assert_eq!(
            a.sostituite.iter().filter(|s| s.contains("HTTP")).count(),
            7,
            "{:?}",
            a.sostituite
        );
        assert!(
            a.sostituite.iter().any(|s| s.contains("statusLine")),
            "{:?}",
            a.sostituite
        );
        // Una voce nuova di un altro percorso.
        let altro = Piano {
            aiutante: PathBuf::from(r"D:\vecchio\isola-hook.exe"),
            comando_barra: Some("D:/vecchio/isola-hook.exe barra".into()),
        };
        let a = anteprima(&unisci(fixture(), &altro), &piano());
        assert_eq!(a.sostituite.len(), 8, "{:?}", a.sostituite);
        // File già aggiornato: nulla da sostituire.
        assert!(anteprima(&unisci(fixture(), &piano()), &piano())
            .sostituite
            .is_empty());
        // File pulito: nulla da sostituire.
        assert!(anteprima(&fixture(), &piano()).sostituite.is_empty());
        // Barra nostra che il piano non scrive più: «verrà tolta».
        let senza = Piano {
            comando_barra: None,
            ..piano()
        };
        let a = anteprima(&unisci(fixture(), &piano()), &senza);
        assert!(
            a.sostituite.iter().any(|s| s.contains("verrà tolta")),
            "{:?}",
            a.sostituite
        );
    }

    #[test]
    fn anteprima_mostra_le_voci_come_resteranno() {
        let a = anteprima(&fixture(), &piano());
        assert!(
            !a.testo.contains("permissions") && !a.testo.contains("echo ciao"),
            "{}",
            a.testo
        );
        for e in EVENTI {
            assert!(a.testo.contains(e), "{e}");
        }
        assert!(a.testo.contains("isola-hook.exe") && a.testo.contains("\"statusLine\""));
        let j: Value = serde_json::from_str(&a.testo).expect("è JSON");
        assert_eq!(j["hooks"]["Stop"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn unisci_conserva_tutto() {
        let prima = fixture();
        let dopo = unisci(prima.clone(), &piano());
        let vecchie = chiavi(&prima);
        let nuove: Vec<String> = chiavi(&dopo)
            .into_iter()
            .filter(|k| vecchie.contains(k))
            .collect();
        assert_eq!(nuove, vecchie);
        for k in ["model", "permissions", "env", "effortLevel"] {
            assert_eq!(dopo[k], prima[k], "{k}");
        }
        let pre = dopo["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre[0], prima["hooks"]["PreToolUse"][0]);
        assert_eq!(pre.len(), 2);
        let stop = dopo["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop[0], prima["hooks"]["Stop"][0]);
        assert_eq!(stop.len(), 2);
        let eventi: Vec<String> = chiavi(&dopo["hooks"]);
        assert_eq!(&eventi[..2], ["PreToolUse", "Stop"]);
    }

    #[test]
    fn unisci_due_volte_come_una_anche_con_voci_altrui_dopo_le_nostre() {
        let una = unisci(fixture(), &piano());
        assert_eq!(testo(&unisci(una.clone(), &piano())), testo(&una));
        assert_eq!(
            unisci(unisci(json!({}), &piano()), &piano()),
            unisci(json!({}), &piano())
        );
        // Un'altra applicazione aggiunge un suo gruppo dopo il nostro: non c'è nulla da aggiornare
        // e `unisci` non sposta le nostre voci.
        let mut dopo_altri = una.clone();
        dopo_altri["hooks"]["Stop"]
            .as_array_mut()
            .unwrap()
            .push(json!({ "hooks": [ { "type": "command", "command": "altro" } ] }));
        assert_eq!(stato(&dopo_altri, &piano()), Stato::Aggiornato);
        assert_eq!(
            testo(&unisci(dopo_altri.clone(), &piano())),
            testo(&dopo_altri)
        );
    }

    #[test]
    fn stato_riconosce_voci_mancanti_o_diverse() {
        assert_eq!(stato(&fixture(), &piano()), Stato::Assente);
        let completo = unisci(fixture(), &piano());
        assert_eq!(stato(&completo, &piano()), Stato::Aggiornato);
        // Un evento mancante.
        let mut manca = completo.clone();
        manca["hooks"]
            .as_object_mut()
            .unwrap()
            .shift_remove("SessionEnd");
        assert_eq!(stato(&manca, &piano()), Stato::DaAggiornare);
        // Forma diversa (async tolto, timeout cambiato).
        let mut forma = completo.clone();
        forma["hooks"]["Stop"][1]["hooks"][0]
            .as_object_mut()
            .unwrap()
            .shift_remove("async");
        assert_eq!(stato(&forma, &piano()), Stato::DaAggiornare);
        let mut tempo = completo.clone();
        tempo["hooks"]["PermissionRequest"][0]["hooks"][0]["timeout"] = json!(5);
        assert_eq!(stato(&tempo, &piano()), Stato::DaAggiornare);
        // Un altro percorso.
        let altro = Piano {
            aiutante: PathBuf::from(r"D:\x\isola-hook.exe"),
            ..piano()
        };
        assert_eq!(stato(&completo, &altro), Stato::DaAggiornare);
        // Una voce doppia.
        let mut doppia = completo.clone();
        let g = doppia["hooks"]["Stop"][1].clone();
        doppia["hooks"]["Stop"].as_array_mut().unwrap().push(g);
        assert_eq!(stato(&doppia, &piano()), Stato::DaAggiornare);
        // Barra cambiata o sparita.
        let mut barra_vecchia = completo.clone();
        barra_vecchia["statusLine"]["command"] = json!("D:/x/isola-hook.exe barra");
        assert_eq!(stato(&barra_vecchia, &piano()), Stato::DaAggiornare);
        let mut senza_barra = completo.clone();
        senza_barra
            .as_object_mut()
            .unwrap()
            .shift_remove("statusLine");
        assert_eq!(stato(&senza_barra, &piano()), Stato::DaAggiornare);
        // Con la barra di altri, nessuna barra attesa: a posto.
        let mut altrui = completo;
        altrui["statusLine"] = json!({ "type": "command", "command": "ccstatusline" });
        assert_eq!(stato(&altrui, &piano()), Stato::Aggiornato);
    }

    #[test]
    fn togli_ridà_l_originale() {
        let originale = fixture();
        let giro = togli(unisci(originale.clone(), &piano()));
        assert_eq!(testo(&giro), testo(&originale));
        assert_eq!(togli(unisci(json!({}), &piano())), json!({}));
        // Un nostro gruppo con dentro anche un hook di altri: via solo il nostro.
        let mut misto = unisci(json!({}), &piano());
        misto["hooks"]["Stop"][0]["hooks"]
            .as_array_mut()
            .unwrap()
            .push(json!({"type":"command","command":"x"}));
        let tolto = togli(misto);
        assert_eq!(
            tolto["hooks"]["Stop"][0]["hooks"],
            json!([{"type":"command","command":"x"}])
        );
        assert!(tolto["hooks"].get("PreToolUse").is_none());
    }

    #[test]
    fn togli_non_tocca_cio_che_era_vuoto_prima() {
        // `hooks: {}` di altri, e un evento con elenco vuoto, e un gruppo vuoto: restano, nell'ordine.
        let altrui: Value = serde_json::from_str(r#"{ "z": 1, "hooks": {}, "a": 2 }"#).unwrap();
        assert_eq!(testo(&togli(altrui.clone())), testo(&altrui));
        let originale: Value = serde_json::from_str(
            r#"{ "model": "opus",
                 "hooks": { "SessionStart": [], "Stop": [ { "hooks": [] } ], "PreToolUse": [ { "matcher": "Bash", "hooks": [ { "type": "command", "command": "echo" } ] } ] },
                 "effortLevel": "high" }"#,
        )
        .unwrap();
        let giro = togli(unisci(originale.clone(), &piano()));
        assert_eq!(testo(&giro), testo(&originale), "{}", testo(&giro));
        assert_eq!(giro["hooks"]["SessionStart"], json!([]));
        assert_eq!(giro["hooks"]["Stop"], json!([ { "hooks": [] } ]));
        assert_eq!(
            chiavi(&giro["hooks"]),
            ["SessionStart", "Stop", "PreToolUse"]
        );
        // Limite noto: un elenco vuoto di un evento a cui ci iscriviamo (qui `Notification: []`) non si
        // distingue da «evento assente» dopo il giro, e sparisce. Per Claude Code è la stessa cosa.
        let vuoto_nostro = json!({ "hooks": { "Notification": [] } });
        assert_eq!(togli(unisci(vuoto_nostro, &piano())), json!({}));
        // Stesso limite per un `hooks: {}` iniziale: dopo unisci è pieno di cose nostre e togli lo
        // svuota per causa nostra, quindi sparisce. `togli` da solo su `hooks: {}` non lo tocca (sopra).
        let vuoto = json!({ "hooks": {}, "model": "m" });
        assert_eq!(
            testo(&togli(unisci(vuoto, &piano()))),
            testo(&json!({ "model": "m" }))
        );
        // E anche togliendo le vecchie voci HTTP, senza mai riordinare.
        let con_http = con_vecchie_http(originale.clone());
        assert_eq!(testo(&togli(con_http)), testo(&originale));
    }

    #[test]
    fn json_non_valido_non_si_scrive() {
        let d = TempDir::new().unwrap();
        let f = d.path().join("settings.json");
        for (contenuto, parola) in [
            ("{ // commento\n \"model\": \"opus\" }", "commento"),
            ("{ \"model\": \"opus\", }", "virgola"),
            ("[1, 2]", "oggetto"),
            ("{ \"hooks\": [] }", "hooks"),
            ("{ \"hooks\": { \"Stop\": {} } }", "Stop"),
        ] {
            std::fs::write(&f, contenuto).unwrap();
            let prima = std::fs::read(&f).unwrap();
            match leggi(&f) {
                Err(ErroreInstalla::NonValido(m)) => assert!(!m.is_empty(), "{parola}"),
                altro => panic!("{parola}: {altro:?}"),
            }
            assert_eq!(
                std::fs::read(&f).unwrap(),
                prima,
                "i byte non cambiano ({parola})"
            );
            assert!(!d.path().join("settings.json.isola-bak").exists());
        }
    }

    #[test]
    fn file_assente_vuoto_e_bom() {
        let d = TempDir::new().unwrap();
        let f = d.path().join("settings.json");
        assert_eq!(leggi(&f).unwrap(), json!({}), "assente → {{}}");
        std::fs::write(&f, "  \n").unwrap();
        assert_eq!(leggi(&f).unwrap(), json!({}), "vuoto → {{}}");
        std::fs::write(&f, "\u{feff}{ \"model\": \"opus\" }").unwrap();
        assert_eq!(leggi(&f).unwrap(), json!({ "model": "opus" }));
    }

    #[test]
    fn riserva_e_scrittura_atomica() {
        let d = TempDir::new().unwrap();
        let f = d.path().join("settings.json");
        let vecchio = "{\n\t\"model\": \"opus\"\n}\n";
        std::fs::write(&f, vecchio).unwrap();
        scrivi_sicuro(&f, &unisci(leggi(&f).unwrap(), &piano())).unwrap();
        assert_eq!(
            std::fs::read_to_string(d.path().join("settings.json.isola-bak")).unwrap(),
            vecchio
        );
        let nuovo = std::fs::read_to_string(&f).unwrap();
        assert!(nuovo.ends_with("}\n"), "ritorno a capo finale");
        assert!(
            nuovo.contains("\n  \"model\": \"opus\""),
            "indentazione di 2 spazi: {nuovo}"
        );
        let rimasti: Vec<String> = std::fs::read_dir(d.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(rimasti.is_empty(), "nessun .tmp rimasto: {rimasti:?}");
        assert_eq!(
            leggi(&f).unwrap(),
            unisci(serde_json::from_str(vecchio).unwrap(), &piano())
        );
    }

    #[test]
    fn la_riserva_e_il_file_di_prima_e_non_si_sovrascrive() {
        let d = TempDir::new().unwrap();
        let f = d.path().join("settings.json");
        let bak = d.path().join("settings.json.isola-bak");
        let originale = "{\"model\":\"opus\"}\n";
        std::fs::write(&f, originale).unwrap();
        scrivi_sicuro(&f, &unisci(leggi(&f).unwrap(), &piano())).unwrap();
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), originale);
        // Una seconda scrittura (il file ha già le nostre voci): la riserva resta quella di prima.
        let altro = Piano {
            aiutante: PathBuf::from(r"D:\x\isola-hook.exe"),
            ..piano()
        };
        scrivi_sicuro(&f, &unisci(leggi(&f).unwrap(), &altro)).unwrap();
        assert_eq!(
            std::fs::read_to_string(&bak).unwrap(),
            originale,
            "la riserva non cambia"
        );
        // Anche «Scollega» (togli) non la tocca.
        scrivi_sicuro(&f, &togli(leggi(&f).unwrap())).unwrap();
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), originale);
        // Un file che non ha più niente di nostro e poi cambia: una nuova riserva lo rispecchia.
        std::fs::write(&f, "{\"model\":\"sonnet\"}\n").unwrap();
        scrivi_sicuro(&f, &unisci(leggi(&f).unwrap(), &piano())).unwrap();
        assert_eq!(
            std::fs::read_to_string(&bak).unwrap(),
            "{\"model\":\"sonnet\"}\n"
        );
    }

    #[test]
    fn riserva_in_sola_lettura_non_ferma_la_scrittura() {
        let d = TempDir::new().unwrap();
        let f = d.path().join("settings.json");
        let bak = d.path().join("settings.json.isola-bak");
        std::fs::write(&f, "{\"model\":\"opus\"}\n").unwrap();
        std::fs::write(&bak, "vecchia").unwrap();
        let mut perm = std::fs::metadata(&bak).unwrap().permissions();
        perm.set_readonly(true);
        std::fs::set_permissions(&bak, perm).unwrap();
        scrivi_sicuro(&f, &unisci(leggi(&f).unwrap(), &piano())).expect("la scrittura riesce");
        assert!(ha_nostre(&leggi(&f).unwrap()));
        assert_eq!(
            std::fs::read_to_string(&bak).unwrap(),
            "vecchia",
            "la riserva presente resta"
        );
        let mut perm = std::fs::metadata(&bak).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perm.set_readonly(false);
        std::fs::set_permissions(&bak, perm).unwrap();
    }

    #[test]
    fn scrive_anche_se_il_file_e_la_cartella_mancano() {
        let d = TempDir::new().unwrap();
        let f = d.path().join("nuova").join("settings.json");
        scrivi_sicuro(&f, &json!({ "model": "opus" })).unwrap();
        assert!(f.exists());
        assert!(
            !d.path()
                .join("nuova")
                .join("settings.json.isola-bak")
                .exists(),
            "nulla da riservare"
        );
    }

    #[test]
    fn barra_altrui_non_toccata() {
        let mut v = fixture();
        v["statusLine"] = json!({ "type": "command", "command": "ccstatusline" });
        let dopo = unisci(v.clone(), &piano());
        assert_eq!(dopo["statusLine"], v["statusLine"]);
        let a = anteprima(&v, &piano());
        assert!(a.barra_occupata);
        assert!(
            !a.testo.contains("statusLine"),
            "la barra altrui non si scrive: {}",
            a.testo
        );
        assert_eq!(togli(dopo)["statusLine"], v["statusLine"]);
        let nostra = unisci(fixture(), &piano());
        assert_eq!(
            nostra["statusLine"],
            json!({ "type": "command", "command": "C:/Users/u/isola/isola-hook.exe barra" })
        );
        assert!(togli(nostra).get("statusLine").is_none());
        assert!(!anteprima(&fixture(), &piano()).barra_occupata);
    }

    #[test]
    fn senza_comando_per_la_barra_non_si_scrive_la_barra() {
        let p = Piano {
            comando_barra: None,
            ..piano()
        };
        assert!(unisci(fixture(), &p).get("statusLine").is_none());
        let vecchia = unisci(fixture(), &piano());
        assert!(unisci(vecchia, &p).get("statusLine").is_none());
    }

    #[test]
    fn comando_barra_con_spazi_usa_il_percorso_corto() {
        let exe = Path::new("C:\\Program Files\\x\\isola-hook.exe");
        let corto = |_: &Path| Some(PathBuf::from("C:\\PROGRA~1\\x\\isola-hook.exe"));
        assert_eq!(
            comando_barra(exe, corto).as_deref(),
            Some("C:/PROGRA~1/x/isola-hook.exe barra")
        );
        assert_eq!(
            comando_barra(exe, |_| None),
            None,
            "senza percorso corto niente barra"
        );
        assert_eq!(comando_barra(exe, |p| Some(p.to_path_buf())), None);
        let semplice = Path::new("C:\\Users\\u\\isola\\isola-hook.exe");
        assert_eq!(
            comando_barra(semplice, |_| None).as_deref(),
            Some("C:/Users/u/isola/isola-hook.exe barra")
        );
        assert_eq!(
            comando_barra(Path::new("C:\\Users\\o'b\\a.exe"), |_| None),
            None
        );
    }

    #[test]
    fn file_in_claude_config_dir() {
        let home = Path::new("/home/u");
        assert_eq!(
            file_impostazioni(Some(OsStr::new("/dati/claude")), home),
            Path::new("/dati/claude").join("settings.json")
        );
        assert_eq!(
            file_impostazioni(None, home),
            home.join(".claude").join("settings.json")
        );
        assert_eq!(
            file_impostazioni(Some(OsStr::new("")), home),
            home.join(".claude").join("settings.json"),
            "vuota = assente"
        );
    }

    #[test]
    fn hook_disabilitati_segnalato() {
        let mut v = fixture();
        assert!(!anteprima(&v, &piano()).hook_disabilitati);
        v["disableAllHooks"] = json!(true);
        assert!(anteprima(&v, &piano()).hook_disabilitati);
        v["disableAllHooks"] = json!(false);
        assert!(!anteprima(&v, &piano()).hook_disabilitati);
    }

    #[test]
    fn altre_chiavi_non_oggetto_restano() {
        let v: Value = serde_json::from_str(
            r#"{"n": 1.5, "grande": 12345678901234, "t": "è ✓", "a": [1, [2], {"b": null}]}"#,
        )
        .unwrap();
        let d = TempDir::new().unwrap();
        let f = d.path().join("s.json");
        scrivi_sicuro(&f, &unisci(v.clone(), &piano())).unwrap();
        let letto = leggi(&f).unwrap();
        for k in ["n", "grande", "t", "a"] {
            assert_eq!(letto[k], v[k], "{k}");
        }
    }

    #[test]
    fn riconosce_l_aiutante_con_barre_e_maiuscole() {
        for c in [
            r"C:\a b\isola-hook.exe",
            "C:/a/ISOLA-HOOK.EXE",
            "isola-hook.exe",
        ] {
            assert!(
                genere(&json!({"type":"command","command":c,"args":["evento"]})).is_some(),
                "{c}"
            );
        }
        for e in [
            json!({"type":"command","command":"C:/a/isola-hook.exe","args":["altro"]}),
            json!({"type":"command","command":"C:/a/isola-hook.exe"}),
            json!({"type":"command","command":"C:/a/altro.exe","args":["evento"]}),
        ] {
            assert!(genere(&e).is_none(), "{e}");
        }
    }

    #[test]
    fn la_vecchia_barra_solo_nella_forma_completa() {
        let barra = |c: &str| json!({ "type": "command", "command": c });
        for c in [
            "C:/x/assistente.exe --barra-stato 47613",
            r"C:\x\ASSISTENTE.EXE --barra-stato 1",
        ] {
            assert!(barra_nostra(&barra(c)), "{c}");
        }
        for c in [
            "ccstatusline --barra-stato 1",
            "C:/x/assistente.exe --barra-stato",
            "C:/x/assistente.exe --barra-stato abc",
            "C:/x/assistente.exe --barra-stato 1 altro",
            "altro.exe --barra-stato 5",
            "echo --barra-stato",
        ] {
            assert!(!barra_nostra(&barra(c)), "{c}");
        }
    }

    #[test]
    fn file_cambiato_prima_della_scrittura_non_si_scrive() {
        let d = TempDir::new().unwrap();
        let f = d.path().join("settings.json");
        std::fs::write(&f, "{\"model\":\"opus\"}\n").unwrap();
        let letto = leggi(&f).unwrap();
        std::fs::write(&f, "{\"model\":\"sonnet\"}\n").unwrap();
        let r = scrivi_se_invariato(&f, &letto, &unisci(letto.clone(), &piano())).unwrap();
        assert_eq!(r, Scrittura::Cambiato);
        assert_eq!(
            std::fs::read_to_string(&f).unwrap(),
            "{\"model\":\"sonnet\"}\n"
        );
        let nomi: Vec<String> = std::fs::read_dir(d.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            nomi,
            ["settings.json"],
            "né riserva né temporanei: {nomi:?}"
        );
        // Invariato: scrive.
        let letto = leggi(&f).unwrap();
        assert_eq!(
            scrivi_se_invariato(&f, &letto, &unisci(letto.clone(), &piano())).unwrap(),
            Scrittura::Scritto
        );
    }

    #[test]
    fn la_riserva_esistente_resta_intera_e_nessun_temporaneo_resta() {
        let d = TempDir::new().unwrap();
        let f = d.path().join("settings.json");
        let bak = d.path().join("settings.json.isola-bak");
        std::fs::write(&f, "{\"model\":\"opus\"}\n").unwrap();
        std::fs::write(&bak, "riserva vecchia").unwrap();
        scrivi_sicuro(&f, &unisci(leggi(&f).unwrap(), &piano())).unwrap();
        // Copia in un temporaneo e rename: la riserva è intera (qui sostituita dalla nuova).
        assert_eq!(
            std::fs::read_to_string(&bak).unwrap(),
            "{\"model\":\"opus\"}\n"
        );
        let rimasti: Vec<_> = std::fs::read_dir(d.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(rimasti.is_empty(), "{rimasti:?}");
    }

    #[test]
    fn percorso_non_utf8_non_valido() {
        assert!(piano().percorso_valido());
        assert!(!Piano {
            aiutante: PathBuf::new(),
            ..piano()
        }
        .percorso_valido());
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStringExt;
            let strano = std::ffi::OsString::from_wide(&[0x61, 0xD800, 0x62]);
            assert!(!Piano {
                aiutante: PathBuf::from(strano),
                ..piano()
            }
            .percorso_valido());
        }
    }
}
