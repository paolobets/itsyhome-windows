/// Raw types returned by the Home Assistant WebSocket / REST API.
/// These map 1-to-1 to the original TypeScript `models.ts`.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct HaState {
    pub entity_id: String,
    pub state: String,
    #[serde(default)]
    pub attributes: Value,
    #[serde(default)]
    pub last_changed: String,
    #[serde(default)]
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
    /// La classe scelta dall'utente e quella data dall'integrazione: dicono se un sensore senza
    /// stato (non ancora caricato) sarà di temperatura o umidità.
    #[serde(default)]
    pub device_class: Option<String>,
    #[serde(default)]
    pub original_device_class: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HaUnitSystem {
    pub temperature: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HaConfig {
    pub unit_system: HaUnitSystem,
    pub location_name: String,
    /// Lo stato del core (`CoreState` di `homeassistant/core.py`: «NOT_RUNNING», «STARTING»,
    /// «RUNNING»…; `Config.as_dict` in `core_config.py` lo mette in `get_config`).
    #[serde(default)]
    pub state: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HaNotification {
    pub notification_id: String,
    pub status: String,
}
