/// EntityMapper — converts raw HA API data into typed AppEntity objects.
/// This is a faithful Rust port of `entity-mapper.ts`.
use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::models::{HaArea, HaConfig, HaDevice, HaEntityEntry, HaState};
use crate::types::{
    AlarmEntity, AppEntity, BaseEntity, CameraEntity, ClimateEntity, CoverEntity, FanEntity,
    HumidifierEntity, LightEntity, LockEntity, MenuData, Room, SceneEntity, SensorEntity,
    SwitchEntity, ValveEntity,
};

// ─── Constants ────────────────────────────────────────────────────────────────

struct AlarmModeBit {
    mode: &'static str,
    bit: u32,
}

const ALARM_MODE_BITS: &[AlarmModeBit] = &[
    AlarmModeBit { mode: "armed_home", bit: 1 },
    AlarmModeBit { mode: "armed_away", bit: 2 },
    AlarmModeBit { mode: "armed_night", bit: 4 },
    AlarmModeBit { mode: "armed_vacation", bit: 16 },
    AlarmModeBit { mode: "armed_custom_bypass", bit: 32 },
];

const EXCLUDED_DOMAINS: &[&str] = &[
    "binary_sensor",
    "button",
    "input_button",
    "number",
    "input_number",
    "select",
    "input_select",
    "device_tracker",
    "person",
    "sun",
    "weather",
    "update",
    "automation",
    "script",
    "media_player",
    "vacuum",
    "remote",
    "todo",
    "calendar",
    "event",
    "image",
    "siren",
    "notify",
    "persistent_notification",
];

// ─── EntityMapper ─────────────────────────────────────────────────────────────

pub struct EntityMapper {
    temp_unit: String,
}

impl EntityMapper {
    pub fn new() -> Self {
        Self {
            temp_unit: "°C".to_owned(),
        }
    }

    fn set_temp_unit(&mut self, unit: &str) {
        let upper = unit.trim().to_uppercase();
        // HA riporta "°F" / "°C": il simbolo del grado viene prima della lettera.
        self.temp_unit = if upper.trim_start_matches('°').starts_with('F') {
            "°F".to_owned()
        } else {
            "°C".to_owned()
        };
    }

