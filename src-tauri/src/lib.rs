/// ItsyHome Tauri 2.0 backend — library root.
use std::sync::{Arc, Mutex};

use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Listener, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder,
};
use tauri_plugin_autostart::MacosLauncher;

pub mod commands;
pub mod ha;
pub mod notification;
pub mod refresh;
pub mod state;
pub mod store;
pub mod types;

use commands::calc_popup_position;
use state::AppState;
use store::StoreWrapper;

// ─── Library entry point ──────────────────────────────────────────────────────

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_popup(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            // Initialise shared state
            let store = StoreWrapper::new(app.handle().clone());
            let app_state = Arc::new(Mutex::new(AppState::new(store)));
            app.manage(app_state.clone());

            // Start notification webhook server if already registered
            {
                let reg = app_state.lock().unwrap().store.get_notif_registration();
                if let Some(reg) = reg {
                    let push_secret = reg.push_secret.clone();
                    let port = reg.port;
                    let app2 = app.handle().clone();
                    let handle = tauri::async_runtime::spawn(async move {
                        crate::notification::start_webhook_server(port, push_secret, app2).await;
                    });
                    app_state.lock().unwrap().webhook_handle = Some(handle);
                }
            }

            // Create popup window (hidden initially)
            let popup = WebviewWindowBuilder::new(
                app,
                "popup",
                WebviewUrl::App("popup/index.html".into()),
            )
            .title("ItsyHome")
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .visible(false)
            .resizable(true)
            .min_inner_size(300.0, 54.0)
            .inner_size(300.0, 54.0)
            .build()?;

            // Hide popup on blur (only if settings window is not open).
            // Guard: ignore blur events within 500 ms of the popup being shown
            // to avoid it disappearing immediately after a tray-icon click.
            {
                let app_handle = app.handle().clone();
                popup.on_window_event(move |event| {
                    if let tauri::WindowEvent::Focused(false) = event {
                        // Suppress the spurious blur fired right after show().
                        let just_shown = {
                            let s = app_handle.state::<Arc<Mutex<AppState>>>();
                            let shown_at = s.lock().unwrap().popup_shown_at;
                            shown_at
                                .map(|t| t.elapsed() < std::time::Duration::from_millis(500))
                                .unwrap_or(false)
                        };
                        if just_shown {
                            return;
                        }
                        if app_handle.get_webview_window("settings").is_none() {
                            if let Some(w) = app_handle.get_webview_window("popup") {
                                let _ = w.hide();
                            }
                        }
                    }
                });
            }

            // Build tray context menu (right-click)
            let tray_menu = Menu::with_items(app, &[
                &MenuItem::with_id(app, "settings", "Impostazioni", true, None::<&str>)?,
                &PredefinedMenuItem::separator(app)?,
                &MenuItem::with_id(app, "quit", "Esci da ItsyHome", true, None::<&str>)?,
            ])?;

            // Build tray icon
            let tray_icon = load_tray_icon(app.handle(), "default");
            TrayIconBuilder::with_id("tray")
                .icon(tray_icon)
                .tooltip("ItsyHome – Home Assistant")
                .menu(&tray_menu)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "settings" => { open_settings_window(app).ok(); }
                    "quit"     => { app.exit(0); }
                    _          => {}
                })
                .on_tray_icon_event({
                    let app_handle = app.handle().clone();
                    move |_tray, event| {
                        // Only fire on left-button UP to avoid double-toggle
                        // (Tauri fires Click for both MouseDown and MouseUp).
                        if let TrayIconEvent::Click {
                            position,
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event {
                            // Store tray position for popup positioning
                            {
                                let s = app_handle.state::<Arc<Mutex<AppState>>>();
                                let mut guard = s.lock().unwrap();
                                guard.last_tray_pos = Some(PhysicalPosition::new(
                                    position.x as i32,
                                    position.y as i32,
                                ));
                            }
                            toggle_popup(&app_handle);
                        }
                    }
                })
                .build(app)?;

            // Listen for HA status changes to update the tray icon.
            {
                let app_handle = app.handle().clone();
                app.listen("ha:statusChange", move |event: tauri::Event| {
                    // payload is a JSON string like `"connected"`
                    let status: String = serde_json::from_str(event.payload())
                        .unwrap_or_else(|_| "disconnected".to_owned());
                    update_tray_icon(&app_handle, &status);
                });
            }

            // Connect to HA or open settings on first launch
            {
                let has_creds = {
                    let s = app.state::<Arc<Mutex<AppState>>>();
                    let result = s.lock().unwrap().store.has_credentials();
                    result
                };

                let app_handle = app.handle().clone();
                if has_creds {
                    tauri::async_runtime::spawn(async move {
                        commands::connect_ha_internal(
                            &app_handle.state::<Arc<Mutex<AppState>>>(),
                            &app_handle,
                        )
                        .await;
                    });
                } else {
                    open_settings_window(app.handle())?;
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // HA connection
            commands::reconnect_ha,
            commands::disconnect_ha,
            commands::get_status,
            commands::call_service,
            commands::get_camera_snapshot,
            commands::test_connection,
            // Menu data
            commands::get_menu_data,
            // Config
            commands::get_config,
            commands::set_ha_credentials,
            commands::set_launch_at_login,
            commands::set_cameras_enabled,
            commands::config_disconnect,
            // Environments
            commands::environments_get_all,
            commands::environments_get_active_id,
            commands::environments_add,
            commands::environments_update,
            commands::environments_remove,
            commands::environments_connect,
            // Favorites
            commands::favorites_get,
            commands::favorites_add,
            commands::favorites_remove,
            // Accessories
            commands::accessories_get_all,
            commands::accessories_get_rooms,
            commands::accessories_get_hidden,
            commands::accessories_set_hidden,
            commands::accessories_get_room_order,
            commands::accessories_set_room_order,
            commands::accessories_get_device_order,
            commands::accessories_set_device_order,
            commands::accessories_get_area_icons,
            commands::accessories_set_area_icon,
            commands::accessories_get_favorites,
            commands::accessories_set_favorites_order,
            // Window
            commands::window_hide,
            commands::window_quit,
            commands::window_open_settings,
            commands::window_close_settings,
            commands::window_open_ha_url,
            commands::window_resize,
            // Notifications
            commands::notifications_get_status,
            commands::notifications_register,
            commands::notifications_unregister,
            commands::notifications_test,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ItsyHome");
}

// ─── Window helpers ───────────────────────────────────────────────────────────

fn toggle_popup(app: &AppHandle) {
    if let Some(popup) = app.get_webview_window("popup") {
        if popup.is_visible().unwrap_or(false) {
            let _ = popup.hide();
        } else {
            show_popup(app);
        }
    }
}

pub fn show_popup(app: &AppHandle) {
    let popup = match app.get_webview_window("popup") {
        Some(w) => w,
        None => return,
    };

    // Reposition relative to last tray click.
    // Use the known logical width (300px × scale) rather than outer_size() so
    // the X calculation is stable even if the size was changed while hidden.
    if let Some(tray_pos) = app
        .state::<Arc<Mutex<AppState>>>()
        .lock()
        .unwrap()
        .last_tray_pos
    {
        if let Ok(Some(monitor)) = popup.current_monitor() {
            let scale = popup.scale_factor().unwrap_or(1.0);
            let known_phys_w = (300.0_f64 * scale).round() as u32;
            let actual_h    = popup.outer_size().map(|s| s.height).unwrap_or(54);
            let size = tauri::PhysicalSize::new(known_phys_w, actual_h);
            let pos  = calc_popup_position(tray_pos, size, &monitor);
            let _ = popup.set_position(pos);
        }
    }

    // Record the time the popup becomes visible so the blur guard works.
    {
        let s = app.state::<Arc<Mutex<AppState>>>();
        s.lock().unwrap().popup_shown_at = Some(std::time::Instant::now());
    }
    let _ = popup.show();
    let _ = popup.set_focus();
}

fn open_settings_window(app: &AppHandle) -> Result<(), tauri::Error> {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }
    WebviewWindowBuilder::new(
        app,
        "settings",
        WebviewUrl::App("settings/index.html".into()),
    )
    .title("ItsyHome Settings")
    .inner_size(640.0, 500.0)
    .min_inner_size(520.0, 400.0)
    .build()?;
    Ok(())
}

// ─── Status / tray icon updates ───────────────────────────────────────────────

/// Update tray icon and persist status on every HA status change.
/// Called from the `ha:statusChange` event listener wired up in setup.
pub fn update_tray_icon(app: &AppHandle, status: &str) {
    if let Some(tray) = app.tray_by_id("tray") {
        let icon = load_tray_icon(app, status);
        let _ = tray.set_icon(Some(icon));
    }
    // Update the in-memory status field
    let s = app.state::<Arc<Mutex<AppState>>>();
    s.lock().unwrap().status = status.to_owned();
}

// ─── Icon loading ─────────────────────────────────────────────────────────────

fn load_tray_icon(app: &AppHandle, status: &str) -> Image<'static> {
    let name = match status {
        "error" => "tray-error.png",
        "connecting" => "tray-connecting.png",
        _ => "tray-default.png",
    };

    // Try to load from the bundled resources directory
    let resources_dir = app
        .path()
        .resource_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("resources"));
    let path = resources_dir.join(name);

    if let Some(img) = load_png_as_image(&path) {
        return img;
    }

    // Fallback: 16×16 solid colour square
    let (r, g, b): (u8, u8, u8) = match status {
        "error" => (0xFF, 0x45, 0x3A),
        "connecting" => (0xFF, 0x9F, 0x0A),
        _ => (0x03, 0xA9, 0xF4),
    };
    const SIZE: u32 = 16;
    let pixel_count = (SIZE * SIZE) as usize;
    let mut buf: Vec<u8> = Vec::with_capacity(pixel_count * 4);
    for _ in 0..pixel_count {
        buf.push(r);
        buf.push(g);
        buf.push(b);
        buf.push(0xFF);
    }
    // Image::new borrows the slice; we need the owned variant for 'static lifetime.
    // Tauri 2 exposes Image::new_owned on the RGBA buffer.
    Image::new_owned(buf, SIZE, SIZE)
}

/// Decode a PNG file from disk into a Tauri `Image<'static>`.
/// Returns `None` if the file cannot be read or decoded.
fn load_png_as_image(path: &std::path::Path) -> Option<Image<'static>> {
    let img = image::open(path).ok()?.to_rgba8();
    let (width, height) = image::GenericImageView::dimensions(&img);
    Some(Image::new_owned(img.into_raw(), width, height))
}
