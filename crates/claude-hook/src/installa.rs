//! Il collegamento a Claude Code nel suo `settings.json` (M3, Task 7, spec §6.1).
//!
//! Qui solo funzioni pure sul JSON e la scrittura sicura del file: chi chiama (l'assistente) decide
//! *quando* scrivere, e scrive solo dopo che l'utente ha letto l'anteprima e confermato. Il codice
//! segreto non entra mai nel file: gli hook lo prendono dalla variabile d'ambiente
//! `ISOLA_HOOK_TOKEN` (`"Authorization": "Bearer $ISOLA_HOOK_TOKEN"`).
//!
//! Note dalla documentazione di Claude Code (letta il 18/11/2026 su code.claude.com/docs/en/settings,
//! settings-reference e hooks; non da memoria):
//! - Hook `http`: campi `type`, `url`, `headers` (valori con `$VAR` o `${VAR}`, risolti solo se la
//!   variabile è in `allowedEnvVars`, altrimenti stringa vuota), `timeout` (secondi), `onFailure`
//!   (`continue`/`block`: non lo scriviamo mai, resta il comportamento di serie, cioè un errore di
//!   connessione o un non-2xx non blocca nulla). Il corpo è un POST JSON.
//! - `allowedHttpHookUrls` («qualunque file», array che si fondono fra i file, `*` jolly, assente =
//!   ogni URL permesso) e `httpHookAllowedEnvVars` (stessa fusione, assente = vale la lista del
//!   singolo hook): se *definiti* e senza di noi, l'hook non parte. Non li scriviamo: li segnaliamo
//!   nell'anteprima (`fuori_lista`). Vediamo solo questo file: una lista in un altro file può
//!   comunque lasciarci passare o fermarci.
//! - `statusLine`: oggetto `{"type":"command","command":…}` (+ `padding`, `refreshInterval`, …);
//!   «qualunque file». Se ce n'è già una di altri non la tocchiamo.
//! - `disableAllHooks: true` spegne hook *e* barra di stato personalizzata (fuori dai gestiti):
//!   l'anteprima lo dice (`hook_disabilitati`).
//! - Claude Code ricarica le modifiche a `hooks` senza riavvio; le variabili d'ambiente le legge
//!   all'avvio del terminale, quindi i terminali già aperti vanno riaperti.

use crate::PERCORSO_EVENTO;
use serde_json::{json, Map, Value};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Il nome della variabile d'ambiente che porta il codice segreto (unica fonte: l'assistente, la
/// barra di stato e le voci installate usano tutti questo).
pub const VARIABILE_CODICE: &str = "ISOLA_HOOK_TOKEN";

/// Come comincia, in `PERCORSO_EVENTO` e nell'URL delle nostre voci, il percorso con la versione.
const PREFISSO_PERCORSO: &str = "/isola/v";
/// La versione delle voci scritte: letta dal percorso, così il numero e l'URL non possono divergere.
pub const VERSIONE_VOCI: u32 = versione_del_percorso(PERCORSO_EVENTO);

const fn versione_del_percorso(percorso: &str) -> u32 {
    let b = percorso.as_bytes();
    let mut i = PREFISSO_PERCORSO.len();
    let mut v: u32 = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        v = v * 10 + (b[i] - b'0') as u32;
        i += 1;
    }
    v
}

/// Come comincia l'URL delle nostre voci.
const PREFISSO_URL: &str = "http://127.0.0.1:";
/// Ciò che riconosce il comando della nostra barra di stato.
const MARCATORE_BARRA: &str = "--barra-stato";

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

/// Secondi di attesa dell'hook: brevi, a parte il permesso che aspetta una decisione dell'utente.
const TIMEOUT_EVENTO: u32 = 5;
const TIMEOUT_PERMESSO: u32 = 120;

/// Che cosa installare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piano {
    pub porta: u16,
    /// Il comando della barra di stato; `None` se non si può scrivere in modo sicuro (nessun
    /// percorso corto per un percorso con spazi) o se non lo si vuole.
    pub comando_barra: Option<String>,
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

/// Che cosa c'è già di nostro nel file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatoInstallazione {
    Assente,
    Presente { porta: u16, versione: u32, comando_barra: Option<String> },
}