    /// Map raw HA API results into a fully-typed `MenuData`.
    pub fn map(
        &mut self,
        states: &[HaState],
        areas: &[HaArea],
        devices: &[HaDevice],
        entries: &[HaEntityEntry],
        ha_config: Option<&HaConfig>,
        notification_count: u32,
        favorites_order: &[String],
        hidden_entities: &[String],
        room_order: &[String],
        device_orders: &HashMap<String, Vec<String>>,
    ) -> MenuData {
        if let Some(cfg) = ha_config {
            self.set_temp_unit(&cfg.unit_system.temperature);
        }

        // Build lookup maps
        let device_map: HashMap<&str, &HaDevice> =
            devices.iter().map(|d| (d.id.as_str(), d)).collect();
        let entry_map: HashMap<&str, &HaEntityEntry> =
            entries.iter().map(|e| (e.entity_id.as_str(), e)).collect();
        let area_map: HashMap<&str, &HaArea> =
            areas.iter().map(|a| (a.area_id.as_str(), a)).collect();

        let excluded_set: HashSet<&str> = EXCLUDED_DOMAINS.iter().copied().collect();

        let get_area_id = |state: &HaState| -> Option<String> {
            let entry = entry_map.get(state.entity_id.as_str())?;
            if let Some(aid) = &entry.area_id {
                if !aid.is_empty() {
                    return Some(aid.clone());
                }
            }
            if let Some(did) = &entry.device_id {
                if let Some(dev) = device_map.get(did.as_str()) {
                    return dev.area_id.clone();
                }
            }
            None
        };

        let mut update_count: u32 = 0;
        let mut all_entities: Vec<AppEntity> = Vec::new();
        let mut scenes: Vec<SceneEntity> = Vec::new();
        let mut cameras: Vec<AppEntity> = Vec::new();

        for state in states {
            let domain = domain_of(&state.entity_id);

            // Count pending updates regardless of filtering
            if domain == "update" && state.state == "on" {
                update_count += 1;
            }

            if excluded_set.contains(domain) {
                continue;
            }

            // Skip disabled / hidden / config entities.
            // Exception: cameras always pass — HA sometimes tags them with
            // entity_category but they are explicitly user-facing.
            if domain != "camera" {
                if let Some(entry) = entry_map.get(state.entity_id.as_str()) {
                    if entry.disabled_by.is_some() {
                        continue;
                    }
                    if entry.hidden_by.is_some() {
                        continue;
                    }
                    if entry.entity_category.is_some() {
                        continue;
                    }
                }
            }

            let area_id = get_area_id(state);
            let entry = entry_map.get(state.entity_id.as_str()).copied();

            let Some(entity) = self.map_entity(state, entry, area_id) else {
                continue;
            };

            match &entity {
                AppEntity::Scene(s) => scenes.push(s.clone()),
                AppEntity::Camera(_) => cameras.push(entity.clone()),
                _ => all_entities.push(entity),
            }
        }

        // Hidden / favorites sets
        let hidden_set: HashSet<&str> = hidden_entities.iter().map(|s| s.as_str()).collect();
        let fav_set: HashSet<&str> = favorites_order.iter().map(|s| s.as_str()).collect();

        let visible: Vec<&AppEntity> = all_entities
            .iter()
            .filter(|e| !hidden_set.contains(e.entity_id()))
            .collect();

        let fav_map: HashMap<&str, &AppEntity> = visible
            .iter()
            .filter(|e| fav_set.contains(e.entity_id()))
            .map(|e| (e.entity_id(), *e))
            .collect();

        // Restore favorites in saved order, append any new ones alphabetically
        let mut fav_list: Vec<AppEntity> = favorites_order
            .iter()
            .filter_map(|id| fav_map.get(id.as_str()).copied().cloned())
            .collect();
        {
            let fav_id_set: HashSet<&str> = favorites_order.iter().map(|s| s.as_str()).collect();
            let mut extra: Vec<AppEntity> = visible
                .iter()
                .filter(|e| fav_set.contains(e.entity_id()) && !fav_id_set.contains(e.entity_id()))
                .map(|e| (*e).clone())
                .collect();
            extra.sort_by(|a, b| a.name().cmp(b.name()));
            fav_list.extend(extra);
        }

        // Group by area (area_id was already resolved during map_entity).
        let mut room_map: HashMap<String, Vec<AppEntity>> = HashMap::new();
        let mut no_area_list: Vec<AppEntity> = Vec::new();

        for entity in &all_entities {
            if hidden_set.contains(entity.entity_id()) {
                continue;
            }
            match entity_base_area_id(entity) {
                Some(id) if !id.is_empty() => {
                    room_map.entry(id.to_owned()).or_default().push(entity.clone());
                }
                _ => {
                    no_area_list.push(entity.clone());
                }
            }
        }

        // Determine ordered area IDs
        let room_order_set: HashSet<&str> = room_order.iter().map(|s| s.as_str()).collect();
        let all_area_ids: Vec<&str> = room_map.keys().map(|s| s.as_str()).collect();

        let mut other_area_ids: Vec<&str> = all_area_ids
            .iter()
            .copied()
            .filter(|id| !room_order_set.contains(id))
            .collect();
        other_area_ids.sort_by(|a, b| {
            let na = area_map.get(a).map(|ar| ar.name.as_str()).unwrap_or(a);
            let nb = area_map.get(b).map(|ar| ar.name.as_str()).unwrap_or(b);
            na.cmp(nb)
        });

        let ordered_area_ids: Vec<&str> = room_order
            .iter()
            .map(|s| s.as_str())
            .filter(|id| room_map.contains_key(*id))
            .chain(other_area_ids)
            .collect();

        let mut rooms: Vec<Room> = Vec::new();
        for area_id in &ordered_area_ids {
            let area = match area_map.get(area_id) {
                Some(a) => a,
                None => continue,
            };
            let entities_in_room = room_map.get(*area_id).cloned().unwrap_or_default();
            let device_order = device_orders.get(*area_id).map(|v| v.as_slice()).unwrap_or(&[]);
            let ordered = Self::ordered_entities(area_id, entities_in_room, device_order);
            if !ordered.is_empty() {
                rooms.push(Room {
                    area_id: area_id.to_string(),
                    name: area.name.clone(),
                    entities: ordered,
                });
            }
        }

        if !no_area_list.is_empty() {
            let device_order = device_orders.get("").map(|v| v.as_slice()).unwrap_or(&[]);
            let ordered = Self::ordered_entities("", no_area_list, device_order);
            rooms.push(Room {
                area_id: String::new(),
                name: "Altro".to_owned(),
                entities: ordered,
            });
        }

        MenuData {
            favorites: fav_list,
            rooms,
            scenes: scenes
                .into_iter()
                .filter(|s| !hidden_set.contains(s.base.entity_id.as_str()))
                .collect(),
            // Cameras are always visible — ignore hidden_set and cameras_enabled.
            // The user explicitly wants all HA cameras reachable from the popup.
            cameras: cameras.into_iter().collect(),
            temp_unit: self.temp_unit.clone(),
            notification_count,
            update_count,
        }
    }

