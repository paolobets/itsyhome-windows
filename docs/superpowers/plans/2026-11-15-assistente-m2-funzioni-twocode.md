# Assistente "isola" — M2 Funzioni TwoCode — Piano di implementazione

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** i quattro gruppi della «sessione funzioni TwoCode» decisi da Paolo il 15/11 sera: pillola informativa, robustezza e ritirata, pannello più comodo, «PC+» (volume, Wi-Fi/Bluetooth, batteria cuffie, GPU), senza perdere la leggerezza di M1.

**Architecture:** tutto dentro i due crate esistenti. `casa-core` impara solo il meteo (`weather.*`) e i campi nuovi di `Config`; il resto vive in `assistente`: logica pura in moduli piccoli e testati (`lati.rs`, `sistema/volume.rs`, `sistema/radio.rs`, `sistema/batterie_bt.rs`, aggiunte a `pc_stats.rs`, `finestre.rs`, `attenzione.rs`) e la parte di sistema dietro `#[cfg(windows)]`, con eventi nativi (callback di Core Audio, eventi WinRT, `SetWinEventHook`) su thread di lavoro che dormono e consegnano con `slint::invoke_from_event_loop`. Le sorgenti si accendono solo quando ciò che alimentano è visibile o scelto.

**Tech Stack:** Rust 2021, Slint `=1.18.1` (renderer Skia software), `windows-sys 0.52` (già usato), **`windows 0.62`** (già nel `Cargo.lock` tramite accesskit: Core Audio e WinRT), sysinfo 0.38, starship-battery 0.12, muda/tray-icon 0.26, chrono.

**Spec:** `docs/superpowers/specs/2026-11-13-assistente-isola-design.md` (in particolare §3bis e §4). Fonti delle funzioni: `.superpowers/ux/twocode-funzioni.md` (§3.3, §3.4, §4.1, §4.2 righe 15, 16, 21, 22), decisioni vincolanti in `.superpowers/ux/decisioni.md` (§12–13). Stile e convenzioni: `docs/superpowers/plans/2026-11-13-assistente-m1-fondamenta.md`.

---

## Inventario: cosa c'è già (ramo `v3-assistente`, commit `bde590a`)

Righe del codice al 15/11. «Già fatto» = solo verifica dal vivo, nessun codice.

| Gruppo | Funzione | Stato | Dove (file:riga) | Cosa manca |
|---|---|---|---|---|
| 1 | Pillola 124 × 32, occhi 6 × 10 | **già fatto** | `ui/faccia.slint:14-25`, `src/finestre.rs:39-41` | — |
| 1 | Lati degli occhi (niente/ora/batteria/CPU/RAM/rete/temperatura) | **da fare** | `ui/pillola.slint` non ha lati; `casa-core/src/config.rs:12-30` non ha i campi | tutto (Task 4) |
| 1 | CPU/RAM campionate | **da completare** | `src/pc_stats.rs:66-191` (`Campionatore`), solo a pannello aperto ogni 2 s (`src/pannello.rs:193-230`) | campionatore leggero ogni 5 s solo se un lato li chiede (Task 4) |
| 1 | Ora | **da completare** | `src/orologio.rs:11-21`; timer al minuto solo a pannello aperto (`src/pannello.rs:109-112, 234-245`) | timer al minuto per la pillola solo se il lato «Ora» è scelto (Task 4) |
| 1 | Lettura batteria | **da completare** | `src/pc_stats.rs:15-49` (thread ogni 60 s), `src/main.rs:142-146` | consegna solo «scollegata»: manca «in carica» per il glifo (Task 1) |
| 1 | Batteria 10 % = attenzione | **già fatto** | `src/attenzione.rs:227-230, 341-352`, `src/ponte.rs:1700-1706` | — |
| 1 | Batteria 20 % = informazione | **da fare** | — | Task 1 |
| 1 | Glifo batteria | **da fare** | `ui/viste/pc.slint:123-132` mostra solo testo e barra | Task 1 |
| 1 | Temperatura di una stanza | **da completare** | `src/ponte.rs:382-398` (prima temperatura per la riga di stato), `src/ponte.rs:429` (`sensori_quadranti`) | scelta del sensore per il lato (Task 4) |
| 1 | Temperatura esterna (`weather.*`) | **da fare** | `casa-core/src/ha/mapper.rs:29-55`: `weather` è fra i domini esclusi (riga 40) | Task 2 |
| 1 | Icona della rete | **da fare** | `src/pc_stats.rs:117-135` misura solo la velocità | Task 8 |
| 2 | Schermo intero con ritardi 3 s / 1,5 s | **già fatto** | `src/sistema/mod.rs:93-133` (`IsteresiSchermoIntero`), `src/sistema/servizi.rs:338-351` | — |
| 2 | … solo sullo schermo della pillola | **già fatto** | `src/sistema/schermo_intero.rs:34-46` (`sul_monitor`) | — |
| 2 | … rilevamento | **da completare** | solo nel controllo da 5 s (`src/sistema/servizi.rs:312-336`): fino a 5 s in più prima che partano i 3 s / 1,5 s | `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` (Task 5) |
| 2 | Ritorno in primo piano ogni 5 s senza fuoco | **già fatto** | `src/sistema/servizi.rs:325-329` + `src/finestre.rs:260-268` (`HWND_TOPMOST`, `SWP_NOACTIVATE`), solo se non nascosta | — |
| 2 | Linea 300 × 5 col colore del livello | **già fatto** | `ui/pillola.slint:172-180`, `src/pillola.rs:125-140`, `src/sistema/servizi.rs:281-288` | — |
| 2 | Linea che si **riempie** con l'avanzamento | **da fare** | — | Task 5 |
| 2 | Ritorno tenendo il mouse 450 ms | **già fatto** | `src/sistema/ritirata.rs:12-26, 51-63`, `src/sistema/servizi.rs:396-404` (sondaggio 100 ms solo da ritirata) | — |
| 2 | Scorciatoie Ctrl+Alt+Maiusc+Spazio / I (anche «salvataggio») | **già fatto** | `src/sistema/scorciatoie.rs:18-24, 74-75`, `src/sistema/servizi.rs:240-249` | — |
| 2 | Menu della pillola (clic destro), icona nella barra spenta di serie | **già fatto** | `src/sistema/menu.rs` (voci fisse `voci()`), `casa-core/src/config.rs:22-23` | — |
| 2 | Scelta del monitor | **da fare** | `src/finestre.rs:84-95`: `schermo_di` usa sempre `primary_monitor()` | Task 6 |
| 3 | Riga alta: ora a destra | **già fatto** | `ui/pannello.slint:695-705` | — |
| 3 | Riga alta: data (+ meteo) a sinistra nella Panoramica | **da fare** | a sinistra c'è solo «‹ Indietro» nelle app (`ui/pannello.slint:707-750`); la data sta nel layout Orologio (`ui/layout/orologio.slint:8, 56`) | Task 3 |
| 3 | Etichetta al passaggio sulle icone del dock | **già fatto** | `ui/pannello.slint:32-104` (sosta 600 ms, 12 px, subito col fuoco da tastiera) | solo verifica con Narrator (Task 3) |
| 3 | Scorrimento 26 px / 280 ms fra le viste | **da fare** | viste come `if vista == …` senza animazione (`ui/pannello.slint:798-905`); movimento ridotto pronto (`src/sistema/movimento.rs`, global `Movimento`) | Task 3 |
| 4 | Volume del PC (rotella + cursore) | **da fare** | la rotella c'è solo sulla Panoramica per i layout (`ui/pannello.slint:816-834`) | Task 7 |
| 4 | Pannellino Wi-Fi / Bluetooth | **da fare** | — | Task 8 |
| 4 | Batteria cuffie BT ≤ 20 % | **da fare** | — | Task 9 |
| 4 | GPU % nella vista PC | **da fare** | nota in `src/pc_stats.rs:7-9`: PDH lascia ~7 MB e ~100 handle dopo il primo uso | Task 10 |

---

## Decisioni di Paolo (il piano si esegue coi default)

Sono le sole scelte che cambiano cosa si costruisce. Ogni task che ne dipende lo dice.

- **D1 — DECISIONE DI PAOLO: quale entità meteo.** Default: **automatica** = la prima `weather.*` disponibile in ordine di `entity_id`, con un selettore «Meteo da» nelle Impostazioni › Isola (Automatico + le `weather.*` presenti). Motivo: la maggior parte delle case ne ha una sola (`weather.forecast_casa` di Met.no); chi ne ha due sceglie una volta.
- **D2 — DECISIONE DI PAOLO: rotella sulla pillola = volume.** Default: **sì, sempre, con un interruttore** «Volume con la rotella sulla pillola» (acceso di serie) nelle Impostazioni › Isola. Nel pannello la rotella resta ai layout della Panoramica: sono due finestre diverse, quindi nessun conflitto (la pillola è nascosta quando il pannello è aperto).
- **D3 — DECISIONE DI PAOLO: dove stanno volume e Wi-Fi/Bluetooth nel pannello.** Default: **nella fascia del dock, fuori dalla capsula**: a sinistra l'icona della rete e quella del Bluetooth (aprono il pannellino 264 px sopra il dock), a destra l'altoparlante (al passaggio o col fuoco si apre il cursore 76 px). Visibili in tutte le viste, come in TwoCode, senza toccare la decisione 5 (il dock resta di sole app). Alternativa: tutto dentro la vista PC.
- **D4 — DECISIONE DI PAOLO: riga alta nel layout Orologio.** Default: **solo il meteo** («9° · Nuvoloso»), perché la data è già sotto l'ora grande; nei Quadranti **data + meteo** («VEN 13 NOV · 9°»). Alternativa: data + meteo in tutti e due.
- **D5 — DECISIONE DI PAOLO: avviso al 20 %.** Default: voce *informazione* con **«Ho capito»** che la zittisce fino alla prossima ricarica (o sopra il 22 %); il 10 % (*attenzione*) arriva comunque. Motivo: un puntino che non si può togliere per un'ora irrita.
- **D6 — DECISIONE DI PAOLO: avvisi batteria cuffie.** Default: **accesi di serie**, controllo ogni **15 minuti** (una enumerazione SetupAPI, pochi millisecondi), interruttore nelle Impostazioni › Isola. Windows non dà un evento per la batteria dei dispositivi BT.
- **D7 — DECISIONE DI PAOLO: GPU se PDH lascia memoria.** Default: si misura; se dopo aver chiuso la vista PC restano **più di 2 MB** (budget spec §4) si passa ai contatori del kernel grafico (`D3DKMTQueryStatistics`) invece di accettare la memoria in più.

