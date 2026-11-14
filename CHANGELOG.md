# Changelog

All notable changes to ItsyHome are documented in this file.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

---

## [Unreleased] — v3 «assistente isola» (ramo `v3-assistente`, milestone M1)

### Added
- Assistente sempre presente in alto al centro: pillola con occhi e umori, pannello 800 × 300 (Panoramica Orologio/Quadranti, Casa, PC), Impostazioni in finestra dedicata, finestra di gestione in processo separato (`itsyhome-gestione`).
- Workflow CI `v3-build` (Windows, macOS, Ubuntu): `cargo build -p assistente -p gestione` e `cargo test --workspace`.
- `--prova-gestione-cicli <n>` per misurare aperture e chiusure ripetute della finestra di gestione (un numero non valido ora lo dice).
- `--prova-avviso <livello> --lungo`: titolo che non sta nella pillola e testo da scorrere, per le prove.
- Movimento ridotto: con «Effetti di animazione» spento in Windows niente pulsazioni né scivolamenti.
- CI: `libdbus-1-dev` esplicita su Linux e `cargo clippy --workspace --all-targets -- -D warnings`.

### Changed (revisioni UI/UX e frontend, giro 1)
- «Pensa» solo alla prima connessione e dopo un'azione dell'utente (Salva, Connetti, Riprova), a 20 fps e senza ombra sfocata mentre si muove; le riconnessioni automatiche restano tranquille.
- Permessi: nella pillola, se il testo è tagliato, «Nega» + «Apri» (il consenso si dà dal pannello col testo intero); nel pannello fuoco iniziale su «Nega» e «Consenti» solo con Ctrl+Invio o clic; 10 px fra i pulsanti.
- Casa senza connessione: niente colore di stato, card tenui leggibili, testata «Casa non raggiungibile · dati delle HH:MM» con «Riprova»; vista vuota con «Apri Impostazioni».
- Decimali con la virgola; livelli per l'utente «Aspetta te» / «Guarda presto» / «Quando vuoi»; «Casa» nell'assistente e «Home Assistant» solo dove si configura.
- Pillola: puntino 5 × 5 pieno (attenzione) o ad anello (informazione), pulsanti 26/28 px larghi 64–120, bloccante con bordo da 4 px, anello a riposo al 45%, linea della ritirata 300 × 5 con filo scuro.
- Pannello: Esc riduce prima la voce espansa, testo lungo scorrevole, puntini 24 × 24 nascosti sotto la voce, Orologio centrato, traccia degli archi visibile e icone distinte per CPU, RAM e disco.
- Impostazioni: Informazioni con «© Paolo Bets» e il riquadro AboutSlint (licenza Slint royalty-free 2.0, punto 2(a)), senza «Licenza MIT»; suggerimenti generici e più tenui.
- Finestra di gestione: adattata all'area di lavoro (al più 780 × 680), contenuto rientrato sotto una linea fidata con l'etichetta «CONTENUTO ESTERNO», titolo ripulito (controlli, direzione, larghezza zero, 200 caratteri), Tab porta nel contenuto.
- Scorciatoie: Ctrl+Alt+Maiusc+Spazio (apri) e Ctrl+Alt+Maiusc+I (ritirata), anche nella spec.

### Fixed
- Dopo la prima apertura del pannello la pillola restava a 16,5–18,5 MB (budget 15) con ~100 handle in più: la CPU letta da sysinfo passava da PDH, che non libera mai le sue tabelle. Su Windows ora si usa `GetSystemTimes`.

### Misure (14/11/2026, build release, Home Assistant finto, Windows 11, schermo 3840 × 2160 al 175%)

RAM privata e CPU dell'intero albero di processi (`tools/misura.ps1` e campionamento al secondo), ogni stato per almeno 2 minuti.