    /// Restore saved ordering within a room/area, append unseen entities by name.
    fn ordered_entities(
        _area_id: &str,
        entities: Vec<AppEntity>,
        saved_order: &[String],
    ) -> Vec<AppEntity> {
        let entity_map: HashMap<&str, &AppEntity> =
            entities.iter().map(|e| (e.entity_id(), e)).collect();
        let saved_set: HashSet<&str> = saved_order.iter().map(|s| s.as_str()).collect();

        let mut ordered: Vec<AppEntity> = saved_order
            .iter()
            .filter_map(|id| entity_map.get(id.as_str()).copied().cloned())
            .collect();

        let mut rest: Vec<AppEntity> = entities
            .into_iter()
            .filter(|e| !saved_set.contains(e.entity_id()))
            .collect();
        rest.sort_by(|a, b| a.name().cmp(b.name()));

        ordered.extend(rest);
        ordered
    }

    fn map_entity(
        &self,
        state: &HaState,
        entry: Option<&HaEntityEntry>,
        area_id: Option<String>,
    ) -> Option<AppEntity> {
        let domain = domain_of(&state.entity_id);

        let display_name = entry
            .and_then(|e| e.name.as_deref())
            .or_else(|| state.attributes["friendly_name"].as_str())
            .unwrap_or(&state.entity_id)
            .to_owned();

        let is_available = state.state != "unavailable" && state.state != "unknown";

        let base = BaseEntity {
            entity_id: state.entity_id.clone(),
            name: display_name,
            area_id,
            state: state.state.clone(),
            is_available,
        };

        match domain {
            "light" => Some(AppEntity::Light(self.map_light(state, base))),
            "switch" | "input_boolean" => Some(AppEntity::Switch(Self::map_switch(state, base))),
            "climate" => Some(AppEntity::Climate(Self::map_climate(state, base))),
            "cover" => Some(AppEntity::Cover(Self::map_cover(state, base))),
            "lock" => Some(AppEntity::Lock(Self::map_lock(state, base))),
            "fan" => Some(AppEntity::Fan(Self::map_fan(state, base))),
            "humidifier" => Some(AppEntity::Humidifier(Self::map_humidifier(state, base))),
            "valve" => Some(AppEntity::Valve(Self::map_valve(state, base))),
            "sensor" => Self::map_sensor(state, base).map(AppEntity::Sensor),
            "alarm_control_panel" => Some(AppEntity::Alarm(Self::map_alarm(state, base))),
            "camera" => Some(AppEntity::Camera(CameraEntity { base })),
            "scene" => Some(AppEntity::Scene(SceneEntity { base })),
            _ => None,
        }
    }

    // ─── Per-domain mappers ───────────────────────────────────────────────────

