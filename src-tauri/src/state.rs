/// Shared mutable application state, held behind `Arc<Mutex<AppState>>`.
use tauri::PhysicalPosition;

use crate::ha::client::HaClientHandle;
use crate::store::StoreWrapper;
use crate::types::{AppEntity, MenuData};

pub struct AppState {
    /// Persistent store — access via the outer `Arc<Mutex<AppState>>` lock.
    pub store: StoreWrapper,
    pub ha_handle: Option<HaClientHandle>,
    pub menu_data: Option<MenuData>,
    pub all_entities: Vec<AppEntity>,
    /// "connected" | "connecting" | "disconnected" | "error"
    pub status: String,
    /// Last known tray icon position, used to reposition the popup.
    pub last_tray_pos: Option<PhysicalPosition<i32>>,
}

impl AppState {
    pub fn new(store: StoreWrapper) -> Self {
        Self {
            store,
            ha_handle: None,
            menu_data: None,
            all_entities: Vec::new(),
            status: "disconnected".to_owned(),
            last_tray_pos: None,
        }
    }
}
