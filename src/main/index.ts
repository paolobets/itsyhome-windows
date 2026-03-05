/**
 * ItsyHome Windows – Main Process
 * Direct port of the macOS ItsyHome app.
 */

import { app, BrowserWindow, Tray, Menu, ipcMain, nativeImage, screen, powerMonitor } from 'electron'
import { join } from 'path'
import { Store }         from './store/store'
import { HAClient }      from './ha/client'
import { EntityMapper }  from './ha/entity-mapper'
import { setupIpcHandlers } from './ipc/handlers'
import type { MenuData, ConnectionStatus, AppEntity } from '@shared/types'
import type { HAState } from './ha/models'

const gotLock = app.requestSingleInstanceLock()
if (!gotLock) { app.quit(); process.exit(0) }

// ─── Global state ─────────────────────────────────────────────────────────────
let tray:           Tray          | null = null
let popupWindow:    BrowserWindow | null = null
let settingsWindow: BrowserWindow | null = null
let haClient:       HAClient      | null = null
let entityMapper:   EntityMapper
let store:          Store
let connectionStatus: ConnectionStatus = 'disconnected'
let menuData:       MenuData | null = null
let allEntities:    AppEntity[]    = []
let lastTrayBounds: Electron.Rectangle | null = null

const POPUP_PRELOAD    = join(__dirname, '../preload/popup.js')
const SETTINGS_PRELOAD = join(__dirname, '../preload/settings.js')

// ─── App lifecycle ────────────────────────────────────────────────────────────

app.whenReady().then(async () => {
  if (process.platform === 'darwin') app.dock?.hide()
  Menu.setApplicationMenu(null)   // remove the default File/Edit/View/Window/Help bar

  store        = new Store()
  entityMapper = new EntityMapper(store)

  setupIpcHandlers(store, entityMapper, {
    connectToHA,
    refreshMenuData,
    disconnectFromHA,
    getMenuData:       () => menuData,
    getStatus:         () => connectionStatus,
    getHAClient:       () => haClient,
    openSettings:      openSettingsWindow,
    hidePopup,
    getAllEntities:     () => allEntities,
    repositionPopup: () => {
      if (lastTrayBounds && popupWindow && !popupWindow.isDestroyed()) {
        const { x, y } = calcPopupPosition(lastTrayBounds, popupWindow.getBounds())
        popupWindow.setPosition(x, y, false)
      }
    },
    repositionPopupYOnly: () => {
      if (lastTrayBounds && popupWindow && !popupWindow.isDestroyed()) {
        const bounds = popupWindow.getBounds()
        const { y } = calcPopupPosition(lastTrayBounds, bounds)
        // Keep X unchanged – preserves the right-edge anchor while detail panel is open
        popupWindow.setPosition(bounds.x, y, false)
      }
    }
  })

  // Reconnect when the system wakes from sleep
  powerMonitor.on('resume', () => connectToHA())

  await createTray()
  createPopupWindow()
  setupAutostart()

  if (store.hasCredentials()) connectToHA()
  else openSettingsWindow()
})

app.on('second-instance', () => showPopup())
app.on('before-quit',     () => haClient?.disconnect())
app.on('window-all-closed', (e: Event) => e.preventDefault())

// ─── Tray ─────────────────────────────────────────────────────────────────────

async function createTray(): Promise<void> {
  const icon = getTrayIcon('default')
  tray = new Tray(icon)
  tray.setToolTip('ItsyHome – Home Assistant')

  tray.on('click', (_e, bounds) => togglePopup(bounds))
  tray.on('right-click', () => {
    const envs     = store.getEnvironments()
    const activeId = store.getActiveEnvironmentId()

    const switchItems: Electron.MenuItemConstructorOptions[] = envs.map(env => ({
      label:   (env.id === activeId ? '✓  ' : '     ') + env.name,
      enabled: env.id !== activeId,
      click: () => {
        store.setActiveEnvironmentId(env.id)
        connectToHA()
      }
    }))

    const menu = Menu.buildFromTemplate([
      { label: 'Settings',      click: () => openSettingsWindow() },
      {
        label:   'Switch Server',
        enabled: envs.length > 0,
        submenu: switchItems.length ? switchItems : [{ label: 'No servers configured', enabled: false }]
      },
      { label: 'Refresh',       click: () => connectionStatus === 'connected' ? void refreshMenuData() : connectToHA() },
      { type: 'separator' },
      { label: 'Quit ItsyHome', click: () => app.quit() }
    ])
    tray?.popUpContextMenu(menu)
  })
}

