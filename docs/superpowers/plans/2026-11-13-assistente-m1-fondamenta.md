# Assistente "isola" — M1 Fondamenta — Piano di implementazione

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** la v3 di ItsyHome esiste come assistente "isola" in Slint: pillola con umori, pannello unico con 4 layout e dock, dati reali di casa e PC, impostazioni nel pannello, tray/autoavvio/istanza unica, riquadro web dentro il pannello — entro il budget di risorse.

**Architecture:** workspace Cargo alla radice del repo con due crate: `casa-core` (backend Home Assistant estratto da `src-tauri/src`, senza Tauri, con un proprio runtime tokio su un thread di lavoro) e `assistente` (binario Slint, renderer Skia software). Gli eventi di `casa-core` arrivano all'interfaccia tramite una callback e `slint::invoke_from_event_loop`. `src-tauri/` (v2.1.2) resta intatto sul ramo e fuori dal workspace fino alla milestone Casa.

**Tech Stack:** Rust 2021, Slint 1.18 (`renderer-skia`, `unstable-winit-030`, `raw-window-handle-06`), wry 0.57, tokio (current_thread), tokio-tungstenite + rustls (native roots), serde/serde_json, keyring 3, directories, sysinfo, starship-battery, tray-icon + muda, auto-launch, single-instance, notify-rust, windows (Win32).

**Spec:** `docs/superpowers/specs/2026-11-13-assistente-isola-design.md`

## Global Constraints

- Lavoro sul ramo `v3-assistente`; `main` non si tocca. Commit solo con l'ok di Paolo (regola di lavoro); niente push senza ok.
- Build e test da **PowerShell**, non Git Bash (il `link` di Git Bash copre quello MSVC).
- Budget (spec §4), **con Home Assistant connesso**: pillola ferma ≤ 15 MB RAM privata e ≤ 0,1% CPU; pillola animata ≤ 1% CPU; pannello aperto fermo ≤ 30 MB, ~0% CPU; dopo chiusura del riquadro web ritorno entro +2 MB.
- Nessun polling di Home Assistant: solo eventi WebSocket. Statistiche PC campionate solo a pannello aperto.
- Renderer: `BackendSelector` con backend `winit` e renderer `skia-software`.
- Colori brand: `#c084fc #818cf8 #60a5fa #22d3ee #2dd4bf #e879f9`; fondo `#2a0a4e → #0a0015`; testo icone `rgba(236,230,250,.62)`, tratto icone 1,55; colore solo per lo stato. Font Inter Tight + JetBrains Mono (inclusi negli asset).
- Pannello 800×300 al bordo alto: in alto occhi al centro, ora a destra, «‹ Indietro» a sinistra solo dentro le app; Panoramica con riga «In attesa» e layout Orologio/Quadranti (puntini); dock di sole icone. Finestra separata solo per la gestione approfondita (spec §3bis).
- Riquadro web: JavaScript disattivato, devtools spenti, navigazione bloccata, distrutto alla chiusura.
- Token mai nel file di configurazione (portachiavi del sistema); mai nei log.
- Dipendenze escluse in M1: axum, reqwest, native-tls.
- Licenza Slint royalty-free: riga "Made with Slint" visibile nelle Impostazioni (sezione Informazioni).
- Testi dell'interfaccia in italiano.

## Review Focus

1. **Home Assistant irraggiungibile o token sbagliato all'avvio** → la pillola non resta per sempre su "pensa": torna "tranquilla", le Impostazioni mostrano l'errore, i tentativi continuano con attesa crescente (Task 3, Task 4).
2. **Raffica di `state_changed`** (casa con molte entità) → una sola rilettura ogni 300 ms al massimo, CPU nel budget (Task 3).
3. **Monitor scollegato, cambio di monitor principale o di scala DPI** → la pillola ricompare in alto al centro dello schermo principale visibile (Task 5).
4. **Comando su un'entità diventata non disponibile o rifiutato da HA** → nessun "contento", errore leggibile sulla voce, nessun crash (Task 8).
5. **Configurazione v2 mancante, corrotta o parziale, o portachiavi non disponibile** → l'app parte con configurazione vuota o parziale, non sovrascrive il file v2, e senza portachiavi avvisa nelle Impostazioni invece di salvare il token in chiaro (Task 2).

---

## Struttura dei file

```
Cargo.toml                         workspace: members crates/*, exclude src-tauri
crates/casa-core/
  Cargo.toml
  src/lib.rs                       riesporta i moduli
  src/types.rs                     copia di src-tauri/src/types.rs (HaEnvironment senza token)
  src/ha/{mod,models,mapper}.rs    copie di src-tauri/src/ha/*
  src/ha/client.rs                 attore WS (da src-tauri) con Ascoltatore al posto di AppHandle, rustls
  src/ha/eventi.rs                 interpretazione pura dei messaggi WS
  src/eventi.rs                    CasaEvento, StatoConnessione, Ascoltatore
  src/config.rs                    Config, ConfigStore, importa_v2
  src/segreti.rs                   trait Segreti, KeyringSegreti, MemoriaSegreti (test)
  src/servizio.rs                  CasaService (thread + runtime + debounce + comandi)
  tests/finto_ha.rs                server WS finto per i test d'integrazione
crates/assistente/
  Cargo.toml, build.rs             slint-build
  assets/font/*, assets/icone/*.svg, assets/tray.png
  ui/tema.slint, ui/icone.slint, ui/pillola.slint, ui/pannello.slint
  ui/layout/{orologio,quadranti,colori,tutte}.slint
  ui/viste/{casa,pc,impostazioni}.slint
  src/main.rs                      avvio, wiring
  src/umore.rs                     Cervello degli umori (puro)
  src/registro.rs                  app disponibili
  src/layout.rs                    enum Layout
  src/finestre.rs                  posizioni pillola/pannello (puro) + applicazione
  src/pc_stats.rs                  Campionatore
  src/ponte.rs                     CasaEvento → modelli UI, preferiti, comandi
  src/riquadro_web.rs              wry figlio del pannello
  src/sistema/{mod,tray,autoavvio,istanza,notifiche,schermo_intero,inattivita}.rs
tools/misura.ps1                   linea del tempo RAM/CPU (da spike-isola/misura-misto.ps1)
tools/foto-zona.ps1                foto della sola zona dell'app
```