| Stato | RAM privata | CPU media (max) | Budget |
|---|---|---|---|
| Pillola ferma | 7,8–8,3 MB | 0,014% (0,44) | ≤ 15 MB, ≤ 0,1% |
| Pillola animata («pensa», 25 fps) | 7,9–8,2 MB | 0,58% (1,22) | ≤ 1% |
| Pillola che chiama e pulsa (attenzione)¹ | 9,5–9,7 MB | 0,52% (0,98) | ≤ 1% |
| Pillola ritirata (linea) | 7,9–8,2 MB | 0,008% (0,12) | ≤ 15 MB, ≤ 0,1% |
| Pannello aperto fermo | 13,8 MB (prima della correzione 20,3) | 0,08% (0,44) | ≤ 30 MB, ~0% |
| Pillola dopo 1 / 25 aperture del pannello | 9,96 / 10,53 MB (prima 16,5–18,5) | 0,02% a riposo | entro +2 MB |
| Finestra di gestione aperta (9 processi, WebView2) | 166–178 MB | picco ~14% all'apertura | solo mentre è aperta |
| Assistente dopo 5 cicli della gestione | 8,2 MB, nessun processo figlio vivo | 0% | entro +2 MB |

Thread 6–12 e handle 302–326 per tutta la prova lunga, senza crescita; oggetti GDI/USER tornano a 17/20 dopo ogni chiusura del pannello.

¹ Misurata con una build temporanea (pillola larga per 600 s invece di 20), non quella del ramo: da sola la pillola che chiama pulsa solo 20 s.

### Misure dopo il giro 1 delle revisioni (14/11/2026, stesse condizioni, 5 minuti per stato)

Campioni da 1 s del solo processo dell'assistente; il budget delle animazioni vale sulla media, il 95° percentile dice quanto spesso si va oltre.

| Stato | RAM privata | CPU media | 95° percentile | max | Budget |
|---|---|---|---|---|---|
| Pillola ferma | 8,2–8,3 MB | 0,010% | 0,000% | 0,39% | ≤ 15 MB, ≤ 0,1% |
| Pillola «pensa» (20 fps, senza ombra in moto) | 8,0–8,2 MB | 0,44% | 0,77% | 1,16% | ≤ 1% |
| Pillola che chiama e pulsa¹ | 9,6–9,7 MB | 0,56% | 0,90% | 1,28% | ≤ 1% |

---

## [2.1.2] - 2026-03-24

### Fixed
- **App non si avvia se le notifiche push erano registrate** — `tokio::spawn` nel callback `setup()` di Tauri causava panic "no reactor running" perché il main thread non è nel contesto del runtime Tokio. Sostituito con `tauri::async_runtime::spawn`. Il crash si verificava solo con una registrazione `notifRegistration` presente nello store.
- **Webhook server non arrestabile** — il server HTTP per le notifiche push (Axum, porta 7421) veniva avviato senza conservare il `JoinHandle`. Chiamate successive a `notifications_register` lasciavano il vecchio server attivo; il nuovo `bind()` falliva silenziosamente. Fix: `webhook_handle` in `AppState`, vecchio server abortito prima di avviarne uno nuovo.
- **`notifications_unregister` non arrestava il server** — la de-registrazione cancellava il record nello store JSON ma il server HTTP continuava ad ascoltare sulla porta. Fix: `handle.abort()` chiamato su unregister.

---

## [2.1.1] - 2026-03-19

### Fixed
- **Mobile app registration schema validation** — HA `mobile_app/registrations` endpoint requires both `push_token` and `push_url` to be included in the `app_data` inclusion group; registration was failing with 400 Bad Request. Fixed by adding `push_token = device_id` to satisfy the schema validation. Now `notify.mobile_app_itsyhome_<hostname>` service is created correctly in HA automations.

---

## [2.1.0] - 2026-03-19

### Added
- **Push notifications from Home Assistant** — Three notification channels now available:
  1. **WebSocket channel** (sempre attivo): Subscribe to `persistent_notification_created` HA events. Works immediately with any HA automation using `persistent_notification.create` service — no registration required.
  2. **Mobile app registration**: POST to `/api/mobile_app/registrations` creates `notify.mobile_app_itsyhome_<hostname>` service in HA automations, exactly like the official HA companion app. One-click setup from Settings > Notifiche tab.
  3. **Local webhook server** (Axum backend, configurable port default 7421): HA pushes notifications directly to the app when `notify.mobile_app_*` is called in an automation. Requires Windows Firewall rule to allow inbound traffic on the configured port.