    fn map_light(&self, s: &HaState, base: BaseEntity) -> LightEntity {
        let a = &s.attributes;
        let cms: Vec<&str> = a["supported_color_modes"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();

        let supports_rgb = ["hs", "rgb", "rgbw", "rgbww", "xy"]
            .iter()
            .any(|m| cms.contains(m));
        let supports_color_temp = ["color_temp", "rgbww", "rgbw", "white"]
            .iter()
            .any(|m| cms.contains(m));
        let supports_brightness = !cms.contains(&"onoff")
            || cms.len() > 1
            || a["brightness"] != Value::Null;

        let min_ct = a["min_mireds"].as_f64().map(|v| v as f32).or_else(|| {
            a["min_color_temp_kelvin"]
                .as_f64()
                .map(|k| (1_000_000.0_f64 / k).round() as f32)
        });
        let max_ct = a["max_mireds"].as_f64().map(|v| v as f32).or_else(|| {
            a["max_color_temp_kelvin"]
                .as_f64()
                .map(|k| (1_000_000.0_f64 / k).round() as f32)
        });

        let brightness = if s.state == "on" {
            a["brightness"]
                .as_f64()
                .map(|b| (b / 255.0 * 100.0).round() as f32)
        } else {
            None
        };

        let hs = a["hs_color"].as_array();
        let hue = hs
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_f64())
            .map(|v| v as f32);
        let saturation = hs
            .and_then(|arr| arr.get(1))
            .and_then(|v| v.as_f64())
            .map(|v| v as f32);

        LightEntity {
            base,
            is_on: s.state == "on",
            brightness,
            hue,
            saturation,
            color_temp: a["color_temp"].as_f64().map(|v| v as f32),
            min_color_temp: min_ct,
            max_color_temp: max_ct,
            supports_brightness,
            supports_color_temp,
            supports_rgb,
        }
    }

    fn map_switch(s: &HaState, base: BaseEntity) -> SwitchEntity {
        SwitchEntity {
            base,
            is_on: s.state == "on",
        }
    }

    fn map_climate(s: &HaState, base: BaseEntity) -> ClimateEntity {
        let a = &s.attributes;
        ClimateEntity {
            base,
            hvac_mode: s.state.clone(),
            hvac_action: a["hvac_action"].as_str().map(str::to_owned),
            hvac_modes: a["hvac_modes"]
                .as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
                .unwrap_or_default(),
            current_temp: a["current_temperature"].as_f64().map(|v| v as f32),
            target_temp: a["temperature"].as_f64().map(|v| v as f32),
            target_temp_high: a["target_temp_high"].as_f64().map(|v| v as f32),
            target_temp_low: a["target_temp_low"].as_f64().map(|v| v as f32),
            min_temp: a["min_temp"].as_f64().map(|v| v as f32).unwrap_or(7.0),
            max_temp: a["max_temp"].as_f64().map(|v| v as f32).unwrap_or(35.0),
            temp_step: a["target_temp_step"]
                .as_f64()
                .map(|v| v as f32)
                .unwrap_or(0.5),
        }
    }

    fn map_cover(s: &HaState, base: BaseEntity) -> CoverEntity {
        let a = &s.attributes;
        let feat = a["supported_features"].as_u64().unwrap_or(0) as u32;
        const VALID_STATES: &[&str] = &["open", "closed", "opening", "closing", "stopped"];
        let cover_state = if VALID_STATES.contains(&s.state.as_str()) {
            s.state.clone()
        } else {
            "stopped".to_owned()
        };
        CoverEntity {
            base,
            cover_state,
            position: a["current_position"].as_f64().map(|v| v as f32),
            tilt: a["current_tilt_position"].as_f64().map(|v| v as f32),
            supports_position: feat & 4 != 0,
            supports_tilt: feat & 128 != 0,
            device_class: a["device_class"].as_str().map(str::to_owned),
        }
    }

