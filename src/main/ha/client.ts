import WebSocket from 'ws'
import type { HAState, HAArea, HADevice, HAEntityEntry, HAConfig, WSMessage } from './models'

type StateChangedCallback = (entityId: string, newState: HAState) => void
type StatusCallback       = (status: 'connected' | 'connecting' | 'disconnected' | 'error') => void

const RECONNECT_DELAYS = [1000, 2000, 4000, 8000, 16000, 30000]

export class HAClient {
  private url:    string
  private token:  string
  private ws:     WebSocket | null = null
  private msgId = 1
  private pending = new Map<number, { resolve: (v: unknown) => void; reject: (e: Error) => void }>()
  private subscriptionId: number | null = null
  private onStateChanged: StateChangedCallback | null = null
  private onStatus:       StatusCallback | null = null
  private reconnectAttempt = 0
  private reconnectTimer:  ReturnType<typeof setTimeout> | null = null
  private pingInterval:    ReturnType<typeof setInterval> | null = null
  private pingTimeout:     ReturnType<typeof setTimeout> | null = null
  private disposed = false

  constructor(url: string, token: string) {
    this.url   = url.replace(/\/$/, '')
    this.token = token
  }

  // ─── Public API ─────────────────────────────────────────────────────────────

  connect(onStateChanged: StateChangedCallback, onStatus: StatusCallback): void {
    this.onStateChanged = onStateChanged
    this.onStatus       = onStatus
    this.openSocket()
  }

  disconnect(): void {
    this.disposed = true
    this.clearReconnect()
    this.stopPingHeartbeat()
    for (const p of this.pending.values()) p.reject(new Error('Disconnected'))
    this.pending.clear()
    this.ws?.close()
    this.ws = null
  }

  async getStates():  Promise<HAState[]>        { return this.wsCommand('get_states') as Promise<HAState[]> }
  async getAreas():   Promise<HAArea[]>          { return this.wsCommand('config/area_registry/list') as Promise<HAArea[]> }
  async getDevices(): Promise<HADevice[]>        { return this.wsCommand('config/device_registry/list') as Promise<HADevice[]> }
  async getEntities(): Promise<HAEntityEntry[]>  { return this.wsCommand('config/entity_registry/list') as Promise<HAEntityEntry[]> }
  async getConfig():  Promise<HAConfig>          { return this.wsCommand('get_config') as Promise<HAConfig> }
  /** Returns active persistent notifications via the dedicated HA WS API (not state machine). */
  async getNotifications(): Promise<{ notification_id: string; status: string }[]> {
    return this.wsCommand('persistent_notification/get') as Promise<{ notification_id: string; status: string }[]>
  }

  async callService(domain: string, service: string, target: { entity_id: string }, serviceData?: Record<string, unknown>): Promise<void> {
    await this.wsCommand('call_service', { domain, service, target, service_data: serviceData ?? {} })
  }

  async getCameraSnapshot(entityId: string, haBaseUrl: string): Promise<string | null> {
    try {
      const res = await fetch(`${haBaseUrl}/api/camera_proxy/${entityId}`, {
        headers: { Authorization: `Bearer ${this.token}` },
        signal: AbortSignal.timeout(8000)
      })
      if (!res.ok) return null
      const buf = await res.arrayBuffer()
      const b64 = Buffer.from(buf).toString('base64')
      return `data:image/jpeg;base64,${b64}`
    } catch { return null }
  }

  // One-shot connection test (does not set up the long-lived connection)
  static async test(url: string, token: string): Promise<{ ok: boolean; entityCount?: number; error?: string }> {
    return new Promise(resolve => {
      const wsUrl = url.replace(/^http/, 'ws') + '/api/websocket'
      let ws: WebSocket
      try { ws = new WebSocket(wsUrl) } catch (e) {
        return resolve({ ok: false, error: String(e) })
      }
      let msgId = 1
      const timeout = setTimeout(() => {
        ws.close(); resolve({ ok: false, error: 'Timeout' })
      }, 10_000)

      ws.on('message', raw => {
        const msg = JSON.parse(String(raw)) as WSMessage
        if (msg.type === 'auth_required') {
          ws.send(JSON.stringify({ type: 'auth', access_token: token }))
        } else if (msg.type === 'auth_ok') {
          ws.send(JSON.stringify({ type: 'get_states', id: msgId++ }))
        } else if (msg.type === 'auth_invalid') {
          clearTimeout(timeout); ws.close(); resolve({ ok: false, error: 'Invalid token' })
        } else if (msg.type === 'result') {
          clearTimeout(timeout); ws.close()
          if (msg.success) resolve({ ok: true, entityCount: (msg.result as HAState[]).length })
          else resolve({ ok: false, error: msg.error?.message })
        }
      })
      ws.on('error', e => { clearTimeout(timeout); resolve({ ok: false, error: e.message }) })
    })
  }

