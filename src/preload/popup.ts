import { contextBridge, ipcRenderer } from 'electron'
import type { PopupAPI } from '@shared/types'

const api: PopupAPI = {
  menu: {
    getData:  ()    => ipcRenderer.invoke('menu:getData'),
    onUpdate: (cb)  => {
      const handler = (_e: Electron.IpcRendererEvent, data: Parameters<typeof cb>[0]) => cb(data)
      ipcRenderer.on('menu:update', handler)
      return () => ipcRenderer.removeListener('menu:update', handler)
    }
  },
  ha: {
    callService: (domain, service, entityId, data) =>
      ipcRenderer.invoke('ha:callService', domain, service, entityId, data),
    reconnect:   ()   => ipcRenderer.invoke('ha:reconnect'),
    getStatus:   ()   => ipcRenderer.invoke('ha:getStatus'),
    onStatusChange: (cb) => {
      const handler = (_e: Electron.IpcRendererEvent, s: Parameters<typeof cb>[0]) => cb(s)
      ipcRenderer.on('ha:statusChange', handler)
      return () => ipcRenderer.removeListener('ha:statusChange', handler)
    }
  },
  favorites: {
    get:    ()   => ipcRenderer.invoke('favorites:get'),
    add:    (id) => ipcRenderer.invoke('favorites:add', id),
    remove: (id) => ipcRenderer.invoke('favorites:remove', id)
  },
  cameras: {
    getSnapshot: (id) => ipcRenderer.invoke('camera:getSnapshot', id)
  },
  accessories: {
    getAreaIcons: () => ipcRenderer.invoke('accessories:getAreaIcons'),
  },
  environments: {
    getAll:      () => ipcRenderer.invoke('environments:getAll'),
    getActiveId: () => ipcRenderer.invoke('environments:getActiveId'),
    connect:     (id) => ipcRenderer.invoke('environments:connect', id)
  },
  window: {
    hide:         () => ipcRenderer.send('window:hide'),
    quit:         () => ipcRenderer.send('window:quit'),
    openSettings: () => ipcRenderer.send('window:openSettings'),
    openHaUrl:    () => ipcRenderer.send('window:openHaUrl'),
    resize:       (h, w?) => ipcRenderer.send('window:resize', h, w ?? 300)
  }
}

contextBridge.exposeInMainWorld('api', api)
