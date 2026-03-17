# Changelog

All notable changes to ItsyHome are documented in this file.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

---

## [2.0.0] - 2026-03-17

### Changed (Breaking)
- **Migrated from Electron 33 to Tauri 2.0** — installer size reduced from ~180 MB to ~8 MB (22x smaller)
- Backend rewritten in **Rust** (tokio async, tokio-tungstenite WebSocket, reqwest HTTP)
- IPC layer migrated from Electron `contextBridge` / `ipcRenderer` to Tauri `invoke` / `listen`
- Bundler changed from `electron-vite` to **Vite 6**
- Persistence migrated from `electron-store` to `tauri-plugin-store`
- Autostart migrated from `app.setLoginItemSettings()` to `tauri-plugin-autostart`

### Added
- **Toast notifications** — visual feedback for every action (success and error)
- **Search / filter** (Ctrl+F or 🔍 button) — find any device in the popup by name
- **Keyboard shortcuts** — Escape (close/back), Ctrl+F (open search)
- **Error reporting** — failed Home Assistant service calls shown as error toast
- Modular frontend architecture: `entities.ts` (per-domain card builders), `sections.ts` (section builders)
- Unified IPC adapter `src/lib/api.ts` — single source of truth for all backend calls

### Performance
| Metric | v1.x (Electron) | v2.0 (Tauri) | Improvement |
|---|---|---|---|
| Installer size | ~180 MB | ~8 MB | 22x smaller |
| RAM (idle) | ~120 MB | ~25 MB | 5x less |
| Cold startup | ~2 s | < 500 ms | 4x faster |

### Removed
- `electron`, `electron-vite`, `electron-builder`, `electron-store` (all replaced by Tauri stack)
- `src/main/` — Electron main process (replaced by `src-tauri/`)
- `src/preload/` — Electron preload scripts (not needed in Tauri)
- `electron.vite.config.ts`, `electron-builder.json`

---

## [1.0.3] - 2025-12-01

### Fixed
- Performance: entity mapping rewritten as O(n) single-pass (was O(n²))
- Memory leak in 2D hue picker canvas — draw functions now stored in `WeakMap`
- `pointercancel` event handled in color picker drag to prevent stuck state

---

## [1.0.2] - 2025-11-15

### Added
- 2D canvas color picker (drag-and-drop, X = hue, Y = saturation/brightness)
- Popup UI redesign with smooth animations (140 ms spring easing)
- Detail side panel for per-room entity controls (slides in from right)
- Dark / light mode auto-detection via `prefers-color-scheme`

---

## [1.0.1] - 2025-11-01

### Added
- Multi-environment support (multiple Home Assistant instances)
- Drag-and-drop reordering of rooms and devices in Settings
- Eye toggle (show/hide) per individual entity
- One-time migration from legacy single `haUrl`/`haToken` to environment list

---

## [1.0.0] - 2025-10-15

### Added
- Initial release — port of ItsyHome macOS (Swift/SwiftUI) to Windows via Electron
- System tray with connected / connecting / error icon states
- Popup menu with Favorites, Rooms (collapsible), Scenes, Cameras sections
- Real-time WebSocket connection to Home Assistant with exponential-backoff reconnect
- 12 entity domains: light, switch, climate, cover, lock, fan, humidifier, valve, sensor, alarm, camera, scene
- Notification badge and update badge in tray and popup header
- Settings window: HA connection, accessories ordering, cameras toggle, launch at login
- Persistent notifications via `persistent_notification/get` HA WS API
- Ping/pong heartbeat (30 s interval, 10 s timeout)
- Single-instance lock
- NSIS installer for Windows x64