- **Notifications Settings tab** — new UI section for notification management:
  - Register/Unregister buttons to manage mobile app registration
  - Port configuration field (default 7421)
  - Status card showing device name, HA service name (`notify.mobile_app_*`), and webhook push URL after registration
  - Test notification button
  - Built-in example HA automation YAML

Example Home Assistant automation using the registered service:
```yaml
alias: Alarm triggered
trigger:
  platform: state
  entity_id: binary_sensor.front_door
  to: 'on'
action:
  - service: notify.mobile_app_itsyhome_MYPC
    data:
      title: "Alarm triggered"
      message: "Motion detected at entrance"
```

---

## [2.0.1] - 2026-03-19

### Fixed
- **App fails on clean installation** — Missing `custom-protocol` Cargo feature meant embedded frontend was not served correctly. `tauri build` adds the feature automatically but direct `cargo build --release` does not. Added `custom-protocol` feature explicitly to `Cargo.toml` so frontend is always served via `tauri://localhost/` regardless of build method.
- **Version number misalignment** — `package.json`, `Cargo.toml`, `tauri.conf.json`, and git tag were not in sync (files at 2.0.0, git tag v2.0.1). All manifests bumped to 2.0.1 for consistency.
- **Build script Node.js PATH** — `build-release.bat` updated with correct Node.js PATH for build pipeline.

---

## [2.0.0] - 2026-03-19 (Patch 2)

### Fixed
- **Camera visibility in popup** — `MenuData.cameras` refactored from `Vec<CameraEntity>` to `Vec<AppEntity>` with `"type":"camera"` in JSON serialization; cameras now render correctly in popup instead of being invisible
- **Entity registry resilience** — `HaEntityEntry.platform` changed from `String` to `Option<String>` to prevent deserialization failure when a single entity has `platform: null` in HA registry
- **Popup positioning drift (follow-up)** — `window_resize` now skips all operations if popup is hidden via `is_visible()` guard, eliminating 30s position drift caused by background JS resize calls; `reposition_from_tray` now uses deterministic `tray_pos` formula instead of reading `outer_pos.x`
- **Drag & drop rewrite** — Complete replacement of HTML5 Drag & Drop API with pointer events (`pointerdown/pointermove/pointerup`) via `setupPointerDrag` helper; WebView2 bug workaround: missing `user-select: none` on `.acc-row` caused text-selection drag to override element drag. Solution: full DnD API bypass with `setPointerCapture`, `elementFromPoint` with `pointerEvents: none`, 5px drag threshold
- **Detail panel background transparency** — Detail panel changed from `rgba(30,30,32,0.92)` (8% transparent) to `rgb(30,30,32)` (opaque); removed `backdrop-filter` from detail panel to prevent blur of internal content; new animation: slide + fade from right (14px translate) with spring easing

### Fixed (Build)
- **Build pipeline order** — `build-release.bat` corrected with proper sequence: 1) Vite build → 2) cargo build → 3) tauri bundle; previous order ran cargo before Vite, causing binary to embed old frontend and fail with "localhost refused connection"

---

## [2.0.0] - 2026-03-17 (Patch)

### Fixed
- **Popup positioning drift** — popup no longer shifts right on repeated tray-icon clicks. Root cause: `window_resize` was calling `set_position` on the hidden window while JS continued running in background; `show_popup` now uses known logical width (300px × DPI scale) instead of `outer_size()` for stable X calculation.
- **Drag & drop areas in Settings** — dragstart guard `closest('.acc-drag')` incorrectly blocked all area drags (e.target is always the header div, never the span); removed the guard.
- **Drag & drop devices** — added `dragover` fallback on the entityList container so drops work even in gaps between rows.
- **Camera visibility** — cameras with `entity_category` set in HA registry were silently filtered; bypass this filter for the camera domain so all cameras always appear.
- **Tray icon right-click menu** — added "Impostazioni" and "Esci da ItsyHome" context menu items (right-click on tray icon).

### Added
- **Area hide/show button** — eye button on each area header in Settings > Accessories allows hiding/showing all entities in an area at once.

---

## [2.0.0] - 2026-03-16 (Initial Release)

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
