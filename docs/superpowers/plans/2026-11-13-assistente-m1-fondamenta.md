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
- Pannello: in alto occhi al centro e **solo l'ora a destra**; centro = 4 layout (Orologio, Quadranti, Colori, Tutte le app); dock centrato in basso. Nessuna finestra esterna per le funzioni.
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

### Task 9: Impostazioni dentro il pannello

**Files:**
- Create: `ui/viste/impostazioni.slint`
- Modify: `src/main.rs`, `src/ponte.rs`

**Interfaces:**
- Consumes: `CasaService::{config, aggiorna_config, prova_connessione, connetti, disconnetti}`, `Segreti`, `sistema::autoavvio` (Task 10; fino ad allora l'interruttore salva solo `launch_at_login`).
- Produces: vista `impostazioni` con: elenco ambienti (nome, URL, stato), aggiungi/modifica (nome, URL, token in campo password), **Prova connessione** (mostra n. entità o errore), **Rimuovi** con conferma che nomina l'ambiente, **Connetti**; interruttore "Avvia con il sistema"; sezione Informazioni con versione, "Made with Slint" e licenza MIT; avviso se il portachiavi non è disponibile. `fn valida_url(s: &str) -> Result<String, String>` (aggiunge `http://` se manca lo schema, rifiuta spazi e vuoto, toglie `/` finale).

- [ ] **Step 1: test (falliscono)**

```rust
#[test] fn url_senza_schema_diventa_http() // "192.168.1.95:8123" → "http://192.168.1.95:8123"
#[test] fn url_slash_finale_tolto()
#[test] fn url_vuoto_o_con_spazi_rifiutato()
```

- [ ] **Step 2:** FAIL → implementa → PASS.
- [ ] **Step 3:** prova dal vivo: aggiungi un ambiente con token sbagliato → errore leggibile, pillola non bloccata su "pensa"; token giusto → connesso; rimuovi → token cancellato dal portachiavi.
- [ ] **Step 4:** commit `feat(assistente): impostazioni nel pannello`.

### Task 10: Servizi di sistema

**Files:**
- Create: `src/sistema/{mod,tray,autoavvio,istanza,notifiche,schermo_intero,inattivita}.rs`, `assets/tray.png` (da `src-tauri/icons`)
- Modify: `src/main.rs`

**Interfaces:**
- Produces:
  - `istanza::prendi() -> Option<SingleInstance>` (nome `"itsyhome-assistente"`); se già attiva, il processo esce subito.
  - `tray::crea(on_apri, on_impostazioni, on_esci) -> TrayIcon` (`tray-icon` + `muda`, creata dopo l'avvio del ciclo eventi; eventi del menu inoltrati con `invoke_from_event_loop`); voci "Apri", "Impostazioni", "Esci".
  - `autoavvio::imposta(attivo: bool) -> Result<(), String>` (`auto-launch`, nome app `"ItsyHome"`, percorso `current_exe`).
  - `notifiche::mostra(titolo: &str, messaggio: &str)` (`notify-rust`, errori solo nel log).
  - `schermo_intero::attivo() -> bool`: Windows `SHQueryUserNotificationState` (`QUNS_BUSY`, `QUNS_RUNNING_D3D_FULL_SCREEN`, `QUNS_PRESENTATION_MODE` → true); altri sistemi `false`.
  - `inattivita::secondi() -> Option<u64>`: Windows `GetLastInputInfo`; altri `None`.
  - Un `slint::Timer` ogni 5 s controlla schermo intero (nasconde/mostra la pillola, mai il pannello se aperto) e inattività (`Segnale::Inattivo`).

- [ ] **Step 1: test (falliscono)** in `sistema/mod.rs` sulla decisione pura `fn pillola_visibile(schermo_intero: bool, pannello_aperto: bool) -> bool`:

```rust
#[test] fn nascosta_a_schermo_intero()      // (true,false) → false
#[test] fn visibile_normalmente()           // (false,false) → true
#[test] fn pannello_aperto_pillola_nascosta() // (_,true) → false
```

- [ ] **Step 2:** FAIL → implementa → PASS.
- [ ] **Step 3:** prova dal vivo: seconda istanza esce; tray Apri/Impostazioni/Esci; autoavvio attiva/disattiva (verifica chiave `HKCU\...\Run`); video a schermo intero → pillola sparisce e torna; notifica HA di prova (`persistent_notification.create` innocua) → toast + pillola "ti cerca" allargata.
- [ ] **Step 4:** commit `feat(assistente): tray, istanza unica, autoavvio, notifiche, schermo intero`.

### Task 11: Riquadro web dentro il pannello

**Files:**
- Create: `src/riquadro_web.rs`, `assets/prova-mail.html` (mail finta dello spike)
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `Pannello::{centro-x, centro-y, centro-l, centro-a}`.
- Produces:
  - `struct RiquadroWeb`; `RiquadroWeb::new(cartella_dati: PathBuf) -> Self`; `apri_html(&mut self, finestra: &slint::Window, html: &str, area: slint::LogicalRect) -> Result<(), String>` (wry `build_as_child`, `with_javascript_disabled`, `with_devtools(false)`, `with_navigation_handler` che consente solo il caricamento iniziale; Linux non-X11 → `Err("non supportato")`); `nascondi(&self, nascosto: bool)`; `chiudi(&mut self)` (distrugge WebView e WebContext); `aperto(&self) -> bool`.
  - Il riquadro si nasconde durante le animazioni del pannello e si chiude quando il pannello si chiude.
  - Flag di prova `--prova-riquadro`: apre il pannello e mostra `prova-mail.html` nel centro, si chiude a 10 s e si riapre a 18 s (come lo spike).

- [ ] **Step 1: test (fallisce)**

```rust
#[test] fn navigazione_consentita_solo_al_primo_caricamento() // fn consenti_navigazione(url, gia_caricato) : (about:blank,false)→true, (https://x,true)→false
```

- [ ] **Step 2:** FAIL → implementa → PASS.
- [ ] **Step 3:** `tools/misura.ps1 -Argomenti --prova-riquadro -Secondi 50` → dopo la chiusura RAM entro +2 MB dal valore prima, nessun `msedgewebview2` rimasto vivo; foto di zona: mail dentro il centro del pannello.
- [ ] **Step 4:** commit `feat(assistente): riquadro web nel pannello`.

### Task 12: Verifica finale della milestone

**Files:**
- Create: `.github/workflows/v3-build.yml` (build `cargo build -p assistente` su windows-latest, macos-latest, ubuntu-latest con le librerie di sistema per webkit2gtk/gtk/xdo/appindicator; `cargo test --workspace`)
- Modify: `README.md` (sezione "v3 in sviluppo" breve), `CHANGELOG.md`

- [ ] **Step 1:** `cargo test --workspace` → tutti PASS; `cargo clippy --workspace -- -D warnings` → pulito.
- [ ] **Step 2:** misure con HA di casa connesso (≥ 5 minuti per stato) con `tools/misura.ps1`: pillola ferma, pillola animata, pannello aperto fermo, riquadro web aperto/chiuso → tabella nel `CHANGELOG.md`; ogni valore nel budget dei Global Constraints, altrimenti si torna al task responsabile.
- [ ] **Step 3:** primo avvio pulito con il file v2 reale di Paolo presente: ambienti, preferiti e ordini importati; `assistente.json` senza token; file v2 invariato (hash prima/dopo).
- [ ] **Step 4:** prova dal vivo con Paolo dei Review Focus 1–5.
- [ ] **Step 5:** commit `chore(v3): CI multipiattaforma e verifica M1`; il workflow parte solo dopo un push autorizzato da Paolo.
