//! Configurazione dell'assistente (`assistente.json`) e import dalla v2 Tauri.
use crate::segreti::Segreti;
use crate::types::HaEnvironment;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub environments: Vec<HaEnvironment>,
    pub active_env_id: Option<String>,
    pub favorites: Vec<String>,
    pub hidden_entities: Vec<String>,
    pub room_order: Vec<String>,
    pub device_order: HashMap<String, Vec<String>>,
    pub area_icons: HashMap<String, String>,
    pub cameras_enabled: bool,
    pub launch_at_login: bool,
    /// L'icona nell'area di notifica (spenta di serie: il menu sta nel «⋯» del pannello).
    pub mostra_icona_barra: bool,
    // `ultimoLayout`, `quadranti` e `massimiQuadranti` di un file di prima (layout Orologio e
    // Quadranti, tolti col ridisegno F1) si ignorano leggendo e spariscono al primo salvataggio.
    pub importata_da_v2: bool,
    /// I collegamenti veloci (Task 14f): pagine web delle gestioni, nell'ordine scelto. Un file di
    /// prima, senza la chiave, si legge con l'elenco vuoto.
    #[serde(default)]
    pub collegamenti: Vec<Collegamento>,
    /// L'entità `weather.*` scelta per il meteo; `None` = automatico (la prima disponibile).
    #[serde(default)]
    pub meteo_entita: Option<String>,
    /// Che cosa mostra la pillola a sinistra e a destra degli occhi (M2, Task 4): le stringhe di
    /// `assistente::lati::Lato` («ora», «cpu», «sensore:<entity_id>»…); `""` = niente. Un file di
    /// prima, senza le chiavi, si legge con niente / niente.
    #[serde(default)]
    pub lato_sinistro: String,
    #[serde(default)]
    pub lato_destro: String,
    /// Il monitor su cui stanno pillola e pannello (M2, Task 6): il nome come lo dà winit
    /// (`MonitorHandle::name()`); `None` = il principale. Resta salvato anche mentre il monitor
    /// non c'è. Un file di prima, senza la chiave, si legge con `None`.
    #[serde(default)]
    pub schermo: Option<String>,
    /// La rotella sulla pillola cambia il volume del PC (M2, Task 7, solo Windows): accesa di
    /// serie (D2), anche per un file di prima senza la chiave.
    #[serde(default = "vero")]
    pub volume_rotella: bool,
    /// Avvisa quando le cuffie (o un altro dispositivo) Bluetooth sono scariche (M2, Task 9, solo
    /// Windows): acceso di serie, anche per un file di prima senza la chiave.
    #[serde(default = "vero")]
    pub avvisi_bt: bool,
    /// I dispositivi Bluetooth già avvisati: impronta (FNV-1a 64 bit del contenitore
    /// del dispositivo fisico, o dell'id di istanza del nodo se il contenitore manca; mai
    /// l'indirizzo in chiaro) → data dell'avviso («2026-11-15»); così l'avviso è uno al giorno
    /// anche dopo un riavvio dell'app. Le impronte più vecchie di 7 giorni si tolgono quando
    /// si salva un nuovo avviso (non a ogni salvataggio: il file resta limitato lo stesso, senza
    /// riscrivere la configurazione per questo). Un file di prima, senza la chiave, si legge vuoto.
    #[serde(default)]
    pub bt_avvisati: HashMap<String, String>,
    /// Claude Code (M3): collegamento e soglie. Un file di M2, senza la chiave, si legge
    /// scollegato.
    #[serde(default)]
    pub claude: ConfigClaude,
    /// L'ordine dei moduli del pannello (ridisegno F1) e quali sono fissati nella barra. Vuoto o
    /// mancante = l'ordine del registro, tutti fissati. L'id `claude` di prima si legge come
    /// `agenti` (lo fa il registro dell'assistente).
    #[serde(default)]
    pub moduli: Vec<ConfigModulo>,
    /// La modalità della Panoramica (ridisegno F1, Task 9): «griglia» o «orologio». Vuota (un
    /// file di prima, senza la chiave) o con un altro valore = Griglia; lo decide l'assistente
    /// (`ModoPanoramica::da_config`).
    #[serde(default)]
    pub panoramica: String,
}

/// Un modulo nell'ordine scelto dall'utente. Entrambi i campi hanno un valore di serie: una voce
/// scritta male si legge (e il registro scarta un id sconosciuto) invece di far rileggere tutta la
/// configurazione come nuova.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigModulo {
    #[serde(default)]
    pub id: String,
    #[serde(default = "vero")]
    pub fissato: bool,
}