I lati di serie sono **niente / niente** (deciso da Paolo: «di serie niente»).

---

## Global Constraints

- Ramo `v3-assistente`; `main` non si tocca. **Commit solo con l'ok esplicito di Paolo**, niente push né tag senza ok.
- Build e test **solo da PowerShell** (il `link` di Git Bash copre quello MSVC): `cargo test -p assistente`, `cargo test -p casa-core`, `cargo clippy --workspace --all-targets -- -D warnings`.
- **Leggerezza** (spec §4): riposo della pillola **non peggiore di M1** (≤ 15 MB RAM privata, ≤ 0,1 % CPU con lati «niente»; misura M1 sulla casa vera: 0,193 % / 17 MB, da non superare); pannello aperto fermo ≤ 30 MB, ~0 % CPU; dopo chiusura di vista/pannellino ritorno entro +2 MB.
- **Nessun polling** di ciò che non è visibile o non è scelto. Si preferiscono eventi nativi: callback Core Audio (`IAudioEndpointVolumeCallback`, `IMMNotificationClient`), eventi WinRT (`Radio.StateChanged`, `NetworkInformation.NetworkStatusChanged`), `SetWinEventHook`. Gli unici intervalli ammessi sono quelli scritti nei task (CPU/RAM 5 s, segnale Wi-Fi 30 s, batteria 60 s già esistente, cuffie 15 min, GPU 2 s a vista PC aperta), tutti spenti quando non servono. In «dorme» nessun ridisegno (decisione 8).
- **Ridisegno solo quando cambia**: ogni proprietà Slint si riscrive solo se il valore è diverso (percentuali intere, glifo al pixel).
- **Niente PowerShell né processi esterni** per leggere o comandare il sistema (solo API native). Le pagine di Windows si aprono con `ShellExecuteW` su URI fissi.
- **Prima di usare un'API di sistema** (Core Audio, WinRT Radios, NetworkInformation, SetupAPI e chiave della batteria BT, PDH, D3DKMT, `SetWinEventHook`, URI `ms-settings:`) l'implementatore **legge la documentazione Microsoft** (learn.microsoft.com) e annota nel commento del modulo la pagina e i vincoli trovati (thread, COM, valori di ritorno). Per Home Assistant vale lo stesso con developers.home-assistant.io (regola: mai ipotesi, prima la documentazione).
- **Test dal vivo**: input sintetico **solo** `PostMessage`/`SendMessage`/UI Automation verso finestre del **proprio PID** (o di un processo di prova lanciato dal test); mai clic o tasti reali, mai `taskkill` per nome (fermare solo i propri PID). Mai provare azioni rumorose senza ok: il volume nei test si **legge**, e dal vivo si abbassa soltanto e si rimette al valore di prima.
- **Nessun dato personale** in log, test, fixture, foto: niente nomi di rete (SSID), nomi di dispositivi BT, indirizzi MAC, entità vere della casa. Identificativi di dispositivo salvati solo come impronta (FNV-1a 64 bit).
- **Il nome della rete non compare mai nella pillola** (privacy da schermo condiviso); nel pannellino sì, mai nei log.
- Testi ≥ 12 px; contrasto testo ≥ 4,5:1, icone spente ≥ 3:1; colori **solo come token di ruolo in `ui/tema.slint`** (il test dei colori a mano fallisce altrimenti).
- **Accessibilità**: ogni controllo nuovo con ruolo e nome per Narrator, raggiungibile da tastiera (Tab, frecce, Invio, Esc), anello del fuoco visibile; movimento ridotto rispettato (nessuno scorrimento, solo dissolvenza ≤ 120 ms).
- **macOS/Linux**: compilano; ciò che dipende da Windows è **assente** (non disattivato a vista) o in sola lettura, e lo si dichiara in CHANGELOG e nelle Impostazioni («Solo su Windows»). Lati CPU/RAM/ora/batteria/temperature funzionano ovunque.
- Tutto in italiano: interfaccia, commenti, messaggi di commit, CHANGELOG.
- Ogni task aggiorna `CHANGELOG.md` (sezione Unreleased v3) nel proprio commit.

## Review Focus

1. **Monitor scelto scollegato, ricollegato o con nome cambiato** → la pillola va subito sul principale e torna da sola sul preferito appena ricompare, pannello compreso; mai fuori schermo (Task 6, test `monitor_scollegato_usa_il_principale_e_poi_torna`).
2. **Dispositivo audio predefinito cambiato o assente** (cuffie USB staccate mentre il pannello è aperto, PC senza uscita audio) → il volume segue il nuovo dispositivo; senza dispositivo la rotella non fa nulla e l'altoparlante non c'è; nessun crash né thread bloccato (Task 7, test `senza_dispositivo_audio_niente_volume`).
3. **Meteo o sensore non disponibile** (`unavailable`, `unknown`, senza `temperature`, entità tolta, casa disconnessa) → lato e riga alta senza valore, mai «0°» né «nan°» (Task 2 `meteo_non_disponibile_e_nessuno`, Task 4 `lato_senza_dato_resta_vuoto`).
4. **Windows rifiuta il cambio della radio** (modalità aereo, criteri aziendali, accesso negato, nessun adattatore) → «Non riuscito» in ambra, si apre la pagina giusta delle Impostazioni di Windows, il riquadro torna allo stato reale (Task 8, test `rifiuto_mostra_errore_e_stato_reale`).
5. **Cuffie BT spente o disconnesse con un valore di batteria vecchio** → nessun avviso; una sola volta al giorno per dispositivo anche dopo un riavvio dell'app (Task 9, test `dispositivo_non_connesso_non_avvisa` e `una_volta_al_giorno_anche_dopo_il_riavvio`).

---
## Struttura dei file

```
crates/casa-core/src/types.rs          + Meteo, MenuData.meteo, MenuData.meteo_elenco
crates/casa-core/src/ha/mapper.rs      + meteo_da, scegli_meteo (weather resta fuori dalle stanze)
crates/casa-core/src/servizio.rs       + meteo seguito e aggiornato senza rimappare
crates/casa-core/src/config.rs         + lato_sinistro, lato_destro, meteo_entita, schermo, volume_rotella, avvisi_bt, bt_avvisati
crates/casa-core/examples/finto_ha.rs  + weather.casa
crates/assistente/src/lati.rs          NUOVO: Lato, scelte, bisogni, testi dei lati
crates/assistente/src/sorgenti_lati.rs NUOVO: accende e spegne le sorgenti dei lati (timer 5 s, minuto, rete)
crates/assistente/src/sistema/volume.rs      NUOVO: rotella pura + Core Audio (Windows)
crates/assistente/src/sistema/radio.rs       NUOVO: Wi-Fi/BT (WinRT Radios) + stato della rete
crates/assistente/src/sistema/batterie_bt.rs NUOVO: batteria dei dispositivi BT (SetupAPI)
crates/assistente/src/sistema/primo_piano.rs NUOVO: SetWinEventHook(EVENT_SYSTEM_FOREGROUND)
crates/assistente/src/pc_stats.rs      + LetturaBatteria, CampionatoreLeggero, GPU
crates/assistente/src/attenzione.rs    + SorveglianzaBatteria (20 % / 10 %), voci BT, Avanzamento
crates/assistente/src/finestre.rs      + InfoMonitor, scegli_monitor, etichetta_monitor
crates/assistente/src/sistema/menu.rs  + sottomenu «Schermo»
crates/assistente/src/sistema/browser.rs + PaginaWindows, apri_pagina
crates/assistente/ui/componenti.slint  + GlifoBatteria, Scelta (selettore), IconaRete
crates/assistente/ui/pillola.slint     + lati, barra del volume, linea con avanzamento
crates/assistente/ui/pannello.slint    + riga alta sinistra, scorrimento delle viste, fascia del dock
crates/assistente/ui/connessioni.slint NUOVO: pannellino Wi-Fi/Bluetooth
crates/assistente/ui/impostazioni.slint + sezione «Isola»
crates/assistente/ui/viste/pc.slint    + glifo batteria, riga GPU
crates/assistente/examples/schermo_pieno.rs NUOVO: finestra di prova a schermo intero (processo lanciato dal test)
```

---

### Task 1: Batteria: glifo e soglia 20 % = informazione

**Files:**
- Modify: `crates/assistente/src/pc_stats.rs:15-49`, `src/attenzione.rs:214-230, 333-352`, `src/ponte.rs:518-523, 1700-1706`, `src/main.rs:142-146`, `src/pannello.rs:126-153` (`stat_ui`), `ui/componenti.slint`, `ui/viste/pc.slint:123-132`, `ui/voci.slint:28-45`, `ui/tema.slint`, `CHANGELOG.md`
- Test: in `src/pc_stats.rs`, `src/attenzione.rs`, `src/test_ui.rs`