/// Che cosa si scriverebbe, per l'anteprima.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anteprima {
    /// Il JSON delle sole voci aggiunte (indentato).
    pub testo: String,
    /// La barra di stato è già di altri: la nostra non si scrive.
    pub barra_occupata: bool,
    /// `allowedHttpHookUrls` o `httpHookAllowedEnvVars` ci sono in questo file e non ci includono.
    pub fuori_lista: bool,
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
        Err(e) => return Err(ErroreInstalla::Io(format!("Non riesco a leggere {}: {e}", file.display()))),
    };
    let testo = String::from_utf8(byte)
        .map_err(|_| ErroreInstalla::NonValido(format!("{} non è un testo UTF-8.", file.display())))?;
    let testo = testo.strip_prefix('\u{feff}').unwrap_or(&testo);
    if testo.trim().is_empty() {
        return Ok(json!({}));
    }
    let v: Value = serde_json::from_str(testo).map_err(|e| {
        ErroreInstalla::NonValido(format!("{} non è un JSON valido (niente commenti né virgole finali): {e}", file.display()))
    })?;
    if !v.is_object() {
        return Err(ErroreInstalla::NonValido(format!("{} non contiene un oggetto JSON.", file.display())));
    }
    match v.get("hooks") {
        None => {}
        Some(Value::Object(eventi)) => {
            if let Some((nome, _)) = eventi.iter().find(|(_, g)| !g.is_array()) {
                return Err(ErroreInstalla::NonValido(format!("In «hooks», «{nome}» non è un elenco.")));
            }
        }
        Some(_) => return Err(ErroreInstalla::NonValido("«hooks» non è un oggetto.".into())),
    }
    Ok(v)
}

fn url_evento(porta: u16) -> String {
    format!("{PREFISSO_URL}{porta}{PERCORSO_EVENTO}")
}

/// Porta e versione di un URL che è nostro (`http://127.0.0.1:<porta>/isola/v<n>/…`).
fn analizza_url(url: &str) -> Option<(u16, u32)> {
    let resto = url.strip_prefix(PREFISSO_URL)?;
    let (porta, percorso) = resto.split_once('/')?;
    let porta = porta.parse().ok()?;
    let percorso = format!("/{percorso}");
    let versione = percorso.strip_prefix(PREFISSO_PERCORSO)?;
    let cifre: String = versione.chars().take_while(char::is_ascii_digit).collect();
    if cifre.is_empty() || !versione[cifre.len()..].starts_with('/') {
        return None;
    }
    Some((porta, cifre.parse().ok()?))
}

/// Una voce di hook è nostra: `http` verso 127.0.0.1 sul percorso `/isola/v…`.
fn voce_nostra(e: &Value) -> Option<(u16, u32)> {
    if e.get("type")?.as_str()? != "http" {
        return None;
    }
    analizza_url(e.get("url")?.as_str()?)
}

fn barra_nostra(s: &Value) -> bool {
    s.get("command").and_then(Value::as_str).is_some_and(|c| c.contains(MARCATORE_BARRA))
}

fn comando_della_barra(v: &Value) -> Option<String> {
    let s = v.get("statusLine")?;
    barra_nostra(s).then(|| s["command"].as_str().map(str::to_owned)).flatten()
}

/// Le voci di hook (dentro i gruppi di ogni evento) di un file.
fn tutte_le_voci(v: &Value) -> impl Iterator<Item = &Value> {
    v.get("hooks")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|h| h.values())
        .filter_map(Value::as_array)
        .flatten()
        .filter_map(|g| g.get("hooks").and_then(Value::as_array))
        .flatten()
}

/// Che cosa c'è già di nostro: porta e versione dalle voci, comando dalla barra.
pub fn nostre(v: &Value) -> StatoInstallazione {
    match tutte_le_voci(v).find_map(voce_nostra) {
        Some((porta, versione)) => StatoInstallazione::Presente { porta, versione, comando_barra: comando_della_barra(v) },
        None => StatoInstallazione::Assente,
    }
}

fn voce(porta: u16, evento: &str) -> Value {
    json!({
        "type": "http",
        "url": url_evento(porta),
        "headers": { "Authorization": format!("Bearer ${VARIABILE_CODICE}") },
        "allowedEnvVars": [VARIABILE_CODICE],
        "timeout": if evento == "PermissionRequest" { TIMEOUT_PERMESSO } else { TIMEOUT_EVENTO },
    })
}

/// Il gruppo di un evento: una sola voce, nessun `matcher`.
fn gruppo(porta: u16, evento: &str) -> Value {
    json!({ "hooks": [voce(porta, evento)] })
}