/// Le scelte su Claude Code (M3). Nessun segreto e nessuna porta: il canale è il tubo protetto.
/// Un file di prima con la chiave `porta` si legge lo stesso (le chiavi sconosciute si ignorano).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ConfigClaude {
    /// Gli hook sono collegati: l'assistente ascolta sul tubo.
    pub collegato: bool,
    /// Percentuale dei limiti dell'abbonamento oltre la quale si avvisa (predefinita 80).
    pub soglia_limite: u8,
    /// Spesa del mese (dollari) oltre la quale si avvisa; `None` = nessuna soglia.
    pub soglia_spesa: Option<f64>,
    /// Finestra di limite (chiave) → fine della finestra già avvisata (secondi Unix).
    pub limiti_avvisati: HashMap<String, i64>,
    /// Mese («2026-11») → ultima soglia di spesa già avvisata (percentuale).
    pub spesa_avvisata: HashMap<String, u8>,
    /// Il giorno («2026-11-17») dell'ultimo avviso di spesa insolita.
    pub insolita_avvisata: Option<String>,
    /// L'utente ha detto «Ho capito» alla voce «il collegamento va aggiornato»: all'avvio non si
    /// ripete finché non collega o scollega di nuovo (la sezione Claude lo dice comunque).
    pub aggiornamento_ignorato: bool,
    /// L'ultimo valore noto del limite delle 5 ore e di quello dei 7 giorni (RC-56): dopo un
    /// riavvio la vista Claude li mostra attenuati, con «dalle…», finché non arriva un dato
    /// nuovo. Scritti solo quando il valore cambia e al massimo una volta al minuto.
    pub ultimo_cinque_ore: Option<LimiteVisto>,
    pub ultimo_sette_giorni: Option<LimiteVisto>,
}

/// Un limite dell'abbonamento com'era l'ultima volta che lo si è visto.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LimiteVisto {
    /// Quanto era pieno (0..100).
    pub percentuale: f32,
    /// Quando si azzera (secondi Unix): passato, il valore non si mostra più.
    pub azzera_alle: i64,
    /// Quando è stato visto (secondi Unix).
    pub visto_alle: i64,
}

impl Default for ConfigClaude {
    fn default() -> ConfigClaude {
        ConfigClaude {
            collegato: false,
            soglia_limite: 80,
            soglia_spesa: None,
            limiti_avvisati: HashMap::new(),
            spesa_avvisata: HashMap::new(),
            insolita_avvisata: None,
            aggiornamento_ignorato: false,
            ultimo_cinque_ore: None,
            ultimo_sette_giorni: None,
        }
    }
}

fn vero() -> bool {
    true
}

/// Come il `Default` derivato (tutto vuoto, falso, `None`), salvo `volume_rotella` acceso: una
/// configurazione nuova e un file senza la chiave dicono la stessa cosa.
impl Default for Config {
    fn default() -> Config {
        Config {
            environments: Vec::new(),
            active_env_id: None,
            favorites: Vec::new(),
            hidden_entities: Vec::new(),
            room_order: Vec::new(),
            device_order: HashMap::new(),
            area_icons: HashMap::new(),
            cameras_enabled: false,
            launch_at_login: false,
            mostra_icona_barra: false,
            importata_da_v2: false,
            collegamenti: Vec::new(),
            meteo_entita: None,
            lato_sinistro: String::new(),
            lato_destro: String::new(),
            schermo: None,
            volume_rotella: vero(),
            avvisi_bt: vero(),
            bt_avvisati: HashMap::new(),
            claude: ConfigClaude::default(),
            moduli: Vec::new(),
            panoramica: String::new(),
        }
    }
}

/// Il prefisso di una fonte che è un sensore di Home Assistant («sensore:sensor.sala_t»).
pub const PREFISSO_SENSORE: &str = "sensore:";

impl Config {
    /// I sensori di Home Assistant scelti per i lati della pillola: il servizio li segue sempre
    /// (`subscribe_entities`), anche se un giorno non fossero più fra quelli mostrati.
    pub fn sensori_scelti(&self) -> BTreeSet<String> {
        [&self.lato_sinistro, &self.lato_destro]
            .into_iter()
            .filter_map(|f| f.strip_prefix(PREFISSO_SENSORE))
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .collect()
    }
}

/// Un collegamento veloce: si apre nel browser predefinito. `url` è già controllato e
/// normalizzato dalle Impostazioni (solo http/https), ma chi lo apre lo ricontrolla: il file si
/// può scrivere anche a mano.
///
/// `lanci` e `ultimo_lancio` sono gli unici dati nuovi del ridisegno F1: quante volte il
/// collegamento è stato aperto dal pannello e quando l'ultima (secondi Unix). Restano solo nella
/// configurazione locale, mai in rete; un file di prima, senza le chiavi, li legge a zero.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Collegamento {
    pub id: String,
    pub nome: String,
    pub url: String,
    #[serde(default)]
    pub lanci: u32,
    #[serde(default)]
    pub ultimo_lancio: i64,
}

pub struct ConfigStore {
    file: PathBuf,
}

impl ConfigStore {
    pub fn apri(dir: &Path) -> ConfigStore {
        ConfigStore { file: dir.join("assistente.json") }
    }

