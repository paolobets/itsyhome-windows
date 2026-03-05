# ItsyHome for Windows

Client per **Home Assistant** nella system tray di Windows.
Controlla luci, termostati, cover, scene e molto altro direttamente dalla barra delle applicazioni, senza aprire il browser.

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
  - 💡 **Luci** — toggle on/off + slider luminosità + selettore colore temperatura
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
- Tutte le preferenze salvate localmente con `electron-store`
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
npm run dev        # modalità sviluppo con hot-reload
npm run build      # compila in out/
npm run dist:win   # build + installer Windows (.exe)
```

### Stack tecnico
- **Electron** (main process + preload)
- **electron-vite** — bundler con alias TypeScript
- **TypeScript** — tutto il codice sorgente
- **WebSocket** nativo (`ws`) — connessione real-time con HA
- **electron-store** — persistenza configurazione
- **electron-builder** — packaging e installer NSIS

---

## Struttura del progetto

```
src/
├── main/               # Main process Electron
│   ├── index.ts        # Entry point, tray, finestre
│   ├── ipc/handlers.ts # IPC handlers (main → renderer)
│   ├── ha/
│   │   ├── client.ts       # Client WebSocket Home Assistant
│   │   └── entity-mapper.ts# Mappa stati HA → AppEntity
│   └── store/store.ts  # Persistenza (electron-store)
├── preload/
│   ├── popup.ts        # API esposta al popup
│   └── settings.ts     # API esposta alle impostazioni
├── shared/
│   ├── types.ts        # Tipi condivisi (AppEntity, MenuData…)
│   └── roomIcons.ts    # Icone emoji per le aree
└── renderer/
    ├── popup/          # UI menu popup (HTML + CSS + TS)
    └── settings/       # UI impostazioni (HTML + CSS + TS)
```

---

## Versioning

| Versione | Note |
|----------|------|
| 1.0.1    | Menu popup spostabile (drag dalla top bar), bottone _ per nascondere nella tray invece di chiudere |
| 1.0.0    | Prima release pubblica — multi-env, badge notifiche/aggiornamenti, preferiti, eye-toggle, icone cover/sensor |

---

## Licenza

MIT
