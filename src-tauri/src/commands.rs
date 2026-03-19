/// All Tauri commands exposed to the frontend.
/// Each command receives `State<Arc<Mutex<AppState>>>` and/or `AppHandle`.
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::ha::client::{spawn_ha_task, test_connection as ha_test_connection};
use crate::state::AppState;
use crate::types::{AppConfig, AppEntity, HaEnvironment, MenuData, TestConnectionResult};

type AppStateArg<'a> = State<'a, Arc<Mutex<AppState>>>;

// ─── HA connection ────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn reconnect_ha(
    state: AppStateArg<'_>,
    app: AppHandle,
) -> Result<(), String> {
    connect_ha_internal(&state, &app).await;
    Ok(())
}

#[tauri::command]
pub async fn disconnect_ha(
    state: AppStateArg<'_>,
    app: AppHandle,
) -> Result<(), String> {
    // Take and disconnect the HA handle.
    let handle = {
        let mut guard = state.lock().unwrap();
        guard.ha_handle.take()
    };
    if let Some(h) = handle {
        h.disconnect().await;
    }
    // Deactivate env without deleting credentials.
    {
        let mut guard = state.lock().unwrap();
        guard.store.set_active_environment_id(None);
        guard.menu_data = None;
        guard.all_entities.clear();
        guard.status = "disconnected".to_owned();
    }
    let _ = app.emit("ha:statusChange", "disconnected");
    let _ = app.emit("menu:update", Option::<MenuData>::None);
    Ok(())
}

#[tauri::command]
pub async fn get_status(state: AppStateArg<'_>) -> Result<String, String> {
    Ok(state.lock().unwrap().status.clone())
}

#[tauri::command]
pub async fn call_service(
    state: AppStateArg<'_>,
    domain: String,
    service: String,
    entity_id: String,
    service_data: Option<Value>,
) -> Result<(), String> {
    let handle = state
        .lock()
        .unwrap()
        .ha_handle
        .clone()
        .ok_or_else(|| "Not connected".to_owned())?;

    handle
        .call_service(domain, service, entity_id, service_data)
        .await
}

#[tauri::command]
pub async fn get_camera_snapshot(
    state: AppStateArg<'_>,
    entity_id: String,
) -> Result<Option<String>, String> {
    let (handle, ha_url, token) = {
        let guard = state.lock().unwrap();
        (
            guard.ha_handle.clone(),
            guard.store.get_ha_url(),
            guard.store.get_ha_token(),
        )
    };
    let handle = match handle {
        Some(h) => h,
        None => return Ok(None),
    };
    Ok(handle.get_camera_snapshot(entity_id, ha_url, token).await)
}

#[tauri::command]
pub async fn test_connection(url: String, token: String) -> TestConnectionResult {
    ha_test_connection(url, token).await
}

// ─── Menu data ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_menu_data(state: AppStateArg<'_>) -> Result<Option<MenuData>, String> {
    Ok(state.lock().unwrap().menu_data.clone())
}

// ─── App config ───────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_config(state: AppStateArg<'_>) -> Result<AppConfig, String> {
    let guard = state.lock().unwrap();
    Ok(AppConfig {
        ha_url: guard.store.get_ha_url(),
        ha_token: guard.store.get_ha_token(),
        cameras_enabled: guard.store.get_cameras_enabled(),
        launch_at_login: guard.store.get_launch_at_login(),
        active_env_id: guard.store.get_active_environment_id(),
    })
}

#[tauri::command]
pub async fn set_ha_credentials(
    state: AppStateArg<'_>,
    app: AppHandle,
    url: String,
    token: String,
) -> Result<(), String> {
    {
        let guard = state.lock().unwrap();
        guard.store.set_ha_credentials(&url, &token);
    }
    connect_ha_internal(&state, &app).await;
    Ok(())
}