---

### Task 1: Ramo, workspace e `casa-core` con tipi e mapper

**Files:**
- Create: `Cargo.toml`, `crates/casa-core/Cargo.toml`, `crates/casa-core/src/{lib,types}.rs`, `crates/casa-core/src/ha/{mod,models,mapper}.rs`, `tools/misura.ps1`, `tools/foto-zona.ps1`
- Copy from: `src-tauri/src/types.rs`, `src-tauri/src/ha/{models,mapper}.rs`, `C:\Work\Sviluppo\spike-isola\misura-misto.ps1`, `...\foto-pannello.ps1`

**Interfaces:**
- Produces: `casa_core::types::{AppEntity, MenuData, Room, SceneEntity, HaEnvironment { id, name, ha_url }, TestConnectionResult}`; `casa_core::ha::mapper::EntityMapper::map(...) -> MenuData` (firma invariata rispetto a v2).

- [ ] **Step 1:** `git switch -c v3-assistente`. Workspace radice: `members = ["crates/casa-core", "crates/assistente"]` (assistente aggiunto nel Task 4), `exclude = ["src-tauri"]`, profilo release come negli spike (`opt-level="s"`, `lto=true`, `codegen-units=1`, `strip=true`).
- [ ] **Step 2:** copia tipi, modelli e mapper; togli `ha_token` da `HaEnvironment`; nessuna dipendenza da Tauri.
- [ ] **Step 3: test del mapper (failing finché la copia non compila)** in `crates/casa-core/src/ha/mapper.rs`:

```rust
#[test] fn preferiti_nell_ordine_salvato() // stati: light.a, switch.b; favorites ["switch.b","light.a"] → menu.favorites ids == ["switch.b","light.a"]
#[test] fn nascosti_esclusi_da_stanze_e_preferiti() // hidden ["light.a"] → light.a assente ovunque
#[test] fn stanze_nell_ordine_salvato() // aree cucina, sala; room_order ["sala","cucina"] → rooms[0].area_id == "sala"
#[test] fn unita_fahrenheit() // HaConfig temperature "°F" → menu.temp_unit == "°F"
```

- [ ] **Step 4:** `cargo test -p casa-core` → PASS.
- [ ] **Step 5:** copia gli script di misura in `tools/`: `misura.ps1` con parametri `-Exe -Argomenti -Log -Secondi`, stessa logica (PID mai visti, chiusura per PID); `foto-zona.ps1` con parametri `-X -Y -L -A` in pixel logici e `-Out`, mai schermo intero.
- [ ] **Step 6:** commit `feat(v3): workspace e casa-core con tipi e mapper`.

### Task 2: Configurazione, import da v2 e portachiavi

**Files:**
- Create: `crates/casa-core/src/{config,segreti}.rs`, `crates/casa-core/tests/fixture/v2-completa.json`, `v2-parziale.json`, `v2-corrotta.json` (valori finti)

**Interfaces:**
- Produces:
  - `trait Segreti: Send + Sync { fn salva(&self, env_id: &str, token: &str) -> Result<(), String>; fn leggi(&self, env_id: &str) -> Result<Option<String>, String>; fn cancella(&self, env_id: &str) -> Result<(), String>; }`
  - `KeyringSegreti` (servizio `"itsyhome"`, utente = `env_id`), `MemoriaSegreti` (pubblica, per test)
  - `#[derive(Serialize, Deserialize, Default, Clone)] struct Config { environments: Vec<HaEnvironment>, active_env_id: Option<String>, favorites: Vec<String>, hidden_entities: Vec<String>, room_order: Vec<String>, device_order: HashMap<String, Vec<String>>, area_icons: HashMap<String, String>, cameras_enabled: bool, launch_at_login: bool, ultimo_layout: String, importata_da_v2: bool }`
  - `ConfigStore::apri(dir: &Path) -> ConfigStore` (file `assistente.json`), `leggi(&self) -> Config` (file assente o illeggibile → `Config::default()`), `salva(&self, &Config) -> io::Result<()>` (scrittura su `.tmp` + rename)
  - `importa_v2(v2: &Path, segreti: &dyn Segreti) -> Result<Option<Config>, String>`: `None` se il file non c'è; legge le chiavi v2 `environments[].{id,name,haUrl,haToken}`, `activeEnvId`, `favorites`, `hiddenEntities`, `roomOrder`, `deviceOrder`, `areaIcons`, `camerasEnabled`, `launchAtLogin`, e le chiavi legacy `haUrl`/`haToken` se `environments` è vuoto (stessa regola di `store.rs:75-90`); i token vanno in `segreti`; il file v2 non viene mai scritto.
  - `fn cartella_config() -> PathBuf` (crate `directories`, `ProjectDirs::from("com", "itsyhome", "assistente")`), `fn file_config_v2() -> Option<PathBuf>` (`%APPDATA%\com.itsyhome.app\itsyhome-config.json` su Windows; equivalenti `config_dir()/com.itsyhome.app/` altrove)

- [ ] **Step 1: test (falliscono)**

```rust
#[test] fn import_completo_sposta_i_token() // v2-completa → config.environments.len()==2, nessun campo token nel JSON salvato, segreti.leggi("env1")==Some("tok-finto-1")
#[test] fn import_legacy_haurl_hatoken() // environments vuoto + haUrl/haToken → 1 ambiente "Home", token in segreti
#[test] fn import_parziale_usa_i_default() // solo favorites → config.favorites ok, il resto default
#[test] fn import_corrotto_da_errore_e_non_tocca_il_file() // Err(_), byte del file v2 identici prima e dopo
#[test] fn file_v2_assente_da_none()
#[test] fn salva_e_rileggi_uguale() // tempdir, salva, leggi → uguale
#[test] fn config_illeggibile_da_default()
#[test] fn portachiavi_guasto_non_scrive_token_in_chiaro() // Segreti che restituisce Err → importa_v2 Err contiene "portachiavi"; nessun "haToken" nel file salvato
```

