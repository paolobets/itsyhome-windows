/// All shared application types serialised with camelCase for the TypeScript frontend.
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ─── Entity enum ──────────────────────────────────────────────────────────────

/// Tagged union that the frontend reads as `{ type: "light", ... }` etc.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AppEntity {
    Light(LightEntity),
    Switch(SwitchEntity),
    Climate(ClimateEntity),
    Cover(CoverEntity),
    Lock(LockEntity),
    Fan(FanEntity),
    Humidifier(HumidifierEntity),
    Valve(ValveEntity),
    Sensor(SensorEntity),
    Alarm(AlarmEntity),
    Camera(CameraEntity),
    Scene(SceneEntity),
}

impl AppEntity {
    pub fn entity_id(&self) -> &str {
        match self {
            Self::Light(e) => &e.base.entity_id,
            Self::Switch(e) => &e.base.entity_id,
            Self::Climate(e) => &e.base.entity_id,
            Self::Cover(e) => &e.base.entity_id,
            Self::Lock(e) => &e.base.entity_id,
            Self::Fan(e) => &e.base.entity_id,
            Self::Humidifier(e) => &e.base.entity_id,
            Self::Valve(e) => &e.base.entity_id,
            Self::Sensor(e) => &e.base.entity_id,
            Self::Alarm(e) => &e.base.entity_id,
            Self::Camera(e) => &e.base.entity_id,
            Self::Scene(e) => &e.base.entity_id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Light(e) => &e.base.name,
            Self::Switch(e) => &e.base.name,
            Self::Climate(e) => &e.base.name,
            Self::Cover(e) => &e.base.name,
            Self::Lock(e) => &e.base.name,
            Self::Fan(e) => &e.base.name,
            Self::Humidifier(e) => &e.base.name,
            Self::Valve(e) => &e.base.name,
            Self::Sensor(e) => &e.base.name,
            Self::Alarm(e) => &e.base.name,
            Self::Camera(e) => &e.base.name,
            Self::Scene(e) => &e.base.name,
        }
    }
}

// ─── Base ─────────────────────────────────────────────────────────────────────

/// Common fields present on every entity.  Flattened into each concrete type so
/// the wire format matches the TypeScript interface (no nested `base` key).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseEntity {
    pub entity_id: String,
    pub name: String,
    pub area_id: Option<String>,
    pub state: String,
    pub is_available: bool,
}

