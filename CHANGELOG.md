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
- Impostazioni: Informazioni con «© Paolo Bets» e il riquadro AboutSlint (licenza Slint royalty-free 2.0, punto 2(a)), raggiungibile anche dalla voce «Informazioni» del menu della tray, senza «Licenza MIT»; suggerimenti generici e più tenui. AboutSlint porta con sé lo stile dei widget standard di Slint: l'eseguibile dell'assistente passa a circa 24 MB (RAM a riposo invariata).
- Finestra di gestione: adattata all'area di lavoro (al più 780 × 680), contenuto rientrato sotto una linea fidata con l'etichetta «CONTENUTO ESTERNO», titolo ripulito (controlli, direzione, larghezza zero, 200 caratteri), Tab porta nel contenuto.
- Scorciatoie: Ctrl+Alt+Maiusc+Spazio (apri) e Ctrl+Alt+Maiusc+I (ritirata), anche nella spec.

### Changed (revisioni UI/UX e frontend, giro 2: struttura dell'interfaccia)
- Colori solo in `tema.slint`, come token di ruolo (un test fallisce se un `.slint` ne scrive uno a mano); scale fisse di raggi e corpi del testo.
- Componenti comuni (`componenti.slint`: anello del fuoco, pulsante, etichetta; `faccia.slint`: misure, occhi e battito) ed enum Slint al posto delle stringhe di stato.
- Un solo stile di pulsante (raggio 10, 8 nella pillola; neutro senza bordo, azione e pericolo col bordo): nelle Impostazioni i pulsanti non sono più a capsula.
- Misure condivise fra Slint e Rust controllate da `cargo test` (backend di prova di Slint, senza finestre), non più solo all'avvio in debug.
- Accessibilità: il fuoco da tastiera del pannello è un fuoco vero che Narrator legge (schede, voci, card, pulsanti); dock, puntini e sezioni come schede con lo stato; voci espandibili; quadranti e righe PC con nome e valore; pillola come pulsante.
- Sensori dei Quadranti a casa non connessa leggibili (circa 7:1 invece di 2,9:1).
- Vista PC: righe su tutta l'altezza, rete «↓ … · ↑ …» su una riga.
- Gestione: il riquadro web si rimette al suo posto quando la finestra cambia scala o misura; spazio predisposto per mittente e data.
- Spessori sottili (filo del pannello, anello della pillola e del fuoco) arrotondati al pixel fisico a 125% e 175%.

### Fixed
- Dopo la prima apertura del pannello la pillola restava a 16,5–18,5 MB (budget 15) con ~100 handle in più: la CPU letta da sysinfo passava da PDH, che non libera mai le sue tabelle. Su Windows ora si usa `GetSystemTimes`.
- «Informazioni» dalla tray non lascia più la sezione in sospeso se le Impostazioni non si aprono; due letture della CPU senza tempo trascorso tengono il valore di prima invece di «—».

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

### Misure dopo il giro 2 delle revisioni (14/11/2026, stesse condizioni, 5 minuti per stato)

| Stato | RAM privata | CPU media | 95° percentile | max | Budget |
|---|---|---|---|---|---|
| Pillola ferma | 8,0–8,2 MB | 0,016% | 0,13% | 0,65% | ≤ 15 MB, ≤ 0,1% |
| Pillola ferma e sveglia (`--umore calm`; giro 1 ricostruito, stessa prova: 0,023% / 0,26% / 0,51%) | 8,0–8,2 MB | 0,019% | 0,13% | 0,52% | ≤ 15 MB, ≤ 0,1% |
| Pillola «pensa» | 8,2–8,3 MB | 0,45% | 0,77% | 1,15% | ≤ 1% |
| Pillola che chiama e pulsa¹ | 9,7–9,8 MB | 0,53% | 0,90% | 1,04% | ≤ 1% |

Nel giro 2 la pillola che chiama era salita a 10,6–10,7 MB: i suoi pulsanti usavano il carattere di sistema (un font in più in memoria). Corretto prima di questa misura.

