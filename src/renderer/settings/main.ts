import type { AppConfig, AppEntity, CoverEntity, HAEnvironment, SensorEntity } from '@shared/types'
import { AREA_ICONS, roomIcon } from '@shared/roomIcons'
import { api } from '@lib/api'

let config: AppConfig = { haUrl: '', haToken: '', camerasEnabled: true, launchAtLogin: false, activeEnvId: null }
let allEntities: AppEntity[] = []
let hiddenSet   = new Set<string>()
let rooms: Array<{ areaId: string; name: string }> = []
let areaIcons:  Record<string, string> = {}
let favorites:  string[] = []

// ─── Pointer-based drag (replaces HTML5 DnD — unreliable in WebView2) ────────
function setupPointerDrag<T extends HTMLElement>(opts: {
  handle:     HTMLElement
  dragEl:     HTMLElement
  getTargets: () => T[]
  onMove:     (target: T | null) => void
  onDrop:     (target: T) => Promise<void> | void
}): void {
  const { handle, dragEl, getTargets, onMove, onDrop } = opts
  handle.style.touchAction = 'none'
  handle.addEventListener('pointerdown', (e: PointerEvent) => {
    if (e.button !== 0) return
    e.preventDefault()
    handle.setPointerCapture(e.pointerId)
    let active = false
    let lastTarget: T | null = null
    const startX = e.clientX, startY = e.clientY

    const findTarget = (x: number, y: number): T | null => {
      dragEl.style.pointerEvents = 'none'
      const under = document.elementFromPoint(x, y)
      dragEl.style.pointerEvents = ''
      if (!under) return null
      for (const t of getTargets()) {
        if (t === dragEl) continue
        if (t === under || t.contains(under)) return t
      }
      return null
    }

    const onMoveHandler = (ev: PointerEvent) => {
      if (!active) {
        if (Math.hypot(ev.clientX - startX, ev.clientY - startY) < 5) return
        active = true
        dragEl.classList.add('dragging')
      }
      const target = findTarget(ev.clientX, ev.clientY)
      if (target !== lastTarget) {
        onMove(null)
        lastTarget = target
        if (target) onMove(target)
      }
    }

    const onUpHandler = (ev: PointerEvent) => {
      handle.removeEventListener('pointermove', onMoveHandler)
      handle.removeEventListener('pointerup', onUpHandler)
      handle.removeEventListener('pointercancel', onUpHandler)
      dragEl.classList.remove('dragging')
      const t = active ? findTarget(ev.clientX, ev.clientY) : null
      onMove(null)
      if (t) onDrop(t)
    }

    handle.addEventListener('pointermove', onMoveHandler)
    handle.addEventListener('pointerup', onUpHandler)
    handle.addEventListener('pointercancel', onUpHandler)
  })
}

// UI state for accessories tab
const areaCollapsed         = new Map<string, boolean>()
let   iconPickerOpenAreaId: string | null = null

// Multi-environment state
let environments: HAEnvironment[] = []
let activeEnvId:  string | null   = null
let editingEnvId: string | null   = null  // null = adding new

const FAVORITES_AREA = '__favorites__'

window.addEventListener('DOMContentLoaded', () => {
  setupTabs()
  load()
  api.ha.onStatusChange(() => renderEnvList())
  renderNotificationsTab()
})

async function load() {
  const [cfg, entities, envs, envId, roomList, icons, hidden, favIds] = await Promise.all([
    api.config.get(),
    api.accessories.getAll(),
    api.environments.getAll(),
    api.environments.getActiveId(),
    api.accessories.getRooms(),
    api.accessories.getAreaIcons(),
    api.accessories.getHidden(),
    api.accessories.getFavorites(),
  ])
  config       = cfg
  allEntities  = entities
  environments = envs
  activeEnvId  = envId
  rooms        = roomList
  areaIcons    = icons
  hiddenSet    = new Set(hidden)
  favorites    = favIds

  renderHATab()
  renderAccessoriesTab()
  renderCamerasTab()
  renderGeneralTab()
}