fn barra(comando: &str) -> Value {
    json!({ "type": "command", "command": comando })
}

/// Toglie le nostre voci e la nostra barra, e solo ciò che è rimasto vuoto per causa nostra
/// (gruppi, eventi, `hooks`). Il resto, vuoto o no, non si tocca.
pub fn togli(mut v: Value) -> Value {
    let Some(radice) = v.as_object_mut() else { return v };
    let mut tolto_qualcosa = false;
    if let Some(hooks) = radice.get_mut("hooks").and_then(Value::as_object_mut) {
        let mut svuotati = Vec::new();
        for (evento, gruppi) in hooks.iter_mut() {
            let Some(gruppi) = gruppi.as_array_mut() else { continue };
            let mut tolto_qui = false;
            gruppi.retain_mut(|g| {
                let Some(lista) = g.get_mut("hooks").and_then(Value::as_array_mut) else { return true };
                let prima = lista.len();
                lista.retain(|e| voce_nostra(e).is_none());
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
    if radice.get("statusLine").is_some_and(barra_nostra) {
        radice.shift_remove("statusLine");
    }
    v
}

/// Mette le nostre voci (le vecchie, di qualunque porta o versione, prima si tolgono) in coda a
/// quelle già presenti, e la barra se manca o era nostra. Tutto il resto resta com'è, nell'ordine.
pub fn unisci(v: Value, p: &Piano) -> Value {
    let mut v = togli(v);
    let Some(radice) = v.as_object_mut() else { return v };
    let hooks = radice.entry("hooks").or_insert_with(|| Value::Object(Map::new()));
    if let Some(hooks) = hooks.as_object_mut() {
        for evento in EVENTI {
            let elenco = hooks.entry(evento).or_insert_with(|| Value::Array(Vec::new()));
            if let Some(elenco) = elenco.as_array_mut() {
                elenco.push(gruppo(p.porta, evento));
            }
        }
    }
    if let Some(comando) = p.comando_barra.as_deref() {
        if !radice.contains_key("statusLine") {
            radice.insert("statusLine".into(), barra(comando));
        }
    }
    v
}

/// Una voce di `allowedHttpHookUrls` («*» jolly) che copre `url`.
fn corrisponde(modello: &str, url: &str) -> bool {
    let m: Vec<char> = modello.chars().collect();
    let s: Vec<char> = url.chars().collect();
    let (mut i, mut j, mut stella, mut ripresa) = (0, 0, None, 0);
    while j < s.len() {
        if i < m.len() && m[i] == '*' {
            stella = Some(i);
            i += 1;
            ripresa = j;
        } else if i < m.len() && m[i] == s[j] {
            i += 1;
            j += 1;
        } else if let Some(st) = stella {
            i = st + 1;
            ripresa += 1;
            j = ripresa;
        } else {
            return false;
        }
    }
    m[i..].iter().all(|c| *c == '*')
}

/// Che cosa si scriverebbe nel file `v` con il piano `p`: il JSON delle sole voci aggiunte e gli
/// avvisi che l'utente deve leggere prima di confermare.
pub fn anteprima(v: &Value, p: &Piano) -> Anteprima {
    let barra_occupata = v.get("statusLine").is_some_and(|s| !barra_nostra(s));
    let mut hooks = Map::new();
    for evento in EVENTI {
        hooks.insert(evento.into(), Value::Array(vec![gruppo(p.porta, evento)]));
    }
    let mut aggiunte = Map::new();
    aggiunte.insert("hooks".into(), Value::Object(hooks));
    if let (Some(comando), false) = (p.comando_barra.as_deref(), barra_occupata) {
        aggiunte.insert("statusLine".into(), barra(comando));
    }
    let url = url_evento(p.porta);
    let urls_senza_noi = v
        .get("allowedHttpHookUrls")
        .and_then(Value::as_array)
        .is_some_and(|l| !l.iter().filter_map(Value::as_str).any(|m| corrisponde(m, &url)));
    let variabili_senza_noi = v
        .get("httpHookAllowedEnvVars")
        .and_then(Value::as_array)
        .is_some_and(|l| !l.iter().any(|x| x.as_str() == Some(VARIABILE_CODICE)));
    Anteprima {
        testo: serde_json::to_string_pretty(&Value::Object(aggiunte)).unwrap_or_default(),
        barra_occupata,
        fuori_lista: urls_senza_noi || variabili_senza_noi,
        hook_disabilitati: v.get("disableAllHooks").and_then(Value::as_bool) == Some(true),
    }
}

/// Scrive il file senza perdere nulla se qualcosa va storto: prima la riserva `<file>.isola-bak`
/// (se il file c'era), poi `<file>.tmp` e `rename`. Indentazione di 2 spazi, ritorno a capo finale.
/// Una scorciatoia (symlink) si segue: si riscrive il file vero, non il collegamento.
pub fn scrivi_sicuro(file: &Path, v: &Value) -> Result<(), ErroreInstalla> {
    let io = |cosa: &str, e: std::io::Error| ErroreInstalla::Io(format!("{cosa}: {e}"));
    let destinazione = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    let mut testo = serde_json::to_string_pretty(v).map_err(|e| ErroreInstalla::Io(e.to_string()))?;
    testo.push('\n');
    if let Some(cartella) = destinazione.parent() {
        std::fs::create_dir_all(cartella).map_err(|e| io("Non riesco a creare la cartella", e))?;
    }
    let con_suffisso = |s: &str| {
        let mut n = destinazione.file_name().unwrap_or(OsStr::new("settings.json")).to_os_string();
        n.push(s);
        destinazione.with_file_name(n)
    };
    if destinazione.exists() {
        std::fs::copy(&destinazione, con_suffisso(".isola-bak")).map_err(|e| io("Non riesco a fare la copia di riserva", e))?;
    }
    let tmp = con_suffisso(".tmp");
    let esito = std::fs::write(&tmp, testo.as_bytes()).and_then(|_| std::fs::rename(&tmp, &destinazione));
    if let Err(e) = esito {
        let _ = std::fs::remove_file(&tmp);
        return Err(io("Non riesco a scrivere il file", e));
    }
    Ok(())
}

/// I caratteri ammessi in un percorso scritto in un comando senza virgolette: la shell che lo
/// esegue (Git Bash o altra) non deve poterlo rompere.
fn percorso_sicuro(p: &str) -> bool {
    !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | ':' | '~' | '-' | '+'))
}

/// Il comando della barra di stato: il percorso dell'exe con `/` al posto di `\` (Claude Code lo
/// passa a una shell), e se non è «pulito» (spazi, apici…) il suo percorso corto 8.3 (`corto`); se
/// nemmeno quello lo è, `None`: meglio nessuna barra che un comando rotto.
pub fn comando_barra(exe: &Path, porta: u16, corto: impl Fn(&Path) -> Option<PathBuf>) -> Option<String> {
    let a_barre = |p: &Path| p.to_str().map(|s| s.replace('\\', "/"));
    let direttamente = a_barre(exe).filter(|s| percorso_sicuro(s));
    let percorso = match direttamente {
        Some(s) => s,
        None => corto(exe).and_then(|c| a_barre(&c)).filter(|s| percorso_sicuro(s))?,
    };
    Some(format!("{percorso} {MARCATORE_BARRA} {porta}"))
}

#[cfg(test)]
mod test {
    use super::*;
    use tempfile::TempDir;

    const PORTA: u16 = 47613;

    fn piano() -> Piano {
        Piano { porta: PORTA, comando_barra: Some(format!("C:/x/assistente.exe --barra-stato {PORTA}")) }
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

    fn chiavi(v: &Value) -> Vec<String> {
        v.as_object().unwrap().keys().cloned().collect()
    }

    #[test]
    fn unisci_conserva_tutto() {
        let prima = fixture();
        let dopo = unisci(prima.clone(), &piano());
        // Ogni chiave di prima c'è ancora, nello stesso ordine, e `hooks` è dove stava.
        let vecchie = chiavi(&prima);
        let nuove: Vec<String> = chiavi(&dopo).into_iter().filter(|k| vecchie.contains(k)).collect();
        assert_eq!(nuove, vecchie);
        for k in ["model", "permissions", "env", "effortLevel"] {
            assert_eq!(dopo[k], prima[k], "{k}");
        }
        // L'hook di altri è intatto e per primo; il nostro gruppo è in coda.
        let pre = dopo["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre[0], prima["hooks"]["PreToolUse"][0]);
        assert_eq!(pre.len(), 2);
        let stop = dopo["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop[0], prima["hooks"]["Stop"][0]);
        assert_eq!(stop.len(), 2);
        // Gli eventi di prima restano nel loro ordine.
        let eventi: Vec<String> = chiavi(&dopo["hooks"]);
        assert_eq!(&eventi[..2], ["PreToolUse", "Stop"]);
    }

    #[test]
    fn unisci_due_volte_come_una() {
        let una = unisci(fixture(), &piano());
        let due = unisci(una.clone(), &piano());
        assert_eq!(serde_json::to_string(&una).unwrap(), serde_json::to_string(&due).unwrap());
        // Anche su un file vuoto.
        let una = unisci(json!({}), &piano());
        assert_eq!(unisci(una.clone(), &piano()), una);
    }

    #[test]
    fn togli_ridà_l_originale() {
        let originale = fixture();
        let giro = togli(unisci(originale.clone(), &piano()));
        assert_eq!(serde_json::to_string(&giro).unwrap(), serde_json::to_string(&originale).unwrap());
        // Su un file senza hook né barra: torna `{}` (via gruppi, eventi e `hooks` vuoti per causa nostra).
        assert_eq!(togli(unisci(json!({}), &piano())), json!({}));
        // Un `hooks` vuoto di altri, che non è stato svuotato da noi, resta.
        let altrui = json!({ "hooks": {} });
        assert_eq!(togli(altrui.clone()), altrui);
        // Un nostro gruppo con dentro anche un hook di altri: via solo il nostro.
        let mut misto = unisci(json!({}), &piano());
        misto["hooks"]["Stop"][0]["hooks"].as_array_mut().unwrap().push(json!({"type":"command","command":"x"}));
        let tolto = togli(misto);
        assert_eq!(tolto["hooks"]["Stop"][0]["hooks"], json!([{"type":"command","command":"x"}]));
        assert!(tolto["hooks"].get("PreToolUse").is_none());
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
            assert_eq!(std::fs::read(&f).unwrap(), prima, "i byte non cambiano ({parola})");
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
    }

    #[test]
    fn bom_tollerato() {
        let d = TempDir::new().unwrap();
        let f = d.path().join("settings.json");
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
        assert_eq!(std::fs::read_to_string(d.path().join("settings.json.isola-bak")).unwrap(), vecchio);
        let nuovo = std::fs::read_to_string(&f).unwrap();
        assert!(nuovo.ends_with("}\n"), "ritorno a capo finale");
        assert!(nuovo.contains("\n  \"model\": \"opus\""), "indentazione di 2 spazi: {nuovo}");
        let rimasti: Vec<String> = std::fs::read_dir(d.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(rimasti.is_empty(), "nessun .tmp rimasto: {rimasti:?}");
        // Il file scritto si rilegge uguale.
        assert_eq!(leggi(&f).unwrap(), unisci(serde_json::from_str(vecchio).unwrap(), &piano()));
    }

    #[test]
    fn scrive_anche_se_il_file_e_la_cartella_mancano() {
        let d = TempDir::new().unwrap();
        let f = d.path().join("nuova").join("settings.json");
        scrivi_sicuro(&f, &json!({ "model": "opus" })).unwrap();
        assert!(f.exists());
        assert!(!d.path().join("nuova").join("settings.json.isola-bak").exists(), "nulla da riservare");
    }

    #[test]
    fn barra_altrui_non_toccata() {
        let mut v = fixture();
        v["statusLine"] = json!({ "type": "command", "command": "ccstatusline" });
        let dopo = unisci(v.clone(), &piano());
        assert_eq!(dopo["statusLine"], v["statusLine"]);
        let a = anteprima(&v, &piano());
        assert!(a.barra_occupata);
        assert!(!a.testo.contains("statusLine"), "la barra altrui non si scrive: {}", a.testo);
        // Togliere non la tocca.
        assert_eq!(togli(dopo)["statusLine"], v["statusLine"]);
        // Senza barra di altri la nostra c'è, e `togli` la porta via.
        let nostra = unisci(fixture(), &piano());
        assert_eq!(nostra["statusLine"]["type"], "command");
        assert!(nostra["statusLine"]["command"].as_str().unwrap().contains(MARCATORE_BARRA));
        assert!(togli(nostra).get("statusLine").is_none());
        assert!(!anteprima(&fixture(), &piano()).barra_occupata);
    }

    #[test]
    fn senza_comando_per_la_barra_non_si_scrive_la_barra() {
        let p = Piano { comando_barra: None, ..piano() };
        let v = unisci(fixture(), &p);
        assert!(v.get("statusLine").is_none());
        // E una nostra barra vecchia, che non si può più riscrivere, se ne va.
        let vecchia = unisci(fixture(), &piano());
        assert!(unisci(vecchia, &p).get("statusLine").is_none());
    }

    #[test]
    fn nessun_codice_in_settings() {
        // Il file scritto non ha nessuna sequenza di 64 cifre esadecimali (un codice), e gli hook
        // portano il riferimento alla variabile.
        let testo = serde_json::to_string_pretty(&unisci(fixture(), &piano())).unwrap();
        assert!(testo.contains("Bearer $ISOLA_HOOK_TOKEN"));
        assert!(!contiene_codice(&testo), "{testo}");
        assert!(contiene_codice(&"a1".repeat(32)), "il controllo sa riconoscere un codice");
        assert!(!contiene_codice(&"a1".repeat(31)));
    }

    /// Una sequenza di almeno 64 cifre esadecimali di fila.
    fn contiene_codice(s: &str) -> bool {
        let mut n = 0;
        for c in s.chars() {
            n = if c.is_ascii_hexdigit() { n + 1 } else { 0 };
            if n >= 64 {
                return true;
            }
        }
        false
    }

    #[test]
    fn eventi_e_timeout() {
        let v = unisci(json!({}), &piano());
        let hooks = v["hooks"].as_object().unwrap();
        assert_eq!(hooks.len(), 7);
        for e in EVENTI {
            let gruppi = hooks[e].as_array().unwrap();
            assert_eq!(gruppi.len(), 1, "{e}");
            assert!(gruppi[0].get("matcher").is_none(), "{e}: nessun matcher");
            let voce = &gruppi[0]["hooks"][0];
            assert_eq!(voce["type"], "http");
            assert_eq!(voce["url"], format!("http://127.0.0.1:{PORTA}{PERCORSO_EVENTO}"));
            assert_eq!(voce["headers"]["Authorization"], "Bearer $ISOLA_HOOK_TOKEN");
            assert_eq!(voce["allowedEnvVars"], json!(["ISOLA_HOOK_TOKEN"]));
            assert_eq!(voce["timeout"], if e == "PermissionRequest" { 120 } else { 5 }, "{e}");
        }
        assert!(hooks.get("SessionStart").is_none());
        assert!(hooks.get("PostToolUse").is_none());
        assert!(!serde_json::to_string(&v).unwrap().contains("onFailure"));
    }

    #[test]
    fn la_versione_viene_dal_percorso() {
        assert_eq!(VERSIONE_VOCI, 1);
        assert!(PERCORSO_EVENTO.starts_with(PREFISSO_PERCORSO));
        assert_eq!(versione_del_percorso("/isola/v12/evento"), 12);
    }

    #[test]
    fn percorso_cambiato_rilevato() {
        assert_eq!(nostre(&fixture()), StatoInstallazione::Assente);
        let nostro = unisci(fixture(), &piano());
        assert_eq!(
            nostre(&nostro),
            StatoInstallazione::Presente {
                porta: PORTA,
                versione: VERSIONE_VOCI,
                comando_barra: piano().comando_barra,
            }
        );
        // Voci di un'altra porta e di un altro exe: `nostre` dice i valori vecchi.
        let vecchio = Piano { porta: 1111, comando_barra: Some("D:/vecchio/assistente.exe --barra-stato 1111".into()) };
        let letto = nostre(&unisci(fixture(), &vecchio));
        assert_eq!(
            letto,
            StatoInstallazione::Presente { porta: 1111, versione: 1, comando_barra: vecchio.comando_barra.clone() }
        );
        assert_ne!(letto, nostre(&nostro));
        // Una versione futura delle voci si riconosce (e `unisci` la sostituisce).
        let mut futuro = unisci(json!({}), &piano());
        futuro["hooks"]["Stop"][0]["hooks"][0]["url"] = json!("http://127.0.0.1:47613/isola/v2/evento");
        assert!(matches!(nostre(&futuro), StatoInstallazione::Presente { .. }));
        let rifatto = unisci(futuro, &piano());
        assert_eq!(rifatto["hooks"]["Stop"].as_array().unwrap().len(), 1, "le vecchie voci sono sostituite");
        // Una voce http di altri non è nostra.
        let altrui = json!({ "hooks": { "Stop": [ { "hooks": [ { "type": "http", "url": "http://localhost:9/isola/v1/evento" } ] } ] } });
        assert_eq!(nostre(&altrui), StatoInstallazione::Assente);
    }

    #[test]
    fn comando_barra_con_spazi_usa_il_percorso_corto() {
        let exe = Path::new("C:\\Program Files\\x\\assistente.exe");
        let corto = |_: &Path| Some(PathBuf::from("C:\\PROGRA~1\\x\\assistente.exe"));
        assert_eq!(
            comando_barra(exe, 47613, corto).as_deref(),
            Some("C:/PROGRA~1/x/assistente.exe --barra-stato 47613")
        );
        assert_eq!(comando_barra(exe, 47613, |_| None), None, "senza percorso corto niente barra");
        // Il «corto» che spazi li ha ancora non va bene.
        assert_eq!(comando_barra(exe, 47613, |p| Some(p.to_path_buf())), None);
        // Senza spazi non serve il corto (e `/` al posto di `\`).
        let semplice = Path::new("C:\\Users\\u\\isola\\assistente.exe");
        assert_eq!(
            comando_barra(semplice, 5, |_| None).as_deref(),
            Some("C:/Users/u/isola/assistente.exe --barra-stato 5")
        );
        // Un apice romperebbe la shell: stessa strada dei percorsi con spazi.
        assert_eq!(comando_barra(Path::new("C:\\Users\\o'b\\a.exe"), 5, |_| None), None);
    }

    #[test]
    fn file_in_claude_config_dir() {
        let home = Path::new("/home/u");
        assert_eq!(file_impostazioni(Some(OsStr::new("/dati/claude")), home), Path::new("/dati/claude").join("settings.json"));
        assert_eq!(file_impostazioni(None, home), home.join(".claude").join("settings.json"));
        assert_eq!(file_impostazioni(Some(OsStr::new("")), home), home.join(".claude").join("settings.json"), "vuota = assente");
    }

    #[test]
    fn fuori_lista_segnalato() {
        let mut v = fixture();
        assert!(!anteprima(&v, &piano()).fuori_lista, "nessuna lista: tutto permesso");
        v["allowedHttpHookUrls"] = json!(["https://hooks.example.com/*"]);
        assert!(anteprima(&v, &piano()).fuori_lista);
        v["allowedHttpHookUrls"] = json!(["http://127.0.0.1:*"]);
        assert!(!anteprima(&v, &piano()).fuori_lista, "il jolly ci include");
        v["allowedHttpHookUrls"] = json!([]);
        assert!(anteprima(&v, &piano()).fuori_lista, "lista definita e vuota: non passa nessuno");
        v["allowedHttpHookUrls"] = json!(["http://127.0.0.1:*/isola/*"]);
        v["httpHookAllowedEnvVars"] = json!(["ALTRA"]);
        assert!(anteprima(&v, &piano()).fuori_lista, "variabile non ammessa");
        v["httpHookAllowedEnvVars"] = json!(["ALTRA", "ISOLA_HOOK_TOKEN"]);
        assert!(!anteprima(&v, &piano()).fuori_lista);
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
    fn anteprima_mostra_solo_le_voci_aggiunte() {
        let a = anteprima(&fixture(), &piano());
        assert!(!a.testo.contains("permissions") && !a.testo.contains("echo ciao"), "{}", a.testo);
        for e in EVENTI {
            assert!(a.testo.contains(e), "{e}");
        }
        assert!(a.testo.contains("statusLine") && a.testo.contains("--barra-stato"));
        assert!(a.testo.contains("$ISOLA_HOOK_TOKEN"));
        assert!(serde_json::from_str::<Value>(&a.testo).is_ok(), "è JSON");
    }

    #[test]
    fn altre_chiavi_non_oggetto_restano() {
        // Numeri, nidificazioni e stringhe con caratteri non ASCII passano il giro intatti.
        let v: Value = serde_json::from_str(r#"{"n": 1.5, "grande": 12345678901234, "t": "è ✓", "a": [1, [2], {"b": null}]}"#).unwrap();
        let d = TempDir::new().unwrap();
        let f = d.path().join("s.json");
        scrivi_sicuro(&f, &unisci(v.clone(), &piano())).unwrap();
        let letto = leggi(&f).unwrap();
        for k in ["n", "grande", "t", "a"] {
            assert_eq!(letto[k], v[k], "{k}");
        }
    }
}
