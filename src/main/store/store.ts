import ElectronStore from 'electron-store'
import { randomUUID } from 'crypto'
import type { HAEnvironment } from '@shared/types'

interface StoreSchema {
  // Legacy single-env keys – kept only for one-time migration
  haUrl:          string
  haToken:        string
  // Multi-environment
  environments:   HAEnvironment[]
  activeEnvId:    string
  // Entity prefs
  favorites:      string[]
  hiddenEntities: string[]
  roomOrder:      string[]
  deviceOrder:    Record<string, string[]>
  areaIcons:      Record<string, string>   // custom emoji per areaId
  // App settings
  camerasEnabled: boolean
  launchAtLogin:  boolean
}

const defaults: StoreSchema = {
  haUrl:          '',
  haToken:        '',
  environments:   [],
  activeEnvId:    '',
  favorites:      [],
  hiddenEntities: [],
  roomOrder:      [],
  deviceOrder:    {},
  areaIcons:      {},
  camerasEnabled: true,
  launchAtLogin:  false,
}

export class Store {
  private s: ElectronStore<StoreSchema>

  constructor() {
    this.s = new ElectronStore<StoreSchema>({ defaults, name: 'itsyhome-config' })
    this.migrate()
  }

  /** One-time migration: convert legacy single haUrl/haToken into the first environment. */
  private migrate(): void {
    const legacyUrl   = this.s.get('haUrl')
    const legacyToken = this.s.get('haToken')
    const envs        = this.getEnvironments()
    if ((legacyUrl || legacyToken) && envs.length === 0) {
      const env: HAEnvironment = {
        id: randomUUID(), name: 'Home',
        haUrl: legacyUrl, haToken: legacyToken,
      }
      this.s.set('environments', [env])
      this.s.set('activeEnvId', env.id)
      this.s.set('haUrl',  '')
      this.s.set('haToken', '')
    }
  }

  // ─── Multi-environment ────────────────────────────────────────────────────

  getEnvironments(): HAEnvironment[] {
    return (this.s.get('environments') ?? []) as HAEnvironment[]
  }

  addEnvironment(name: string, url: string, token: string): HAEnvironment {
    const env: HAEnvironment = {
      id:      randomUUID(),
      name:    name.trim() || 'Home',
      haUrl:   url.trim().replace(/\/$/, ''),
      haToken: token.trim(),
    }
    this.s.set('environments', [...this.getEnvironments(), env])
    return env
  }

  updateEnvironment(env: HAEnvironment): void {
    const updated = this.getEnvironments().map(e =>
      e.id === env.id
        ? { ...env, haUrl: env.haUrl.trim().replace(/\/$/, ''), haToken: env.haToken.trim() }
        : e
    )
    this.s.set('environments', updated)
  }

  removeEnvironment(id: string): void {
    this.s.set('environments', this.getEnvironments().filter(e => e.id !== id))
    if (this.getActiveEnvironmentId() === id) this.s.set('activeEnvId', '')
  }

  getActiveEnvironmentId(): string | null {
    const id = this.s.get('activeEnvId')
    return id || null
  }

  setActiveEnvironmentId(id: string | null): void {
    this.s.set('activeEnvId', id ?? '')
  }

  // ─── Derived connection credentials (from active environment) ────────────

  getActiveEnvironment(): HAEnvironment | null {
    const id = this.getActiveEnvironmentId()
    if (!id) return null
    return this.getEnvironments().find(e => e.id === id) ?? null
  }

  getHAUrl():   string { return this.getActiveEnvironment()?.haUrl   ?? '' }
  getHAToken(): string { return this.getActiveEnvironment()?.haToken ?? '' }

  hasCredentials(): boolean {
    const env = this.getActiveEnvironment()
    return !!env?.haUrl && !!env?.haToken
  }

  /** Legacy compat: update active env's credentials, or create a new env if none active. */
  setHACredentials(url: string, token: string): void {
    const activeId = this.getActiveEnvironmentId()
    if (activeId) {
      const updated = this.getEnvironments().map(e =>
        e.id === activeId
          ? { ...e, haUrl: url.trim().replace(/\/$/, ''), haToken: token.trim() }
          : e
      )
      this.s.set('environments', updated)
    } else {
      const env = this.addEnvironment('Home', url, token)
      this.setActiveEnvironmentId(env.id)
    }
  }

  // ─── Favorites ───────────────────────────────────────────────────────────

  getFavorites():              string[] { return this.s.get('favorites') }
  setFavorites(ids: string[]): void    { this.s.set('favorites', ids) }
  addFavorite(id: string):     void { this.s.set('favorites', [...new Set([...this.getFavorites(), id])]) }
  removeFavorite(id: string):  void { this.s.set('favorites', this.getFavorites().filter(f => f !== id)) }

  // ─── Hidden entities ─────────────────────────────────────────────────────

  getHiddenEntities():              string[] { return this.s.get('hiddenEntities') }
  setHiddenEntities(ids: string[]): void     { this.s.set('hiddenEntities', ids) }

  // ─── Room / device ordering ──────────────────────────────────────────────

  getRoomOrder():               string[] { return this.s.get('roomOrder') }
  setRoomOrder(ids: string[]):  void     { this.s.set('roomOrder', ids) }

  getDeviceOrder(areaId: string): string[] {
    return (this.s.get('deviceOrder') as Record<string, string[]>)[areaId] ?? []
  }
  setDeviceOrder(areaId: string, ids: string[]): void {
    const all = this.s.get('deviceOrder') as Record<string, string[]>
    this.s.set('deviceOrder', { ...all, [areaId]: ids })
  }

  // ─── Area icons ───────────────────────────────────────────────────────────

  getAreaIcons(): Record<string, string> {
    return (this.s.get('areaIcons') ?? {}) as Record<string, string>
  }

  setAreaIcon(areaId: string, icon: string | null): void {
    const icons = this.getAreaIcons()
    if (icon) {
      this.s.set('areaIcons', { ...icons, [areaId]: icon })
    } else {
      const next = { ...icons }
      delete next[areaId]
      this.s.set('areaIcons', next)
    }
  }

  // ─── App settings ────────────────────────────────────────────────────────

  getCamerasEnabled():            boolean { return this.s.get('camerasEnabled') }
  setCamerasEnabled(v: boolean):  void    { this.s.set('camerasEnabled', v) }
  getLaunchAtLogin():             boolean { return this.s.get('launchAtLogin') }
  setLaunchAtLogin(v: boolean):   void    { this.s.set('launchAtLogin', v) }
}