### Changed (Task 14d: dalla prima prova sulla casa vera, 15/11/2026)
- Casa reattiva e leggera: uno `state_changed` aggiorna in memoria solo la sua entità (dal `new_state` dell'evento, come da documentazione di Home Assistant) e si pubblica solo se cambia ciò che l'app mostra; raffiche visibili in una pubblicazione (50 ms, tetto 300 ms). Stati, registri e configurazione si leggono tutti solo alla (ri)connessione, dopo gli eventi `*_registry_updated` (ora seguiti) o con uno stato illeggibile. Prima ogni evento della casa, sensori compresi, faceva rileggere tutto almeno una volta al secondo.
- Memoria: stati mai mostrati senza attributi, risultati delle richieste presi dal messaggio invece che copiati, niente più `persistent_notification/get`.
- Tocco: l'icona cambia subito (stato ottimistico, riusabile per ogni dispositivo); torna com'è con l'errore se il comando fallisce o se in 5 s Home Assistant non conferma.
- Niente icona nella barra di serie (decisione di Paolo): il clic destro sulla pillola apre il menu dell'isola (Apri, Impostazioni, Informazioni, Ritira/Mostra l'isola, Esci), nativo di Windows, sotto la pillola. L'icona si accende o spegne subito da Impostazioni › Sistema › «Mostra icona nella barra». La ritirata ora si sceglie dal menu.
- `finto_ha`: `--raffica <n>`, `--casa-grande <n>`, `--lenta <ms>` per provare una casa che si muove, pesante o lenta.

### Misure del Task 14d (15/11/2026, build release, Home Assistant finto con 7 entità e 7 preferiti, `--umore calm`)

| Stato | Prima (bd02ec2, 2 min) | Dopo (5 min) | Obiettivo |
|---|---|---|---|
| Riposo | 0,022%, 10,6 MB | 0,015% (p95 0,13, max 0,39), 8,0–8,2 MB | ≤ 0,1% |
| Riposo con 20 `state_changed`/s | 0,074%, 10,7–11,2 MB | 0,053% (p95 0,26, max 0,39), 8,3–8,4 MB | ≤ 0,05%, RAM +≤ 1 MB |
| Casa grande (807 entità) con 20/s, 2 min | 0,182%, 15,0–17,5 MB | 0,046%, 11,1–11,3 MB | |
| Pillola che chiama e pulsa¹ | 0,53% (giro 2) | 0,557% (p95 0,91, max 1,43), 9,4–9,5 MB | ≤ 1% |

Tocco su una casa lenta (`--lenta 800`): l'icona cambia sullo schermo 32–51 ms dopo l'invocazione (prima 1150 ms); la conferma arriva dopo circa 880 ms e non cambia più nulla. Con la casa veloce la conferma arriva in circa 60 ms. Sotto lo 0,05% con la raffica non si scende di molto: un client WebSocket minimo che riceve gli stessi eventi e non fa altro costa già circa 140 µs di CPU per messaggio su questo PC.

### Added (Task 14e: stanze e gestione dei dispositivi, 15/11/2026)
- Vista Casa: a sinistra le stanze (ordine salvato, senza le nascoste, «Altro» e «Scene» in fondo), a destra i preferiti; una stanza aperta mostra i suoi dispositivi al posto dei preferiti, come il dettaglio della v2. Luci, interruttori e scene col tocco (stato ottimistico come i preferiti), il resto in sola lettura con lo stato in italiano. Tastiera: Tab fra stanze e card, Esc e «‹ Indietro» chiudono prima la stanza.
- Impostazioni › Dispositivi (la scheda «Accessories» della v2): preferiti in cima con il loro ordine, stanze con ordine, icona (le 60 della v2, ridisegnate con icone Lucide, licenza ISC) e occhio, dispositivi con ordine, occhio e stella; stanze comprimibili. Ogni modifica è salvata subito con le chiavi della v2 e si vede subito nel pannello.

### Changed (Task 14e)
- Gli stati arrivano da `subscribe_entities` con le sole entità mostrate (preferiti, stanze, scene, telecamere, aggiornamenti), non più da `state_changed` di tutta la casa; l'iscrizione si rifà dopo ogni lettura completa e quando cambia ciò che si mostra, mai con l'elenco vuoto (per HA vorrebbe dire «tutte»).
- Prima lettura fallita: nuovi tentativi dopo 2, 4, 8… fino a 60 s, annullati da una lettura riuscita. L'esito di un comando porta il numero del suo tocco. Dopo il menu della pillola il primo piano torna a chi lo aveva solo se è ancora della pillola.
- Un `input_boolean` si commuta col suo servizio; le scene senza ordine salvato vanno per nome.