function getTrayIcon(state: 'default' | 'error' | 'connecting'): Electron.NativeImage {
  const names: Record<string, string> = {
    default:    'tray-default.png',
    error:      'tray-error.png',
    connecting: 'tray-connecting.png',
  }
  const resourcesDir = app.isPackaged
    ? process.resourcesPath
    : join(__dirname, '../../resources')
  const iconPath = join(resourcesDir, names[state])
  const img = nativeImage.createFromPath(iconPath)
  if (!img.isEmpty()) return img

  // Fallback: colored square if file not found
  const size = 16
  const colors: Record<string, [number,number,number]> = {
    default:    [0x03, 0xA9, 0xF4],
    error:      [0xFF, 0x45, 0x3A],
    connecting: [0xFF, 0x9F, 0x0A],
  }
  const [r,g,b] = colors[state]
  const buf = Buffer.alloc(size * size * 4)
  for (let i = 0; i < size * size; i++) {
    buf[i*4]=r; buf[i*4+1]=g; buf[i*4+2]=b; buf[i*4+3]=255
  }
  return nativeImage.createFromBuffer(buf, { width: size, height: size })
}

function updateTrayIcon(): void {
  if (!tray) return
  tray.setImage(getTrayIcon(
    connectionStatus === 'connected'   ? 'default' :
    connectionStatus === 'error'       ? 'error'   : 'connecting'
  ))
}

// ─── Popup window ─────────────────────────────────────────────────────────────

function createPopupWindow(): void {
  popupWindow = new BrowserWindow({
    width: 300, height: 54,
    frame: false, transparent: true,
    alwaysOnTop: true, skipTaskbar: true,
    resizable: true, movable: true,
    minWidth: 300, minHeight: 54,
    show: false,
    webPreferences: {
      preload: POPUP_PRELOAD,
      contextIsolation: true,
      sandbox: false
    }
  })

  const url = process.env.ELECTRON_RENDERER_URL
  if (url) popupWindow.loadURL(`${url}/popup/index.html`)
  else     popupWindow.loadFile(join(__dirname, '../renderer/popup/index.html'))

  popupWindow.on('blur', () => {
    if (!settingsWindow || settingsWindow.isDestroyed()) hidePopup()
  })
  popupWindow.on('closed', () => { popupWindow = null })
}

function togglePopup(trayBounds: Electron.Rectangle): void {
  lastTrayBounds = trayBounds
  if (!popupWindow || popupWindow.isDestroyed()) { createPopupWindow(); showPopup(trayBounds); return }
  if (popupWindow.isVisible()) hidePopup()
  else showPopup(trayBounds)
}

function showPopup(trayBounds?: Electron.Rectangle): void {
  if (!popupWindow || popupWindow.isDestroyed()) return
  const bounds = trayBounds ?? lastTrayBounds ?? undefined
  if (bounds) {
    const { x, y } = calcPopupPosition(bounds, popupWindow.getBounds())
    popupWindow.setPosition(x, y, false)
  }
  popupWindow.show()
  popupWindow.focus()
}

function hidePopup(): void {
  popupWindow?.hide()
}

function calcPopupPosition(tray: Electron.Rectangle, win: Electron.Rectangle): { x: number; y: number } {
  const display = screen.getDisplayNearestPoint({ x: tray.x, y: tray.y })
  const work    = display.workArea
  let x = Math.round(tray.x + tray.width / 2 - win.width / 2)
  let y = tray.y < work.height / 2 ? tray.y + tray.height + 4 : tray.y - win.height - 4
  x = Math.max(work.x + 4, Math.min(x, work.x + work.width  - win.width  - 4))
  y = Math.max(work.y + 4, Math.min(y, work.y + work.height - win.height - 4))
  return { x, y }
}