**Interfaces:**
- Consumes: `Attese`, `Voce`, `Livello`, `PULSANTE_HO_CAPITO` di M1.
- Produces:
  - `pub enum StatoBatteria { InCarica, Piena, Scarica, Sconosciuto }`; `pub struct LetturaBatteria { pub percentuale: f32, pub stato: StatoBatteria }` con `fn scollegata(&self) -> bool` (= `Scarica`).
  - `pub fn sorveglia_batteria(consegna: impl Fn(LetturaBatteria) -> bool + Send + 'static)` (firma nuova, stesso periodo 60 s).
  - `pub enum TintaBatteria { Carica, Normale, Informazione, Attenzione }`; `pub fn tinta_batteria(l: &LetturaBatteria) -> TintaBatteria`: in carica o piena → `Carica`; scarica ≤ 10 → `Attenzione`; scarica ≤ 20 → `Informazione`; altrimenti `Normale`.
  - `pub const SOGLIA_BATTERIA_INFO: f32 = 20.0; pub const SOGLIA_BATTERIA_INFO_FINE: f32 = 22.0;`
  - `pub struct SorveglianzaBatteria { zittita: bool }` con `fn osserva(&mut self, l: &LetturaBatteria, presente: Option<Livello>, ora: Instant) -> Option<Voce>` e `fn zittisci(&mut self)`. Regole: scollegata e < 10 (12 se già *attenzione*) → *attenzione* (titolo e dettaglio di oggi, nessun pulsante); altrimenti scollegata e < 20 (22 se già presente) e non zittita → *informazione*, titolo «Batteria al 19%», dettaglio «Collega l'alimentatore quando puoi.», pulsante `PULSANTE_HO_CAPITO` «Ho capito»; in carica o ≥ 22 → `None` e `zittita = false`. Stesso `ID_BATTERIA` per i due livelli (D5).
  - Slint `export component GlifoBatteria`: corpo 17 × 8, raggio 2, bordo 1 px, beccuccio 2 × 4, riempimento proporzionale a `frazione`; colore da `tinta` (`Carica` → nuovo token `Tema.batteria-carica`, verde da `petalo-5`; `Informazione`/`Attenzione` → `Tema.colore-livello(...)`; `Normale` → `Tema.testo`); `accessible-role: none` (il testo accanto dice tutto). Sul pixel fisico (`Tema.sul-pixel`).
  - `StatPcUi` + `tinta-batteria: int` (0..3 nell'ordine di `TintaBatteria`).

- [ ] **Step 1: test (falliscono)**

```rust
// pc_stats.rs
#[test] fn tinta_della_batteria() // (50,InCarica)→Carica; (15,Piena)→Carica; (10,Scarica)→Attenzione; (20,Scarica)→Informazione; (21,Scarica)→Normale; (5,Sconosciuto)→Normale
// attenzione.rs
#[test] fn venti_per_cento_e_informazione_con_ho_capito() // 19 scarica → Informazione, pulsanti == [«Ho capito»], titolo "Batteria al 19%"
#[test] fn dieci_per_cento_resta_attenzione() // 9 scarica → Attenzione, nessun pulsante (come in M1)
#[test] fn ho_capito_zittisce_fino_alla_ricarica() // zittisci(); 18 → None; 9 → Attenzione; in carica → None; poi 19 scarica → Informazione di nuovo
#[test] fn isteresi_ventidue() // presente Informazione: 21,5 → Some; 22 → None; non presente: 21 → None
```
Il test M1 `batteria_con_isteresi` si adatta alla firma nuova senza cambiare le attese.

- [ ] **Step 2:** `cargo test -p assistente batteria` → FAIL (tipi mancanti).
- [ ] **Step 3:** implementa: `LetturaBatteria` dallo `starship_battery::State`; `SorveglianzaBatteria` nel `Ponte` al posto di `voce_batteria_con_isteresi`; «Ho capito» su `ID_BATTERIA` chiama `zittisci()` e toglie la voce; `GlifoBatteria` accanto al valore nella riga BATTERIA della vista PC.
- [ ] **Step 4:** `cargo test -p assistente` → PASS (compresi `misure_slint_e_rust` e il test dei colori a mano).
- [ ] **Step 5:** prova dal vivo sul portatile di Paolo, senza scaricare nulla: vista PC col glifo in carica e (se Paolo stacca l'alimentatore) a batteria; aspetto della voce 20 % con `--prova-avviso informazione`; foto di zona della vista PC a 100 % e 175 %.
- [ ] **Step 6:** commit `feat(assistente): glifo della batteria e avviso al 20%` (con l'ok di Paolo).

### Task 2: Meteo da Home Assistant in `casa-core`

**Files:**
- Modify: `crates/casa-core/src/types.rs:247-268`, `src/ha/mapper.rs`, `src/servizio.rs:196-312, 650-735`, `src/config.rs:12-30`, `examples/finto_ha.rs`, `tests/finto_ha.rs`, `CHANGELOG.md`

**Interfaces:**
- Produces:
  - `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] pub struct Meteo { pub entity_id: String, pub condizione: String, pub temperatura: Option<f64>, pub unita: String }`
  - `MenuData { …, #[serde(default)] pub meteo: Option<Meteo>, #[serde(default)] pub meteo_elenco: Vec<VoceCatalogo> }` (`kind = "weather"`, per il selettore D1).
  - `Config { …, #[serde(default)] pub meteo_entita: Option<String> }` (`None` = automatico).
  - `pub fn scegli_meteo<'a>(stati: &'a [HaState], preferita: Option<&str>) -> Option<&'a HaState>`: la preferita se esiste (anche se non disponibile: allora `meteo: None`, non un'altra); altrimenti la prima `weather.*` disponibile per `entity_id`.
  - `pub fn meteo_da(stato: &HaState, unita_casa: &str) -> Option<Meteo>`: `None` se `state` è `unavailable` o `unknown`; `temperatura` da `attributes.temperature` se numero; `unita` da `attributes.temperature_unit`, altrimenti `unita_casa`.
  - `servizio.rs`: l'entità meteo scelta entra in `da_seguire` e non viene alleggerita; `Casa::applica` restituisce il nuovo `Effetto::Meteo(Option<Meteo>)` quando cambia lei: si scrive `Arc::make_mut(&mut casa.menu).meteo` e si pubblica come una sostituzione (niente mappatura completa). Cambiare `meteo_entita` fa rimappare.
- `weather` resta in `EXCLUDED_DOMAINS`: non entra in stanze né preferiti.

- [ ] **Step 0:** leggere su developers.home-assistant.io la pagina «Weather entity»: stati ammessi (`clear-night`, `cloudy`, `exceptional`, `fog`, `hail`, `lightning`, `lightning-rainy`, `partlycloudy`, `pouring`, `rainy`, `snowy`, `snowy-rainy`, `sunny`, `windy`, `windy-variant`) e attributi `temperature`, `temperature_unit`; le previsioni (`weather.get_forecasts`) **non** servono. Annotare la pagina in testa a `meteo_da`.
- [ ] **Step 1: test (falliscono)**

```rust
// mapper.rs
#[test] fn meteo_dalla_prima_weather_in_ordine() // weather.b e weather.a disponibili, preferita None → weather.a
#[test] fn meteo_preferita_anche_se_seconda() // preferita "weather.b" → weather.b
#[test] fn meteo_non_disponibile_e_nessuno() // state "unavailable" → None; temperature "abc" → temperatura None, condizione tenuta
#[test] fn meteo_unita_dalla_casa_se_manca() // senza temperature_unit, casa "°F" → unita "°F"
#[test] fn weather_fuori_da_stanze_e_preferiti() // weather.a in un'area e nei preferiti → assente da rooms/favorites, presente in meteo
// servizio.rs
#[test] fn cambio_del_meteo_aggiorna_senza_rimappare() // applica(weather.a 9→10) == Effetto::Meteo(Some(.. temperatura: Some(10.0) ..))
#[test] fn meteo_seguito_nell_iscrizione() // da_seguire contiene "weather.a"
// tests/finto_ha.rs
#[test] fn il_finto_ha_porta_il_meteo() // MenuData.meteo: condizione "cloudy", temperatura Some(9.0)
```

- [ ] **Step 2:** `cargo test -p casa-core meteo` → FAIL.
- [ ] **Step 3:** implementa; nel finto HA aggiungi `weather.casa` (`cloudy`, `temperature: 9`, `temperature_unit: "°C"`) e un comando per cambiarne lo stato.
- [ ] **Step 4:** `cargo test -p casa-core` e `cargo test -p assistente` → PASS.
- [ ] **Step 5:** commit `feat(casa-core): meteo da weather.* di Home Assistant` (con l'ok di Paolo).

### Task 3: Pannello più comodo: riga alta con data e meteo, scorrimento fra le viste

**Files:**
- Modify: `crates/assistente/src/orologio.rs`, `src/pannello.rs:93-112, 338-380`, `src/ponte.rs`, `ui/pannello.slint:667-750, 790-905`, `ui/tema.slint`, `CHANGELOG.md`
- Test: `src/orologio.rs`, `src/ponte.rs`, `src/test_ui.rs`

**Interfaces:**
- Consumes: `MenuData.meteo` (Task 2); `Layout` di M1.
- Produces:
  - `pub fn data_breve(t: NaiveDateTime) -> String` → `"VEN 13 NOV"` (LUN MAR MER GIO VEN SAB DOM; GEN FEB MAR APR MAG GIU LUG AGO SET OTT NOV DIC).
  - `pub fn al_prossimo_minuto(secondo: u32, nano: u32) -> Duration` spostata da `pannello.rs` a `orologio.rs` (la usa anche il Task 4).
  - `pub fn condizione_italiana(c: &str) -> &'static str`: sunny e clear-night → «Sereno», partlycloudy → «Poco nuvoloso», cloudy → «Nuvoloso», fog → «Nebbia», rainy → «Pioggia», pouring → «Pioggia forte», lightning e lightning-rainy → «Temporale», snowy → «Neve», snowy-rainy → «Nevischio», hail → «Grandine», windy e windy-variant → «Vento», exceptional → «Eccezionale», altro → «».
  - `pub fn temperatura_breve(m: &Meteo) -> Option<String>` → `"9°"` (arrotondata; `None` senza temperatura).
  - `pub fn riga_sinistra(layout: Layout, data: &str, meteo: Option<&Meteo>) -> (String, String)` (testo, nome per Narrator). D4: Quadranti → `"VEN 13 NOV · 9°"`, senza meteo `"VEN 13 NOV"`; Orologio → `"9° · Nuvoloso"`, senza meteo `""`. Nome per Narrator: «venerdì 13 novembre, 9 gradi, nuvoloso».
  - Pannello: `in property <string> riga-sinistra`, `in property <string> riga-sinistra-nome`; mono 12 px (`Tema.testo-piccolo`, `Tema.carattere-mono`), colore `Tema.testo-tenue` (contrasto ≥ 4,5:1 sul fondo); solo con `vista == "panoramica"` (nelle app resta «‹ Indietro»).
  - Scorrimento: `out property <length> scarto-contenuto` sul contenitore delle viste (riga alta e dock fermi). Dalla scheda `a` alla scheda `b` del dock il contenuto parte da `+26px` se `b > a`, `-26px` se `b < a`, e arriva a 0 in **280 ms** `ease-out` con opacità 0 → 1; «Indietro» conta come verso la Panoramica (indice 0). Tecnica: `animate x { duration: in-arrivo ? 280ms : 0ms; }` + un `Timer` Slint da 16 ms che mette `in-arrivo = true` e lo scarto a 0. **Movimento ridotto**: niente scarto, solo opacità in 120 ms. Nessun ridisegno fuori dai 280 ms.

- [ ] **Step 1: test (falliscono)**

```rust
// orologio.rs
#[test] fn data_breve_venerdi_13_novembre() // (2026,11,13) → "VEN 13 NOV"; (2027,1,3) → "DOM 3 GEN"
// ponte.rs
#[test] fn riga_alta_quadranti_con_e_senza_meteo() // (Quadranti, "VEN 13 NOV", Some(9°, cloudy)) → "VEN 13 NOV · 9°"; None → "VEN 13 NOV"
#[test] fn riga_alta_orologio_solo_meteo() // (Orologio, …, Some) → "9° · Nuvoloso"; None → ""
#[test] fn meteo_senza_temperatura_niente_gradi() // temperatura None, Quadranti → "VEN 13 NOV", nessun "°"
// test_ui.rs (backend di prova + i_slint_backend_testing::mock_elapsed_time)
#[test] fn scorrimento_dal_lato_della_scheda() // panoramica→pc: scarto-contenuto 26px subito, 0px dopo 300 ms; pc→panoramica: -26px
#[test] fn scorrimento_spento_col_movimento_ridotto() // Movimento.ridotto = true: scarto-contenuto resta 0px
#[test] fn riga_sinistra_solo_nella_panoramica() // vista "casa": riga sinistra non visibile, «Indietro» sì
```

- [ ] **Step 2:** `cargo test -p assistente` → FAIL.
- [ ] **Step 3:** implementa; il ponte riscrive `riga-sinistra` solo se cambia (minuto nuovo per la data, `MenuData.meteo` nuovo, cambio di layout).
- [ ] **Step 4:** `cargo test -p assistente` → PASS.
- [ ] **Step 5:** prova dal vivo con HA finto (`ITSYHOME_ASSISTENTE_DIR`): riga alta nei due layout con e senza `weather.casa`; cambio del meteo dal finto HA che arriva senza riaprire; scorrimento col mouse e con le frecce del dock; «Effetti di animazione» spento in Windows (lo cambia Paolo) → solo dissolvenza; **Narrator** sull'etichetta del dock (fatta in M1: solo verifica) e sulla riga alta; `tools/misura.ps1`: picco di CPU durante lo scorrimento annotato, ~0 % a fine animazione; foto di zona.
- [ ] **Step 6:** commit `feat(assistente): data e meteo nella riga alta, viste che scorrono` (con l'ok di Paolo).

### Task 4: Lati della pillola e sezione «Isola» nelle Impostazioni

**Files:**
- Create: `crates/assistente/src/lati.rs`, `src/sorgenti_lati.rs`
- Modify: `crates/casa-core/src/config.rs:12-30`, `crates/assistente/src/pc_stats.rs`, `src/pillola.rs`, `src/ponte.rs`, `src/pannello.rs:355-372` (partenza degli occhi), `src/finestre.rs:36-70`, `src/finestra_impostazioni.rs:18-55`, `src/main.rs`, `ui/pillola.slint`, `ui/componenti.slint`, `ui/impostazioni.slint:577, 861-870, 1739-1835`, `ui/voci.slint`, `CHANGELOG.md`
- Test: `src/lati.rs`, `src/pc_stats.rs`, `src/test_ui.rs`, `crates/casa-core/tests/config.rs`

**Interfaces:**
- Consumes: `LetturaBatteria`, `TintaBatteria`, `GlifoBatteria` (Task 1); `MenuData.meteo`, `meteo_elenco`, `Config.meteo_entita` (Task 2); `orologio::al_prossimo_minuto` (Task 3); `Visibilita` e `Umore` di M1.
- Produces:
  - `Config { …, #[serde(default)] pub lato_sinistro: String, #[serde(default)] pub lato_destro: String }` (`""` = niente; un file di M1 si legge con niente / niente).
  - `pub enum Lato { Niente, Ora, Batteria, Cpu, Ram, Rete, Meteo, Sensore(String) }`; `Lato::da_str(s: &str) -> Lato` (`"ora"`, `"batteria"`, `"cpu"`, `"ram"`, `"rete"`, `"meteo"`, `"sensore:<entity_id>"`; sconosciuto o vuoto → `Niente`); `fn come_str(&self) -> String`.
  - `pub fn scelte(meteo: bool, sensori: &[(String, String)], rete: bool) -> Vec<(Lato, String)>`: «Niente», «Ora», «Batteria», «CPU», «RAM», poi «Rete» solo se `rete` (falso fino al Task 8 e sempre fuori da Windows), «Temperatura esterna» solo se c'è un meteo, «Temperatura · <nome>» per ogni sensore di temperatura disponibile in `MenuData` (stanze e preferiti). «Batteria» solo se il PC ne ha una.
  - `pub struct Bisogni { pub ora: bool, pub batteria: bool, pub cpu_ram: bool, pub rete: bool, pub casa: bool }`; `pub fn bisogni(sinistro: &Lato, destro: &Lato) -> Bisogni`.
  - `pub struct DatiLati { pub ora: String, pub batteria: Option<LetturaBatteria>, pub cpu: Option<f32>, pub ram: Option<f32>, pub meteo: Option<String>, pub sensori: HashMap<String, String> }` (il campo `rete` lo aggiunge il Task 8).
  - `pub enum ContenutoLato { Vuoto, Testo(String), Batteria { testo: String, frazione: f32, tinta: TintaBatteria } }`; `pub fn contenuto(l: &Lato, d: &DatiLati) -> ContenutoLato`: «CPU 23%», «RAM 61%», «14:05», «80%» col glifo, «9°», «21,5°» (virgola italiana); CPU illeggibile → «CPU —»; meteo, sensore o batteria senza dato → `Vuoto` (mai «0°»).
  - `pub struct CampionatoreLeggero` in `pc_stats.rs` (solo CPU da `GetSystemTimes`/sysinfo e RAM, nessun disco né rete): `fn new() -> Self`, `fn campiona(&mut self) -> (Option<f32>, f32)`; `pub const PERIODO_LATI: Duration = Duration::from_secs(5)`.
  - `sorgenti_lati.rs`: `pub fn aggiorna(bisogni: Bisogni, visibile: bool, dorme: bool)` accende o spegne: timer Slint da 5 s col `CampionatoreLeggero` sul thread dell'interfaccia (le due letture costano microsecondi) solo se `cpu_ram && visibile && !dorme`; timer al minuto solo se `ora && visibile && !dorme`. Visibile = `Visibilita::Pillola` (non ritirata, non nascosta, pannello chiuso). La batteria resta sul thread da 60 s di M1; meteo e sensori arrivano dal ponte a ogni `MenuData`.
  - Pillola: `in property <ContenutoLatoUi> lato-sinistro`, `lato-destro` (struct con `tipo: int` 0 vuoto / 1 testo / 2 batteria / 3 rete, `testo`, `frazione`, `tinta`); testo `Tema.testo-corpo` (13 px) peso 600 in `Tema.carattere-mono` (cifre a larghezza fissa: il numero che cambia non fa ballare la pillola); margine 14, mezzi-occhi 19, distanza 12 (TwoCode). Larghezza a riposo: con due lati `max(124px, 2 × (14 + max(l_sx, l_dx) + 12 + 19))`; con un lato solo occhi e valore affiancati al centro, `max(124px, 14 + ingombro occhi + 12 + l + 14)`; sempre `≤ 440px` (finestra fissa: `LARGHEZZA_BLOCCANTE`). Animazione di larghezza come oggi (220 ms, 0 col movimento ridotto). Lati visibili a riposo e col puntino, nascosti da allargata e da ritirata. `out property <length> scarto-occhi-riposo` (0 con due lati o nessuno): il pannello lo usa come partenza degli occhi quando si apre dalla pillola a riposo (`pannello.rs:364-367`).
  - Narrator: il nome del pulsante della pillola diventa «ItsyHome, CPU 23%, batteria 80%» (i valori dei lati, nell'ordine).
  - Impostazioni: nuova sezione **«Isola»** all'indice 3 (`["Home Assistant", "Dispositivi", "Collegamenti", "Isola", "Sistema", "Informazioni"]`); `SEZIONE_INFORMAZIONI` passa da 4 a 5. Contenuto: «A sinistra degli occhi», «A destra degli occhi» (selettori con `scelte()`), «Meteo da» (Automatico + `meteo_elenco`, D1), nota «Il nome della rete non compare mai nella pillola.». I Task 7 e 9 ci aggiungono i loro interruttori.
  - Nuovo componente `Scelta` in `componenti.slint`: pulsante con valore e «›» che apre un `PopupWindow` con l'elenco; ↑/↓ e Invio scelgono, Esc chiude e ridà il fuoco al pulsante; `accessible-role: combobox`, nome = etichetta, valore = scelta.

- **Aggiunta del 16/11 (decisione di Paolo): quadranti configurabili** con lo stesso catalogo.
  - `Config { …, #[serde(default)] pub quadranti: Vec<String> }` (3 voci, stesse stringhe di `Lato`; vuoto o mancante = i tre quadranti attuali di M1; `#[serde(default)] pub massimi_quadranti: HashMap<String, f64>` per i massimi scelti dall'utente).
  - Il catalogo si allarga per i quadranti con `Disco`, `ReteGiu`, `ReteSu`, `Gpu` (dal Task 10; prima assente) e con **qualunque sensore numerico** di HA disponibile in `MenuData` (temperatura, umidità, potenza W, energia/produzione FV): `Lato::Sensore(entity_id)` vale per entrambi; i lati della pillola mostrano solo ciò che ci sta (stessa funzione `scelte`, con un parametro `per_quadranti: bool`).
  - `pub fn scala(fonte: &Lato, unita: Option<&str>, massimo_utente: Option<f64>) -> (f64, f64)`: `%` → 0..100; `°C` → −10..40; `W` → 0..massimo (predefinito 3000, modificabile nelle Impostazioni accanto alla scelta); `kW` → 0..massimo (predefinito 6); altre unità → 0..massimo utente, o 0..(valore × 2) finché l'utente non lo imposta; valore fuori scala → arco pieno o vuoto, testo col valore vero.
  - Impostazioni › Isola: «Quadrante 1/2/3» (selettori `Scelta`) + campo «Massimo» accanto quando l'unità è W/kW o ignota.
  - Campionamento: il `pc_stats` dei quadranti legge solo le fonti PC scelte (niente disco se nessun quadrante lo mostra); i sensori arrivano già con `MenuData` (nessuna lettura in più). L'insieme di entità seguite (`subscribe_entities`) deve includere i sensori scelti per quadranti e lati.
  - Test in più: `quadranti_di_m1_predefiniti` (config senza chiave → i tre di M1), `scala_per_unita` (%, °C, W col massimo predefinito e utente, unità ignota), `quadranti_leggono_solo_cio_che_mostrano` (nessun disco scelto → il campionatore non legge il disco), `sensore_scelto_e_seguito` (un sensore scelto per un quadrante entra nell'insieme seguito), test Slint del selettore del quadrante.

- [ ] **Step 1: test (falliscono)**

```rust
// lati.rs
#[test] fn lato_da_e_verso_stringa() // ogni variante fa andata e ritorno; "boh" e "" → Niente; "sensore:sensor.sala_t" → Sensore("sensor.sala_t")
#[test] fn scelte_senza_meteo_ne_rete() // scelte(false, &[], false) non contiene Meteo né Rete; contiene Niente, Ora, Cpu, Ram
#[test] fn bisogni_solo_di_cio_che_e_scelto() // (Niente, Niente) → tutto falso; (Cpu, Ora) → cpu_ram e ora veri, batteria falso
#[test] fn contenuto_dei_lati() // Cpu con 23.4 → Testo("CPU 23%"); Ram 61.0 → "RAM 61%"; Sensore con "21.5" → "21,5°"; Meteo "9°" → "9°"
#[test] fn lato_senza_dato_resta_vuoto() // Meteo None → Vuoto; Sensore assente → Vuoto; Batteria None → Vuoto; Cpu None → Testo("CPU —")
#[test] fn larghezza_con_uno_e_due_lati() // funzione pura larghezza_riposo(l_sx, l_dx): (0,0) → 124; (40,40) → 2*(14+40+12+19); (400,0) ≤ 440
// casa-core/tests/config.rs
#[test] fn config_di_m1_ha_lati_vuoti() // JSON senza le chiavi → lato_sinistro == "" e lato_destro == ""
// test_ui.rs
#[test] fn misure_dei_lati_slint_e_rust() // pillola con lato-destro testo "CPU 23%": larghezza-pillola == larghezza_riposo(0, larghezza misurata)
#[test] fn sezione_isola_e_informazioni_spostata() // impostazioni: sezione 5 = «Informazioni», sezione 3 = «Isola»
#[test] fn scelta_da_tastiera() // Scelta: Invio apre, ↓ e Invio scelgono la seconda voce, Esc lascia il fuoco sul pulsante
```

- [ ] **Step 2:** `cargo test -p assistente lati` → FAIL.
- [ ] **Step 3:** implementa `lati.rs` (puro) e il `CampionatoreLeggero`.
- [ ] **Step 4:** implementa `sorgenti_lati.rs`, la pillola coi lati e la sezione «Isola»; il ponte chiama `sorgenti_lati::aggiorna` a ogni cambio di scelta, di visibilità (servizi) e d'umore (`Dorme` ferma tutto); ogni lato si riscrive solo se il `ContenutoLato` cambia.
- [ ] **Step 5:** `cargo test -p assistente` e `cargo test -p casa-core` → PASS.
- [ ] **Step 6:** prova dal vivo con HA finto: ogni scelta a sinistra e a destra, un lato solo, due lati, batteria col glifo, temperatura esterna e di un sensore del finto HA, CPU che cambia; pillola allargata con `--prova-avviso attenzione` (lati nascosti) e ritirata; Narrator legge i valori; a 100 / 150 / 175 %. **Misure** (`tools/misura.ps1`, 60 s a riposo): lati niente → come M1; CPU + RAM → ≤ 0,1 % CPU e nessuna crescita della RAM; con l'umore «dorme» nessun campione (verifica col registro di debug o contando i ridisegni). Foto di zona della pillola nei casi principali.
- [ ] **Step 7:** commit `feat(assistente): informazioni ai lati degli occhi e sezione Isola` (con l'ok di Paolo).

### Task 5: Ritirata che si riempie e schermo intero più pronto

**Files:**
- Create: `crates/assistente/src/sistema/primo_piano.rs`, `crates/assistente/examples/schermo_pieno.rs`
- Modify: `src/attenzione.rs`, `src/ponte.rs`, `src/pillola.rs:120-140`, `src/sistema/mod.rs`, `src/sistema/servizi.rs:66-140, 281-288, 338-351`, `src/main.rs:123-190`, `ui/pillola.slint:56-60, 172-180`, `crates/assistente/Cargo.toml` (feature `Win32_UI_Accessibility` di windows-sys), `CHANGELOG.md`
- Test: `src/attenzione.rs`, `src/pillola.rs`, `src/test_ui.rs`

**Interfaces:**
- Consumes: `Ritirata`, `IsteresiSchermoIntero`, `Servizi::controlla_schermo_intero` di M1.
- Produces:
  - `pub struct Avanzamento { pub id: String, pub frazione: f32, pub livello: Livello }` e `Ponte::metti_avanzamento(a: Avanzamento)`, `Ponte::togli_avanzamento(id: &str)` (pronto per Claude Code e Focus; in M2 lo usa solo la prova).
  - `pub fn riempimento_linea(livello_massimo: Option<Livello>, avanzamento: Option<&Avanzamento>) -> (Option<Livello>, f32)`: colore = il livello più urgente fra voci e avanzamento (nessuno → neutro, come oggi); frazione = quella dell'avanzamento (clamp 0..1), altrimenti 1.0 (linea piena come in M1).
  - `pub fn pixel_riempiti(frazione: f32) -> u32` = `round(frazione × 300)`; la frazione si riscrive in Slint solo se cambiano i pixel riempiti.
  - Pillola: `in property <float> riempimento-linea: 1.0`; la linea mostra una traccia neutra (`Tema.traccia`) e sopra, da sinistra, il riempimento del colore del livello largo `riempimento × 300px`; filo scuro di M1 invariato.
  - Flag `--prova-avanzamento <secondi>`: un avanzamento da 0 a 1 in quei secondi, aggiornato una volta al secondo, che sparisce alla fine (solo per la prova).
  - `primo_piano.rs` (Windows): `pub fn ascolta(al_cambio: fn()) -> Option<GanciPrimoPiano>`: `SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, 0, callback, 0, 0, WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS)` sul thread dell'interfaccia; `Drop` chiama `UnhookWinEvent`. Il callback chiama `servizi::primo_piano_cambiato()`, che fa `controlla_schermo_intero()` subito (l'isteresi 3 s / 1,5 s resta la stessa). Il controllo da 5 s resta (F11 in un browser non cambia la finestra in primo piano). **Niente** `EVENT_OBJECT_LOCATIONCHANGE`: scatta a ogni movimento del cursore e sveglierebbe l'app di continuo. Fuori da Windows: `None`.
  - `examples/schermo_pieno.rs`: finestra winit senza bordi che dopo 2 s va a schermo intero sul monitor indicato da `--monitor <n>`, ci resta `--per <s>` secondi e si chiude (processo di prova lanciato dal test: nessun input su app altrui).

- [ ] **Step 0:** leggere su learn.microsoft.com `SetWinEventHook` (contesto out-of-context: il callback arriva sul thread che ha installato il gancio, che deve avere un ciclo di messaggi — winit lo ha) e `UnhookWinEvent`; annotarlo in testa a `primo_piano.rs`.
- [ ] **Step 1: test (falliscono)**

```rust
// attenzione.rs / pillola.rs
#[test] fn linea_piena_senza_avanzamento() // riempimento_linea(Some(Attenzione), None) == (Some(Attenzione), 1.0); (None, None) == (None, 1.0)
#[test] fn linea_con_avanzamento() // (None, Some(0.4, Informazione)) == (Some(Informazione), 0.4); (Some(Bloccante), Some(0.4, Informazione)) == (Some(Bloccante), 0.4)
#[test] fn frazione_fuori_scala_ritagliata() // frazione 1.7 → 1.0; -0.2 → 0.0
#[test] fn pixel_riempiti_al_pixel() // 0.5 → 150; 0.001 → 0; 1.0 → 300
// test_ui.rs
#[test] fn linea_riempita_a_meta() // pillola ritirata, riempimento-linea 0.5: il riempimento è largo 150px
```

- [ ] **Step 2:** `cargo test -p assistente linea` → FAIL.
- [ ] **Step 3:** implementa avanzamento, linea e flag di prova; poi il gancio del primo piano installato in `servizi::avvia` (al primo giro del ciclo, come menu e scorciatoie) e tolto in `servizi::spegni`.
- [ ] **Step 4:** `cargo test -p assistente` → PASS.
- [ ] **Step 5:** prova dal vivo: `--prova-avanzamento 20` con la pillola ritirata (linea che si riempie, ritorno tenendo il mouse 450 ms già di M1: verificato col cursore spostato solo da Paolo); `cargo run --example schermo_pieno -- --per 8`: la pillola sparisce **3 s** dopo lo schermo intero (non fino a 8 s come prima) e torna **1,5 s** dopo la chiusura, misurato col registro con l'ora; con due monitor, finestra di prova sull'altro → la pillola resta. Misura a riposo invariata rispetto a M1 (il gancio non sveglia l'app se nessuno cambia finestra).
- [ ] **Step 6:** commit `feat(assistente): linea della ritirata che si riempie e schermo intero rilevato subito` (con l'ok di Paolo).

### Task 6: Scelta del monitor

**Files:**
- Modify: `crates/casa-core/src/config.rs`, `crates/assistente/src/finestre.rs:84-120`, `src/sistema/menu.rs`, `src/sistema/servizi.rs:150-250`, `src/ponte.rs`, `src/pannello.rs:355-359`, `CHANGELOG.md`
- Test: `src/finestre.rs`, `src/sistema/menu.rs`

**Interfaces:**
- Produces:
  - `Config { …, #[serde(default)] pub schermo: Option<String> }` (nome del monitor come lo dà winit, `MonitorHandle::name()`; `None` = principale).
  - `pub struct InfoMonitor { pub nome: String, pub principale: bool, pub area: RettangoloFisico, pub scala: f32, pub larghezza_px: u32, pub altezza_px: u32 }`.
  - `pub fn scegli_monitor(monitor: &[InfoMonitor], preferito: Option<&str>) -> Option<usize>`: il preferito se c'è; altrimenti il principale; altrimenti il primo; `None` solo con elenco vuoto.
  - `pub fn etichetta_monitor(monitor: &[InfoMonitor], i: usize) -> String`: il principale «Schermo principale», gli altri «Schermo N · 2560×1440» (N = posizione da 1 nell'elenco ordinato per `area.x`, poi `area.y`).
  - `schermo_di` usa `available_monitors()` + `scegli_monitor` con la preferenza letta da un `thread_local! { static SCHERMO_PREFERITO: RefCell<Option<String>> }` che il ponte aggiorna da `Config.schermo`; pillola, pannello, zona della linea e controllo dello schermo intero lo seguono da soli (tutti passano da `schermo_di` o `monitor_di`).
  - **Ritorno automatico**: nessun codice nuovo di sorveglianza; il controllo da 5 s di M1 chiama già `pillola::al_cambio_schermo` → `posiziona_pillola` → `schermo_di`, che sceglie di nuovo il preferito appena ricompare. Il preferito resta salvato anche mentre manca.
  - Menu: `Comando::Schermo(usize)` (`Copy`, indice nell'elenco mostrato) e sottomenu **«Schermo»** con una voce a spunta per monitor, **solo con più di un monitor**; il menu si ricostruisce prima di ogni apertura (i monitor possono cambiare). Scelta → salva `Config.schermo`, sposta subito pillola (e pannello se aperto).
- `voci()` e i test del menu cambiano: «Schermo» sta fra «Informazioni» e «Ritira l'isola».

- [ ] **Step 1: test (falliscono)**

```rust
// finestre.rs
#[test] fn senza_preferenza_il_principale() // [secondario, principale] con preferito None → Some(1)
#[test] fn preferito_presente_scelto() // preferito "\\\\.\\DISPLAY2" presente → il suo indice
#[test] fn monitor_scollegato_usa_il_principale_e_poi_torna() // preferito assente → principale; elenco di nuovo con il preferito → il preferito
#[test] fn nessun_principale_il_primo_ed_elenco_vuoto_nessuno() // tutti principale=false → Some(0); [] → None
#[test] fn etichette_dei_monitor() // principale → "Schermo principale"; secondo 2560×1440 → "Schermo 2 · 2560×1440"
// menu.rs
#[test] fn schermo_solo_con_piu_monitor() // voci con 1 monitor: nessuna «Schermo»; con 2: sottomenu con 2 voci e la spunta sul preferito
#[test] fn ogni_voce_torna_al_suo_comando() // aggiornato: anche Schermo(0) e Schermo(1) fanno andata e ritorno
```

- [ ] **Step 2:** `cargo test -p assistente monitor` → FAIL.
- [ ] **Step 3:** implementa; il nome del monitor non va nei log (è un identificativo tecnico, ma i log restano puliti).
- [ ] **Step 4:** `cargo test -p assistente` → PASS.
- [ ] **Step 5:** prova dal vivo **solo se Paolo ha due schermi collegati**: scelta dal menu del clic destro, pillola e pannello sull'altro schermo, riavvio dell'app (ricordata), schermo scollegato da Paolo → principale entro 5 s, ricollegato → torna entro 5 s; schermi a scale diverse (100 % e 150 %). Con un solo schermo: il sottomenu non compare.
- [ ] **Step 6:** commit `feat(assistente): scelta dello schermo dal menu della pillola` (con l'ok di Paolo).

### Task 7: Volume del PC: rotella sulla pillola e cursore nel pannello

**Files:**
- Create: `crates/assistente/src/sistema/volume.rs`
- Modify: `crates/assistente/Cargo.toml` (`windows = { version = "0.62", features = ["Win32_Media_Audio", "Win32_Media_Audio_Endpoints", "Win32_System_Com", "Win32_Foundation", "implement"] }` solo `cfg(windows)`, stessa versione del `Cargo.lock`), `crates/casa-core/src/config.rs`, `src/sistema/mod.rs`, `src/ponte.rs`, `src/pillola.rs`, `src/pannello.rs`, `ui/pillola.slint:229-245`, `ui/pannello.slint` (fascia del dock), `ui/impostazioni.slint` (sezione Isola), `ui/icone.slint` + `assets/icone/altoparlante.svg`, `altoparlante-muto.svg`, `CHANGELOG.md`
- Test: `src/sistema/volume.rs`, `src/test_ui.rs`

**Interfaces:**
- Consumes: sezione «Isola» e `Scelta` (Task 4); `scarto-occhi-riposo` (Task 4: la barra sta dove stanno gli occhi).
- Produces:
  - `Config { …, #[serde(default = "vero")] pub volume_rotella: bool }` (D2: acceso di serie).
  - Puro: `pub const PX_PER_PUNTO: f32 = 30.0;` (1 punto percentuale ogni 30 px logici di rotella; **da tarare dal vivo** perché uno scatto della rotella dia 2 punti come TwoCode); `pub struct Rotella { resto: f32 }` con `fn scorri(&mut self, delta_px: f32) -> i32` (in su = positivo, il resto si conserva fra gli eventi); `pub fn nuovo_volume(attuale: f32, punti: i32) -> f32` (clamp 0.0..=1.0); `pub struct StatoVolume { pub livello: f32, pub muto: bool }`; `pub const BARRA_PER: Duration = Duration::from_millis(1200)`.
  - Windows: `pub struct Audio` con `fn avvia(consegna: fn(Option<StatoVolume>)) -> Audio`, `fn imposta(&self, livello: f32)`, `fn muto(&self, si: bool)`. Un thread `audio` (COM `COINIT_MULTITHREADED`) che: prende `IMMDeviceEnumerator::GetDefaultAudioEndpoint(eRender, eMultimedia)`, attiva `IAudioEndpointVolume`, registra un `IAudioEndpointVolumeCallback` (`OnNotify` → `consegna(Some(stato))` via `invoke_from_event_loop`) e un `IMMNotificationClient` (`OnDefaultDeviceChanged` per `eRender` → si ricollega al nuovo dispositivo); senza dispositivo `consegna(None)`. I comandi arrivano da un canale: il thread dorme su `recv()`, **nessun sondaggio**. Si avvia la prima volta che serve (primo evento di rotella sulla pillola o prima apertura del pannello) e resta (le callback non costano nulla a riposo). Fuori da Windows `Audio` non esiste e `pub fn disponibile() -> bool` è falso.
  - Pillola: `callback rotella(length)` dalla `TouchArea` (`scroll-event`, solo a riposo o col puntino, non allargata né ritirata) → `Rotella::scorri` → `Audio::imposta`. `in property <float> volume-mostrato: -1` (−1 = occhi normali): gli occhi diventano una barra 40 × 6 raggio 3 con traccia `Tema.traccia` e riempimento `Tema.occhio`, centrata dove stanno gli occhi; da muto solo la traccia; torna agli occhi `BARRA_PER` dopo l'ultimo cambio (una sveglia singola). Nome per Narrator durante la barra: «Volume 40%».
  - Pannello (D3): a destra del dock, fuori dalla capsula, l'icona dell'altoparlante 20 px in un bersaglio 36 × 32 (`accessible-role: button`, «Volume 40%», clic = muto sì/no); al passaggio o col fuoco si apre accanto un cursore orizzontale 76 px (`accessible-role: slider`, `accessible-value`, ←/→ ±2 punti, Home/Fine 0/100), che si chiude uscendo col mouse o con Esc. Solo se `disponibile()` e c'è un dispositivo.
  - Impostazioni › Isola: interruttore «Volume con la rotella sulla pillola» (solo su Windows; altrove la riga non c'è).

- [ ] **Step 0:** leggere su learn.microsoft.com: `IMMDeviceEnumerator::GetDefaultAudioEndpoint`, `IAudioEndpointVolume::SetMasterVolumeLevelScalar`, `SetMute`, `RegisterControlChangeNotify` / `UnregisterControlChangeNotify`, `IAudioEndpointVolumeCallback::OnNotify` (gira su un thread di Windows: niente lavoro pesante, nessuna chiamata che blocchi), `IMMNotificationClient::OnDefaultDeviceChanged` (non chiamare metodi di registrazione dentro il callback: si segnala al thread `audio`). Annotare i vincoli in testa a `volume.rs`; per `#[implement]` consultare la documentazione del crate `windows` 0.62.
- [ ] **Step 1: test (falliscono)**

```rust
// volume.rs
#[test] fn uno_scatto_due_punti() // Rotella::scorri(60.0) == 2 (con PX_PER_PUNTO 30)
#[test] fn il_resto_si_conserva() // scorri(20.0) == 0, poi scorri(20.0) == 1; in giù scorri(-30.0) == -1
#[test] fn volume_nei_limiti() // nuovo_volume(0.99, 5) == 1.0; nuovo_volume(0.01, -5) == 0.0; nuovo_volume(0.40, 2) ≈ 0.42
#[test] fn senza_dispositivo_audio_niente_volume() // con StatoVolume None: la rotella non chiama imposta (fake di Audio dietro un trait Volume)
// test_ui.rs
#[test] fn barra_del_volume_al_posto_degli_occhi() // volume-mostrato 0.5: barra visibile, occhi nascosti; -1: occhi visibili
#[test] fn altoparlante_assente_senza_volume() // pannello con volume-disponibile false: nessun elemento «Volume»
#[test] fn cursore_da_tastiera() // fuoco sul cursore, → due volte: callback imposta-volume con +4 punti
```
Per il test con il fake: `pub trait Volume { fn imposta(&self, livello: f32); fn muto(&self, si: bool); }`, implementato da `Audio`.

- [ ] **Step 2:** `cargo test -p assistente volume` → FAIL.
- [ ] **Step 3:** implementa la parte pura, poi `Audio` su Windows; un test `#[cfg(windows)] #[ignore]` `legge_il_volume_attuale` che avvia `Audio`, aspetta il primo stato e **non cambia nulla**.
- [ ] **Step 4:** `cargo test -p assistente` → PASS; `cargo test -p assistente -- --ignored legge_il_volume_attuale` → PASS sulla macchina di Paolo.
- [ ] **Step 5:** prova dal vivo **con l'ok di Paolo e a volume basso**: rotella simulata con `PostMessage(WM_MOUSEWHEEL)` alla finestra della pillola (proprio PID), solo in giù, poi volume rimesso al valore di prima; taratura di `PX_PER_PUNTO` (uno scatto = 2 punti); tasti volume della tastiera premuti da Paolo → la barra **non** compare (solo la rotella la mostra) ma il cursore del pannello aperto si aggiorna; cuffie USB o BT staccate da Paolo col pannello aperto → il cursore segue il nuovo dispositivo. Misura a riposo invariata (thread audio fermo su `recv`).
- [ ] **Step 6:** commit `feat(assistente): volume del PC con la rotella sulla pillola e nel pannello` (con l'ok di Paolo).

### Task 8: Pannellino Wi-Fi / Bluetooth e icona della rete

**Files:**
- Create: `crates/assistente/src/sistema/radio.rs`, `ui/connessioni.slint`, `assets/icone/{wifi-0,wifi-1,wifi-2,wifi-3,cavo,bluetooth}.svg`
- Modify: `crates/assistente/Cargo.toml` (features `windows`: `Devices_Radios`, `Networking_Connectivity`, `Foundation`, `Foundation_Collections`), `src/sistema/browser.rs`, `src/sistema/mod.rs`, `src/lati.rs`, `src/sorgenti_lati.rs`, `src/pannello.rs`, `src/ponte.rs`, `ui/pannello.slint` (fascia del dock), `ui/pillola.slint` (lato «rete»), `ui/componenti.slint` (`IconaRete`), `ui/tema.slint`, `CHANGELOG.md`
- Test: `src/sistema/radio.rs`, `src/sistema/browser.rs`, `src/lati.rs`, `src/test_ui.rs`

**Interfaces:**
- Consumes: `Lato::Rete`, `DatiLati`, `scelte(.., rete)` (Task 4); fascia del dock (Task 7).
- Produces:
  - `pub enum StatoRete { Nessuna, Cavo, WiFi { tacche: u8 } }`; `pub fn tacche_da(barre: Option<u8>) -> u8` (barre di Windows 0..=5 → 0..=3: 0→0, 1–2→1, 3–4→2, 5→3; `None`→0). `DatiLati.rete: Option<StatoRete>`; `ContenutoLato::Rete(StatoRete)` (solo icona, **mai il nome della rete**). `scelte(.., rete = cfg!(windows))`.
  - `pub enum TipoRadio { WiFi, Bluetooth }`; `pub struct StatoRadio { pub tipo: TipoRadio, pub acceso: bool }`; `pub enum EsitoRadio { Fatto, Negato, Errore }`; `pub fn esito_da(stato_accesso: i32) -> EsitoRadio` (mappa `RadioAccessStatus`: `Allowed` → `Fatto`, `DeniedByUser`/`DeniedBySystem` → `Negato`, altro → `Errore`).
  - Windows: `pub struct Radio` con `fn avvia(consegna: fn(Vec<StatoRadio>)) -> Radio`, `fn commuta(&self, tipo: TipoRadio, acceso: bool, esito: fn(TipoRadio, EsitoRadio))`. Thread `radio` (MTA): `Radio::RequestAccessAsync`, `Radio::GetRadiosAsync`, un gestore `StateChanged` per radio (evento, nessun sondaggio), `SetStateAsync`. Vive **solo a pannello aperto**: nasce all'apertura, alla chiusura toglie i gestori e libera le radio.
  - Windows: `pub struct OsservaRete` con `fn avvia(consegna: fn(StatoRete)) -> OsservaRete`: `NetworkInformation::NetworkStatusChanged` (evento) + `GetInternetConnectionProfile()` → `IsWlanConnectionProfile()`, `GetSignalBars()`; rilettura delle tacche ogni **30 s** solo se Wi-Fi. Acceso solo se il lato «Rete» è scelto e la pillola visibile, o il pannello è aperto (icona nella fascia); altrimenti spento. Nome della rete (`WlanConnectionProfileDetails::GetConnectedSsid`) letto **solo** a pannellino aperto.
  - `browser.rs`: `pub enum PaginaWindows { RetiWifi, ImpostazioniWifi, Bluetooth }`, `pub fn apri_pagina(p: PaginaWindows) -> Result<(), String>` con URI fissi `ms-availablenetworks:`, `ms-settings:network-wifi`, `ms-settings:bluetooth`, via `ShellExecuteW`; `apri(url)` resta solo http/https.
  - Pannellino (`ui/connessioni.slint`, D3): si apre dalle icone Wi-Fi e Bluetooth a sinistra del dock; 264 px, sopra il dock; due riquadri 92 px alti raggio 14 (`accessible-role: switch`, `accessible-checked`): «Wi-Fi» col nome della rete o «Spento», «Bluetooth» con «Acceso»/«Spento»; acceso = token `Tema.radio-acceso` (da `petalo-3`); sotto, righe 38 px «Reti Wi-Fi ›» e «Dispositivi Bluetooth ›». Rifiuto: riga in ambra «Non riuscito: apro le Impostazioni di Windows», si apre `ImpostazioniWifi` o `Bluetooth`, il riquadro torna allo stato reale (quello dell'ultimo `StateChanged`). Radio assente → riquadro «Non presente», non cliccabile. Esc chiude e ridà il fuoco all'icona; Tab resta nel pannellino finché è aperto.
  - Fuori da Windows: icone, pannellino e scelta «Rete» assenti.

- [ ] **Step 0:** leggere su learn.microsoft.com: `Windows.Devices.Radios.Radio` (`RequestAccessAsync`, `GetRadiosAsync`, `SetStateAsync`, `StateChanged`, `RadioAccessStatus`; capacità richieste per app desktop non impacchettate), `Windows.Networking.Connectivity.NetworkInformation` (`NetworkStatusChanged`, `GetInternetConnectionProfile`, `ConnectionProfile.GetSignalBars`, `IsWlanConnectionProfile`) e l'elenco degli URI `ms-settings:` («Launch the Windows Settings app») più `ms-availablenetworks:`. Annotare in testa a `radio.rs` e `browser.rs`. Se un'API richiede un'app impacchettata, fermarsi e dirlo a Paolo prima di cambiare strada.
- [ ] **Step 1: test (falliscono)**

```rust
// radio.rs
#[test] fn tacche_da_barre() // None→0, 0→0, 1→1, 2→1, 3→2, 4→2, 5→3
#[test] fn esito_dell_accesso() // Allowed→Fatto, DeniedByUser→Negato, DeniedBySystem→Negato, Unspecified→Errore
// lati.rs
#[test] fn il_lato_rete_e_solo_un_icona() // contenuto(Rete, dati con WiFi{2}) == ContenutoLato::Rete(WiFi{tacche:2}); nessun testo
// browser.rs
#[test] fn pagine_di_windows_fisse() // uri(RetiWifi)=="ms-availablenetworks:", uri(Bluetooth)=="ms-settings:bluetooth"; apri("ms-settings:bluetooth") rifiutato
// test_ui.rs
#[test] fn rifiuto_mostra_errore_e_stato_reale() // pannellino: commuta Wi-Fi su spento, esito Negato → riga «Non riuscito…» visibile, riquadro di nuovo acceso
#[test] fn pannellino_da_tastiera() // Invio sull'icona apre, Tab fra i riquadri e le righe, Esc chiude col fuoco sull'icona
```

- [ ] **Step 2:** `cargo test -p assistente radio` → FAIL.
- [ ] **Step 3:** implementa puro, poi `Radio` e `OsservaRete` su Windows; abilita «Rete» nelle scelte dei lati.
- [ ] **Step 4:** `cargo test -p assistente` → PASS.
- [ ] **Step 5:** prova dal vivo: icona della rete nella pillola e nella fascia (Wi-Fi e cavo, se Paolo ha entrambi); pannellino aperto e chiuso; **spegnere il Wi-Fi solo con l'ok di Paolo** (se è collegato via Wi-Fi perde la rete: provare prima il Bluetooth, e il Wi-Fi solo se è collegato via cavo); un rifiuto simulato è coperto dal test. Misure: pannello aperto col pannellino ~0 % CPU fermo; dopo la chiusura del pannello RAM entro +2 MB da prima dell'apertura (le radio liberate); a riposo col lato «Rete» ≤ 0,1 % CPU. Foto di zona senza il nome della rete (coprirlo o usare una rete di prova).
- [ ] **Step 6:** commit `feat(assistente): Wi-Fi e Bluetooth dal pannello, icona della rete` (con l'ok di Paolo).

### Task 9: Batteria delle cuffie Bluetooth

**Files:**
- Create: `crates/assistente/src/sistema/batterie_bt.rs`
- Modify: `crates/assistente/Cargo.toml` (features windows-sys `Win32_Devices_DeviceAndDriverInstallation`, `Win32_Devices_Properties`), `crates/casa-core/src/config.rs`, `src/attenzione.rs`, `src/ponte.rs`, `src/main.rs`, `ui/impostazioni.slint` (sezione Isola), `CHANGELOG.md`
- Test: `src/sistema/batterie_bt.rs`, `src/attenzione.rs`

**Interfaces:**
- Consumes: `Voce`, `PULSANTE_HO_CAPITO`, sezione «Isola» (Task 4).
- Produces:
  - `Config { …, #[serde(default = "vero")] pub avvisi_bt: bool, #[serde(default)] pub bt_avvisati: HashMap<String, String> }` (impronta → data `"2026-11-15"`; D6).
  - `pub struct LetturaBt { pub impronta: String, pub nome: String, pub percentuale: u8, pub connesso: bool }`; `pub fn impronta(id_istanza: &str) -> String` (FNV-1a 64 bit in esadecimale: l'id di istanza contiene l'indirizzo MAC e non si salva mai in chiaro).
  - `pub const SOGLIA_BT: u8 = 20; pub const SOGLIA_BT_FINE: u8 = 25; pub const PERIODO_BT: Duration = Duration::from_secs(15 * 60);`
  - `pub fn da_avvisare(letture: &[LetturaBt], avvisati: &HashMap<String, String>, oggi: &str) -> Vec<LetturaBt>`: connessi, ≤ 20 %, non avvisati oggi.
  - `pub fn voce_bt(l: &LetturaBt, ora: Instant) -> Voce`: id `pc.bt.<impronta>`, *informazione*, `Fonte::Pc`, titolo «<nome> al 18%», dettaglio «Ricarica il dispositivo Bluetooth quando puoi.», pulsante «Ho capito». La voce sparisce sopra il 25 %, a dispositivo disconnesso o con «Ho capito».
  - Windows: `pub fn leggi() -> Vec<LetturaBt>`: `SetupDiGetClassDevsW(null, null, 0, DIGCF_PRESENT | DIGCF_ALLCLASSES)`, per ogni dispositivo `SetupDiGetDevicePropertyW` con la chiave della batteria `{104EA319-6EE2-4701-BD47-8DDBF425BBE5}`, pid 2 (`DEVPROP_TYPE_BYTE`), nome amichevole (`DEVPKEY_Device_FriendlyName`), id di istanza; `connesso` dallo stato del nodo (`CM_Get_DevNode_Status`, `DN_STARTED`) o dalla proprietà di connessione che la documentazione indica. Thread `batterie-bt` che dorme: prima lettura 60 s dopo l'avvio, poi ogni 15 min, solo se `avvisi_bt`; si ferma spegnendo l'interruttore. Fuori da Windows non esiste e l'interruttore non c'è.
  - Impostazioni › Isola: interruttore «Avvisa quando le cuffie Bluetooth sono scariche» (solo Windows).

- [ ] **Step 0:** cercare su learn.microsoft.com `SetupDiGetClassDevsW`, `SetupDiGetDevicePropertyW`, `CM_Get_DevNode_Status` e la chiave della batteria BT (se non è documentata, annotarlo: è in sola lettura e il codice deve sopportarne l'assenza); verificare quale proprietà distingue un dispositivo associato ma **non connesso** (Review Focus 5).
- [ ] **Step 1: test (falliscono)**

```rust
#[test] fn impronta_stabile_e_senza_mac() // impronta("BTHENUM\\{…}_AA:BB…") uguale a ogni chiamata, 16 cifre esadecimali, non contiene "AA"
#[test] fn avvisa_sotto_venti() // [18 connesso] → 1; [21 connesso] → 0
#[test] fn dispositivo_non_connesso_non_avvisa() // [5, connesso: false] → 0
#[test] fn una_volta_al_giorno_anche_dopo_il_riavvio() // avvisati {imp: "2026-11-15"}, oggi "2026-11-15" → 0; oggi "2026-11-16" → 1
#[test] fn voce_bt_informazione_con_ho_capito() // livello Informazione, id "pc.bt.<impronta>", pulsanti [«Ho capito»]
```

- [ ] **Step 2:** `cargo test -p assistente bt` → FAIL.
- [ ] **Step 3:** implementa; all'avviso il ponte scrive la data in `bt_avvisati` (config salvata) e mette la voce; le impronte più vecchie di 7 giorni si tolgono a ogni salvataggio.
- [ ] **Step 4:** `cargo test -p assistente` → PASS; un test `#[cfg(windows)] #[ignore]` `legge_senza_errori` chiama `leggi()` e controlla solo che finisca (nessun nome stampato).
- [ ] **Step 5:** prova dal vivo con un dispositivo BT di Paolo, se ne ha uno con batteria: valore letto uguale a quello delle Impostazioni di Windows (confronto fatto da Paolo, nulla nei log); dispositivo spento → nessun avviso. Misura: nessun effetto a riposo (thread che dorme).
- [ ] **Step 6:** commit `feat(assistente): avviso di batteria scarica delle cuffie Bluetooth` (con l'ok di Paolo).

### Task 10: GPU % nella vista PC

**Files:**
- Modify: `crates/assistente/src/pc_stats.rs`, `src/pannello.rs:126-230`, `ui/voci.slint:28-45`, `ui/viste/pc.slint:70-137`, `crates/assistente/Cargo.toml` (features windows-sys `Win32_System_Performance`; in alternativa D7 `Wdk_Graphics_Direct3D` se presente in 0.52, altrimenti `windows` 0.62), `CHANGELOG.md`
- Test: `src/pc_stats.rs`, `src/test_ui.rs`

**Interfaces:**
- Consumes: `Campionatore` e il thread dei campioni da 2 s (M1); vista corrente del pannello.
- Produces:
  - `StatPc { …, pub gpu_pct: Option<f32> }`; `StatPcUi { …, ha-gpu: bool, gpu: float, gpu-testo: string }`.
  - `pub fn gpu_da_istanze(istanze: &[(String, f64)]) -> Option<f32>`: istanze PDH come `pid_1234_luid_0x…_phys_0_eng_0_engtype_3D`; si sommano i valori per `engtype_*`, si prende il massimo fra i tipi (come Gestione attività), clamp 0..100; `None` se l'elenco è vuoto.
  - `Campionatore::usa_gpu(&mut self, si: bool)`: la query PDH (`PdhOpenQueryW`, `PdhAddEnglishCounterW("\\GPU Engine(*)\\Utilization Percentage")`, `PdhCollectQueryData`, `PdhGetFormattedCounterArrayW` con `PDH_FMT_DOUBLE`) nasce quando la **vista PC** è visibile e si chiude (`PdhCloseQuery`) quando la si lascia o il pannello si chiude. Il pannello lo dice al thread con un `AtomicBool vista_pc`.
  - Vista PC: riga «GPU» nel terzo posto della colonna destra (oggi vuoto: `ui/viste/pc.slint:132`), con barra come CPU; assente se `!ha-gpu` (fuori da Windows, o lettura fallita).
  - D7: se la misura dello Step 5 supera +2 MB, `LettoreGpu` passa a `D3DKMTQueryStatistics` (adattatori da `D3DKMTEnumAdapters2`, tempo di esecuzione per nodo fra due campioni / tempo trascorso, massimo fra i nodi), stessa firma `fn gpu(&mut self) -> Option<f32>`.

- [ ] **Step 0:** leggere su learn.microsoft.com PDH (`PdhOpenQueryW`, `PdhAddEnglishCounterW`, `PdhGetFormattedCounterArrayW`, `PdhCloseQuery`) e il contatore «GPU Engine»; per D7 `D3DKMTQueryStatistics` e `D3DKMTEnumAdapters2` (documentazione WDK). Ricordare la nota di `pc_stats.rs:7-9`: PDH al primo uso carica tabelle che non libera.
- [ ] **Step 1: test (falliscono)**

```rust
#[test] fn gpu_massimo_fra_i_tipi() // [("…engtype_3D",30.0),("…engtype_3D",25.0),("…engtype_VideoDecode",40.0)] → Some(55.0)
#[test] fn gpu_vuota_e_fuori_scala() // [] → None; [("…engtype_3D",180.0)] → Some(100.0)
// test_ui.rs
#[test] fn riga_gpu_solo_se_c_e() // VistaPc con ha-gpu false: nessuna riga «GPU»; true: riga con «GPU 37%»
```

- [ ] **Step 2:** `cargo test -p assistente gpu` → FAIL.
- [ ] **Step 3:** implementa la lettura PDH e la riga della vista.
- [ ] **Step 4:** `cargo test -p assistente` → PASS.
- [ ] **Step 5: cancello di memoria (D7).** `tools/misura.ps1` con pannello aperto sulla Panoramica, poi sulla vista PC per 20 s, poi pannello chiuso, per 5 cicli: RAM privata a pannello chiuso dopo i cicli entro **+2 MB** da quella prima del primo ciclo, handle stabili. Se non passa → implementa la via D3DKMT con gli stessi test e ripeti la misura; annota i numeri nel CHANGELOG.
- [ ] **Step 6:** prova dal vivo: GPU che si muove con un video aperto da Paolo; valore vicino a quello di Gestione attività; fuori dalla vista PC nessun campione GPU.
- [ ] **Step 7:** commit `feat(assistente): utilizzo della GPU nella vista PC` (con l'ok di Paolo).

---

## Chiusura della milestone

- [ ] **Misure** con `tools/misura.ps1` (casa vera connessa, 60 s per stato): pillola a riposo lati niente (≤ M1: 0,193 % / 17 MB, e dentro il budget ≤ 0,1 % / ≤ 15 MB se nel frattempo è stata diagnosticata la differenza con la casa finta), lati CPU+RAM, lato Rete, pannello aperto fermo con fascia del dock, 20 cicli apri/chiudi pannello con vista PC e pannellino (ritorno entro +2 MB).
- [ ] **Piattaforme**: `cargo build -p assistente` nella CI su macOS e Ubuntu (workflow `v3-build`); in CHANGELOG e nelle Impostazioni la riga «Solo su Windows» per volume, Wi-Fi/Bluetooth, cuffie BT, GPU e lato Rete.
- [ ] **Revisione** UI/UX e frontend (agenti su Opus) di `ui/` e delle foto di zona: testi ≥ 12 px, contrasti, Narrator, tastiera, movimento ridotto.
- [ ] **Prova con Paolo** sulla casa vera dei Review Focus 1–5 e delle decisioni D1–D7; poi aggiornare `.superpowers/ux/decisioni.md` con le scelte confermate.
