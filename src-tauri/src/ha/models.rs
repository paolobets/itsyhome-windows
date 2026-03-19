/// Raw types returned by the Home Assistant WebSocket / REST API.
/// These map 1-to-1 to the original TypeScript `models.ts`.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HaState {
    pub entity_id: String,
    pub state: String,
    pub attributes: Value,
    pub last_changed: String,
    pub last_updated: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HaArea {
    pub area_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HaDevice {
    pub id: String,
    pub name: String,
    pub name_by_user: Option<String>,
    pub area_id: Option<String>,
    pub disabled_by: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HaEntityEntry {
    pub entity_id: String,
    pub name: Option<String>,
    pub platform: Option<String>,
    pub device_id: Option<String>,
    pub area_id: Option<String>,
    pub disabled_by: Option<String>,
    pub hidden_by: Option<String>,
    pub icon: Option<String>,
    pub entity_category: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HaUnitSystem {
    pub temperature: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HaConfig {
    pub unit_system: HaUnitSystem,
    pub location_name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HaNotification {
    pub notification_id: String,
    pub status: String,
}
