/// refresh_menu_data — fetches all HA API data in parallel, maps entities,
/// stores the result in AppState and broadcasts `menu:update`.
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager};

use crate::ha::mapper::EntityMapper;
use crate::state::AppState;
use crate::types::{AppEntity, MenuData};

/// Called after `auth_ok` and after debounced `state_changed` events.
pub async fn trigger_refresh(app: &AppHandle) {
    let state_arc = {
        let s = app.state::<Arc<Mutex<AppState>>>();
        Arc::clone(&s)
    };

    // Snapshot all store values and the HA handle while holding the mutex briefly.
    let (ha_handle, cameras_enabled, favorites, hidden, room_order, device_orders) = {
        let guard = state_arc.lock().unwrap();
        let ha_handle = guard.ha_handle.clone();
        let cameras_enabled = guard.store.get_cameras_enabled();
        let favorites = guard.store.get_favorites();
        let hidden = guard.store.get_hidden_entities();
        let room_order = guard.store.get_room_order();
        let device_orders = guard.store.get_device_order_all();
        drop(guard);
        (ha_handle, cameras_enabled, favorites, hidden, room_order, device_orders)
    };

    let handle = match ha_handle {
        Some(h) => h,
        None => return,
    };

    // Fetch all data in parallel — no mutex held during await.
    let (states_r, areas_r, devices_r, entries_r, config_r, notif_r) = tokio::join!(
        handle.get_states(),
        handle.get_areas(),
        handle.get_devices(),
        handle.get_entities(),
        handle.get_config(),
        handle.get_notifications(),
    );

    let states = match states_r {
        Ok(v) => v,
        Err(e) => {
            log::warn!("[refresh] get_states failed: {e}");
            return;
        }
    };
    let areas = areas_r.unwrap_or_default();
    let devices = devices_r.unwrap_or_default();
    let entries = entries_r.unwrap_or_default();
    let config = config_r.ok();
    let notification_count = notif_r.unwrap_or_default().len() as u32;

    // Build MenuData (pure computation, no mutex).
    let menu_data: MenuData = {
        let mut mapper = EntityMapper::new();
        mapper.map(
            &states,
            &areas,
            &devices,
            &entries,
            config.as_ref(),
            notification_count,
            cameras_enabled,
            &favorites,
            &hidden,
            &room_order,
            &device_orders,
        )
    };

    // Build a deduped flat list: rooms + scenes + cameras.
    let mut seen: HashSet<String> = HashSet::new();
    let mut all_entities: Vec<AppEntity> = Vec::new();

    for room in &menu_data.rooms {
        for entity in &room.entities {
            if seen.insert(entity.entity_id().to_owned()) {
                all_entities.push(entity.clone());
            }
        }
    }
    for scene in &menu_data.scenes {
        if seen.insert(scene.base.entity_id.clone()) {
            all_entities.push(AppEntity::Scene(scene.clone()));
        }
    }
    for camera in &menu_data.cameras {
        if seen.insert(camera.base.entity_id.clone()) {
            all_entities.push(AppEntity::Camera(camera.clone()));
        }
    }

    // Write back into state.
    {
        let mut guard = state_arc.lock().unwrap();
        guard.menu_data = Some(menu_data.clone());
        guard.all_entities = all_entities;
    }

    let _ = app.emit("menu:update", &menu_data);
}