  // ─── Internal ────────────────────────────────────────────────────────────────

  private openSocket(): void {
    if (this.disposed) return
    this.onStatus?.('connecting')
    const wsUrl = this.url.replace(/^http/, 'ws') + '/api/websocket'
    try { this.ws = new WebSocket(wsUrl) } catch (e) {
      this.scheduleReconnect(); return
    }
    this.ws.on('open',    ()    => { /* wait for auth_required */ })
    this.ws.on('message', raw  => this.handleMessage(JSON.parse(String(raw)) as WSMessage))
    this.ws.on('close',   ()   => { if (!this.disposed) this.scheduleReconnect() })
    this.ws.on('error',   ()   => { this.ws?.close() })
  }

  private handleMessage(msg: WSMessage): void {
    switch (msg.type) {
      case 'auth_required':
        this.ws?.send(JSON.stringify({ type: 'auth', access_token: this.token }))
        break
      case 'auth_ok':
        this.reconnectAttempt = 0
        this.onStatus?.('connected')
        this.subscribeToStateChanges()
        this.startPingHeartbeat()
        break
      case 'auth_invalid':
        this.disposed = true
        this.onStatus?.('error')
        break
      case 'result': {
        const p = this.pending.get(msg.id)
        if (p) {
          this.pending.delete(msg.id)
          if (msg.success) p.resolve(msg.result)
          else p.reject(new Error(msg.error?.message ?? 'HA error'))
        }
        break
      }
      case 'pong':
        if (this.pingTimeout) { clearTimeout(this.pingTimeout); this.pingTimeout = null }
        break
      case 'event': {
        const event = msg.event as { event_type: string; data: { entity_id: string; new_state: HAState } }
        if (event.event_type === 'state_changed') {
          this.onStateChanged?.(event.data.entity_id, event.data.new_state)
        }
        break
      }
    }
  }

  private subscribeToStateChanges(): void {
    const id = this.msgId++
    this.subscriptionId = id
    this.ws?.send(JSON.stringify({ type: 'subscribe_events', event_type: 'state_changed', id }))
  }

  private wsCommand(type: string, extra?: Record<string, unknown>): Promise<unknown> {
    return new Promise((resolve, reject) => {
      if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
        return reject(new Error('WebSocket not connected'))
      }
      const id = this.msgId++
      this.pending.set(id, { resolve, reject })
      this.ws.send(JSON.stringify({ type, id, ...extra }))
    })
  }

  private startPingHeartbeat(): void {
    this.stopPingHeartbeat()
    this.pingInterval = setInterval(() => {
      if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return
      const id = this.msgId++
      this.ws.send(JSON.stringify({ type: 'ping', id }))
      // Force-close if no pong within 10 s — triggers scheduleReconnect
      this.pingTimeout = setTimeout(() => { this.ws?.close() }, 10_000)
    }, 30_000)
  }

  private stopPingHeartbeat(): void {
    if (this.pingInterval) { clearInterval(this.pingInterval); this.pingInterval = null }
    if (this.pingTimeout)  { clearTimeout(this.pingTimeout);   this.pingTimeout  = null }
  }

  private scheduleReconnect(): void {
    if (this.disposed) return
    this.stopPingHeartbeat()
    this.onStatus?.('disconnected')
    const delay = RECONNECT_DELAYS[Math.min(this.reconnectAttempt, RECONNECT_DELAYS.length - 1)]
    this.reconnectAttempt++
    this.reconnectTimer = setTimeout(() => this.openSocket(), delay)
  }

  private clearReconnect(): void {
    if (this.reconnectTimer) { clearTimeout(this.reconnectTimer); this.reconnectTimer = null }
  }
}
