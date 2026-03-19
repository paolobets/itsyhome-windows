/// All shared application types serialised with camelCase for the TypeScript frontend.
use serde::{Deserialize, Serialize};

// ─── Entity enum ──────────────────────────────────────────────────────────────

/// Tagged union that the frontend reads as `{ type: "light", ... }` etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseEntity {
    pub entity_id: String,
    pub name: String,
    pub area_id: Option<String>,
    pub state: String,
    pub is_available: bool,
}

// ─── Concrete entity types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchEntity {
    #[serde(flatten)]
    pub base: BaseEntity,
    pub is_on: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockEntity {
    #[serde(flatten)]
    pub base: BaseEntity,
    pub lock_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValveEntity {
    #[serde(flatten)]
    pub base: BaseEntity,
    pub is_open: bool,
    pub position: Option<f32>,
    pub supports_position: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SensorEntity {
    #[serde(flatten)]
    pub base: BaseEntity,
    pub value: String,
    pub unit: Option<String>,
    pub device_class: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlarmEntity {
    #[serde(flatten)]
    pub base: BaseEntity,
    pub alarm_state: String,
    pub supported_modes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraEntity {
    #[serde(flatten)]
    pub base: BaseEntity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneEntity {
    #[serde(flatten)]
    pub base: BaseEntity,
}

// ─── Menu data ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Room {
    pub area_id: String,
    pub name: String,
    pub entities: Vec<AppEntity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuData {
    pub favorites: Vec<AppEntity>,
    pub rooms: Vec<Room>,
    pub scenes: Vec<SceneEntity>,
    pub cameras: Vec<AppEntity>,
    pub temp_unit: String,
    pub notification_count: u32,
    pub update_count: u32,
}

// ─── Config / environment ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HaEnvironment {
    pub id: String,
    pub name: String,
    pub ha_url: String,
    pub ha_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub ha_url: String,
    pub ha_token: String,
    pub cameras_enabled: bool,
    pub launch_at_login: bool,
    pub active_env_id: Option<String>,
}

// ─── test_connection result ───────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestConnectionResult {
    pub ok: bool,
    pub entity_count: Option<u32>,
    pub error: Option<String>,
}
