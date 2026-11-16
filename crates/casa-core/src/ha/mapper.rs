/// EntityMapper — converts raw HA API data into typed AppEntity objects.
/// This is a faithful Rust port of `entity-mapper.ts`.
use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::models::{HaArea, HaConfig, HaDevice, HaEntityEntry, HaState};
use crate::types::{
    AlarmEntity, AppEntity, BaseEntity, CameraEntity, ClimateEntity, CoverEntity, FanEntity,
    HumidifierEntity, LightEntity, LockEntity, MenuData, Room, SceneEntity, SensorEntity,
    StanzaCatalogo, SwitchEntity, ValveEntity, VoceCatalogo, GRUPPO_SCENE,
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

/// Dati grezzi letti da Home Assistant, da mappare.
pub struct DatiHa<'a> {
    pub states: &'a [HaState],
    pub areas: &'a [HaArea],
    pub devices: &'a [HaDevice],
    pub entries: &'a [HaEntityEntry],
    pub config: Option<&'a HaConfig>,
    pub notification_count: u32,
}

/// Preferenze dell'utente che decidono ordine e visibilità.
pub struct Preferenze<'a> {
    pub favorites_order: &'a [String],
    pub hidden_entities: &'a [String],
    pub room_order: &'a [String],
    pub device_orders: &'a HashMap<String, Vec<String>>,
}

/// I registri di Home Assistant (stanze, dispositivi, entità) indicizzati per id. Il servizio li
/// costruisce a ogni lettura dei registri e li tiene: servono alla mappatura completa e a quella
/// di una sola entità quando ne cambia lo stato (`EntityMapper::mappa_entita`).
#[derive(Debug, Default)]
pub struct Indici {
    entries: HashMap<String, HaEntityEntry>,
    devices: HashMap<String, HaDevice>,
    areas: HashMap<String, HaArea>,
}

impl Indici {
    pub fn new(areas: Vec<HaArea>, devices: Vec<HaDevice>, entries: Vec<HaEntityEntry>) -> Indici {
        Indici {
            entries: entries.into_iter().map(|e| (e.entity_id.clone(), e)).collect(),
            devices: devices.into_iter().map(|d| (d.id.clone(), d)).collect(),
            areas: areas.into_iter().map(|a| (a.area_id.clone(), a)).collect(),
        }
    }
}

impl Indici {
    /// Le entità del registro che l'app mostrerebbe appena avranno uno stato (anche se adesso
    /// non ne hanno: integrazione non ancora caricata dopo un riavvio di HA, ricaricamento): stesso
    /// filtro di `mappa_entita` per dominio, disattivate, nascoste e categorie; per i sensori conta
    /// la classe del registro (scelta dall'utente o data dall'integrazione).
    pub fn mostrabili_dal_registro(&self) -> impl Iterator<Item = &str> {
        self.entries.values().filter(|e| registro_mostrabile(e)).map(|e| e.entity_id.as_str())
    }

    /// La voce del registro di un'entità.
    pub fn voce(&self, entity_id: &str) -> Option<&HaEntityEntry> {
        self.entries.get(entity_id)
    }

    /// Le voci delle entità di un dispositivo.
    pub fn voci_del_dispositivo<'a>(&'a self, device_id: &'a str) -> impl Iterator<Item = &'a HaEntityEntry> + 'a {
        self.entries.values().filter(move |e| e.device_id.as_deref() == Some(device_id))
    }

    /// La stanza c'è nel registro.
    pub fn ha_area(&self, area_id: &str) -> bool {
        self.areas.contains_key(area_id)
    }
}

fn registro_mostrabile(e: &HaEntityEntry) -> bool {
    let dominio = domain_of(&e.entity_id);
    if dominio != "camera" && (e.disabled_by.is_some() || e.hidden_by.is_some() || e.entity_category.is_some()) {
        return false;
    }
    match dominio {
        "light" | "switch" | "input_boolean" | "climate" | "cover" | "lock" | "fan" | "humidifier" | "valve"
        | "alarm_control_panel" | "camera" | "scene" => true,
        "sensor" => matches!(
            e.device_class.as_deref().or(e.original_device_class.as_deref()),
            Some("temperature" | "humidity")
        ),
        _ => false,
    }
}