// ─── Concrete entity types ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LightEntity {
    #[serde(flatten)]
    pub base: BaseEntity,
    pub is_on: bool,
    pub brightness: Option<f32>,
    pub hue: Option<f32>,
    pub saturation: Option<f32>,
    pub color_temp: Option<f32>,
    pub min_color_temp: Option<f32>,
    pub max_color_temp: Option<f32>,
    pub supports_brightness: bool,
    pub supports_color_temp: bool,
    pub supports_rgb: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchEntity {
    #[serde(flatten)]
    pub base: BaseEntity,
    pub is_on: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClimateEntity {
    #[serde(flatten)]
    pub base: BaseEntity,
    pub hvac_mode: String,
    pub hvac_action: Option<String>,
    pub hvac_modes: Vec<String>,
    pub current_temp: Option<f32>,
    pub target_temp: Option<f32>,
    pub target_temp_high: Option<f32>,
    pub target_temp_low: Option<f32>,
    pub min_temp: f32,
    pub max_temp: f32,
    pub temp_step: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverEntity {
    #[serde(flatten)]
    pub base: BaseEntity,
    pub cover_state: String,
    pub position: Option<f32>,
    pub tilt: Option<f32>,
    pub supports_position: bool,
    pub supports_tilt: bool,
    pub device_class: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockEntity {
    #[serde(flatten)]
    pub base: BaseEntity,
    pub lock_state: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FanEntity {
    #[serde(flatten)]
    pub base: BaseEntity,
    pub is_on: bool,
    pub percentage: Option<f32>,
    pub oscillating: Option<bool>,
    pub direction: Option<String>,
    pub preset_mode: Option<String>,
    pub preset_modes: Vec<String>,
    pub supports_percentage: bool,
    pub supports_oscillation: bool,
    pub supports_direction: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HumidifierEntity {
    #[serde(flatten)]
    pub base: BaseEntity,
    pub is_on: bool,
    pub target_humidity: Option<f32>,
    pub current_humidity: Option<f32>,
    pub mode: Option<String>,
    pub modes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValveEntity {
    #[serde(flatten)]
    pub base: BaseEntity,
    pub is_open: bool,
    pub position: Option<f32>,
    pub supports_position: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SensorEntity {
    #[serde(flatten)]
    pub base: BaseEntity,
    pub value: String,
    pub unit: Option<String>,
    pub device_class: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlarmEntity {
    #[serde(flatten)]
    pub base: BaseEntity,
    pub alarm_state: String,
    pub supported_modes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraEntity {
    #[serde(flatten)]
    pub base: BaseEntity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneEntity {
    #[serde(flatten)]
    pub base: BaseEntity,
}

// ─── Menu data ────────────────────────────────────────────────────────────────

/// La chiave del gruppo delle scene dove servono le stesse chiavi delle stanze (ordine dei
/// dispositivi in `deviceOrder`, stanza «Scene» della vista Casa). Non è un `area_id` possibile:
/// HA li fa con lettere minuscole, cifre e trattini bassi, mai doppi in testa.
pub const GRUPPO_SCENE: &str = "__scene__";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Room {
    pub area_id: String,
    pub name: String,
    pub entities: Vec<AppEntity>,
}

/// Un dispositivo nel catalogo della gestione (Impostazioni › Dispositivi): solo ciò che serve a
/// riconoscerlo, niente stato (quello delle entità nascoste non è seguito).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoceCatalogo {
    pub entity_id: String,
    pub name: String,
    /// Il tipo come nel tag di `AppEntity` («light», «switch», «scene»…).
    pub kind: String,
}

/// Una stanza nel catalogo: tutte le sue entità, anche quelle nascoste dall'utente, nell'ordine
/// salvato. `area_id` vuoto = «Altro»; `GRUPPO_SCENE` = le scene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StanzaCatalogo {
    pub area_id: String,
    pub name: String,
    pub voci: Vec<VoceCatalogo>,
}

/// Il tempo di adesso da un'entità `weather.*` di Home Assistant (Task 2 della M2): solo la
/// condizione e la temperatura, le previsioni non servono.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meteo {
    pub entity_id: String,
    /// Lo stato dell'entità: una delle condizioni di HA («cloudy», «rainy», «sunny»…).
    pub condizione: String,
    /// `None` se HA non la dà o non è un numero.
    pub temperatura: Option<f64>,
    /// «°C» o «°F»: quella dell'entità, altrimenti quella della casa.
    pub unita: String,
}

/// Un sensore numerico di Home Assistant (temperatura, umidità, potenza, energia…) per i quadranti
/// e i lati della pillola (M2, Task 4): solo ciò che serve a sceglierlo e a mostrarlo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SensoreNumerico {
    pub entity_id: String,
    pub nome: String,
    /// `None` se non disponibile o se lo stato non è un numero.
    pub valore: Option<f64>,
    /// L'unità di misura come la dà HA («°C», «W», «kWh»…); vuota se non c'è.
    pub unita: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuData {
    pub favorites: Vec<AppEntity>,
    pub rooms: Vec<Room>,
    pub scenes: Vec<SceneEntity>,
    pub cameras: Vec<AppEntity>,
    pub temp_unit: String,
    /// Notifiche persistenti al momento dell'ultima mappatura completa: le notifiche da sole non
    /// rimappano, quindi il numero può restare indietro. L'app non lo usa (le notifiche arrivano
    /// come `CasaEvento::NotificaAggiunta`/`NotificaRimossa`).
    pub notification_count: u32,
    pub update_count: u32,
    /// Tutte le stanze con tutte le entità che l'app sa mostrare, nascoste comprese (le stanze
    /// tutte nascoste restano), nell'ordine salvato; poi «Altro» e il gruppo delle scene. Le
    /// telecamere no (si vedono sempre). Serve alla gestione di stanze e dispositivi. In un `Arc`:
    /// le sostituzioni di un'entità (`Arc::make_mut` sul `MenuData`) non lo copiano.
    #[serde(default)]
    pub catalogo: Arc<Vec<StanzaCatalogo>>,
    /// Il meteo scelto (`Config::meteo_entita`, o la prima `weather.*` disponibile). `None`: nessuna
    /// entità meteo, o quella preferita non disponibile. Le `weather.*` non entrano mai in stanze
    /// e preferiti.
    #[serde(default)]
    pub meteo: Option<Meteo>,
    /// Tutte le `weather.*` per `entity_id`, per il selettore delle Impostazioni (`kind` «weather»).
    #[serde(default)]
    pub meteo_elenco: Vec<VoceCatalogo>,
    /// Tutti i sensori numerici di HA (non disattivati, non nascosti, non di diagnostica), per
    /// nome: il catalogo di quadranti e lati, a parte da stanze e preferiti (che non cambiano).
    /// I valori dei sensori scelti si aggiornano a ogni cambio, senza mappatura completa; gli
    /// altri restano quelli dell'ultima mappatura (non sono seguiti). In un `Arc`, come `catalogo`.
    #[serde(default)]
    pub sensori_numerici: Arc<Vec<SensoreNumerico>>,
}

// ─── Config / environment ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaEnvironment {
    pub id: String,
    pub name: String,
    pub ha_url: String,
}

// ─── test_connection result ───────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestConnectionResult {
    pub ok: bool,
    pub entity_count: Option<u32>,
    pub error: Option<String>,
}