// ─── Tab navigation ───────────────────────────────────────────────────────────
function setupTabs() {
  document.querySelectorAll<HTMLButtonElement>('.nav-item').forEach(btn => {
    btn.addEventListener('click', () => {
      document.querySelectorAll('.nav-item').forEach(b => b.classList.remove('active'))
      document.querySelectorAll('.tab-pane').forEach(p => p.classList.remove('active'))
      btn.classList.add('active')
      document.getElementById(`tab-${btn.dataset.tab}`)?.classList.add('active')
    })
  })
}

// ─── Home Assistant tab ───────────────────────────────────────────────────────

function renderHATab() {
  renderEnvList()
  wireEnvFormButtons()

  // "Add Server" button
  const addBtn = document.getElementById('btn-add-env')!
  addBtn.onclick = () => openEnvForm(null)
}

function renderEnvList() {
  const list = document.getElementById('env-list')!
  list.innerHTML = ''

  if (!environments.length) {
    list.innerHTML = '<p class="hint" style="margin-bottom:8px">No servers configured. Click "+ Add Server" to get started.</p>'
    return
  }

  for (const env of environments) {
    const isActive = env.id === activeEnvId
    const row = document.createElement('div')
    row.className = `env-row${isActive ? ' active-env' : ''}`

    row.innerHTML = `
      <span class="env-dot${isActive ? ' active' : ''}"></span>
      <div class="env-info">
        <div class="env-name-label">${esc(env.name)}</div>
        <div class="env-url-label"  title="${esc(env.haUrl)}">${esc(env.haUrl)}</div>
      </div>
      <div class="env-actions">
        ${isActive
          ? `<button class="env-btn danger" data-action="disconnect">Disconnect</button>`
          : `<button class="env-btn primary" data-action="connect">Connect</button>`}
        <button class="env-btn" data-action="edit">Edit</button>
        <button class="env-btn danger" data-action="delete">✕</button>
      </div>
    `

    row.querySelector<HTMLButtonElement>('[data-action="connect"]')?.addEventListener('click', async () => {
      await api.environments.connect(env.id)
      activeEnvId = env.id
      renderEnvList()
    })

    row.querySelector<HTMLButtonElement>('[data-action="disconnect"]')?.addEventListener('click', async () => {
      await api.config.disconnect()
      activeEnvId = null
      renderEnvList()
    })

    row.querySelector<HTMLButtonElement>('[data-action="edit"]')?.addEventListener('click', () => {
      openEnvForm(env)
    })

    row.querySelector<HTMLButtonElement>('[data-action="delete"]')?.addEventListener('click', async () => {
      // eslint-disable-next-line no-alert
      if (!confirm(`Remove "${env.name}"?`)) return
      await api.environments.remove(env.id)
      if (env.id === activeEnvId) {
        await api.config.disconnect()
        activeEnvId = null
      }
      environments = environments.filter(e => e.id !== env.id)
      renderEnvList()
    })

    list.appendChild(row)
  }
}

function openEnvForm(env: HAEnvironment | null) {
  editingEnvId = env?.id ?? null
  const form       = document.getElementById('env-form')!
  const title      = document.getElementById('env-form-title')!
  const nameInput  = document.getElementById('env-name')  as HTMLInputElement
  const urlInput   = document.getElementById('env-url')   as HTMLInputElement
  const tokenInput = document.getElementById('env-token') as HTMLInputElement
  const testResult = document.getElementById('env-test-result')!

  title.textContent = env ? 'Edit Server' : 'New Server'
  nameInput.value   = env?.name    ?? ''
  urlInput.value    = env?.haUrl   ?? ''
  tokenInput.value  = env?.haToken ?? ''
  testResult.className   = 'test-result'
  testResult.style.display = 'none'
  form.style.display = ''
  nameInput.focus()
}

function closeEnvForm() {
  document.getElementById('env-form')!.style.display = 'none'
  editingEnvId = null
  const r = document.getElementById('env-test-result')!
  r.className = 'test-result'
  r.style.display = 'none'
}

