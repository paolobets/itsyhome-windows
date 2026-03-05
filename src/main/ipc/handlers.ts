import { ipcMain, app, shell, BrowserWindow } from 'electron'
import { HAClient }   from '../ha/client'
import type { Store }         from '../store/store'
import type { EntityMapper }  from '../ha/entity-mapper'
import type { MenuData, ConnectionStatus, AppEntity } from '@shared/types'

export interface AppCallbacks {
  connectToHA:          ()                         => void
  refreshMenuData:      ()                         => Promise<void>
  disconnectFromHA:     ()                         => void
  getMenuData:          ()                         => MenuData | null
  getStatus:            ()                         => ConnectionStatus
  getHAClient:          ()                         => HAClient | null
  openSettings:         ()                         => void
  hidePopup:            ()                         => void
  getAllEntities:        ()                         => AppEntity[]
  repositionPopup:      ()                         => void
  /** Recompute Y only – keeps X (right-edge) fixed. Used while detail panel is open. */
  repositionPopupYOnly: ()                         => void
}

export function setupIpcHandlers(
  store:        Store,
  entityMapper: EntityMapper,
  cb:           AppCallbacks
): void {

  // ─── Window ──────────────────────────────────────────────────────────────
  ipcMain.on('window:hide',         ()         => cb.hidePopup())
  ipcMain.on('window:quit',         ()         => app.quit())
  ipcMain.on('window:openSettings', ()         => cb.openSettings())
  ipcMain.on('window:openHaUrl',    ()         => { const url = store.getHAUrl(); if (url) shell.openExternal(url) })
  ipcMain.on('window:closeSettings',()         => {
    BrowserWindow.getAllWindows().find(w => w.webContents.getURL().includes('settings'))?.close()
  })
  ipcMain.on('window:resize', (_e, h: number, w: number = 300) => {
    const popup = BrowserWindow.getAllWindows().find(win => !win.webContents.getURL().includes('settings'))
    if (popup && !popup.isDestroyed()) {
      const newH = Math.max(120, Math.min(680, h))
      const newW = Math.max(300, Math.min(900, w))
      const bounds = popup.getBounds()
      if (newW !== bounds.width) {
        // Width changing (detail panel open/close): keep the right edge fixed.
        // Use setBounds atomically so there is no intermediate frame where the
        // window is the wrong size/position (avoids the visible left-shift).
        const rightEdge = bounds.x + bounds.width
        const newX = Math.max(0, rightEdge - newW)
        popup.setBounds({ x: newX, y: bounds.y, width: newW, height: newH }, false)
      } else if (newW > 300) {
        // Height-only change while detail panel is open: keep X (right-edge
        // stays fixed) but recalculate Y so the popup stays anchored to the tray.
        popup.setSize(newW, newH, false)
        cb.repositionPopupYOnly()
      } else {
        // Normal height change at 300 px width: full reposition to tray anchor.
        popup.setSize(newW, newH, false)
        cb.repositionPopup()
      }
    }
  })

  // ─── HA connection ───────────────────────────────────────────────────────
  ipcMain.handle('ha:reconnect',    ()         => cb.connectToHA())
  ipcMain.handle('ha:getStatus',    ()         => cb.getStatus())
  ipcMain.handle('ha:callService',  async (_e, domain: string, service: string, entityId: string,
                                            serviceData?: Record<string, unknown>) => {
    const client = cb.getHAClient()
    if (!client) throw new Error('Not connected')
    await client.callService(domain, service, { entity_id: entityId }, serviceData)
  })

  // ─── Menu data ───────────────────────────────────────────────────────────
  ipcMain.handle('menu:getData',    ()         => cb.getMenuData())

  // ─── Favorites ───────────────────────────────────────────────────────────
  ipcMain.handle('favorites:get',   ()         => store.getFavorites())
  ipcMain.handle('favorites:add',   (_e, id: string) => store.addFavorite(id))
  ipcMain.handle('favorites:remove',(_e, id: string) => store.removeFavorite(id))

  // ─── Camera snapshots ────────────────────────────────────────────────────
  ipcMain.handle('camera:getSnapshot', async (_e, entityId: string) => {
    const client = cb.getHAClient()
    if (!client) return null
    return client.getCameraSnapshot(entityId, store.getHAUrl())
  })

  // ─── Config (settings window) ────────────────────────────────────────────
  ipcMain.handle('config:get', () => ({
    haUrl:          store.getHAUrl(),
    haToken:        store.getHAToken(),
    camerasEnabled: store.getCamerasEnabled(),
    launchAtLogin:  store.getLaunchAtLogin(),
    activeEnvId:    store.getActiveEnvironmentId(),
  }))

  // ─── Environments ─────────────────────────────────────────────────────────
  ipcMain.handle('environments:getAll',      ()                                   => store.getEnvironments())
  ipcMain.handle('environments:getActiveId', ()                                   => store.getActiveEnvironmentId())
  ipcMain.handle('environments:add',         (_e, name: string, url: string, token: string) => store.addEnvironment(name, url, token))
  ipcMain.handle('environments:update',      (_e, env)                            => store.updateEnvironment(env))
  ipcMain.handle('environments:remove',      (_e, id: string)                     => store.removeEnvironment(id))
  ipcMain.handle('environments:connect',     (_e, id: string) => {
    store.setActiveEnvironmentId(id)
    cb.connectToHA()
  })
  ipcMain.handle('environments:test',        (_e, url: string, token: string)     => HAClient.test(url, token))

  ipcMain.handle('config:disconnect', () => cb.disconnectFromHA())

  ipcMain.handle('config:setHACredentials', async (_e, url: string, token: string) => {
    store.setHACredentials(url, token)
    cb.connectToHA()
  })

  ipcMain.handle('config:testConnection', (_e, url: string, token: string) =>
    HAClient.test(url, token)
  )

  ipcMain.handle('config:setLaunchAtLogin', (_e, enabled: boolean) => {
    store.setLaunchAtLogin(enabled)
    app.setLoginItemSettings({ openAtLogin: enabled, name: 'ItsyHome' })
  })

  ipcMain.handle('config:setCamerasEnabled', (_e, enabled: boolean) => {
    store.setCamerasEnabled(enabled)
    cb.refreshMenuData()
  })

  // ─── Accessories (settings window) ───────────────────────────────────────
  ipcMain.handle('accessories:getAll',   ()                              => cb.getAllEntities())
  ipcMain.handle('accessories:getRooms', () => {
    const md = cb.getMenuData()
    if (!md) return []
    return md.rooms.map(r => ({ areaId: r.areaId, name: r.name }))
  })
  ipcMain.handle('accessories:getHidden',()                              => store.getHiddenEntities())
  ipcMain.handle('accessories:setHidden',(_e, ids: string[])             => store.setHiddenEntities(ids))
  ipcMain.handle('accessories:getRoomOrder',   ()                        => store.getRoomOrder())
  ipcMain.handle('accessories:setRoomOrder',   (_e, ids: string[])       => store.setRoomOrder(ids))
  ipcMain.handle('accessories:getDeviceOrder', (_e, areaId: string)      => store.getDeviceOrder(areaId))
  ipcMain.handle('accessories:setDeviceOrder', (_e, areaId: string, ids: string[]) => store.setDeviceOrder(areaId, ids))
  ipcMain.handle('accessories:getAreaIcons',      ()                        => store.getAreaIcons())
  ipcMain.handle('accessories:setAreaIcon',       (_e, areaId: string, icon: string | null) => store.setAreaIcon(areaId, icon))
  ipcMain.handle('accessories:getFavorites',      ()                        => store.getFavorites())
  ipcMain.handle('accessories:setFavoritesOrder', (_e, ids: string[])       => store.setFavorites(ids))

  // ─── Autostart ───────────────────────────────────────────────────────────
  ipcMain.handle('autostart:get', () => store.getLaunchAtLogin())
}