- [ ] **Step 2:** `cargo test -p casa-core config` → FAIL.
- [ ] **Step 3:** implementa `config.rs` e `segreti.rs` (keyring 3 con feature `windows-native`, `apple-native`, `sync-secret-service`, `crypto-rust`).
- [ ] **Step 4:** `cargo test -p casa-core` → PASS.
- [ ] **Step 5:** commit `feat(casa-core): configurazione, import v2, token nel portachiavi`.

### Task 3: Client Home Assistant senza Tauri e `CasaService`

**Files:**
- Create: `crates/casa-core/src/{eventi,servizio}.rs`, `crates/casa-core/src/ha/{client,eventi}.rs`, `crates/casa-core/tests/finto_ha.rs`
- Copy from: `src-tauri/src/ha/client.rs`, logica di `src-tauri/src/refresh.rs`

**Interfaces:**
- Consumes: Task 1 (mapper), Task 2 (`ConfigStore`, `Segreti`).
- Produces:
  - `enum StatoConnessione { Disconnesso, InConnessione, Connesso, Errore(String) }`
  - `enum CasaEvento { Stato(StatoConnessione), Dati(Arc<MenuData>), Notifica { titolo: String, messaggio: String }, Esito { entity_id: String, ok: bool, errore: Option<String> } }`
  - `type Ascoltatore = Arc<dyn Fn(CasaEvento) + Send + Sync>`
  - `ha::eventi::interpreta(v: &serde_json::Value) -> Option<EventoWs>` con `enum EventoWs { StatoCambiato, Notifica { titolo: String, messaggio: String } }` (titolo assente → `"ItsyHome"`; messaggio vuoto → `None`)
  - `CasaService::avvia(store: Arc<ConfigStore>, segreti: Arc<dyn Segreti>, ascolta: Ascoltatore) -> CasaService` — crea un thread con runtime tokio `current_thread`; metodi non bloccanti: `connetti_attivo(&self)`, `connetti(&self, env_id: &str)`, `disconnetti(&self)`, `comando(&self, entity_id: &str, dominio: &str, servizio: &str)` (esito come `CasaEvento::Esito`), `prova_connessione(&self, url: String, token: String, fatto: impl FnOnce(TestConnectionResult) + Send + 'static)`, `config(&self) -> Config`, `aggiorna_config(&self, f: impl FnOnce(&mut Config))` (salva e, se cambiano preferiti/nascosti/ordini, rimappa i dati)
  - Debounce: 300 ms come v2; riconnessione con `reconnect_delay` di v2; errore di autenticazione = `Errore("token non valido")` senza riconnessione.

- [ ] **Step 1: test unitari (falliscono)** in `ha/eventi.rs`:

```rust
#[test] fn state_changed_e_stato_cambiato()
#[test] fn notifica_con_titolo_e_messaggio()
#[test] fn notifica_senza_titolo_usa_itsyhome()
#[test] fn notifica_vuota_ignorata()
#[test] fn evento_sconosciuto_ignorato()
```

- [ ] **Step 2: test d'integrazione (falliscono)** in `tests/finto_ha.rs`, con un server WS finto (`tokio-tungstenite` accept su `127.0.0.1:0`) che risponde `auth_required`/`auth_ok`/`auth_invalid` e a `get_states`, `config/area_registry/list`, `config/device_registry/list`, `config/entity_registry/list`, `get_config`, `persistent_notification/get`, `call_service`:

```rust
#[tokio::test] async fn connessione_porta_a_connesso_e_dati() // eventi: Stato(InConnessione), Stato(Connesso), Dati(..) con 2 entità
#[tokio::test] async fn token_sbagliato_da_errore_senza_riprovare() // Stato(Errore("token non valido")), il server riceve 1 sola connessione in 5 s
#[tokio::test] async fn server_spento_riprova() // nessun server → Stato(Errore(_)) poi di nuovo InConnessione; mai bloccato su InConnessione
#[tokio::test] async fn raffica_di_state_changed_una_sola_rilettura() // 100 state_changed in 100 ms → il server riceve 1 get_states in più (non 100)
#[tokio::test] async fn notifica_arriva_come_evento()
#[tokio::test] async fn comando_ok_e_comando_rifiutato() // call_service success → Esito{ok:true}; error {code:"not_found"} → Esito{ok:false, errore:Some(_)}
```

