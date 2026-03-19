# ItsyHome for Windows

[![Version](https://img.shields.io/badge/version-2.0.0-blue.svg)](https://github.com/paolobets/itsyhome-windows/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Client per **Home Assistant** nella system tray di Windows.
Controlla luci, termostati, cover, scene e molto altro direttamente dalla barra delle applicazioni, senza aprire il browser.

**Novità in v2.0**: Migrato da Electron a Tauri 2.0 — installer 22x più leggero, RAM ridotto dell'80%, startup 4x più veloce.

---

## Funzionalità

### System Tray
- **Icona nella tray** con stati visivi distinti: connesso, in connessione, disconnesso, errore
- **Badge notifiche** (rosso): conta le notifiche persistenti attive in Home Assistant
- **Badge aggiornamenti** (blu): conta le entità `update.*` disponibili
- **Clic sinistro** → apre/chiude il menu popup
- **Clic destro** → menu contestuale con Impostazioni, Apri HA, Esci

### Menu Popup
- **Barra superiore** con nome dell'ambiente attivo, badge notifiche/aggiornamenti e pulsante 🌐 per aprire HA nel browser
- **Preferiti** in cima: dispositivi contrassegnati come preferiti, ad accesso rapido
- **Room/aree** in ordine personalizzabile con collapse/espandi
- **Controllo per tipo di entità:**
  - 💡 **Luci** — toggle on/off + slider luminosità inline + bottone colore (anello arcobaleno RGB / sfumatura temperatura bianco); clic sul bottone apre un **picker 2D a canvas** drag-and-drop (asse X = tonalità, asse Y = saturazione/luminosità)
  - 🔌 **Switch** — toggle on/off
  - 🌡️ **Clima** (termostati) — temperatura target +/- e modalità operative
  - 🪟 **Cover** (tapparelle, garage, porte) — pulsanti Apri / Stop / Chiudi
  - 🔒 **Lock** (serrature) — blocca / sblocca
  - 💨 **Fan** — toggle on/off
  - 💧 **Humidifier** (umidificatori) — toggle on/off
  - 🚰 **Valve** (valvole) — toggle on/off
  - 📊 **Sensor** (sensori) — valore + unità di misura in sola lettura
  - 🔐 **Alarm** (antifurto) — disarma / arma casa / arma assente / arma notte
  - 📷 **Camere** — anteprima MJPEG integrata nel popup (attivabile/disattivabile)
  - ✨ **Scene** — attivazione con un clic
- **Aggiornamento in tempo reale** via WebSocket (nessun polling)

### Multi-Ambiente
- Gestione di **più server Home Assistant** con credenziali separate
- Connessione / disconnessione per ambiente con indicatore di stato
- Test di connessione prima del salvataggio

### Impostazioni
- **Tab HA** — elenco server, aggiunta, modifica, rimozione, test connessione
- **Tab Accessori** — lista completa di tutti i dispositivi organizzata per area:
  - Gruppo **Preferiti** sempre in cima, riordinabile via drag-and-drop
  - Icona area personalizzabile (picker emoji)
  - Riordinamento aree via drag-and-drop
  - Riordinamento dispositivi all'interno dell'area via drag-and-drop
  - Pulsante 👁 per mostrare/nascondere singoli dispositivi dal menu
  - Nomi grigi per i dispositivi nascosti
  - Icone entità allineate al popup (cover e sensori usano `deviceClass`)
- **Tab Camere** — toggle per abilitare/disabilitare la sezione camere nel popup
- **Tab Generali** — toggle avvio automatico con Windows

### Persistenza
- Tutte le preferenze salvate localmente con `tauri-plugin-store`
- Ordine aree, ordine dispositivi, preferiti, nascosti, icone area
- Token e URL HA salvati in modo sicuro nel profilo utente

---

## Requisiti

- Windows 10/11 x64
- Home Assistant 2022+ con Long-Lived Access Token

---

## Installazione

Scarica `ItsyHome-Setup-x.x.x.exe` dalla sezione **Releases** ed eseguilo.
L'installer NSIS permette di scegliere la cartella di installazione e crea un collegamento nel menu Start.

---

## Sviluppo

```bash
npm install
npm run dev        # modalità sviluppo con hot-reload (Vite dev server + tauri dev)
npm run build:vite # compila frontend in dist/
npm run build:release # compila frontend + backend + crea installer (build-release.bat)
```

Per la build di produzione, è importante mantenere l'ordine:
1. `npm run build:vite` — compila il frontend TypeScript/Vite in `dist/`
2. `cargo build --release` — compila il backend Rust (embeds `dist/` nel binario)
3. `npx tauri bundle` — crea MSI + NSIS installer

È possibile usare lo script `build-release.bat` che automatizza l'intero processo.

### Stack tecnico
- **Tauri 2.0** — framework desktop con Rust backend + WebView2 frontend
- **Rust** (tokio, tokio-tungstenite, reqwest) — backend completo con WebSocket e HTTP
- **TypeScript + Vite 6** — frontend compilato in HTML/CSS/JS
- **tauri-plugin-store** — persistenza configurazione
- **tauri-plugin-autostart** — avvio automatico con Windows
- **tauri-plugin-single-instance** — singola istanza applicazione

---

## Struttura del progetto

```
src-tauri/
├── src/
│   ├── lib.rs          # Setup Tauri: tray, finestre, plugin
│   ├── commands.rs     # Comandi Tauri (window_resize, show_popup, ecc.)
│   ├── refresh.rs      # Ciclo aggiornamento dati HA
│   ├── types.rs        # Tipi condivisi Rust (AppEntity, MenuData, ecc.)
│   └── ha/
│       ├── client.rs   # Client WebSocket Home Assistant
│       ├── mapper.rs   # Mappa stati HA → AppEntity
│       └── models.rs   # Modelli raw HA API
├── Cargo.toml
└── tauri.conf.json
src/
├── lib/api.ts          # Adapter IPC unico (invoke/listen Tauri)
├── shared/
│   ├── types.ts        # Tipi TypeScript condivisi
│   └── roomIcons.ts    # Icone emoji aree
└── renderer/
    ├── popup/          # UI menu popup (main.ts, entities.ts, style.css)
    └── settings/       # UI impostazioni (main.ts, style.css)
build-release.bat       # Script build produzione (Windows)
```

---

## Performance

Miglioramenti significativi rispetto a Electron v1.x:

| Metrica | v1.x (Electron) | v2.0 (Tauri) | Miglioramento |
|---|---|---|---|
| Dimensione installer | ~180 MB | ~8 MB | 22x più piccolo |
| RAM (idle) | ~120 MB | ~25 MB | 5x meno memoria |
| Cold startup | ~2 s | < 500 ms | 4x più veloce |

---

## Versioning

| Versione | Note |
|----------|------|
| 2.0.0    | Migrazione da Electron a Tauri 2.0 — 22x installer più piccolo, 5x meno RAM, 4x startup più veloce |
| 1.0.3    | Bug fix & ottimizzazioni: reject pending WebSocket su disconnect, race condition animationend, interval leak telecamere, pointercancel hue picker, O(n²)→O(n) ordinamento aree/dispositivi, singolo loop stati HA, resize RAF debounce, WeakMap draw fn |
| 1.0.2    | Redesign UX popup: animazioni fade + slide pannello dettaglio, picker colore 2D canvas per luci RGB, slider luminosità inline, bottone colore ring-style, badge con icone 🔔/⬆ |
| 1.0.1    | Menu popup spostabile (drag dalla top bar), bottone _ per nascondere nella tray invece di chiudere |
| 1.0.0    | Prima release pubblica — multi-env, badge notifiche/aggiornamenti, preferiti, eye-toggle, icone cover/sensor |

---

## Licenza

MIT
