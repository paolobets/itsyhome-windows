/**
 * Tauri IPC adapter – single point of access for all backend commands and events.
 *
 * Mirrors the shape of the former window.api (PopupAPI + SettingsAPI) so that
 * renderer code only needs to swap `window.api.` → `api.` with no other changes
 * to call sites.
 *
 * Listener helpers follow the pattern:
 *   onXxx(cb) → () => void   (call the returned fn to unlisten)
 *
 * The underlying listen() returns Promise<UnlistenFn>; we wrap it so callers
 * get a plain synchronous cleanup function.
 */

import { invoke }  from '@tauri-apps/api/core'
import { listen }  from '@tauri-apps/api/event'
import type {
  MenuData,
  ConnectionStatus,
  HAEnvironment,
  AppConfig,
  AppEntity,
} from '@shared/types'

// ─── Helper: wrap an async unlisten into a sync cancel fn ────────────────────
function makeUnlisten(promise: Promise<() => void>): () => void {
  let unlisten: (() => void) | null = null
  let cancelled = false
  promise.then(fn => {
    if (cancelled) { fn(); return }
    unlisten = fn
  })
  return () => {
    cancelled = true
    unlisten?.()
  }
}

// ─── Public API object ────────────────────────────────────────────────────────

export const api = {

  // ── Menu ──────────────────────────────────────────────────────────────────

  menu: {
    getData: (): Promise<MenuData | null> =>
      invoke<MenuData | null>('get_menu_data'),

    onUpdate: (cb: (data: MenuData | null) => void): (() => void) =>
      makeUnlisten(listen<MenuData | null>('menu:update', e => cb(e.payload))),
  },

  // ── Home Assistant ────────────────────────────────────────────────────────

  ha: {
    callService: (
      domain:       string,
      service:      string,
      entityId:     string,
      serviceData?: Record<string, unknown>,
    ): Promise<void> =>
      invoke<void>('call_service', { domain, service, entityId, serviceData }),

    reconnect: (): Promise<void> =>
      invoke<void>('reconnect_ha'),

    disconnect: (): Promise<void> =>
      invoke<void>('disconnect_ha'),

    getStatus: (): Promise<ConnectionStatus> =>
      invoke<ConnectionStatus>('get_status'),

    onStatusChange: (cb: (s: ConnectionStatus) => void): (() => void) =>
      makeUnlisten(listen<ConnectionStatus>('ha:statusChange', e => cb(e.payload))),
  },

  // ── Favorites ─────────────────────────────────────────────────────────────

  favorites: {
    get:    (): Promise<string[]>       => invoke<string[]>('favorites_get'),
    add:    (id: string): Promise<void> => invoke<void>('favorites_add',    { id }),
    remove: (id: string): Promise<void> => invoke<void>('favorites_remove', { id }),
  },

  // ── Cameras ───────────────────────────────────────────────────────────────

  cameras: {
    getSnapshot: (entityId: string): Promise<string | null> =>
      invoke<string | null>('get_camera_snapshot', { entityId }),
  },

  // ── Accessories ───────────────────────────────────────────────────────────

  accessories: {
    getAll: (): Promise<AppEntity[]> =>
      invoke<AppEntity[]>('accessories_get_all'),

    getRooms: (): Promise<Array<{ areaId: string; name: string }>> =>
      invoke<Array<{ areaId: string; name: string }>>('accessories_get_rooms'),

    getHidden: (): Promise<string[]> =>
      invoke<string[]>('accessories_get_hidden'),

    setHidden: (ids: string[]): Promise<void> =>
      invoke<void>('accessories_set_hidden', { ids }),

    getRoomOrder: (): Promise<string[]> =>
      invoke<string[]>('accessories_get_room_order'),

    setRoomOrder: (ids: string[]): Promise<void> =>
      invoke<void>('accessories_set_room_order', { ids }),

    getDeviceOrder: (areaId: string): Promise<string[]> =>
      invoke<string[]>('accessories_get_device_order', { areaId }),

    setDeviceOrder: (areaId: string, ids: string[]): Promise<void> =>
      invoke<void>('accessories_set_device_order', { areaId, ids }),

    getAreaIcons: (): Promise<Record<string, string>> =>
      invoke<Record<string, string>>('accessories_get_area_icons'),

    setAreaIcon: (areaId: string, icon: string | null): Promise<void> =>
      invoke<void>('accessories_set_area_icon', { areaId, icon }),

    getFavorites: (): Promise<string[]> =>
      invoke<string[]>('accessories_get_favorites'),

    setFavoritesOrder: (ids: string[]): Promise<void> =>
      invoke<void>('accessories_set_favorites_order', { ids }),
  },

  // ── Environments ──────────────────────────────────────────────────────────

  environments: {
    getAll: (): Promise<HAEnvironment[]> =>
      invoke<HAEnvironment[]>('environments_get_all'),

    getActiveId: (): Promise<string | null> =>
      invoke<string | null>('environments_get_active_id'),

    add: (name: string, url: string, token: string): Promise<HAEnvironment> =>
      invoke<HAEnvironment>('environments_add', { name, url, token }),

    update: (env: HAEnvironment): Promise<void> =>
      invoke<void>('environments_update', { env }),

    remove: (id: string): Promise<void> =>
      invoke<void>('environments_remove', { id }),

    connect: (id: string): Promise<void> =>
      invoke<void>('environments_connect', { id }),

    test: (
      url:   string,
      token: string,
    ): Promise<{ ok: boolean; entityCount?: number; error?: string }> =>
      invoke<{ ok: boolean; entityCount?: number; error?: string }>(
        'test_connection', { url, token },
      ),
  },

  // ── Config ────────────────────────────────────────────────────────────────

  config: {
    get: (): Promise<AppConfig> =>
      invoke<AppConfig>('get_config'),

    setHACredentials: (url: string, token: string): Promise<void> =>
      invoke<void>('set_ha_credentials', { url, token }),

    setLaunchAtLogin: (enabled: boolean): Promise<void> =>
      invoke<void>('set_launch_at_login', { enabled }),

    setCamerasEnabled: (enabled: boolean): Promise<void> =>
      invoke<void>('set_cameras_enabled', { enabled }),

    disconnect: (): Promise<void> =>
      invoke<void>('config_disconnect'),
  },

  // ── Window ────────────────────────────────────────────────────────────────

  window: {
    hide: (): Promise<void> =>
      invoke<void>('window_hide'),

    quit: (): Promise<void> =>
      invoke<void>('window_quit'),

    openSettings: (): Promise<void> =>
      invoke<void>('window_open_settings'),

    openHaUrl: (): Promise<void> =>
      invoke<void>('window_open_ha_url'),

    resize: (h: number, w?: number): Promise<void> =>
      invoke<void>('window_resize', { h, w: w ?? 300 }),

    closeSettings: (): Promise<void> =>
      invoke<void>('window_close_settings'),
  },
}