function wireEnvFormButtons() {
  document.getElementById('btn-env-cancel')!.addEventListener('click', closeEnvForm)

  document.getElementById('btn-env-test')!.addEventListener('click', async () => {
    const urlInput   = document.getElementById('env-url')   as HTMLInputElement
    const tokenInput = document.getElementById('env-token') as HTMLInputElement
    const result     = document.getElementById('env-test-result')!
    result.className = 'test-result'
    result.textContent = 'Testing…'
    result.style.display = 'block'

    const { ok, entityCount, error } = await api.environments.test(
      urlInput.value.trim(), tokenInput.value.trim()
    )
    result.className  = ok ? 'test-result ok' : 'test-result err'
    result.textContent = ok
      ? `✓ Connected – ${entityCount} entities found`
      : `✗ ${error ?? 'Connection failed'}`
  })

  document.getElementById('btn-env-save')!.addEventListener('click', async () => {
    const nameInput  = document.getElementById('env-name')  as HTMLInputElement
    const urlInput   = document.getElementById('env-url')   as HTMLInputElement
    const tokenInput = document.getElementById('env-token') as HTMLInputElement

    const name  = nameInput.value.trim()  || 'Home'
    const url   = urlInput.value.trim()
    const token = tokenInput.value.trim()

    if (!url || !token) {
      // eslint-disable-next-line no-alert
      alert('Please enter a Server URL and an Access Token.')
      return
    }

    if (editingEnvId) {
      const updated: HAEnvironment = { id: editingEnvId, name, haUrl: url, haToken: token }
      await api.environments.update(updated)
      environments = environments.map(e => e.id === editingEnvId ? updated : e)
      if (editingEnvId === activeEnvId) {
        await api.environments.connect(editingEnvId)
      }
    } else {
      const newEnv = await api.environments.add(name, url, token)
      environments.push(newEnv)
    }

    closeEnvForm()
    renderEnvList()
  })
}

// ─── Accessories tab ──────────────────────────────────────────────────────────
function renderAccessoriesTab() {
  const list = document.getElementById('accessories-list')!
  list.innerHTML = ''

  if (!allEntities.length) {
    list.innerHTML = '<p class="hint" style="padding:16px 0">No entities loaded yet. Connect to Home Assistant first.</p>'
    return
  }

  const entityMap = new Map<string, AppEntity>(allEntities.map(e => [e.entityId, e]))
  const favSet    = new Set(favorites)

  // Favorites group always at top
  if (favorites.length > 0) {
    list.appendChild(buildFavoritesGroup(favorites, entityMap))
  }

  // Group remaining entities by areaId (exclude favorites)
  const byArea = new Map<string, AppEntity[]>()
  const noArea: AppEntity[] = []
  for (const entity of allEntities) {
    if (favSet.has(entity.entityId)) continue
    if (entity.areaId) {
      const arr = byArea.get(entity.areaId) ?? []
      arr.push(entity)
      byArea.set(entity.areaId, arr)
    } else {
      noArea.push(entity)
    }
  }

  const seenIds    = new Set(rooms.map(r => r.areaId))
  const extraAreas = [...byArea.keys()]
    .filter(id => !seenIds.has(id))
    .map(id => ({ areaId: id, name: id }))
  const orderedAreas = [...rooms.filter(r => byArea.has(r.areaId)), ...extraAreas]

  for (const area of orderedAreas) {
    list.appendChild(buildAreaGroup(area.areaId, area.name, byArea.get(area.areaId)!))
  }
  if (noArea.length > 0) {
    list.appendChild(buildAreaGroup('', 'Other', noArea))
  }
}