#[tauri::command]
pub async fn set_launch_at_login(
    state: AppStateArg<'_>,
    app: AppHandle,
    enabled: bool,
) -> Result<(), String> {
    {
        let guard = state.lock().unwrap();
        guard.store.set_launch_at_login(enabled);
    }
    apply_autostart(&app, enabled).await;
    Ok(())
}

#[tauri::command]
pub async fn set_cameras_enabled(
    state: AppStateArg<'_>,
    app: AppHandle,
    enabled: bool,
) -> Result<(), String> {
    {
        let guard = state.lock().unwrap();
        guard.store.set_cameras_enabled(enabled);
    }
    crate::refresh::trigger_refresh(&app).await;
    Ok(())
}

#[tauri::command]
pub async fn config_disconnect(
    state: AppStateArg<'_>,
    app: AppHandle,
) -> Result<(), String> {
    disconnect_ha(state, app).await
}

// ─── Environments ─────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn environments_get_all(state: AppStateArg<'_>) -> Result<Vec<HaEnvironment>, String> {
    Ok(state.lock().unwrap().store.get_environments())
}

#[tauri::command]
pub async fn environments_get_active_id(state: AppStateArg<'_>) -> Result<Option<String>, String> {
    Ok(state.lock().unwrap().store.get_active_environment_id())
}

#[tauri::command]
pub async fn environments_add(
    state: AppStateArg<'_>,
    name: String,
    url: String,
    token: String,
) -> Result<HaEnvironment, String> {
    Ok(state
        .lock()
        .unwrap()
        .store
        .add_environment(&name, &url, &token))
}

#[tauri::command]
pub async fn environments_update(
    state: AppStateArg<'_>,
    env: HaEnvironment,
) -> Result<(), String> {
    state.lock().unwrap().store.update_environment(env);
    Ok(())
}

#[tauri::command]
pub async fn environments_remove(
    state: AppStateArg<'_>,
    id: String,
) -> Result<(), String> {
    state.lock().unwrap().store.remove_environment(&id);
    Ok(())
}

#[tauri::command]
pub async fn environments_connect(
    state: AppStateArg<'_>,
    app: AppHandle,
    id: String,
) -> Result<(), String> {
    {
        let guard = state.lock().unwrap();
        guard.store.set_active_environment_id(Some(&id));
    }
    connect_ha_internal(&state, &app).await;
    Ok(())
}

// ─── Favorites ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn favorites_get(state: AppStateArg<'_>) -> Result<Vec<String>, String> {
    Ok(state.lock().unwrap().store.get_favorites())
}

#[tauri::command]
pub async fn favorites_add(state: AppStateArg<'_>, id: String) -> Result<(), String> {
    state.lock().unwrap().store.add_favorite(&id);
    Ok(())
}

#[tauri::command]
pub async fn favorites_remove(state: AppStateArg<'_>, id: String) -> Result<(), String> {
    state.lock().unwrap().store.remove_favorite(&id);
    Ok(())
}

// ─── Accessories (entity/room management) ────────────────────────────────────

#[tauri::command]
pub async fn accessories_get_all(state: AppStateArg<'_>) -> Result<Vec<AppEntity>, String> {
    Ok(state.lock().unwrap().all_entities.clone())
}