    /// File assente o illeggibile: configurazione di default.
    pub fn leggi(&self) -> Config {
        std::fs::read_to_string(&self.file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Scrittura atomica: `.tmp` poi rename.
    pub fn salva(&self, config: &Config) -> io::Result<()> {
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.file.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(config)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &self.file)
    }
}

pub fn cartella_config() -> PathBuf {
    directories::ProjectDirs::from("com", "itsyhome", "assistente")
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn file_config_v2() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|d| {
        d.config_dir().join("com.itsyhome.app").join("itsyhome-config.json")
    })
}

fn stringa(v: &Value, chiave: &str) -> String {
    v.get(chiave).and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn lista(v: &Value, chiave: &str) -> Vec<String> {
    v.get(chiave)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

fn mappa<T>(v: &Value, chiave: &str, f: impl Fn(&Value) -> Option<T>) -> HashMap<String, T> {
    v.get(chiave)
        .and_then(Value::as_object)
        .map(|o| o.iter().filter_map(|(k, x)| f(x).map(|t| (k.clone(), t))).collect())
        .unwrap_or_default()
}

/// Importa la configurazione v2 senza mai scriverla. I token finiscono in `segreti`.
/// Non salva il risultato: lo fa il chiamante.
/// Se il portachiavi fallisce, i token già salvati vengono cancellati (rollback sugli env_id coinvolti).
pub fn importa_v2(v2: &Path, segreti: &dyn Segreti) -> Result<Option<Config>, String> {
    if !v2.exists() {
        return Ok(None);
    }
    let testo = std::fs::read_to_string(v2).map_err(|e| format!("lettura file v2: {e}"))?;
    let v: Value = serde_json::from_str(testo.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("file v2 non valido: {e}"))?;
    if !v.is_object() {
        return Err("file v2 non valido: radice non è un oggetto".into());
    }

    let mut envs: Vec<HaEnvironment> = Vec::new();
    let mut token: Vec<(String, String)> = Vec::new();
    if let Some(arr) = v.get("environments").and_then(Value::as_array) {
        for e in arr {
            let id = stringa(e, "id");
            if id.is_empty() {
                continue;
            }
            envs.push(HaEnvironment { id: id.clone(), name: stringa(e, "name"), ha_url: stringa(e, "haUrl") });
            let t = stringa(e, "haToken");
            if !t.is_empty() {
                token.push((id, t));
            }
        }
    }
    let mut attivo = v.get("activeEnvId").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_owned);

    // Regola legacy di store.rs: haUrl/haToken top-level con environments vuoto.
    let (lurl, ltok) = (stringa(&v, "haUrl"), stringa(&v, "haToken"));
    if envs.is_empty() && (!lurl.is_empty() || !ltok.is_empty()) {
        let id = "home".to_owned();
        envs.push(HaEnvironment { id: id.clone(), name: "Home".into(), ha_url: lurl });
        if !ltok.is_empty() {
            token.push((id.clone(), ltok));
        }
        attivo = Some(id);
    }

    // Token nel portachiavi; al primo errore si annulla quanto già salvato.
    let mut salvati: Vec<&str> = Vec::new();
    for (id, t) in &token {
        if let Err(e) = segreti.salva(id, t) {
            for s in &salvati {
                let _ = segreti.cancella(s);
            }
            return Err(format!("portachiavi non disponibile: {e}"));
        }
        salvati.push(id);
    }

    Ok(Some(Config {
        environments: envs,
        active_env_id: attivo,
        favorites: lista(&v, "favorites"),
        hidden_entities: lista(&v, "hiddenEntities"),
        room_order: lista(&v, "roomOrder"),
        device_order: mappa(&v, "deviceOrder", |x| {
            x.as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_owned)).collect())
        }),
        area_icons: mappa(&v, "areaIcons", |x| x.as_str().map(str::to_owned)),
        cameras_enabled: v.get("camerasEnabled").and_then(Value::as_bool).unwrap_or(false),
        launch_at_login: v.get("launchAtLogin").and_then(Value::as_bool).unwrap_or(false),
        // La v2 viveva nella barra; la v3 di serie no (decisione di Paolo del 15/11).
        mostra_icona_barra: false,
        importata_da_v2: true,
        // La v2 non aveva collegamenti.
        collegamenti: Vec::new(),
        // La v2 non aveva il meteo: automatico.
        meteo_entita: None,
        // Né i lati della pillola: niente / niente.
        lato_sinistro: String::new(),
        lato_destro: String::new(),
        // La v2 non sceglieva il monitor.
        schermo: None,
        // Né il volume: la rotella sulla pillola accesa, come di serie.
        volume_rotella: vero(),
        // Né i Bluetooth: l'avviso acceso, come di serie, e nessun dispositivo già avvisato.
        avvisi_bt: vero(),
        bt_avvisati: HashMap::new(),
        // La v2 non aveva Claude Code: scollegato.
        claude: ConfigClaude::default(),
        // Né i moduli del pannello: l'ordine del registro.
        moduli: Vec::new(),
        // Né la scelta della Panoramica: la Griglia di serie.
        panoramica: String::new(),
    }))
}