    fn map_lock(s: &HaState, base: BaseEntity) -> LockEntity {
        const VALID: &[&str] = &["locked", "unlocked", "locking", "unlocking", "jammed"];
        let lock_state = if VALID.contains(&s.state.as_str()) {
            s.state.clone()
        } else {
            "unknown".to_owned()
        };
        LockEntity { base, lock_state }
    }

    fn map_fan(s: &HaState, base: BaseEntity) -> FanEntity {
        let a = &s.attributes;
        let feat = a["supported_features"].as_u64().unwrap_or(0) as u32;
        FanEntity {
            base,
            is_on: s.state == "on",
            percentage: a["percentage"].as_f64().map(|v| v as f32),
            oscillating: a["oscillating"].as_bool(),
            direction: a["direction"].as_str().map(str::to_owned),
            preset_mode: a["preset_mode"].as_str().map(str::to_owned),
            preset_modes: a["preset_modes"]
                .as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
                .unwrap_or_default(),
            supports_percentage: a["percentage"] != Value::Null,
            supports_oscillation: feat & 1 != 0,
            supports_direction: feat & 4 != 0,
        }
    }

    fn map_humidifier(s: &HaState, base: BaseEntity) -> HumidifierEntity {
        let a = &s.attributes;
        HumidifierEntity {
            base,
            is_on: s.state == "on",
            target_humidity: a["humidity"].as_f64().map(|v| v as f32),
            current_humidity: a["current_humidity"].as_f64().map(|v| v as f32),
            mode: a["mode"].as_str().map(str::to_owned),
            modes: a["available_modes"]
                .as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
                .unwrap_or_default(),
        }
    }

    fn map_valve(s: &HaState, base: BaseEntity) -> ValveEntity {
        let a = &s.attributes;
        let feat = a["supported_features"].as_u64().unwrap_or(0) as u32;
        ValveEntity {
            base,
            is_open: s.state == "open" || s.state == "opening",
            position: a["current_position"].as_f64().map(|v| v as f32),
            supports_position: feat & 4 != 0,
        }
    }

    fn map_sensor(s: &HaState, base: BaseEntity) -> Option<SensorEntity> {
        let dc = s.attributes["device_class"].as_str()?;
        if dc != "temperature" && dc != "humidity" {
            return None;
        }
        Some(SensorEntity {
            base,
            value: s.state.clone(),
            unit: s.attributes["unit_of_measurement"]
                .as_str()
                .map(str::to_owned),
            device_class: Some(dc.to_owned()),
        })
    }

