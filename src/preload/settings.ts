import { contextBridge, ipcRenderer } from 'electron'
import type { SettingsAPI } from '@shared/types'

const api: SettingsAPI = {
  ha: {
    onStatusChange: (cb) => {
      const handler = (_e: Electron.IpcRendererEvent, s: Parameters<typeof cb>[0]) => cb(s)
      ipcRenderer.on('ha:statusChange', handler)
      return () => ipcRenderer.removeListener('ha:statusChange', handler)
    }
  },
  config: {
    get:               ()            => ipcRenderer.invoke('config:get'),
    setHACredentials:  (url, token)  => ipcRenderer.invoke('config:setHACredentials', url, token),
    testConnection:    (url, token)  => ipcRenderer.invoke('config:testConnection', url, token),
    setLaunchAtLogin:  (v)           => ipcRenderer.invoke('config:setLaunchAtLogin', v),
    setCamerasEnabled: (v)           => ipcRenderer.invoke('config:setCamerasEnabled', v),
    disconnect:        ()            => ipcRenderer.invoke('config:disconnect')
  },
  environments: {
    getAll:      ()                       => ipcRenderer.invoke('environments:getAll'),
    getActiveId: ()                       => ipcRenderer.invoke('environments:getActiveId'),
    add:         (name, url, token)       => ipcRenderer.invoke('environments:add', name, url, token),
    update:      (env)                    => ipcRenderer.invoke('environments:update', env),
    remove:      (id)                     => ipcRenderer.invoke('environments:remove', id),
    connect:     (id)                     => ipcRenderer.invoke('environments:connect', id),
    test:        (url, token)             => ipcRenderer.invoke('environments:test', url, token),
  },
  accessories: {
    getAll:         ()                    => ipcRenderer.invoke('accessories:getAll'),
    getRooms:       ()                    => ipcRenderer.invoke('accessories:getRooms'),
    getHidden:      ()                    => ipcRenderer.invoke('accessories:getHidden'),
    setHidden:      (ids)                 => ipcRenderer.invoke('accessories:setHidden', ids),
    getRoomOrder:   ()                    => ipcRenderer.invoke('accessories:getRoomOrder'),
    setRoomOrder:   (ids)                 => ipcRenderer.invoke('accessories:setRoomOrder', ids),
    getDeviceOrder: (areaId)              => ipcRenderer.invoke('accessories:getDeviceOrder', areaId),
    setDeviceOrder: (areaId, ids)         => ipcRenderer.invoke('accessories:setDeviceOrder', areaId, ids),
    getAreaIcons:      ()           => ipcRenderer.invoke('accessories:getAreaIcons'),
    setAreaIcon:       (areaId, icon) => ipcRenderer.invoke('accessories:setAreaIcon', areaId, icon),
    getFavorites:      ()           => ipcRenderer.invoke('accessories:getFavorites'),
    setFavoritesOrder: (ids)        => ipcRenderer.invoke('accessories:setFavoritesOrder', ids),
  },
  window: {
    close: () => ipcRenderer.send('window:closeSettings')
  }
}

contextBridge.exposeInMainWorld('api', api)