function buildFavoritesGroup(favIds: string[], entityMap: Map<string, AppEntity>): HTMLElement {
  const group = document.createElement('div')
  group.className = 'area-group fav-group'
  group.dataset.areaId = FAVORITES_AREA

  const header = document.createElement('div')
  header.className = 'area-header'

  const starEl = document.createElement('span')
  starEl.className = 'area-icon-btn'
  starEl.style.cursor = 'default'
  starEl.textContent = '⭐'
  header.appendChild(starEl)

  const nameEl = document.createElement('span')
  nameEl.className = 'area-name'
  nameEl.textContent = 'Preferiti'
  header.appendChild(nameEl)

  const countEl = document.createElement('span')
  countEl.className = 'area-count'
  countEl.textContent = String(favIds.length)
  header.appendChild(countEl)

  let isCollapsed = areaCollapsed.get(FAVORITES_AREA) ?? false
  const collapseBtn = document.createElement('button')
  collapseBtn.className = 'area-collapse-btn'
  collapseBtn.title = isCollapsed ? 'Expand' : 'Collapse'
  collapseBtn.textContent = '▾'
  if (isCollapsed) group.classList.add('collapsed')

  const entityList = document.createElement('div')
  entityList.className = 'entity-group-list'
  if (isCollapsed) entityList.style.display = 'none'

  collapseBtn.addEventListener('click', e => {
    e.stopPropagation()
    isCollapsed = !isCollapsed
    areaCollapsed.set(FAVORITES_AREA, isCollapsed)
    entityList.style.display = isCollapsed ? 'none' : ''
    group.classList.toggle('collapsed', isCollapsed)
    collapseBtn.title = isCollapsed ? 'Expand' : 'Collapse'
  })
  header.appendChild(collapseBtn)
  group.appendChild(header)

  for (const entityId of favIds) {
    const entity = entityMap.get(entityId)
    if (!entity) continue

    const row = document.createElement('div')
    row.className = 'acc-row'
    row.dataset.entityId = entityId

    const isHidden = hiddenSet.has(entityId)
    row.innerHTML = `
      <span class="acc-drag" title="Drag to reorder">⠿</span>
      <span class="acc-icon">${entityIcon(entity)}</span>
      <span class="${isHidden ? 'acc-name name-hidden' : 'acc-name'}" title="${esc(entityId)}">${esc(entity.name)}</span>
      <span class="acc-domain">${entity.type}</span>
    `
    row.appendChild(buildEyeBtn(entity, row))

    setupPointerDrag({
      handle:     row.querySelector<HTMLElement>('.acc-drag')!,
      dragEl:     row,
      getTargets: () => [...entityList.querySelectorAll<HTMLElement>('.acc-row')],
      onMove:     (t) => {
        entityList.querySelectorAll('.acc-row').forEach(r => r.classList.remove('drag-over'))
        t?.classList.add('drag-over')
      },
      onDrop: async (t) => {
        const targetId = t.dataset.entityId
        if (!targetId || targetId === entityId) return
        const srcIdx = favorites.indexOf(entityId)
        const dstIdx = favorites.indexOf(targetId)
        if (srcIdx < 0 || dstIdx < 0) return
        const [moved] = favorites.splice(srcIdx, 1)
        favorites.splice(dstIdx, 0, moved)
        await api.accessories.setFavoritesOrder(favorites)
        renderAccessoriesTab()
      },
    })

    entityList.appendChild(row)
  }

  group.appendChild(entityList)
  return group
}

function buildEyeBtn(entity: AppEntity, row: HTMLElement): HTMLButtonElement {
  const isHidden = hiddenSet.has(entity.entityId)
  const btn = document.createElement('button')
  btn.className = isHidden ? 'acc-eye-btn' : 'acc-eye-btn active'
  btn.title = isHidden ? 'Hidden – click to show' : 'Visible – click to hide'
  btn.textContent = '👁'

  btn.addEventListener('click', async () => {
    const wasHidden = hiddenSet.has(entity.entityId)
    if (wasHidden) hiddenSet.delete(entity.entityId)
    else           hiddenSet.add(entity.entityId)
    const nowHidden = !wasHidden
    btn.classList.toggle('active', !nowHidden)
    btn.title = nowHidden ? 'Hidden – click to show' : 'Visible – click to hide'
    row.querySelector<HTMLElement>('.acc-name')?.classList.toggle('name-hidden', nowHidden)
    await api.accessories.setHidden([...hiddenSet])
  })

  return btn
}