// ─── Settings window ──────────────────────────────────────────────────────────

function openSettingsWindow(): void {
  if (settingsWindow && !settingsWindow.isDestroyed()) {
    settingsWindow.focus(); return
  }
  settingsWindow = new BrowserWindow({
    width: 640, height: 500,
    minWidth: 520, minHeight: 400,
    title: 'ItsyHome Settings',
    titleBarStyle: 'default',
    resizable: true,
    show: false,
    webPreferences: {
      preload: SETTINGS_PRELOAD,
      contextIsolation: true,
      sandbox: false
    }
  })

  const url = process.env.ELECTRON_RENDERER_URL
  if (url) settingsWindow.loadURL(`${url}/settings/index.html`)
  else     settingsWindow.loadFile(join(__dirname, '../renderer/settings/index.html'))

  settingsWindow.once('ready-to-show', () => settingsWindow?.show())
  settingsWindow.on('closed', () => { settingsWindow = null })
}

// ─── HA connection ────────────────────────────────────────────────────────────

function connectToHA(): void {
  haClient?.disconnect()
  haClient = null

  const url   = store.getHAUrl()
  const token = store.getHAToken()
  if (!url || !token) return

  setStatus('connecting')

  haClient = new HAClient(url, token)
  haClient.connect(onStateChanged, (status) => {
    setStatus(status)
    if (status === 'connected') refreshMenuData()
  })
}

async function refreshMenuData(): Promise<void> {
  if (!haClient) return
  try {
    const [states, areas, devices, entries, config, notifications] = await Promise.all([
      haClient.getStates(),
      haClient.getAreas(),
      haClient.getDevices(),
      haClient.getEntities(),
      haClient.getConfig().catch(() => undefined),
      haClient.getNotifications().catch(() => [])
    ])
    menuData = entityMapper.map(states, areas, devices, entries, config)
    // Persistent notifications live outside the state machine in newer HA versions —
    // override the count with the dedicated API result (dismissed ones are not returned).
    menuData.notificationCount = notifications.length
    // Build allEntities without duplicates: favorites are a subset of room entities,
    // so only include rooms+scenes+cameras (deduplication prevents double-display in settings).
    const seenIds = new Set<string>()
    allEntities = [
      ...menuData.rooms.flatMap(r => r.entities),
      ...menuData.scenes,
      ...menuData.cameras
    ].filter(e => { if (seenIds.has(e.entityId)) return false; seenIds.add(e.entityId); return true })
    broadcastMenuUpdate()
  } catch (err) {
    console.error('[ItsyHome] refreshMenuData error:', err)
  }
}

let stateChangeTimer: ReturnType<typeof setTimeout> | null = null

function onStateChanged(_entityId: string, _newState: HAState): void {
  // Debounce: coalesce rapid burst state changes into a single refresh
  if (stateChangeTimer) clearTimeout(stateChangeTimer)
  stateChangeTimer = setTimeout(() => {
    stateChangeTimer = null
    void refreshMenuData()
  }, 300)
}

function disconnectFromHA(): void {
  haClient?.disconnect()
  haClient = null
  store.setActiveEnvironmentId(null)  // deactivate without deleting credentials
  menuData    = null
  allEntities = []
  setStatus('disconnected')
  broadcastMenuUpdate()
}

function broadcastMenuUpdate(): void {
  for (const win of [popupWindow, settingsWindow]) {
    if (win && !win.isDestroyed()) win.webContents.send('menu:update', menuData)
  }
}

function setStatus(status: ConnectionStatus): void {
  connectionStatus = status
  updateTrayIcon()
  for (const win of [popupWindow, settingsWindow]) {
    if (win && !win.isDestroyed()) win.webContents.send('ha:statusChange', status)
  }
}

// ─── Autostart ────────────────────────────────────────────────────────────────

function setupAutostart(): void {
  const enabled = store.getLaunchAtLogin()
  app.setLoginItemSettings({ openAtLogin: enabled, name: 'ItsyHome' })
}