### Misure del Task 14e (15/11/2026, stesse condizioni del 14d, finto con 25 entità)

| Stato | Task 14d | Task 14e | Obiettivo |
|---|---|---|---|
| Riposo, 5 min | 0,015%, 8,0–8,2 MB | 0,019% (p95 0,13, max 0,52), 7,9–8,1 MB | ≤ 0,1% |
| Raffica 20/s su entità non mostrate, 5 min | 0,053%, 8,3–8,4 MB | 0,022% (p95 0,13, max 0,39), 8,2–8,3 MB | ≤ 0,03% |
| Casa grande (829 entità) + raffica, 2 min | 0,046%, 11,1–11,3 MB | 0,037%, 11,8 MB | |
| Pillola che chiama e pulsa¹, 5 min | 0,557%, 9,4–9,5 MB | 0,340% (p95 0,64, max 1,04), 9,5–9,6 MB | ≤ 1% |

¹ Build temporanea con `ALLARGATA_PER` = 600 s, **senza toast** (prova senza `ITSYHOME_PROVA_NOTIFICHE`, quindi senza AUMID): il confronto col 14d, misurato col toast, non è alla pari; il codice della pillola non è cambiato.

### Fixed (Task 14e, giro di correzioni 1)
- Entità che nascono dopo la lettura (HA appena riavviato, integrazioni lente, ricaricamenti) non restavano più fuori: si seguono anche le voci del registro che l'app mostrerà appena avranno uno stato, un'entità tolta resta seguita fino alla lettura successiva, e se `get_config` dice che HA non è ancora «RUNNING» si rilegge dopo 5 s (arrivato a «RUNNING» HA manda anche `core_config_updated`, che fa rileggere).
- Riconnessione con lettura fallita: si riprova con l'attesa crescente anche se ci sono i dati di prima, e intanto ci si iscrive agli stati.
- Iscrizione agli stati rifiutata da HA scritta nel log; catalogo in un `Arc` (non si copia a ogni cambio); scelta dell'icona chiusa da un clic fuori; le scene con le scintille (come il «✨» della v2), la stella solo per i preferiti.

### Added (Task 14f: collegamenti veloci, 15/11/2026)
- App «Collegamenti» nel dock (icona a catena, Lucide): le pagine web delle gestioni in una griglia di card (nome e indirizzo accorciato), nell'ordine salvato. Un tocco o Invio apre la pagina nel browser predefinito del sistema (Windows `ShellExecuteW` «open», macOS `open`, Linux `xdg-open`), mai nella finestra di gestione; il pannello poi si chiude. Se il browser non parte il pannello resta aperto e lo dice, il dettaglio va in Impostazioni › Informazioni. Stato vuoto con «Aggiungi un collegamento»; «Modifica» nella testata. Entrambi aprono le Impostazioni sulla scheda Collegamenti.
- Impostazioni › Collegamenti (fra Dispositivi e Sistema): aggiungi, modifica, elimina con una conferma dentro la finestra che nomina il collegamento («Eliminare “Proxmox”?», fuoco su «Annulla», Esc annulla), ordine con le frecce su/giù. Nome obbligatorio (al più 60 caratteri, ripulito da controlli e caratteri invisibili); indirizzo solo `http://` o `https://` (senza schema si aggiunge `https://`; schema e host in minuscolo), rifiutati altri schemi (`file:`, `javascript:`, `data:`…), spazi, controlli, caratteri invisibili o di direzione, virgolette e credenziali nell'indirizzo. Controllo al blur sui campi già toccati, errore che sparisce appena corretto. Ogni modifica è salvata subito (`collegamenti` in `assistente.json`, un file senza la chiave si legge) e si vede subito nel pannello; l'indirizzo si ricontrolla anche quando si apre.

Riposo non rimisurato a schermo libero: durante tutta la prova lo schermo era bloccato (pillola nascosta dalla schermata di blocco), la misura di 5 minuti fatta allora (0,0004%, 7,9–8,0 MB) non si confronta con lo 0,019% del 14e. A pannello chiuso il codice nuovo non fa nulla (nessun timer, la vista nasce solo aperta); misura, raffica anti-glitch e foto restano da fare a schermo sbloccato.

### Fixed (Task 14f, minori del 14e)
- La scelta dell'icona di una stanza non si chiude più con un clic dentro il riquadro ma fuori da un'icona.
- Test di HA in avvio non più instabile.

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