function buildAreaGroup(areaId: string, areaName: string, entities: AppEntity[]): HTMLElement {
  const group = document.createElement('div')
  group.className = 'area-group'
  group.dataset.areaId = areaId

  const isDraggable  = areaId !== ''
  const canSetIcon   = areaId !== ''

  let isCollapsed = areaCollapsed.get(areaId) ?? false

  const header = document.createElement('div')
  header.className = 'area-header'
  header.draggable = isDraggable

  const dragHandle = document.createElement('span')
  dragHandle.className = 'acc-drag'
  dragHandle.title = 'Drag to reorder area'
  dragHandle.textContent = '⠿'
  if (!isDraggable) dragHandle.style.visibility = 'hidden'
  header.appendChild(dragHandle)

  const currentIcon = canSetIcon ? (areaIcons[areaId] ?? roomIcon(areaName)) : ''
  if (canSetIcon) {
    const iconBtn = document.createElement('button')
    iconBtn.className = 'area-icon-btn'
    iconBtn.textContent = currentIcon
    iconBtn.title = 'Change icon'
    iconBtn.draggable = false
    iconBtn.addEventListener('click', e => {
      e.stopPropagation()
      const isOpen = iconPickerOpenAreaId === areaId
      document.querySelectorAll<HTMLElement>('.icon-picker').forEach(p => { p.style.display = 'none' })
      iconPickerOpenAreaId = null
      if (!isOpen) {
        iconPickerOpenAreaId = areaId
        picker.style.display = ''
      }
    })
    header.appendChild(iconBtn)
  }

  const nameEl = document.createElement('span')
  nameEl.className = 'area-name'
  nameEl.textContent = areaName
  header.appendChild(nameEl)

  const countEl = document.createElement('span')
  countEl.className = 'area-count'
  countEl.textContent = String(entities.length)
  header.appendChild(countEl)

  // ── Area hide/show button ─────────────────────────────────────────────
  const areaEyeBtn = document.createElement('button')
  areaEyeBtn.className = 'acc-eye-btn area-eye-btn'
  areaEyeBtn.draggable = false
  const allHidden = () => entities.every(en => hiddenSet.has(en.entityId))
  const syncAreaEye = () => {
    const hidden = allHidden()
    areaEyeBtn.classList.toggle('active', !hidden)
    areaEyeBtn.title = hidden ? 'Area nascosta – click per mostrare' : 'Area visibile – click per nascondere'
    areaEyeBtn.textContent = '👁'
  }
  syncAreaEye()
  areaEyeBtn.addEventListener('click', async e => {
    e.stopPropagation()
    const hide = !allHidden()
    entities.forEach(en => {
      if (hide) hiddenSet.add(en.entityId)
      else      hiddenSet.delete(en.entityId)
    })
    await api.accessories.setHidden([...hiddenSet])
    syncAreaEye()
    renderAccessoriesTab()
  })
  header.appendChild(areaEyeBtn)

  const collapseBtn = document.createElement('button')
  collapseBtn.className = 'area-collapse-btn'
  collapseBtn.title = isCollapsed ? 'Expand' : 'Collapse'
  collapseBtn.textContent = '▾'
  collapseBtn.draggable = false
  if (isCollapsed) group.classList.add('collapsed')
  collapseBtn.addEventListener('click', e => {
    e.stopPropagation()
    isCollapsed = !isCollapsed
    areaCollapsed.set(areaId, isCollapsed)
    entityList.style.display = isCollapsed ? 'none' : ''
    group.classList.toggle('collapsed', isCollapsed)
    collapseBtn.title = isCollapsed ? 'Expand' : 'Collapse'
    if (isCollapsed && iconPickerOpenAreaId === areaId) {
      picker.style.display = 'none'
      iconPickerOpenAreaId = null
    }
  })
  header.appendChild(collapseBtn)

  if (isDraggable) {
    setupPointerDrag({
      handle:     dragHandle,
      dragEl:     header,
      getTargets: () => [...document.querySelectorAll<HTMLElement>('.area-group[data-area-id]')],
      onMove:     (t) => {
        document.querySelectorAll('.area-group').forEach(g => g.classList.remove('drag-over-area'))
        t?.classList.add('drag-over-area')
      },
      onDrop: async (t) => {
        const targetAreaId = t.dataset.areaId
        if (!targetAreaId || targetAreaId === areaId || targetAreaId === '') return
        const srcIdx = rooms.findIndex(r => r.areaId === areaId)
        const dstIdx = rooms.findIndex(r => r.areaId === targetAreaId)
        if (srcIdx < 0 || dstIdx < 0) return
        const [moved] = rooms.splice(srcIdx, 1)
        rooms.splice(dstIdx, 0, moved)
        await api.accessories.setRoomOrder(rooms.map(r => r.areaId))
        renderAccessoriesTab()
      },
    })
  }

  group.appendChild(header)

  // ── Icon picker ───────────────────────────────────────────────────────
  const picker = document.createElement('div')
  picker.className = 'icon-picker'
  picker.style.display = 'none'

  AREA_ICONS.forEach(icon => {
    const btn = document.createElement('button')
    btn.className = 'icon-picker-btn' + (icon === currentIcon ? ' active' : '')
    btn.textContent = icon
    btn.title = icon
    btn.addEventListener('click', async () => {
      await api.accessories.setAreaIcon(areaId, icon)
      areaIcons[areaId] = icon
      const iconBtn = header.querySelector<HTMLButtonElement>('.area-icon-btn')
      if (iconBtn) iconBtn.textContent = icon
      picker.querySelectorAll('.icon-picker-btn').forEach(b => {
        b.classList.toggle('active', (b as HTMLButtonElement).textContent === icon)
      })
      picker.style.display = 'none'
      iconPickerOpenAreaId = null
    })
    picker.appendChild(btn)
  })

  group.appendChild(picker)

  // ── Entity rows ───────────────────────────────────────────────────────
  const entityList = document.createElement('div')
  entityList.className = 'entity-group-list'
  if (isCollapsed) entityList.style.display = 'none'

  for (const entity of entities) {
    const row = document.createElement('div')
    row.className = 'acc-row'
    row.dataset.entityId = entity.entityId

    const isHidden = hiddenSet.has(entity.entityId)
    row.innerHTML = `
      <span class="acc-drag" title="Drag to reorder">⠿</span>
      <span class="acc-icon">${entityIcon(entity)}</span>
      <span class="${isHidden ? 'acc-name name-hidden' : 'acc-name'}" title="${esc(entity.entityId)}">${esc(entity.name)}</span>
      <span class="acc-domain">${entity.type}</span>
    `
    row.appendChild(buildEyeBtn(entity, row))

    setupPointerDrag({
      handle:     row.querySelector<HTMLElement>('.acc-drag')!,
      dragEl:     row,
      getTargets: () => [...entityList.querySelectorAll<HTMLElement>('.acc-row')],
      onMove:     (t) => {
        entityList.querySelectorAll('.acc-row').forEach(r => r.classList.remove('drag-over'))
        t?.classList.add('drag-over')
      },
      onDrop: async (t) => {
        const targetId = t.dataset.entityId
        if (!targetId || targetId === entity.entityId) return
        const srcIdx = allEntities.findIndex(en => en.entityId === entity.entityId)
        const dstIdx = allEntities.findIndex(en => en.entityId === targetId)
        if (srcIdx < 0 || dstIdx < 0) return
        const [moved] = allEntities.splice(srcIdx, 1)
        allEntities.splice(dstIdx, 0, moved)
        const newOrder = allEntities
          .filter(en => (en.areaId ?? '') === areaId)
          .map(en => en.entityId)
        await api.accessories.setDeviceOrder(areaId, newOrder)
        renderAccessoriesTab()
      },
    })

    entityList.appendChild(row)
  }

  group.appendChild(entityList)
  return group
}