/// Un aggiornamento in attesa (`update.*` acceso): `MenuData::update_count` li conta.
pub fn aggiornamento_disponibile(state: &HaState) -> bool {
    domain_of(&state.entity_id) == "update" && state.state == "on"
}

/// Mette `nuova` al posto dell'entità con lo stesso id in preferiti, stanze, scene e telecamere,
/// senza toccare l'ordine. Vale solo se l'ordine non può cambiare: stesso nome (le stanze senza
/// ordine salvato vanno per nome), stessa stanza, stessa presenza. Vero se l'ha trovata almeno
/// una volta (un'entità nascosta dall'utente non c'è).
pub fn sostituisci_in_menu(menu: &mut MenuData, nuova: &AppEntity) -> bool {
    let id = nuova.entity_id();
    let mut trovata = false;
    let liste = menu
        .favorites
        .iter_mut()
        .chain(menu.rooms.iter_mut().flat_map(|r| r.entities.iter_mut()))
        .chain(menu.cameras.iter_mut());
    for e in liste.filter(|e| e.entity_id() == id) {
        *e = nuova.clone();
        trovata = true;
    }
    if let AppEntity::Scene(s) = nuova {
        for vecchia in menu.scenes.iter_mut().filter(|v| v.base.entity_id == id) {
            *vecchia = s.clone();
            trovata = true;
        }
    }
    trovata
}

impl Default for EntityMapper {
    fn default() -> Self {
        Self::new()
    }
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
    pub fn map(&mut self, ha: DatiHa<'_>, preferenze: Preferenze<'_>) -> MenuData {
        let DatiHa { states, areas, devices, entries, config, notification_count } = ha;
        let indici = Indici::new(areas.to_vec(), devices.to_vec(), entries.to_vec());
        self.map_indicizzato(&indici, states, config, notification_count, preferenze)
    }

    /// Come `map`, con i registri già indicizzati (il servizio li tiene fra una lettura e l'altra).
    pub fn map_indicizzato(
        &mut self,
        indici: &Indici,
        states: &[HaState],
        ha_config: Option<&HaConfig>,
        notification_count: u32,
        preferenze: Preferenze<'_>,
    ) -> MenuData {
        let Preferenze { favorites_order, hidden_entities, room_order, device_orders } = preferenze;
        if let Some(cfg) = ha_config {
            self.set_temp_unit(&cfg.unit_system.temperature);
        }
        let area_map = &indici.areas;

        let mut update_count: u32 = 0;
        let mut all_entities: Vec<AppEntity> = Vec::new();
        let mut scenes: Vec<SceneEntity> = Vec::new();
        let mut cameras: Vec<AppEntity> = Vec::new();

        for state in states {
            // Count pending updates regardless of filtering
            if aggiornamento_disponibile(state) {
                update_count += 1;
            }

            let Some(entity) = self.mappa_entita(indici, state) else {
                continue;
            };

            match &entity {
                AppEntity::Scene(s) => scenes.push(s.clone()),
                AppEntity::Camera(_) => cameras.push(entity.clone()),
                _ => all_entities.push(entity),
            }
        }

        // Le scene nell'ordine salvato per il loro gruppo (Impostazioni › Dispositivi), le altre
        // per nome.
        let scenes: Vec<SceneEntity> = {
            let ordine = device_orders.get(GRUPPO_SCENE).map(|v| v.as_slice()).unwrap_or(&[]);
            let tutte: Vec<AppEntity> = scenes.into_iter().map(AppEntity::Scene).collect();
            Self::ordered_entities(GRUPPO_SCENE, tutte, ordine)
                .into_iter()
                .filter_map(|e| match e {
                    AppEntity::Scene(s) => Some(s),
                    _ => None,
                })
                .collect()
        };
        let catalogo = Self::catalogo(area_map, &all_entities, &scenes, room_order, device_orders);

        // Hidden / favorites sets
        let hidden_set: HashSet<&str> = hidden_entities.iter().map(|s| s.as_str()).collect();
        let fav_set: HashSet<&str> = favorites_order.iter().map(|s| s.as_str()).collect();

        let visible: Vec<&AppEntity> = all_entities
            .iter()
            .filter(|e| !hidden_set.contains(e.entity_id()))
            .collect();

        // Le scene possono stare fra i preferiti (v3); per il resto restano solo in `scenes`,
        // mai nelle stanze, come nella v2.
        let scene_preferite: Vec<AppEntity> = scenes
            .iter()
            .filter(|s| fav_set.contains(s.base.entity_id.as_str()) && !hidden_set.contains(s.base.entity_id.as_str()))
            .map(|s| AppEntity::Scene(s.clone()))
            .collect();
        let candidati: Vec<&AppEntity> = visible.into_iter().chain(scene_preferite.iter()).collect();

        let fav_map: HashMap<&str, &AppEntity> = candidati
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
            let mut extra: Vec<AppEntity> = candidati
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
            let na = area_map.get(*a).map(|ar| ar.name.as_str()).unwrap_or(a);
            let nb = area_map.get(*b).map(|ar| ar.name.as_str()).unwrap_or(b);
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
            let area = match area_map.get(*area_id) {
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
            catalogo: std::sync::Arc::new(catalogo),
        }
    }