- [ ] **Step 3:** `cargo test -p casa-core` → FAIL.
- [ ] **Step 4:** porta `client.rs`: `AppHandle`/`emit` → `Ascoltatore`; `trigger_refresh` → funzione interna a `servizio.rs` che usa `EntityMapper` e la `Config` corrente; `tokio-tungstenite` con feature `rustls-tls-native-roots`; `tokio` solo `rt`, `macros`, `sync`, `time`, `net`. Le notifiche diventano `CasaEvento::Notifica` (il toast lo fa l'app, Task 10). Telecamere e registrazione push restano fuori.
- [ ] **Step 5:** `cargo test -p casa-core` → PASS.
- [ ] **Step 6:** commit `feat(casa-core): client HA a eventi e CasaService`.

### Task 4: Crate `assistente`, umori, registro e layout (logica pura)

**Files:**
- Create: `crates/assistente/Cargo.toml`, `build.rs`, `src/main.rs` (scheletro), `src/{umore,registro,layout}.rs`
- Modify: `Cargo.toml` radice (aggiungi membro)

**Interfaces:**
- Consumes: `casa_core::eventi::StatoConnessione`.
- Produces:
  - `enum Umore { Tranquillo, Pensa, TiCerca, Contento, Dorme }` con `fn come_stringa(self) -> &'static str` → `"calm" | "think" | "alert" | "happy" | "sleep"`
  - `enum Segnale { Casa(StatoConnessione), Notifica { titolo: String }, PannelloAperto, ComandoRiuscito, ComandoFallito, Inattivo(Duration) }`
  - `struct Cervello`; `Cervello::new(ora: Instant) -> Self`; `ricevi(&mut self, s: Segnale, ora: Instant)`; `umore(&self, ora: Instant) -> Umore`; `messaggio(&self) -> Option<&str>` (titolo dell'ultima notifica non vista)
  - Costanti: `CONTENTO_PER = 3 s`, `DORME_DOPO = 10 min` di inattività dell'utente
  - Priorità: TiCerca > Pensa > Contento > Dorme > Tranquillo. `Pensa` solo con `InConnessione`; `Errore`/`Disconnesso` → nessun Pensa. `PannelloAperto` azzera le notifiche non viste.
  - `struct VoceApp { id: &'static str, nome: &'static str, icona: &'static str }`; `fn app_disponibili() -> Vec<VoceApp>` → id `["casa", "pc", "impostazioni"]`, nomi `"Casa"`, `"PC"`, `"Impostazioni"`
  - `enum Layout { Orologio, Quadranti, Colori, TutteLeApp }`; `fn da_str(&str) -> Layout` (sconosciuto → `Orologio`); `fn come_str(self) -> &'static str` → `"orologio" | "quadranti" | "colori" | "tutte"`; `fn indice(self) -> i32` 0..3

- [ ] **Step 1: test (falliscono)**

```rust
#[test] fn parte_tranquillo()
#[test] fn in_connessione_pensa_poi_connesso_tranquillo()
#[test] fn errore_non_resta_su_pensa()
#[test] fn notifica_ti_cerca_finche_non_apro_il_pannello() // messaggio()==Some("Lavatrice finita"); dopo PannelloAperto → Tranquillo, messaggio None
#[test] fn contento_dura_tre_secondi() // a +2,9 s Contento, a +3,1 s Tranquillo
#[test] fn comando_fallito_non_e_contento()
#[test] fn inattivo_dieci_minuti_dorme() // Inattivo(9 min) Tranquillo; Inattivo(10 min) Dorme; Inattivo(0) Tranquillo
#[test] fn ti_cerca_vince_su_dorme_e_pensa()
#[test] fn app_disponibili_solo_quelle_vere() // ids == ["casa","pc","impostazioni"]
#[test] fn layout_andata_e_ritorno() // per ogni variante da_str(come_str(l)) == l; da_str("boh") == Orologio
```

- [ ] **Step 2:** `cargo test -p assistente` → FAIL.
- [ ] **Step 3:** implementa i tre moduli. `Cargo.toml` di assistente: `slint = { version = "=1.18.1", features = ["renderer-skia", "unstable-winit-030", "raw-window-handle-06"] }`, `slint-build` in build-dependencies.
- [ ] **Step 4:** `cargo test -p assistente` → PASS.
- [ ] **Step 5:** commit `feat(assistente): umori, registro app e layout`.

### Task 5: La pillola

**Files:**
- Create: `crates/assistente/ui/{tema,pillola}.slint`, `crates/assistente/src/finestre.rs`, `assets/font/*`
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `Umore::come_stringa`, `Cervello::messaggio`.
- Produces:
  - `export component Pillola inherits Window` con `in property <string> umore`, `in property <string> messaggio`, `callback cliccata()`; `no-frame`, `always-on-top`, sfondo trasparente.
  - Aspetto e animazioni dallo spike `C:\Work\Sviluppo\spike-isola\slint\src\main.rs` (200×48, raggio 24, bordo a gradiente rotante con i colori brand, battito ogni 4,6 s): `calm` aura ferma; `think` rotazione 1,6 s; `alert` gradiente ambra-rosa, rotazione 1,2 s + pulsazione, **la pillola si allarga** fino a 360 px e mostra `messaggio` troncato; `happy` occhi ad arco; `sleep` luce bassa, occhi a trattino, nessuna animazione.
  - `fn posizione_pillola(area: RettangoloFisico, scala: f32, larghezza_logica: f32, margine_logico: f32) -> (i32, i32)` (pura): centrata orizzontalmente nell'area di lavoro del monitor principale, `margine_logico = 8`.
  - `fn applica_posizione(finestra: &slint::Window, larghezza_logica: f32)`: monitor principale via `WinitWindowAccessor::with_winit_window`; si ricalcola a `ScaleFactorChanged` e `Moved` (tramite `on_winit_window_event`) e quando il monitor principale cambia (se non c'è, primo monitor disponibile).
  - `main.rs`: `slint::BackendSelector::new().backend_name("winit".into()).renderer_name("skia-software".into()).with_winit_window_attributes_hook(|a| …skip_taskbar(true) su Windows…).select()?`.

- [ ] **Step 1: test (falliscono)** in `finestre.rs`:

```rust
#[test] fn centrata_su_monitor_1920_scala_1() // area (0,0,1920,1040), 200 px → (860, 8)
#[test] fn centrata_con_scala_175() // area (0,0,2560,1528), scala 1.75, 200 → x == (2560 - 350)/2, y == 14
#[test] fn monitor_secondario_a_sinistra() // area (-1920,0,1920,1080) → x negativa centrata
```

- [ ] **Step 2:** `cargo test -p assistente finestre` → FAIL; implementa; → PASS.
- [ ] **Step 3:** avvio con `assistente.exe --umore <calm|think|alert|happy|sleep>` (solo per prova; senza flag umore dal Cervello). Verifica: `tools/misura.ps1` per `calm` e `think` → `calm` ≤ 15 MB e ≤ 0,1% CPU, `think` ≤ 1%. `tools/foto-zona.ps1` su ogni umore → aspetto come nelle bozze; foto cancellate dopo la verifica.
- [ ] **Step 4:** commit `feat(assistente): pillola con umori`.

### Task 6: Il pannello, i layout e il dock

**Files:**
- Create: `ui/pannello.slint`, `ui/icone.slint`, `ui/layout/{orologio,quadranti,colori,tutte}.slint`, `assets/icone/{casa,pc,impostazioni,orologio,quadranti,colori,tutte}.svg`
- Modify: `src/main.rs`, `src/finestre.rs`

**Interfaces:**
- Consumes: `app_disponibili()`, `Layout`, `posizione_pillola`.
- Produces:
  - `export component Pannello inherits Window`: `in property <string> umore`, `in property <string> ora`, `in-out property <int> layout`, `in-out property <string> vista` (`""` = layout, altrimenti id app), `in property <[VoceAppUi]> app`, `callback scegli-layout(int)`, `callback apri-app(string)`, `callback chiudi()`; area centrale esportata come `out property <length> centro-x / centro-y / centro-l / centro-a` (per il Task 11).
  - `struct VoceAppUi { id: string, nome: string, icona: image }`.
  - Larghezza 560, altezza 440 logici (come lo spike del pannello); riga alta con occhi al centro (stessa posizione su schermo della pillola) e ora a destra; dock centrato in capsula, scheda attiva con capsula vetro leggera; icone SVG monocromatiche.
  - `fn posizione_pannello(area: RettangoloFisico, scala: f32) -> (i32, i32)`: gli occhi del pannello cadono dove stavano gli occhi della pillola.
  - Comportamento: clic sulla pillola → pillola nascosta, pannello mostrato con animazione di apertura (opacità + altezza del contenuto, ≤ 220 ms); chiusura con Esc, perdita del focus (`WindowEvent::Focused(false)`), clic sugli occhi → pannello nascosto, pillola mostrata. L'ora si aggiorna con un `Timer` da 1 s **solo a pannello visibile**. Layout scelto salvato in `Config.ultimo_layout`.

- [ ] **Step 1: test (falliscono)** in `finestre.rs`:

```rust
#[test] fn occhi_del_pannello_sugli_occhi_della_pillola() // centro x degli occhi uguale; y della riga occhi == y della pillola
```

- [ ] **Step 2:** `cargo test -p assistente` → FAIL; implementa; → PASS.
- [ ] **Step 3:** verifica dal vivo: apri/chiudi con clic, Esc, clic fuori; cambio layout persistito dopo riavvio; `tools/misura.ps1` pannello aperto fermo ≤ 30 MB e ~0% CPU; foto di zona confrontata con `aperto-dock.html`.
- [ ] **Step 4:** commit `feat(assistente): pannello con layout e dock`.

### Task 7: Statistiche del PC

**Files:**
- Create: `src/pc_stats.rs`, `ui/viste/pc.slint`

**Interfaces:**
- Produces:
  - `struct StatPc { cpu_pct: f32, ram_usata: u64, ram_totale: u64, disco_usato: u64, disco_totale: u64, rete_giu_bps: u64, rete_su_bps: u64, batteria_pct: Option<f32>, in_carica: Option<bool> }`
  - `Campionatore::new() -> Self`; `campiona(&mut self) -> StatPc` (sysinfo con refresh mirati: CPU, memoria, dischi, reti — mai `refresh_all`; disco = somma dei dischi fissi; batteria con `starship-battery`, `None` se assente)
  - `fn formatta_byte(b: u64) -> String` (base 1024, una cifra decimale, virgola italiana: `"1,5 GB"`, `"512 MB"`, `"0 B"`); `fn formatta_velocita(bps: u64) -> String` (`"1,2 MB/s"`)
  - `struct StatPcUi` in Slint con le stesse grandezze già formattate + `cpu: float`, `ram: float` (0..1) per i quadranti; vista PC con elenco dettagliato.
  - Campionamento con `slint::Timer` ogni 2 s **solo a pannello visibile**; fermo quando il pannello si chiude.

- [ ] **Step 1: test (falliscono)**

```rust
#[test] fn formatta_byte_casi() // 0→"0 B", 512*1024*1024→"512 MB", 1610612736→"1,5 GB"
#[test] fn formatta_velocita_casi() // 1258291→"1,2 MB/s"
#[test] fn campione_plausibile() // dopo due campioni: 0<=cpu_pct<=100, ram_usata<=ram_totale, ram_totale>0
```

- [ ] **Step 2:** FAIL → implementa → `cargo test -p assistente pc_stats` PASS.
- [ ] **Step 3:** verifica: pannello chiuso → `tools/misura.ps1` mostra CPU 0% (campionatore fermo).
- [ ] **Step 4:** commit `feat(assistente): statistiche del PC`.

### Task 8: Ponte casa → interfaccia, vista Casa e layout con dati reali

**Files:**
- Create: `src/ponte.rs`, `ui/viste/casa.slint`
- Modify: `ui/layout/*.slint`, `src/main.rs`

**Interfaces:**
- Consumes: `CasaService`, `CasaEvento`, `Cervello`, `StatPc`.
- Produces:
  - `enum Tipo { Interruttore, Scena, Lettura }`; `struct Preferito { entity_id: String, nome: String, tipo: Tipo, acceso: bool, valore: String, disponibile: bool, errore: Option<String> }`
  - `fn preferiti_ui(menu: &MenuData, max: usize) -> Vec<Preferito>`: i preferiti di `menu.favorites`; se vuoti, le prime entità delle stanze nell'ordine; `max = 8`. Luci e interruttori → `Interruttore`; scene → `Scena`; sensori → `Lettura` con `valore = state + unità`; altri tipi → `Lettura` con lo stato.
  - `fn comando_per(e: &AppEntity) -> Option<(&'static str, &'static str)>`: `Light` → `("light","toggle")`, `Switch` → `("switch","toggle")`, `Scene` → `("scene","turn_on")`, altrimenti `None`; entità non disponibile → `None`.
  - `fn collega(casa: &CasaService, pillola: slint::Weak<Pillola>, pannello: slint::Weak<Pannello>, cervello: Rc<RefCell<Cervello>>)`: ogni `CasaEvento` passa da `slint::invoke_from_event_loop`; `Stato` → `Segnale::Casa`; `Notifica` → `Segnale::Notifica` (+ toast nel Task 10); `Esito{ok:true}` → `ComandoRiuscito`; `Esito{ok:false}` → `ComandoFallito` + `errore` sulla voce; `Dati` → modello preferiti.
  - Vista Casa: elenco preferiti, tocco = `comando_per` → `casa.comando(..)`. Layout: Orologio (ora grande, data, una riga di stato casa), Quadranti (CPU, RAM, disco + sensori preferiti), Colori (tessere dei preferiti, colore solo se acceso), Tutte le app (griglia di `app_disponibili()`); dettagli grafici lasciati al costruttore entro le bozze.

- [ ] **Step 1: test (falliscono)**

```rust
#[test] fn preferiti_nell_ordine_e_massimo_otto()
#[test] fn senza_preferiti_usa_le_prime_entita_delle_stanze()
#[test] fn sensore_mostra_valore_con_unita() // "21.5 °C"
#[test] fn comando_luce_interruttore_scena()
#[test] fn niente_comando_su_entita_non_disponibile()
#[test] fn niente_comando_su_sensore()
```

- [ ] **Step 2:** FAIL → implementa → `cargo test -p assistente ponte` PASS.
- [ ] **Step 3:** prova dal vivo con l'HA di casa (solo luci/interruttori/scene innocui, **niente che suona o allarma** senza ok di Paolo): tocco → stato cambia, pillola "contento" 3 s; entità non disponibile → nessun comando.
- [ ] **Step 4:** commit `feat(assistente): casa nel pannello e umori dagli eventi`.

> **Ripianificazione del 13/11 sera.** Dopo il Task 8 Paolo ha deciso la forma definitiva (800×300, pillola al bordo alto → Task 8b) e l'architettura dell'interfaccia (spec §3bis, con le precisazioni dopo la revisione del costruttore). I Task 9–12 originali sono sostituiti dai Task 9–14 qui sotto. Riferimenti UX: `.superpowers/ux/decisioni.md`, `.superpowers/ux/analisi-architettura-informazioni.md`, bozza approvata `.superpowers/ux/bozza-in-attesa.html`.

### Task 8b: Forma definitiva, pillola al bordo alto, glitch bianco

Brief a parte (`.superpowers/sdd/2026-11-13-assistente-m1-fondamenta/task-8b-brief.md`): pannello 800×300 al bordo alto, pillola invariata ma attaccata al bordo alto, causa ed eliminazione della striscia bianca, adattamento minimo dei contenuti.

### Task 9: Panoramica, navigazione e dock nuovi

**Files:**
- Modify: `ui/pannello.slint`, `ui/layout/{orologio,quadranti}.slint`, `ui/viste/casa.slint`, `src/{layout,registro,pannello,ponte,scelte}.rs`, `crates/casa-core/src/ha/mapper.rs`
- Delete: `ui/layout/{colori,tutte}.slint`

**Interfaces:**
- Consumes: Task 8/8b (pannello 800×300, `Ponte`, `CasaService::aggiorna_config`).
- Produces:
  - `enum Layout { Orologio, Quadranti }` (`come_str` → `"orologio" | "quadranti"`; `da_str` di valori vecchi `"colori"`/`"tutte"`/sconosciuti → `Orologio`); `fn successivo(self) -> Layout`, `fn precedente(self) -> Layout` (ciclici).
  - `fn app_disponibili() -> Vec<VoceApp>` → id `["panoramica", "casa", "pc", "impostazioni"]`, nomi `"Panoramica"`, `"Casa"`, `"PC"`, `"Impostazioni"`.
  - Pannello: `in-out property <string> vista` (`"panoramica"` predefinita); riga alta con `«‹ Indietro»` a sinistra **solo** se `vista != "panoramica"` (clic o Backspace/Alt+← → torna alla Panoramica); puntini sotto il contenuto **solo** nella Panoramica (clic sul puntino, frecce ←/→, rotella → `scegli-layout`); dock di **sole icone**, bersagli 36×32, icona spenta con contrasto ≥ 3:1 sul fondo, tooltip col nome.
  - Panoramica = sintesi senza comandi: Orologio (ora, data, una riga di stato per Casa e PC), Quadranti (CPU, RAM, disco + fino a 3 sensori preferiti). Accendi/spegni solo nella vista **Casa**.
  - Preferiti **tenui e non cliccabili** se la connessione a HA non è `Connesso` (`fn voce_toccabile(p: &Preferito, connesso: bool) -> bool`).
  - Mapper: le scene in `favorites` compaiono nei preferiti (prima escluse).
  - Testo ≥ 12 px ovunque; navigazione completa da tastiera (Tab tra dock e contenuto, Invio attiva, Esc chiude).

- [ ] **Step 1: test (falliscono)**

```rust
#[test] fn layout_solo_due_e_ciclici() // successivo(Orologio)==Quadranti, successivo(Quadranti)==Orologio, precedente idem
#[test] fn layout_vecchi_diventano_orologio() // da_str("colori")==Orologio, da_str("tutte")==Orologio
#[test] fn app_disponibili_con_panoramica() // ids == ["panoramica","casa","pc","impostazioni"]
#[test] fn preferiti_non_toccabili_se_disconnesso() // voce_toccabile(luce, false)==false; (luce,true)==true; (sensore,true)==false
// casa-core mapper:
#[test] fn scena_preferita_nei_preferiti() // favorites ["scene.sera"] → menu.favorites contiene scene.sera
```

- [ ] **Step 2:** FAIL → implementa → `cargo test --workspace` PASS.
- [ ] **Step 3:** verifica dal vivo con HA finto (`ITSYHOME_ASSISTENTE_DIR`): navigazione dock/Indietro/puntini con mouse e tastiera; HA finto spento → preferiti tenui; foto di zona di Panoramica (2 layout) e Casa confrontate con `bozza-in-attesa.html` (senza la riga In attesa, che arriva nel Task 10); misure pannello ≤ 30 MB, ~0% CPU.
- [ ] **Step 4:** commit `feat(assistente): panoramica, navigazione e dock di sole icone`.

### Task 10: Attenzione: riga «In attesa», livelli e pillola che chiama

**Files:**
- Create: `crates/assistente/src/attenzione.rs`, `ui/attesa.slint`
- Modify: `src/{umore,ponte,pillola,pannello,main,pc_stats}.rs`, `ui/{pillola,pannello}.slint`, `crates/casa-core/src/{eventi,servizio}.rs`, `crates/casa-core/src/ha/{client,eventi}.rs`, `crates/casa-core/examples/finto_ha.rs`

**Interfaces:**
- Produces:
  - `enum Livello { Bloccante, Attenzione, Informazione }`; `enum Fonte { Casa, Pc, Prova }`; `struct Pulsante { id: String, testo: String }`; `struct Voce { id: String, livello: Livello, fonte: Fonte, titolo: String, dettaglio: String, pulsanti: Vec<Pulsante>, creata: Instant }` (massimo 2 pulsanti).
  - `struct Attese`; `fn metti(&mut self, v: Voce)` (stesso id = aggiorna), `fn togli(&mut self, id: &str)`, `fn ordinate(&self) -> Vec<&Voce>` (bloccante > attenzione > informazione, poi la più recente), `fn piu_urgente(&self) -> Option<&Voce>`.
  - Comportamento della pillola (spec §3bis, precisazioni): `fn aspetto_pillola(attese: &Attese, ora: Instant) -> AspettoPillola` con `enum AspettoPillola { Riposo, Puntino, Allargata { voce_id: String, ferma: bool } }`: bloccante → `Allargata{ferma:true}` finché c'è; attenzione → `Allargata{ferma:false}` per 20 s dalla creazione, poi `Puntino`; sola informazione → `Puntino`; niente → `Riposo`.
  - `Cervello`: «ti cerca» se c'è almeno una voce bloccante o attenzione; l'informazione non cambia umore. Aura **quasi spenta a riposo** (tranquillo), accesa negli altri umori; in «dorme» nessun ridisegno.
  - Pillola allargata: icona della fonte a sinistra degli occhi, una riga di testo, fino a 2 pulsanti (clic → azione, senza aprire il pannello); bloccante con bordo più spesso e fermo; clic sulla pillola (fuori dai pulsanti) → pannello aperto sulla voce.
  - Riga «In attesa» in cima alla Panoramica (fino a 3 voci visibili + «+N»), barretta colorata per livello (rosso/ambra/blu, colore sempre accompagnato dalla parola del livello sotto il titolo); clic → espande la voce sul posto (dettaglio + pulsanti); riga assente se vuota.
  - Fonti di M1: (a) notifiche persistenti HA = informazione, pulsante «Archivia» → `persistent_notification.dismiss`; la voce sparisce quando HA la rimuove. **Prima di scrivere codice verificare nella documentazione/sorgente di Home Assistant** il comando WS per seguire aggiunte e rimozioni (atteso `persistent_notification/subscribe` con eventi added/removed/current/updated) e il servizio di archiviazione; casa-core produce `CasaEvento::NotificaAggiunta { id, titolo, messaggio }` e `CasaEvento::NotificaRimossa { id }` e offre `CasaService::archivia_notifica(id)`; il finto HA le simula. (b) batteria PC < 10% e scollegata = attenzione, sparisce in carica o sopra soglia (controllo ogni 60 s anche a pannello chiuso: unica eccezione al campionamento solo a pannello aperto, solo batteria). (c) HA non connesso da più di 5 minuti = attenzione, pulsante «Riprova» → `connetti_attivo()`, sparisce a connessione tornata.
  - Notifica di sistema: hook pronto (`fn notifica_sistema(v: &Voce)`), collegato a `notify-rust` nel Task 12; chiamata per bloccante, attenzione e notifiche HA.
  - Flag di prova `--prova-avviso <bloccante|attenzione|informazione>`: crea una voce `Fonte::Prova` con 2 pulsanti finti.

- [ ] **Step 1: test (falliscono)**

```rust
#[test] fn ordine_per_livello_poi_recenti()
#[test] fn stesso_id_aggiorna_non_duplica()
#[test] fn bloccante_allarga_ferma_finche_ce() // Allargata{ferma:true} a +0 e a +10 min
#[test] fn attenzione_allarga_venti_secondi_poi_puntino() // +19,9 s Allargata{ferma:false}; +20,1 s Puntino
#[test] fn sola_informazione_puntino()
#[test] fn vuoto_riposo()
#[test] fn al_massimo_due_pulsanti() // metti con 3 pulsanti → tenuti i primi 2
#[test] fn ti_cerca_solo_per_bloccante_e_attenzione() // Cervello
#[test] fn ha_offline_cinque_minuti_diventa_attenzione() // 4:59 nessuna voce, 5:01 voce "Home Assistant non raggiungibile"
#[test] fn batteria_sotto_dieci_scollegata() // 9% scollegata → voce; 9% in carica → nessuna; 11% → nessuna
// casa-core:
#[test] fn notifica_aggiunta_e_rimossa_come_eventi() // con finto HA
#[test] fn archivia_chiama_il_servizio() // finto HA riceve il dismiss con l'id giusto
```

- [ ] **Step 2:** FAIL → implementa → `cargo test --workspace` PASS.
- [ ] **Step 3:** prova dal vivo con HA finto e `--prova-avviso` per i 3 livelli: pillola, riga In attesa, pulsanti, espansione, archiviazione; foto di zona confrontate con `bozza-in-attesa.html`; misure: pillola allargata ferma ~0% CPU, puntino ~0% CPU, riposo ≤ 0,1%.
- [ ] **Step 4:** commit `feat(assistente): attenzione, riga In attesa e pillola che chiama`.

### Task 11: Impostazioni dentro il pannello

(Contenuto del vecchio Task 9, invariato salvo:) vista `impostazioni` del dock; mostra anche l'errore dell'import v2 e `CasaEvento::ConfigNonSalvata`; rimuovere gli `#[allow(dead_code)]` su `errore_import`/`errore_config`; «Made with Slint» e licenza MIT nella sezione Informazioni.

**Interfaces:** `fn valida_url(s: &str) -> Result<String, String>` (aggiunge `http://` se manca lo schema, rifiuta vuoto e spazi, toglie `/` finale).

- [ ] **Step 1: test (falliscono):** `url_senza_schema_diventa_http` ("192.168.1.95:8123" → "http://192.168.1.95:8123"), `url_slash_finale_tolto`, `url_vuoto_o_con_spazi_rifiutato`.
- [ ] **Step 2:** FAIL → implementa → PASS.
- [ ] **Step 3:** prova dal vivo con HA finto: token sbagliato → errore leggibile e pillola mai bloccata su «pensa»; token giusto → connesso; rimozione con conferma che nomina l'ambiente → token cancellato dal portachiavi (solo token di prova).
- [ ] **Step 4:** commit `feat(assistente): impostazioni nel pannello`.

### Task 12: Servizi di sistema e modalità «ritirata»

(Contenuto del vecchio Task 10 — istanza unica, tray con «Apri», «Impostazioni», «Mostra l'isola», «Esci», autoavvio, `notify-rust` collegato a `notifica_sistema`, schermo intero, inattività — più:)
- Il timer da 5 s riapplica anche la posizione della pillola (cambi di area di lavoro/risoluzione senza eventi di finestra).
- **Ritirata**: clic destro sulla pillola o voce della tray → la pillola diventa una linea 300×4 px al bordo alto, colorata col livello più urgente, con **clic passanti** (Windows: `WS_EX_TRANSPARENT | WS_EX_LAYERED`); uscita da tray («Mostra l'isola»), scorciatoia globale **Ctrl+Alt+I** (crate `global-hotkey`), o automatica all'arrivo di un bloccante; non persiste al riavvio. Su macOS/Linux: compila, clic passanti dove la piattaforma lo consente.
- Notifica arrivata a pannello aperto: la voce compare subito nella riga In attesa (nessun toast).

**Interfaces:** `fn pillola_visibile(schermo_intero: bool, pannello_aperto: bool, ritirata: bool) -> Visibilita` con `enum Visibilita { Pillola, Linea, Nascosta }`.

- [ ] **Step 1: test (falliscono):** `nascosta_a_schermo_intero` ((true,_,_)→Nascosta salvo pannello aperto), `pannello_aperto_pillola_nascosta`, `ritirata_mostra_la_linea`, `bloccante_esce_dalla_ritirata` (decisione pura sul flag dopo l'arrivo di un bloccante).
- [ ] **Step 2:** FAIL → implementa → PASS.
- [ ] **Step 3:** prova dal vivo: seconda istanza esce; tray; autoavvio (chiave `HKCU\...\Run`, poi tolta); video a schermo intero; ritirata con clic che passano davvero alla finestra sotto; Ctrl+Alt+I; `--prova-avviso bloccante` fa uscire dalla ritirata.
- [ ] **Step 4:** commit `feat(assistente): servizi di sistema e modalità ritirata`.

### Task 13: Finestra di gestione (processo separato)

**Files:**
- Create: `crates/gestione/` (binario `itsyhome-gestione`: Slint per la cornice + wry per l'area HTML), `crates/assistente/src/gestione.rs`, `crates/gestione/assets/prova-mail.html`
- Modify: `Cargo.toml` radice (membro), `crates/assistente/src/main.rs`

**Interfaces:**
- Protocollo su stdin/stdout, una riga JSON per messaggio: verso la finestra `{"tipo":"apri","titolo":String,"html":String}`, `{"tipo":"chiudi"}`; verso l'assistente `{"tipo":"pronta"}`, `{"tipo":"chiusa"}`, `{"tipo":"azione","id":String}`.
- `fn messaggio_da_riga(s: &str) -> Result<Messaggio, String>` e `fn riga_da_messaggio(m: &Messaggio) -> String` (in un piccolo crate o modulo condiviso).
- `Gestione::apri(titolo: &str, html: &str) -> Result<(), String>` (avvia il processo se non c'è, invia `apri`), `Gestione::chiudi()`; processo figlio terminato e raccolto alla chiusura; nessun processo `msedgewebview2` orfano.
- Cornice Slint nello stile dell'assistente (fondo scuro, bordo coi colori dei petali, titolo, pulsante chiudi); area HTML con JavaScript spento, devtools spenti, navigazione bloccata salvo il caricamento iniziale (`fn consenti_navigazione(url: &str, gia_caricato: bool) -> bool`); piano B (testo semplice) se il motore web manca.
- Flag di prova dell'assistente `--prova-gestione`: apre la finestra con `prova-mail.html`, la chiude a 10 s, la riapre a 18 s.

- [ ] **Step 1: test (falliscono):** `andata_e_ritorno_del_protocollo`, `riga_malformata_e_errore`, `navigazione_consentita_solo_al_primo_caricamento` ((about:blank,false)→true, (https://x,true)→false).
- [ ] **Step 2:** FAIL → implementa → PASS.
- [ ] **Step 3:** `tools/misura.ps1 -Argomenti --prova-gestione -Secondi 50`: finestra aperta ≈ 140 MB solo mentre è aperta; dopo la chiusura RAM dell'assistente entro +2 MB e nessun processo figlio vivo; apertura < 1 s; foto della sola finestra.
- [ ] **Step 4:** commit `feat(gestione): finestra di gestione in processo separato`.

### Task 14: Verifica finale della milestone

(Contenuto del vecchio Task 12, più:)
- Misure con cicli ripetuti apri/chiudi del pannello (≥ 20) e della finestra di gestione (≥ 5): pillola dopo i cicli entro +2 MB dal valore dopo il primo ciclo, CPU 0 a riposo, nessun thread o processo che cresce.
- Prova a raffica anti-glitch ripetuta (nessun fotogramma con pixel chiari estranei).
- Revisione degli specialisti UI/UX e frontend sull'app vera (richiesta da Paolo): foto di zona di tutte le viste e stati, il codice `ui/`, le misure; correzioni prima della prova con Paolo.
- Prova dal vivo con Paolo sull'HA vero (azioni innocue scelte da lui) dei Review Focus 1–5 e delle decisioni UX.
