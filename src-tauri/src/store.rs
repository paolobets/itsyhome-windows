/// StoreWrapper — thin synchronous wrapper around `tauri-plugin-store`.
/// Mirrors the TypeScript `Store` class exactly, including one-time migration.
use std::collections::HashMap;

use serde_json::Value;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt as _;
use uuid::Uuid;

use crate::types::HaEnvironment;

const STORE_FILE: &str = "itsyhome-config.json";

/// Synchronous accessors over the plugin store.
/// Call `StoreWrapper::new(app)` once at startup; the underlying store handle
/// is kept open for the lifetime of the app.
pub struct StoreWrapper {
    app: AppHandle,
}

impl StoreWrapper {
    /// Open (or create) the store and run the one-time migration.
    pub fn new(app: AppHandle) -> Self {
        let wrapper = Self { app };
        wrapper.migrate();
        wrapper
    }

    // ─── Internal helpers ─────────────────────────────────────────────────────

    fn get(&self, key: &str) -> Option<Value> {
        let store = self.app.store(STORE_FILE).ok()?;
        store.get(key)
    }

    fn set(&self, key: &str, value: Value) {
        if let Ok(store) = self.app.store(STORE_FILE) {
            let _ = store.set(key, value);
            if let Err(e) = store.save() {
                log::warn!("[store] Failed to persist key '{key}': {e}");
            }
        } else {
            log::warn!("[store] Could not open store to write key '{key}'");
        }
    }

    fn get_string(&self, key: &str) -> String {
        self.get(key)
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default()
    }

    fn get_bool(&self, key: &str, default: bool) -> bool {
        self.get(key)
            .and_then(|v| v.as_bool())
            .unwrap_or(default)
    }