    fn map_alarm(s: &HaState, base: BaseEntity) -> AlarmEntity {
        let feat = s.attributes["supported_features"]
            .as_u64()
            .unwrap_or(0) as u32;
        let supported_modes = ALARM_MODE_BITS
            .iter()
            .filter(|m| feat & m.bit != 0)
            .map(|m| m.mode.to_owned())
            .collect();
        AlarmEntity {
            base,
            alarm_state: s.state.clone(),
            supported_modes,
        }
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn domain_of(entity_id: &str) -> &str {
    entity_id.split('.').next().unwrap_or("")
}

/// Return the area_id that was already resolved and stored in BaseEntity during mapping.
fn entity_base_area_id(entity: &AppEntity) -> Option<&str> {
    let aid = match entity {
        AppEntity::Light(e) => e.base.area_id.as_deref(),
        AppEntity::Switch(e) => e.base.area_id.as_deref(),
        AppEntity::Climate(e) => e.base.area_id.as_deref(),
        AppEntity::Cover(e) => e.base.area_id.as_deref(),
        AppEntity::Lock(e) => e.base.area_id.as_deref(),
        AppEntity::Fan(e) => e.base.area_id.as_deref(),
        AppEntity::Humidifier(e) => e.base.area_id.as_deref(),
        AppEntity::Valve(e) => e.base.area_id.as_deref(),
        AppEntity::Sensor(e) => e.base.area_id.as_deref(),
        AppEntity::Alarm(e) => e.base.area_id.as_deref(),
        AppEntity::Camera(e) => e.base.area_id.as_deref(),
        AppEntity::Scene(e) => e.base.area_id.as_deref(),
    };
    aid
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn stato(id: &str) -> HaState {
        HaState {
            entity_id: id.to_owned(),
            state: "on".to_owned(),
            attributes: json!({ "friendly_name": id }),
            last_changed: String::new(),
            last_updated: String::new(),
        }
    }

    fn voce(id: &str, area: Option<&str>) -> HaEntityEntry {
        HaEntityEntry {
            entity_id: id.to_owned(),
            name: None,
            platform: None,
            device_id: None,
            area_id: area.map(str::to_owned),
            disabled_by: None,
            hidden_by: None,
            icon: None,
            entity_category: None,
        }
    }

    fn area(id: &str, nome: &str) -> HaArea {
        HaArea { area_id: id.to_owned(), name: nome.to_owned() }
    }

    fn config(unita: &str) -> HaConfig {
        HaConfig {
            unit_system: super::super::models::HaUnitSystem { temperature: unita.to_owned() },
            location_name: "Casa".to_owned(),
        }
    }

    fn ids(v: &[AppEntity]) -> Vec<&str> {
        v.iter().map(|e| e.entity_id()).collect()
    }

    fn mappa(
        stati: &[HaState],
        aree: &[HaArea],
        voci: &[HaEntityEntry],
        cfg: Option<&HaConfig>,
        preferiti: &[&str],
        nascosti: &[&str],
        ordine_stanze: &[&str],
    ) -> MenuData {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        EntityMapper::new().map(
            stati, aree, &[], voci, cfg, 0, &s(preferiti), &s(nascosti), &s(ordine_stanze),
            &HashMap::new(),
        )
    }

    #[test]
    fn preferiti_nell_ordine_salvato() {
        let stati = [stato("light.a"), stato("switch.b")];
        let menu = mappa(&stati, &[], &[], None, &["switch.b", "light.a"], &[], &[]);
        assert_eq!(ids(&menu.favorites), vec!["switch.b", "light.a"]);
    }

    #[test]
    fn nascosti_esclusi_da_stanze_e_preferiti() {
        let stati = [stato("light.a"), stato("switch.b")];
        let aree = [area("sala", "Sala")];
        let voci = [voce("light.a", Some("sala")), voce("switch.b", Some("sala"))];
        let menu = mappa(&stati, &aree, &voci, None, &["light.a", "switch.b"], &["light.a"], &[]);
        assert_eq!(ids(&menu.favorites), vec!["switch.b"]);
        let in_stanze: Vec<&str> =
            menu.rooms.iter().flat_map(|r| r.entities.iter().map(|e| e.entity_id())).collect();
        assert_eq!(in_stanze, vec!["switch.b"]);
    }

    #[test]
    fn stanze_nell_ordine_salvato() {
        let stati = [stato("light.a"), stato("light.b")];
        let aree = [area("cucina", "Cucina"), area("sala", "Sala")];
        let voci = [voce("light.a", Some("cucina")), voce("light.b", Some("sala"))];
        let menu = mappa(&stati, &aree, &voci, None, &[], &[], &["sala", "cucina"]);
        assert_eq!(menu.rooms[0].area_id, "sala");
        assert_eq!(menu.rooms[1].area_id, "cucina");
    }

    #[test]
    fn entita_senza_area_nella_stanza_altro() {
        let stati = [stato("light.a"), stato("light.b")];
        let aree = [area("sala", "Sala")];
        let voci = [voce("light.a", Some("sala")), voce("light.b", None)];
        let menu = mappa(&stati, &aree, &voci, None, &[], &[], &[]);
        let ultima = menu.rooms.last().unwrap();
        assert_eq!(ultima.area_id, "");
        assert_eq!(ultima.name, "Altro");
        assert_eq!(ids(&ultima.entities), vec!["light.b"]);
    }

    #[test]
    fn unita_fahrenheit() {
        let cfg = config("°F");
        let menu = mappa(&[], &[], &[], Some(&cfg), &[], &[], &[]);
        assert_eq!(menu.temp_unit, "°F");
    }
}