    /// Il catalogo per la gestione: le stanze come `rooms` (stesso ordine, stesso «Altro»), ma
    /// con tutte le entità, anche quelle nascoste dall'utente, e anche le stanze che lui ha
    /// nascosto per intero; in fondo il gruppo delle scene.
    fn catalogo(
        area_map: &HashMap<String, HaArea>,
        entita: &[AppEntity],
        scene: &[SceneEntity],
        room_order: &[String],
        device_orders: &HashMap<String, Vec<String>>,
    ) -> Vec<StanzaCatalogo> {
        let mut per_stanza: HashMap<&str, Vec<AppEntity>> = HashMap::new();
        for e in entita {
            // Come `rooms`: una stanza che il registro non ha più non si mostra, e nemmeno qui.
            let area = entity_base_area_id(e).unwrap_or("");
            if area.is_empty() || area_map.contains_key(area) {
                per_stanza.entry(area).or_default().push(e.clone());
            }
        }
        let salvate: HashSet<&str> = room_order.iter().map(String::as_str).collect();
        let mut altre: Vec<&str> =
            per_stanza.keys().copied().filter(|a| !a.is_empty() && !salvate.contains(a)).collect();
        altre.sort_by(|a, b| area_map[*a].name.cmp(&area_map[*b].name));
        let ordine: Vec<&str> = room_order
            .iter()
            .map(String::as_str)
            .filter(|a| !a.is_empty() && per_stanza.contains_key(a))
            .chain(altre)
            .chain(per_stanza.contains_key("").then_some(""))
            .collect();
        let voce = |e: &AppEntity| VoceCatalogo {
            entity_id: e.entity_id().to_owned(),
            name: e.name().to_owned(),
            kind: tipo_di(e).to_owned(),
        };
        let mut catalogo: Vec<StanzaCatalogo> = ordine
            .into_iter()
            .map(|area| {
                let salvato = device_orders.get(area).map(|v| v.as_slice()).unwrap_or(&[]);
                let ordinate = Self::ordered_entities(area, per_stanza.remove(area).unwrap_or_default(), salvato);
                StanzaCatalogo {
                    area_id: area.to_owned(),
                    name: if area.is_empty() { "Altro".to_owned() } else { area_map[area].name.clone() },
                    voci: ordinate.iter().map(voce).collect(),
                }
            })
            .collect();
        if !scene.is_empty() {
            catalogo.push(StanzaCatalogo {
                area_id: GRUPPO_SCENE.to_owned(),
                name: "Scene".to_owned(),
                voci: scene.iter().map(|s| voce(&AppEntity::Scene(s.clone()))).collect(),
            });
        }
        catalogo
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

    /// Una sola entità come la mostrerebbe `map` (prima di nascosti e preferiti, che dipendono
    /// dall'utente): `None` se l'app non la mostra mai (dominio escluso, entità disattivata,
    /// nascosta o di configurazione/diagnostica nel registro, sensore che non è temperatura o
    /// umidità). Due risultati uguali per due stati della stessa entità vogliono dire che il
    /// cambio non si vede.
    pub fn mappa_entita(&self, indici: &Indici, state: &HaState) -> Option<AppEntity> {
        let domain = domain_of(&state.entity_id);
        if EXCLUDED_DOMAINS.contains(&domain) {
            return None;
        }
        let entry = indici.entries.get(state.entity_id.as_str());
        // Skip disabled / hidden / config entities.
        // Exception: cameras always pass — HA sometimes tags them with
        // entity_category but they are explicitly user-facing.
        if domain != "camera" {
            if let Some(entry) = entry {
                if entry.disabled_by.is_some() || entry.hidden_by.is_some() || entry.entity_category.is_some() {
                    return None;
                }
            }
        }
        let area_id = entry.and_then(|entry| {
            if let Some(aid) = entry.area_id.as_ref().filter(|a| !a.is_empty()) {
                return Some(aid.clone());
            }
            let did = entry.device_id.as_ref()?;
            indici.devices.get(did.as_str())?.area_id.clone()
        });
        self.map_entity(state, entry, area_id)
    }

    /// Imposta l'unità di temperatura da `get_config` (per `mappa_entita` fuori da `map`).
    pub fn usa_config(&mut self, config: Option<&HaConfig>) {
        if let Some(cfg) = config {
            self.set_temp_unit(&cfg.unit_system.temperature);
        }
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

/// Il tipo di un'entità come nel tag di `AppEntity`.
fn tipo_di(e: &AppEntity) -> &'static str {
    match e {
        AppEntity::Light(_) => "light",
        AppEntity::Switch(_) => "switch",
        AppEntity::Climate(_) => "climate",
        AppEntity::Cover(_) => "cover",
        AppEntity::Lock(_) => "lock",
        AppEntity::Fan(_) => "fan",
        AppEntity::Humidifier(_) => "humidifier",
        AppEntity::Valve(_) => "valve",
        AppEntity::Sensor(_) => "sensor",
        AppEntity::Alarm(_) => "alarm",
        AppEntity::Camera(_) => "camera",
        AppEntity::Scene(_) => "scene",
    }
}

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
            device_class: None,
            original_device_class: None,
        }
    }

    fn area(id: &str, nome: &str) -> HaArea {
        HaArea { area_id: id.to_owned(), name: nome.to_owned() }
    }

    fn config(unita: &str) -> HaConfig {
        HaConfig {
            unit_system: super::super::models::HaUnitSystem { temperature: unita.to_owned() },
            location_name: "Casa".to_owned(),
            state: None,
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
        let (preferiti, nascosti, ordine_stanze) = (s(preferiti), s(nascosti), s(ordine_stanze));
        EntityMapper::new().map(
            DatiHa {
                states: stati,
                areas: aree,
                devices: &[],
                entries: voci,
                config: cfg,
                notification_count: 0,
            },
            Preferenze {
                favorites_order: &preferiti,
                hidden_entities: &nascosti,
                room_order: &ordine_stanze,
                device_orders: &HashMap::new(),
            },
        )
    }

    /// `mappa_entita` dà la stessa entità che `map` mette nelle stanze, e `None` per ciò che `map` scarta.
    #[test]
    fn una_entita_come_nella_mappatura_completa() {
        let mut nascosta = voce("switch.nascosta", Some("sala"));
        nascosta.hidden_by = Some("user".into());
        let mut diagnostica = voce("sensor.segnale", Some("sala"));
        diagnostica.entity_category = Some("diagnostic".into());
        let mut temperatura = stato("sensor.t");
        temperatura.state = "21.5".into();
        temperatura.attributes = json!({ "device_class": "temperature", "unit_of_measurement": "°C" });
        let stati = [stato("light.a"), stato("switch.nascosta"), stato("sensor.segnale"), stato("binary_sensor.p"), temperatura];
        let aree = [area("sala", "Sala")];
        let voci = [voce("light.a", Some("sala")), nascosta, diagnostica, voce("sensor.t", Some("sala"))];
        let menu = mappa(&stati, &aree, &voci, None, &[], &[], &[]);
        let indici = Indici::new(aree.to_vec(), vec![], voci.to_vec());
        let m = EntityMapper::new();
        let nelle_stanze: Vec<&AppEntity> = menu.rooms.iter().flat_map(|r| &r.entities).collect();
        for s in &stati {
            let una = m.mappa_entita(&indici, s);
            assert_eq!(una.as_ref(), nelle_stanze.iter().copied().find(|e| e.entity_id() == s.entity_id), "{}", s.entity_id);
        }
    }

    #[test]
    fn sostituisce_nei_preferiti_e_nelle_stanze_senza_cambiare_ordine() {
        let stati = [stato("light.a"), stato("light.b")];
        let aree = [area("sala", "Sala")];
        let voci = [voce("light.a", Some("sala")), voce("light.b", Some("sala"))];
        let mut menu = mappa(&stati, &aree, &voci, None, &["light.b"], &[], &[]);
        let indici = Indici::new(aree.to_vec(), vec![], voci.to_vec());
        let mut spenta = stato("light.b");
        spenta.state = "off".into();
        let nuova = EntityMapper::new().mappa_entita(&indici, &spenta).unwrap();
        assert!(sostituisci_in_menu(&mut menu, &nuova));
        assert_eq!(menu.favorites, vec![nuova.clone()]);
        assert_eq!(ids(&menu.rooms[0].entities), vec!["light.a", "light.b"]);
        assert_eq!(menu.rooms[0].entities[1], nuova);
        let fuori = EntityMapper::new().mappa_entita(&indici, &stato("light.zeta")).unwrap();
        assert!(!sostituisci_in_menu(&mut menu, &fuori), "un'entità che non c'è non si aggiunge");
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
    fn scena_preferita_nei_preferiti() {
        let stati = [stato("light.a"), stato("scene.sera"), stato("scene.notte")];
        let menu = mappa(&stati, &[], &[], None, &["scene.sera", "light.a"], &[], &[]);
        assert_eq!(ids(&menu.favorites), vec!["scene.sera", "light.a"]);
        assert!(matches!(menu.favorites[0], AppEntity::Scene(_)));
        // Per il resto come prima: le scene restano in `scenes` e mai nelle stanze. Senza un
        // ordine salvato vanno per nome, come i dispositivi delle stanze (Task 14e).
        let scene: Vec<&str> = menu.scenes.iter().map(|s| s.base.entity_id.as_str()).collect();
        assert_eq!(scene, vec!["scene.notte", "scene.sera"]);
        assert!(menu.rooms.iter().all(|r| r.entities.iter().all(|e| !matches!(e, AppEntity::Scene(_)))));
    }

    #[test]
    fn scena_nascosta_mai_nei_preferiti() {
        let stati = [stato("scene.sera")];
        let menu = mappa(&stati, &[], &[], None, &["scene.sera"], &["scene.sera"], &[]);
        assert!(menu.favorites.is_empty());
        assert!(menu.scenes.is_empty());
    }

    /// Task 14e: il catalogo della gestione ha tutte le entità (anche nascoste) e anche le stanze
    /// nascoste per intero, nell'ordine salvato, con «Altro» e le scene in fondo; le scene seguono
    /// l'ordine salvato per il loro gruppo anche in `scenes`.
    #[test]
    fn catalogo_con_le_nascoste_e_nell_ordine_salvato() {
        let stati = [stato("light.a"), stato("light.b"), stato("switch.c"), stato("light.z"), stato("scene.sera"), stato("scene.alba")];
        let aree = [area("cucina", "Cucina"), area("sala", "Sala")];
        let voci = [voce("light.a", Some("cucina")), voce("light.b", Some("cucina")), voce("switch.c", Some("sala")), voce("light.z", None)];
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let mut ordini = HashMap::new();
        ordini.insert("cucina".to_owned(), s(&["light.b", "light.a"]));
        ordini.insert(GRUPPO_SCENE.to_owned(), s(&["scene.sera"]));
        let (nascosti, ordine) = (s(&["light.a", "switch.c"]), s(&["sala", "cucina"]));
        let menu = EntityMapper::new().map(
            DatiHa { states: &stati, areas: &aree, devices: &[], entries: &voci, config: None, notification_count: 0 },
            Preferenze { favorites_order: &[], hidden_entities: &nascosti, room_order: &ordine, device_orders: &ordini },
        );
        let stanze: Vec<(&str, Vec<&str>)> = menu
            .catalogo
            .iter()
            .map(|st| (st.area_id.as_str(), st.voci.iter().map(|v| v.entity_id.as_str()).collect()))
            .collect();
        assert_eq!(
            stanze,
            [
                ("sala", vec!["switch.c"]),
                ("cucina", vec!["light.b", "light.a"]),
                ("", vec!["light.z"]),
                (GRUPPO_SCENE, vec!["scene.sera", "scene.alba"]),
            ]
        );
        assert_eq!(menu.catalogo[0].name, "Sala");
        assert_eq!(menu.catalogo[1].voci[0].kind, "light");
        // Nelle stanze del pannello la sala (tutta nascosta) non c'è e la luce nascosta nemmeno.
        let nel_pannello: Vec<&str> = menu.rooms.iter().map(|r| r.area_id.as_str()).collect();
        assert_eq!(nel_pannello, ["cucina", ""]);
        assert_eq!(ids(&menu.rooms[0].entities), vec!["light.b"]);
        let scene: Vec<&str> = menu.scenes.iter().map(|s| s.base.entity_id.as_str()).collect();
        assert_eq!(scene, ["scene.sera", "scene.alba"]);
    }

    /// B1 del giro 1: le voci del registro che l'app mostrerà quando avranno uno stato.
    #[test]
    fn mostrabili_dal_registro_anche_senza_stato() {
        let mut nascosta = voce("light.nascosta", None);
        nascosta.hidden_by = Some("user".into());
        let mut disattivata = voce("switch.spenta", None);
        disattivata.disabled_by = Some("integration".into());
        let mut temperatura = voce("sensor.t", None);
        temperatura.original_device_class = Some("temperature".into());
        let mut potenza = voce("sensor.p", None);
        potenza.original_device_class = Some("power".into());
        let mut umidita_utente = voce("sensor.u", None);
        umidita_utente.device_class = Some("humidity".into());
        umidita_utente.original_device_class = Some("moisture".into());
        let mut telecamera = voce("camera.c", None);
        telecamera.entity_category = Some("diagnostic".into());
        let voci = vec![voce("light.a", None), nascosta, disattivata, temperatura, potenza, umidita_utente, telecamera,
                        voce("binary_sensor.b", None), voce("scene.s", None)];
        let indici = Indici::new(vec![], vec![], voci);
        let mut ids: Vec<&str> = indici.mostrabili_dal_registro().collect();
        ids.sort();
        assert_eq!(ids, ["camera.c", "light.a", "scene.s", "sensor.t", "sensor.u"]);
    }

    #[test]
    fn unita_fahrenheit() {
        let cfg = config("°F");
        let menu = mappa(&[], &[], &[], Some(&cfg), &[], &[], &[]);
        assert_eq!(menu.temp_unit, "°F");
    }
}