#[tauri::command]
pub async fn accessories_get_rooms(state: AppStateArg<'_>) -> Result<Vec<RoomSummary>, String> {
    Ok(state
        .lock()
        .unwrap()
        .menu_data
        .as_ref()
        .map(|md| {
            md.rooms
                .iter()
                .map(|r| RoomSummary {
                    area_id: r.area_id.clone(),
                    name: r.name.clone(),
                })
                .collect()
        })
        .unwrap_or_default())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomSummary {
    pub area_id: String,
    pub name: String,
}

#[tauri::command]
pub async fn accessories_get_hidden(state: AppStateArg<'_>) -> Result<Vec<String>, String> {
    Ok(state.lock().unwrap().store.get_hidden_entities())
}

#[tauri::command]
pub async fn accessories_set_hidden(
    state: AppStateArg<'_>,
    ids: Vec<String>,
) -> Result<(), String> {
    state.lock().unwrap().store.set_hidden_entities(ids);
    Ok(())
}

#[tauri::command]
pub async fn accessories_get_room_order(state: AppStateArg<'_>) -> Result<Vec<String>, String> {
    Ok(state.lock().unwrap().store.get_room_order())
}

#[tauri::command]
pub async fn accessories_set_room_order(
    state: AppStateArg<'_>,
    ids: Vec<String>,
) -> Result<(), String> {
    state.lock().unwrap().store.set_room_order(ids);
    Ok(())
}

#[tauri::command]
pub async fn accessories_get_device_order(
    state: AppStateArg<'_>,
    area_id: String,
) -> Result<Vec<String>, String> {
    Ok(state.lock().unwrap().store.get_device_order(&area_id))
}

#[tauri::command]
pub async fn accessories_set_device_order(
    state: AppStateArg<'_>,
    area_id: String,
    ids: Vec<String>,
) -> Result<(), String> {
    state.lock().unwrap().store.set_device_order(&area_id, ids);
    Ok(())
}

#[tauri::command]
pub async fn accessories_get_area_icons(
    state: AppStateArg<'_>,
) -> Result<std::collections::HashMap<String, String>, String> {
    Ok(state.lock().unwrap().store.get_area_icons())
}

#[tauri::command]
pub async fn accessories_set_area_icon(
    state: AppStateArg<'_>,
    area_id: String,
    icon: Option<String>,
) -> Result<(), String> {
    state
        .lock()
        .unwrap()
        .store
        .set_area_icon(&area_id, icon.as_deref());
    Ok(())
}

#[tauri::command]
pub async fn accessories_get_favorites(state: AppStateArg<'_>) -> Result<Vec<String>, String> {
    Ok(state.lock().unwrap().store.get_favorites())
}

#[tauri::command]
pub async fn accessories_set_favorites_order(
    state: AppStateArg<'_>,
    ids: Vec<String>,
) -> Result<(), String> {
    state.lock().unwrap().store.set_favorites(ids);
    Ok(())
}

// ─── Window management ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn window_hide(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("popup") {
        w.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn window_quit(app: AppHandle) -> Result<(), String> {
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub async fn window_open_settings(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("settings") {
        w.show().map_err(|e| e.to_string())?;
        w.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }
    tauri::WebviewWindowBuilder::new(
        &app,
        "settings",
        tauri::WebviewUrl::App("settings/index.html".into()),
    )
    .title("ItsyHome Settings")
    .inner_size(640.0, 500.0)
    .min_inner_size(520.0, 400.0)
    .build()
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn window_close_settings(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("settings") {
        w.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn window_open_ha_url(state: AppStateArg<'_>, app: AppHandle) -> Result<(), String> {
    let url = state.lock().unwrap().store.get_ha_url();
    if !url.is_empty() {
        use tauri_plugin_opener::OpenerExt as _;
        app.opener().open_url(&url, None::<&str>).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Resize the popup window height and reposition relative to the tray anchor.
#[tauri::command]
pub async fn window_resize(
    state: AppStateArg<'_>,
    app: AppHandle,
    h: u32,
    _w: Option<u32>,
) -> Result<(), String> {
    let popup = match app.get_webview_window("popup") {
        Some(win) => win,
        None => return Ok(()),
    };

    // Skip entirely while hidden — background JS (HA refresh every ~30 s) keeps
    // calling resize(); mutating size/position while hidden lets DWM subtly
    // adjust the window origin, accumulating rightward drift over time.
    if !popup.is_visible().unwrap_or(true) {
        return Ok(());
    }

    let new_h = h.max(120).min(680);

    // Detail panel is now an overlay — popup width is always 300 logical px.
    popup
        .set_size(tauri::LogicalSize::new(300.0_f64, new_h as f64))
        .map_err(|e| e.to_string())?;

    // Recalculate position from the stored tray anchor so the popup stays
    // anchored above/below the taskbar correctly after every height change.
    reposition_from_tray(&state, &popup)?;

    Ok(())
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Reconnect to HA using the currently active environment's credentials.
pub async fn connect_ha_internal(state: &Arc<Mutex<AppState>>, app: &AppHandle) {
    // Disconnect any existing session first.
    let old_handle = {
        let mut guard = state.lock().unwrap();
        guard.ha_handle.take()
    };
    if let Some(h) = old_handle {
        h.disconnect().await;
    }

    let (url, token) = {
        let guard = state.lock().unwrap();
        (guard.store.get_ha_url(), guard.store.get_ha_token())
    };

    if url.is_empty() || token.is_empty() {
        return;
    }

    {
        let mut guard = state.lock().unwrap();
        guard.status = "connecting".to_owned();
    }
    let _ = app.emit("ha:statusChange", "connecting");

    let handle = spawn_ha_task(url, token, app.clone());

    {
        let mut guard = state.lock().unwrap();
        guard.ha_handle = Some(handle);
    }
}

/// Apply autostart setting via tauri-plugin-autostart.
async fn apply_autostart(app: &AppHandle, enabled: bool) {
    use tauri_plugin_autostart::ManagerExt as _;
    let autostart = app.autolaunch();
    if enabled {
        let _ = autostart.enable();
    } else {
        let _ = autostart.disable();
    }
}


/// Recalculate and apply the popup's screen position from the stored tray
/// anchor and the known 300 px logical width.  Reads the current physical
/// height so Y stays correct after every height-only resize.
/// Uses the deterministic tray-anchor formula for X — never reads outer_pos.x
/// which can accumulate DWM micro-adjustment drift.
fn reposition_from_tray(
    state: &Arc<Mutex<AppState>>,
    popup: &tauri::WebviewWindow,
) -> Result<(), String> {
    let tray_pos = match state.lock().unwrap().last_tray_pos {
        Some(p) => p,
        None => return Ok(()),
    };
    let monitor = popup
        .current_monitor()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No monitor found".to_owned())?;
    let scale        = popup.scale_factor().unwrap_or(1.0);
    let known_phys_w = (300.0_f64 * scale).round() as u32;
    let phys_h       = popup.outer_size().map(|s| s.height).unwrap_or(120);
    let pos = calc_popup_position(
        tray_pos,
        tauri::PhysicalSize::new(known_phys_w, phys_h),
        &monitor,
    );
    popup.set_position(pos).map_err(|e| e.to_string())
}

/// Compute popup position centred horizontally on the tray icon, anchored
/// above or below depending on which half of the screen the tray is in.
pub fn calc_popup_position(
    tray_pos: tauri::PhysicalPosition<i32>,
    win_size: tauri::PhysicalSize<u32>,
    monitor: &tauri::Monitor,
) -> tauri::PhysicalPosition<i32> {
    let work = monitor.work_area();
    let mon_h = monitor.size().height as i32;

    let mut x = tray_pos.x - (win_size.width / 2) as i32;
    let mut y = if tray_pos.y > mon_h / 2 {
        // Taskbar at bottom — popup goes above tray icon
        tray_pos.y - win_size.height as i32 - 4
    } else {
        // Taskbar at top — popup goes below tray icon
        tray_pos.y + 24
    };

    // Clamp within work area with 4 px margin.
    let min_x = work.position.x + 4;
    let max_x = work.position.x + work.size.width as i32 - win_size.width as i32 - 4;
    let min_y = work.position.y + 4;
    let max_y = work.position.y + work.size.height as i32 - win_size.height as i32 - 4;

    x = x.clamp(min_x, max_x.max(min_x));
    y = y.clamp(min_y, max_y.max(min_y));

    tauri::PhysicalPosition::new(x, y)
}