// ─── Cameras tab ──────────────────────────────────────────────────────────────
function renderCamerasTab() {
  const btn = document.getElementById('cameras-toggle')!
  updateToggle(btn, config.camerasEnabled)
  btn.addEventListener('click', async () => {
    config.camerasEnabled = !config.camerasEnabled
    updateToggle(btn, config.camerasEnabled)
    await api.config.setCamerasEnabled(config.camerasEnabled)
  })
}

// ─── General tab ─────────────────────────────────────────────────────────────
function renderGeneralTab() {
  const btn = document.getElementById('autostart-toggle')!
  updateToggle(btn, config.launchAtLogin)
  btn.addEventListener('click', async () => {
    config.launchAtLogin = !config.launchAtLogin
    updateToggle(btn, config.launchAtLogin)
    await api.config.setLaunchAtLogin(config.launchAtLogin)
  })
}

// ─── Notifications tab ────────────────────────────────────────────────────────

async function renderNotificationsTab() {
  const portInput      = document.getElementById('notif-port')       as HTMLInputElement
  const statusBadge    = document.getElementById('notif-status-badge')!
  const deviceRow      = document.getElementById('notif-device-row')!
  const deviceName     = document.getElementById('notif-device-name')!
  const serviceRow     = document.getElementById('notif-service-row')!
  const serviceName    = document.getElementById('notif-service-name')!
  const urlRow         = document.getElementById('notif-url-row')!
  const pushUrl        = document.getElementById('notif-push-url')!
  const btnRegister    = document.getElementById('btn-notif-register')   as HTMLButtonElement
  const btnUnregister  = document.getElementById('btn-notif-unregister') as HTMLButtonElement
  const btnTest        = document.getElementById('btn-notif-test')       as HTMLButtonElement
  const resultEl       = document.getElementById('notif-result')!

  function applyStatus(s: {
    registered: boolean
    deviceName?: string
    port: number
    pushUrl?: string
    serviceName?: string
  }) {
    portInput.value = String(s.port)

    if (s.registered) {
      statusBadge.className = 'status-badge connected'
      statusBadge.textContent = 'Registrato'
      deviceRow.style.display = ''
      deviceName.textContent = s.deviceName ?? ''
      serviceRow.style.display = ''
      serviceName.textContent = s.serviceName ?? ''
      urlRow.style.display = ''
      pushUrl.textContent  = s.pushUrl ?? ''
      pushUrl.title        = s.pushUrl ?? ''
      btnRegister.style.display   = 'none'
      btnUnregister.style.display = ''
    } else {
      statusBadge.className = 'status-badge disconnected'
      statusBadge.textContent = 'Non registrato'
      deviceRow.style.display  = 'none'
      serviceRow.style.display = 'none'
      urlRow.style.display     = 'none'
      btnRegister.style.display   = ''
      btnUnregister.style.display = 'none'
    }
  }

  function showResult(msg: string, ok: boolean) {
    resultEl.className   = ok ? 'test-result ok' : 'test-result err'
    resultEl.textContent = msg
  }

  // Load initial status
  try {
    const status = await api.notifications.getStatus()
    applyStatus(status)
  } catch {
    // not critical
  }

  btnRegister.addEventListener('click', async () => {
    const port = parseInt(portInput.value.trim(), 10) || 7421
    btnRegister.disabled = true
    resultEl.className = 'test-result'
    resultEl.textContent = ''
    try {
      const status = await api.notifications.register(port)
      applyStatus(status)
      showResult('Registrazione completata. Usa il servizio indicato nelle automazioni HA.', true)
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e)
      showResult(`Errore: ${msg}`, false)
    } finally {
      btnRegister.disabled = false
    }
  })

  btnUnregister.addEventListener('click', async () => {
    // eslint-disable-next-line no-alert
    if (!confirm('Annullare la registrazione notifiche?')) return
    try {
      await api.notifications.unregister()
      applyStatus({ registered: false, port: parseInt(portInput.value, 10) || 7421 })
      showResult('Registrazione rimossa.', true)
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e)
      showResult(`Errore: ${msg}`, false)
    }
  })

  btnTest.addEventListener('click', async () => {
    try {
      await api.notifications.test()
      showResult('Notifica di test inviata.', true)
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e)
      showResult(`Errore: ${msg}`, false)
    }
  })
}

// ─── Helpers ─────────────────────────────────────────────────────────────────
function updateToggle(btn: HTMLElement, on: boolean) {
  btn.classList.toggle('on', on)
}

function entityIcon(e: AppEntity): string {
  switch (e.type) {
    case 'light':      return '💡'
    case 'switch':     return '🔌'
    case 'climate':    return '🌡️'
    case 'cover': {
      const dc = (e as CoverEntity).deviceClass
      return dc === 'garage_door' ? '🚗' : dc === 'door' ? '🚪' : '🪟'
    }
    case 'lock':       return '🔒'
    case 'fan':        return '💨'
    case 'humidifier': return '💧'
    case 'valve':      return '🚰'
    case 'sensor': {
      const dc = (e as SensorEntity).deviceClass
      return dc === 'humidity' ? '💧' : '🌡️'
    }
    case 'alarm':      return '🔐'
    case 'camera':     return '📷'
    case 'scene':      return '✨'
    default:           return '⚙'
  }
}

function esc(s: string): string {
  return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;')
}