    fn get_string_vec(&self, key: &str) -> Vec<String> {
        self.get(key)
            .and_then(|v| {
                v.as_array().map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(str::to_owned))
                        .collect()
                })
            })
            .unwrap_or_default()
    }

    // ─── One-time migration: haUrl/haToken → environments[] ──────────────────

    fn migrate(&self) {
        let legacy_url = self.get_string("haUrl");
        let legacy_token = self.get_string("haToken");
        let envs = self.get_environments();

        if (!legacy_url.is_empty() || !legacy_token.is_empty()) && envs.is_empty() {
            let env = HaEnvironment {
                id: Uuid::new_v4().to_string(),
                name: "Home".to_owned(),
                ha_url: legacy_url,
                ha_token: legacy_token,
            };
            let active_id = env.id.clone();
            self.set(
                "environments",
                serde_json::to_value(vec![&env]).unwrap_or(Value::Array(vec![])),
            );
            self.set("activeEnvId", Value::String(active_id));
            self.set("haUrl", Value::String(String::new()));
            self.set("haToken", Value::String(String::new()));
        }
    }

    // ─── Environments ─────────────────────────────────────────────────────────

    pub fn get_environments(&self) -> Vec<HaEnvironment> {
        self.get("environments")
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default()
    }

    pub fn get_active_environment_id(&self) -> Option<String> {
        let id = self.get_string("activeEnvId");
        if id.is_empty() { None } else { Some(id) }
    }

    pub fn set_active_environment_id(&self, id: Option<&str>) {
        self.set(
            "activeEnvId",
            Value::String(id.unwrap_or("").to_owned()),
        );
    }

    pub fn get_active_environment(&self) -> Option<HaEnvironment> {
        let id = self.get_active_environment_id()?;
        self.get_environments().into_iter().find(|e| e.id == id)
    }

    pub fn add_environment(&self, name: &str, url: &str, token: &str) -> HaEnvironment {
        let env = HaEnvironment {
            id: Uuid::new_v4().to_string(),
            name: if name.trim().is_empty() {
                "Home".to_owned()
            } else {
                name.trim().to_owned()
            },
            ha_url: url.trim().trim_end_matches('/').to_owned(),
            ha_token: token.trim().to_owned(),
        };
        let mut envs = self.get_environments();
        envs.push(env.clone());
        self.set(
            "environments",
            serde_json::to_value(&envs).unwrap_or(Value::Array(vec![])),
        );
        env
    }

    pub fn update_environment(&self, env: HaEnvironment) {
        let updated: Vec<HaEnvironment> = self
            .get_environments()
            .into_iter()
            .map(|e| {
                if e.id == env.id {
                    HaEnvironment {
                        id: env.id.clone(),
                        name: env.name.trim().to_owned(),
                        ha_url: env.ha_url.trim().trim_end_matches('/').to_owned(),
                        ha_token: env.ha_token.trim().to_owned(),
                    }
                } else {
                    e
                }
            })
            .collect();
        self.set(
            "environments",
            serde_json::to_value(&updated).unwrap_or(Value::Array(vec![])),
        );
    }

    pub fn remove_environment(&self, id: &str) {
        let filtered: Vec<HaEnvironment> = self
            .get_environments()
            .into_iter()
            .filter(|e| e.id != id)
            .collect();
        self.set(
            "environments",
            serde_json::to_value(&filtered).unwrap_or(Value::Array(vec![])),
        );
        if self.get_active_environment_id().as_deref() == Some(id) {
            self.set_active_environment_id(None);
        }
    }

    // ─── Derived credential helpers ───────────────────────────────────────────

    pub fn get_ha_url(&self) -> String {
        self.get_active_environment()
            .map(|e| e.ha_url)
            .unwrap_or_default()
    }

    pub fn get_ha_token(&self) -> String {
        self.get_active_environment()
            .map(|e| e.ha_token)
            .unwrap_or_default()
    }

    pub fn has_credentials(&self) -> bool {
        let env = self.get_active_environment();
        env.map(|e| !e.ha_url.is_empty() && !e.ha_token.is_empty())
            .unwrap_or(false)
    }

    /// Legacy compat: update the active env's credentials, or create a new one.
    pub fn set_ha_credentials(&self, url: &str, token: &str) {
        if let Some(id) = self.get_active_environment_id() {
            let updated: Vec<HaEnvironment> = self
                .get_environments()
                .into_iter()
                .map(|e| {
                    if e.id == id {
                        HaEnvironment {
                            ha_url: url.trim().trim_end_matches('/').to_owned(),
                            ha_token: token.trim().to_owned(),
                            ..e
                        }
                    } else {
                        e
                    }
                })
                .collect();
            self.set(
                "environments",
                serde_json::to_value(&updated).unwrap_or(Value::Array(vec![])),
            );
        } else {
            let env = self.add_environment("Home", url, token);
            self.set_active_environment_id(Some(&env.id));
        }
    }

    // ─── Favorites ────────────────────────────────────────────────────────────

    pub fn get_favorites(&self) -> Vec<String> {
        self.get_string_vec("favorites")
    }

    pub fn set_favorites(&self, ids: Vec<String>) {
        self.set("favorites", serde_json::to_value(&ids).unwrap_or(Value::Array(vec![])));
    }

    pub fn add_favorite(&self, id: &str) {
        let mut favs = self.get_favorites();
        if !favs.contains(&id.to_owned()) {
            favs.push(id.to_owned());
            self.set_favorites(favs);
        }
    }

    pub fn remove_favorite(&self, id: &str) {
        let favs: Vec<String> = self
            .get_favorites()
            .into_iter()
            .filter(|f| f != id)
            .collect();
        self.set_favorites(favs);
    }

    // ─── Hidden entities ──────────────────────────────────────────────────────

    pub fn get_hidden_entities(&self) -> Vec<String> {
        self.get_string_vec("hiddenEntities")
    }

    pub fn set_hidden_entities(&self, ids: Vec<String>) {
        self.set(
            "hiddenEntities",
            serde_json::to_value(&ids).unwrap_or(Value::Array(vec![])),
        );
    }

    // ─── Room / device ordering ───────────────────────────────────────────────

    pub fn get_room_order(&self) -> Vec<String> {
        self.get_string_vec("roomOrder")
    }

    pub fn set_room_order(&self, ids: Vec<String>) {
        self.set(
            "roomOrder",
            serde_json::to_value(&ids).unwrap_or(Value::Array(vec![])),
        );
    }

    pub fn get_device_order(&self, area_id: &str) -> Vec<String> {
        let map = self.get_device_order_map();
        map.get(area_id).cloned().unwrap_or_default()
    }

    pub fn set_device_order(&self, area_id: &str, ids: Vec<String>) {
        let mut map = self.get_device_order_map();
        map.insert(area_id.to_owned(), ids);
        self.set(
            "deviceOrder",
            serde_json::to_value(&map).unwrap_or(Value::Object(Default::default())),
        );
    }

    fn get_device_order_map(&self) -> HashMap<String, Vec<String>> {
        self.get("deviceOrder")
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default()
    }

    /// Public version used by the refresh module to snapshot all device orders.
    pub fn get_device_order_all(&self) -> HashMap<String, Vec<String>> {
        self.get_device_order_map()
    }

    // ─── Area icons ───────────────────────────────────────────────────────────

    pub fn get_area_icons(&self) -> HashMap<String, String> {
        self.get("areaIcons")
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default()
    }

    pub fn set_area_icon(&self, area_id: &str, icon: Option<&str>) {
        let mut icons = self.get_area_icons();
        match icon {
            Some(i) => {
                icons.insert(area_id.to_owned(), i.to_owned());
            }
            None => {
                icons.remove(area_id);
            }
        }
        self.set(
            "areaIcons",
            serde_json::to_value(&icons).unwrap_or(Value::Object(Default::default())),
        );
    }

    // ─── App settings ─────────────────────────────────────────────────────────

    // ─── Notification registration ────────────────────────────────────────────

    pub fn get_notif_registration(&self) -> Option<crate::types::NotifRegistration> {
        self.get("notifRegistration")
            .and_then(|v| serde_json::from_value(v).ok())
    }

    pub fn set_notif_registration(&self, reg: Option<&crate::types::NotifRegistration>) {
        match reg {
            Some(r) => self.set(
                "notifRegistration",
                serde_json::to_value(r).unwrap_or(Value::Null),
            ),
            None => self.set("notifRegistration", Value::Null),
        }
    }

    pub fn get_notif_port(&self) -> u16 {
        self.get("notifPort")
            .and_then(|v| v.as_u64())
            .map(|v| v as u16)
            .unwrap_or(7421)
    }

    pub fn set_notif_port(&self, port: u16) {
        self.set("notifPort", Value::Number(port.into()));
    }

    // ─── App settings ─────────────────────────────────────────────────────────

    pub fn get_cameras_enabled(&self) -> bool {
        self.get_bool("camerasEnabled", true)
    }

    pub fn set_cameras_enabled(&self, v: bool) {
        self.set("camerasEnabled", Value::Bool(v));
    }

    pub fn get_launch_at_login(&self) -> bool {
        self.get_bool("launchAtLogin", false)
    }

    pub fn set_launch_at_login(&self, v: bool) {
        self.set("launchAtLogin", Value::Bool(v));
    }
}
