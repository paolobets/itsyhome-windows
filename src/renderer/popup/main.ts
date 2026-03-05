/**
 * Popup renderer – mirrors the original macOS menu bar popup.
 * Sections: Favorites → Scenes → Rooms → Cameras.
 * Entity cards match the original per-domain layouts.
 */

import type {
  MenuData, AppEntity, LightEntity, SwitchEntity, ClimateEntity, CoverEntity,
  LockEntity, FanEntity, HumidifierEntity, ValveEntity, SensorEntity,
  AlarmEntity, CameraEntity, SceneEntity, ConnectionStatus, Room, HAEnvironment
} from '@shared/types'
import { roomIcon } from '@shared/roomIcons'

declare const window: Window & { api: import('@shared/types').PopupAPI }

// ─── State ────────────────────────────────────────────────────────────────────
let menuData:   MenuData | null = null
let favorites:  Set<string>     = new Set()
let status:     ConnectionStatus = 'disconnected'
let envList:    HAEnvironment[]  = []
let activeEnvId: string | null   = null
let areaIcons:  Record<string, string> = {}

// Preserved UI state across full re-renders
const sectionCollapsed = new Map<string, boolean>()   // sectionKey → collapsed
const cardExpanded     = new Map<string, boolean>()   // entityId (+ suffix) → expanded

// Detail panel state
let detailOpen:         boolean            = false
let selectedRoomKey:    string | null      = null
let detailCloseHandler: (() => void) | null = null

// Resize dedupe – only one RAF in flight at a time
let resizeRafId = 0

// 2D hue picker draw fns
const huePickers = new WeakMap<HTMLElement, () => void>()

// ─── Startup ──────────────────────────────────────────────────────────────────
window.addEventListener('DOMContentLoaded', () => {
  setupTopBar()
  load()
  window.api.menu.onUpdate(data => { menuData = data; render() })
  window.api.ha.onStatusChange(async s => {
    status = s
    if (s === 'connected') {
      // Re-fetch env info when connection established (may have switched env)
      const [envs, envId] = await Promise.all([
        window.api.environments.getAll(),
        window.api.environments.getActiveId(),
      ])
      envList     = envs
      activeEnvId = envId
    }
    updateTopBar()
  })
})

async function load() {
  const [data, fav, s, envs, envId, icons] = await Promise.all([
    window.api.menu.getData(),
    window.api.favorites.get(),
    window.api.ha.getStatus(),
    window.api.environments.getAll(),
    window.api.environments.getActiveId(),
    window.api.accessories.getAreaIcons(),
  ])
  menuData    = data
  favorites   = new Set(fav)
  status      = s
  envList     = envs
  activeEnvId = envId
  areaIcons   = icons
  updateTopBar()
  render()
}

// ─── Render ───────────────────────────────────────────────────────────────────
function render() {
  const list = document.getElementById('menu-list')!
  list.innerHTML = ''

  updateTopBar()

  // Update camera button opacity based on available cameras
  const camBtn = document.getElementById('btn-cameras')
  if (camBtn) {
    const hasCams = (menuData?.cameras.length ?? 0) > 0
    camBtn.style.opacity = hasCams ? '1' : '0.35'
    camBtn.title = hasCams ? 'Telecamere' : 'Nessuna telecamera'
  }

  if (!menuData) {
    list.innerHTML = `<div class="empty-state"><div class="icon">🏠</div>Connecting…</div>`
    resize(); return
  }

  const frag = document.createDocumentFragment()

  // 1. Impianto – environment section (always shown)
  frag.appendChild(buildEnvSection())

  // 2. Preferiti – favorites (shown only when at least one is starred)
  if (menuData.favorites.length) {
    frag.appendChild(div('menu-separator'))
    const wrap = div('fav-section')
    menuData.favorites.forEach(e => wrap.appendChild(buildCard(e)))
    frag.appendChild(wrap)
  }

  // 3. Aree – each room opens detail panel
  if (menuData.rooms.length) {
    frag.appendChild(div('menu-separator'))
    for (const room of menuData.rooms) {
      if (!room.entities.length) continue
      frag.appendChild(buildRoomRow(room))
    }
  }

  // 4. Scene – collapsible row (default collapsed)
  if (menuData.scenes.length) {
    frag.appendChild(buildRowSection('scenes', '✨', 'Scene', () => {
      const w = div('section-body')
      menuData!.scenes.forEach(e => w.appendChild(buildCard(e)))
      return w
    }))
  }

  list.appendChild(frag)
  resize()
}

// ─── Environment section ──────────────────────────────────────────────────────
function buildEnvSection(): HTMLElement {
  const sec = div('section')
  const activeName = envList.find(e => e.id === activeEnvId)?.name ?? '–'
  const isExpandable = envList.length > 1

  const head = div('section-header row-header')
  if (isExpandable) head.classList.add('collapsed')
  head.innerHTML = `
    <span class="row-icon">🏠</span>
    <span class="row-name">${esc(activeName)}</span>
    ${isExpandable ? '<span class="row-chevron">›</span>' : ''}`

  if (!isExpandable) {
    sec.appendChild(head)
    return sec
  }

  let collapsed = sectionCollapsed.get('_env') ?? true
  if (!collapsed) head.classList.remove('collapsed')

  const body = div('section-content')
  body.style.display = collapsed ? 'none' : ''

  function renderEnvBody() {
    body.innerHTML = ''
    for (const env of envList) {
      const isActive = env.id === activeEnvId
      const card  = div('entity-card')
      const row   = div('entity-row')
      const iconEl = document.createElement('span')
      iconEl.className   = 'entity-icon'
      iconEl.textContent = isActive ? '🔗' : '○'
      const nameEl = document.createElement('span')
      nameEl.className   = 'entity-name'
      nameEl.textContent = env.name
      const right  = document.createElement('span')
      right.className = 'entity-right'
      if (isActive) {
        const s = document.createElement('span')
        s.className = 'entity-state'
        s.style.color = 'var(--success)'
        s.textContent = 'Connesso'
        right.appendChild(s)
      } else {
        const btn = document.createElement('button')
        btn.className   = 'env-connect-btn'
        btn.textContent = 'Connetti'
        btn.addEventListener('click', async ev => {
          ev.stopPropagation()
          await window.api.environments.connect(env.id)
          activeEnvId = env.id
          updateTopBar()
          renderEnvBody()
        })
        right.appendChild(btn)
      }
      row.appendChild(iconEl)
      row.appendChild(nameEl)
      row.appendChild(right)
      card.appendChild(row)
      body.appendChild(card)
    }
  }

  // Render immediately if expanded
  if (!collapsed) renderEnvBody()

  head.addEventListener('click', () => {
    collapsed = !collapsed
    sectionCollapsed.set('_env', collapsed)
    if (!collapsed && !body.children.length) renderEnvBody()
    body.style.display = collapsed ? 'none' : ''
    head.classList.toggle('collapsed', collapsed)
    resize()
  })

  sec.appendChild(head)
  sec.appendChild(body)
  return sec
}

function resize() {
  if (resizeRafId) cancelAnimationFrame(resizeRafId)
  resizeRafId = requestAnimationFrame(() => {
    resizeRafId = 0
    const topBar   = document.getElementById('top-bar')!
    const menuList = document.getElementById('menu-list')!
    const h = topBar.offsetHeight + menuList.offsetHeight
    const w = detailOpen ? 580 : 300
    window.api.window.resize(h, w)
  })
}

// ─── Sections ────────────────────────────────────────────────────────────────

/**
 * macOS-style collapsible row: icon + name + › chevron.
 * Default state: collapsed (matching the macOS original behaviour).
 * Content is lazy-rendered on first expand.
 */
function buildRowSection(
  key:          string,
  icon:         string,
  name:         string,
  buildContent: () => HTMLElement
): HTMLElement {
  const sec = div('section')

  // Default collapsed = true (rooms/scenes start closed, like macOS)
  let collapsed  = sectionCollapsed.get(key) ?? true
  let rendered   = false

  const head = div('section-header row-header')
  head.innerHTML = `
    <span class="row-icon">${icon}</span>
    <span class="row-name">${esc(name)}</span>
    <span class="row-chevron">›</span>`
  if (collapsed) head.classList.add('collapsed')

  const body = div('section-content')
  if (collapsed) body.style.display = 'none'

  head.addEventListener('click', () => {
    collapsed = !collapsed
    sectionCollapsed.set(key, collapsed)
    // Lazy-render on first expand
    if (!collapsed && !rendered) {
      body.appendChild(buildContent())
      rendered = true
    }
    body.style.display = collapsed ? 'none' : ''
    head.classList.toggle('collapsed', collapsed)
    resize()
  })

  // If restored as expanded, render immediately
  if (!collapsed) {
    body.appendChild(buildContent())
    rendered = true
  }

  sec.appendChild(head)
  sec.appendChild(body)
  return sec
}

// ─── Rows that open the detail panel ─────────────────────────────────────────

/** Shared builder: a single row that opens the detail panel on click. */
function buildDetailRow(key: string, icon: string, name: string, entities: AppEntity[]): HTMLElement {
  const sec  = div('section')
  const head = div('section-header row-header')
  head.dataset.roomKey = key
  head.innerHTML = `
    <span class="row-icon">${icon}</span>
    <span class="row-name">${esc(name)}</span>
    <span class="row-chevron">›</span>`

  if (selectedRoomKey === key) head.classList.add('row-selected')

  head.addEventListener('click', () => {
    if (selectedRoomKey === key) closeDetailPanel()
    else openDetailPanel(key, icon, name, entities)
  })

  sec.appendChild(head)
  return sec
}

function buildRoomRow(room: Room): HTMLElement {
  const icon = areaIcons[room.areaId] ?? roomIcon(room.name)
  return buildDetailRow(`room:${room.areaId}`, icon, room.name, room.entities)
}

function openDetailPanel(key: string, icon: string, name: string, entities: AppEntity[]): void {
  selectedRoomKey = key
  detailOpen = true

  const detailIcon  = document.getElementById('detail-icon')!
  const detailTitle = document.getElementById('detail-title')!
  const detailList  = document.getElementById('detail-list')!

  detailIcon.textContent  = icon
  detailTitle.textContent = name
  detailList.innerHTML    = ''
  entities.forEach(e => detailList.appendChild(buildCard(e)))

  const panel = document.getElementById('detail-panel')!
  // Cancel any in-progress close animation to prevent its animationend from firing
  if (detailCloseHandler) {
    panel.removeEventListener('animationend', detailCloseHandler)
    detailCloseHandler = null
  }
  panel.classList.remove('closing')
  panel.classList.add('open')

  // Highlight the selected row
  document.querySelectorAll<HTMLElement>('[data-room-key]').forEach(el => {
    el.classList.toggle('row-selected', el.dataset.roomKey === key)
  })

  updateTopBar()   // sync camera button highlight
  resize()
}

function closeDetailPanel(): void {
  selectedRoomKey = null
  document.querySelectorAll('.row-selected').forEach(el => el.classList.remove('row-selected'))
  updateTopBar()   // clear camera button highlight

  const panel = document.getElementById('detail-panel')!
  panel.classList.add('closing')
  detailCloseHandler = () => {
    panel.classList.remove('open', 'closing')
    detailCloseHandler = null
    detailOpen = false
    resize()
  }
  panel.addEventListener('animationend', detailCloseHandler, { once: true })
}

// ─── Entity card dispatch ─────────────────────────────────────────────────────
function buildCard(e: AppEntity): HTMLElement {
  const card = buildCardInner(e)
  // Add favorite star for actionable entities (not scenes or cameras)
  if (e.type !== 'scene' && e.type !== 'camera') {
    const right = card.querySelector('.entity-right')
    if (right) right.insertBefore(makeFavoriteStar(e), right.firstChild)
  }
  return card
}

function buildCardInner(e: AppEntity): HTMLElement {
  switch (e.type) {
    case 'light':      return buildLightCard(e)
    case 'switch':     return buildSwitchCard(e)
    case 'climate':    return buildClimateCard(e)
    case 'cover':      return buildCoverCard(e)
    case 'lock':       return buildLockCard(e)
    case 'fan':        return buildFanCard(e)
    case 'humidifier': return buildHumidifierCard(e)
    case 'valve':      return buildValveCard(e)
    case 'sensor':     return buildSensorCard(e)
    case 'alarm':      return buildAlarmCard(e)
    case 'camera':     return buildCameraCard(e)
    case 'scene':      return buildSceneCard(e)
    default:           return div('entity-card')
  }
}

// ─── Favorite star ────────────────────────────────────────────────────────────
function makeFavoriteStar(e: AppEntity): HTMLElement {
  const isFav = favorites.has(e.entityId)
  const btn = document.createElement('button')
  btn.className = 'fav-btn' + (isFav ? ' active' : '')
  btn.textContent = isFav ? '★' : '☆'
  btn.title = isFav ? 'Remove from Favorites' : 'Add to Favorites'
  btn.addEventListener('click', async ev => {
    ev.stopPropagation()
    await toggleFavorite(e)
    // Keep the in-panel star in sync without a full re-render
    const nowFav = favorites.has(e.entityId)
    btn.textContent = nowFav ? '★' : '☆'
    btn.classList.toggle('active', nowFav)
    btn.title = nowFav ? 'Remove from Favorites' : 'Add to Favorites'
  })
  return btn
}

async function toggleFavorite(entity: AppEntity): Promise<void> {
  if (favorites.has(entity.entityId)) {
    favorites.delete(entity.entityId)
    await window.api.favorites.remove(entity.entityId)
    if (menuData) menuData.favorites = menuData.favorites.filter(e => e.entityId !== entity.entityId)
  } else {
    favorites.add(entity.entityId)
    await window.api.favorites.add(entity.entityId)
    if (menuData) menuData.favorites = [...menuData.favorites, entity]
  }
  render()
}

// ─── LIGHT ───────────────────────────────────────────────────────────────────
function buildLightCard(e: LightEntity): HTMLElement {
  const card  = div('entity-card')
  if (!e.isAvailable) card.classList.add('unavailable')

  const hasColor = e.supportsRgb
  const hasTemp  = e.supportsColorTemp && !!e.minColorTemp && !!e.maxColorTemp

  const row   = makeRow('💡', e.name)
  const right = row.querySelector('.entity-right')!

  // Color button (color wins over temp-only)
  let colorExpanded = cardExpanded.get(e.entityId + ':color') ?? false
  let colorBtn: HTMLButtonElement | null = null
  if (hasColor || hasTemp) {
    colorBtn = document.createElement('button')
    colorBtn.className = 'light-color-btn ' + (hasColor ? 'has-rgb' : 'has-temp')
    if (colorExpanded) colorBtn.classList.add('active')
    colorBtn.title = 'Colore / Temperatura'
    right.appendChild(colorBtn)
  }

  // Inline brightness slider
  if (e.supportsBrightness) {
    const wrap  = document.createElement('div')
    const input = document.createElement('input')
    wrap.className   = 'light-brightness-wrap'
    input.className  = 'light-brightness-slider'
    input.type = 'range'; input.min = '0'; input.max = '100'
    input.value = String(e.brightness ?? 0)
    let bTimer = 0
    input.addEventListener('input', ev => {
      ev.stopPropagation()
      clearTimeout(bTimer)
      bTimer = window.setTimeout(() =>
        call('light', 'turn_on', e.entityId, { brightness_pct: Number(input.value) }), 120)
    })
    input.addEventListener('click', ev => ev.stopPropagation())
    wrap.appendChild(input)
    right.appendChild(wrap)
  }

  const toggle = makeToggle(e.isOn, v => call('light', v ? 'turn_on' : 'turn_off', e.entityId))
  right.appendChild(toggle)
  card.appendChild(row)

  // Expandable color controls (only if color/temp supported)
  if (colorBtn) {
    const controls = div('controls controls-color')
    if (hasColor) {
      controls.appendChild(buildHueSatPicker(e))
    }
    if (hasTemp) {
      controls.appendChild(makeSlider('Temp. colore', e.colorTemp ?? e.minColorTemp!, e.minColorTemp!, e.maxColorTemp!, 'K',
        val => call('light', 'turn_on', e.entityId, { color_temp: val }), true))
    }
    controls.style.display = colorExpanded ? '' : 'none'
    card.appendChild(controls)

    colorBtn.addEventListener('click', ev => {
      ev.stopPropagation()
      colorExpanded = !colorExpanded
      cardExpanded.set(e.entityId + ':color', colorExpanded)
      controls.style.display = colorExpanded ? '' : 'none'
      colorBtn!.classList.toggle('active', colorExpanded)
      if (colorExpanded) drawPendingHuePickers(controls)
      resize()
    })

    if (colorExpanded) requestAnimationFrame(() => drawPendingHuePickers(controls))
  }

  return card
}


// ─── 2D HUE-SAT PICKER ───────────────────────────────────────────────────────

function buildHueSatPicker(e: LightEntity): HTMLElement {
  const wrap   = div('light-hue-picker')
  const canvas = document.createElement('canvas')
  const thumb  = div('hue-thumb')
  wrap.appendChild(canvas)
  wrap.appendChild(thumb)

  let curHue = e.hue ?? 0
  let curSat = e.saturation ?? 100

  function positionThumb(h: number, s: number) {
    thumb.style.left = (h / 360 * 100) + '%'
    thumb.style.top  = ((1 - s / 100) * 100) + '%'
  }

  function draw() {
    const w = wrap.offsetWidth
    const h = wrap.offsetHeight
    if (!w || !h) return
    const dpr = window.devicePixelRatio || 1
    canvas.width  = Math.round(w * dpr)
    canvas.height = Math.round(h * dpr)
    canvas.style.width  = w + 'px'
    canvas.style.height = h + 'px'
    const ctx = canvas.getContext('2d')!
    ctx.scale(dpr, dpr)
    // Hue left → right
    const hg = ctx.createLinearGradient(0, 0, w, 0)
    for (let i = 0; i <= 12; i++) hg.addColorStop(i / 12, `hsl(${i * 30},100%,50%)`)
    ctx.fillStyle = hg; ctx.fillRect(0, 0, w, h)
    // White → transparent (top half brightens)
    const wg = ctx.createLinearGradient(0, 0, 0, h)
    wg.addColorStop(0, 'rgba(255,255,255,0.80)')
    wg.addColorStop(0.5, 'rgba(255,255,255,0)')
    ctx.fillStyle = wg; ctx.fillRect(0, 0, w, h)
    // Transparent → black (bottom half darkens)
    const bg = ctx.createLinearGradient(0, 0, 0, h)
    bg.addColorStop(0.5, 'rgba(0,0,0,0)')
    bg.addColorStop(1,   'rgba(0,0,0,0.55)')
    ctx.fillStyle = bg; ctx.fillRect(0, 0, w, h)
  }

  // Register draw fn so callers can trigger it after element becomes visible
  huePickers.set(wrap, draw)

  positionThumb(curHue, curSat)

  let moveHandler: ((ev: PointerEvent) => void) | null = null

  wrap.addEventListener('pointerdown', ev => {
    ev.stopPropagation()
    wrap.setPointerCapture(ev.pointerId)
    moveHandler = (ev2: PointerEvent) => {
      const rect = wrap.getBoundingClientRect()
      const x = Math.max(0, Math.min(1, (ev2.clientX - rect.left)  / rect.width))
      const y = Math.max(0, Math.min(1, (ev2.clientY - rect.top)   / rect.height))
      curHue = Math.round(x * 360)
      curSat = Math.round((1 - y) * 100)
      positionThumb(curHue, curSat)
      call('light', 'turn_on', e.entityId, { hs_color: [curHue, curSat] })
    }
    moveHandler(ev)
    wrap.addEventListener('pointermove', moveHandler)
  })
  const cleanupPointer = () => {
    if (moveHandler) { wrap.removeEventListener('pointermove', moveHandler); moveHandler = null }
  }
  wrap.addEventListener('pointerup',     cleanupPointer)
  wrap.addEventListener('pointercancel', cleanupPointer)

  return wrap
}

function drawPendingHuePickers(container: HTMLElement) {
  requestAnimationFrame(() => {
    container.querySelectorAll<HTMLElement>('.light-hue-picker').forEach(el => huePickers.get(el)?.())
  })
}

// ─── SWITCH ───────────────────────────────────────────────────────────────────
function buildSwitchCard(e: SwitchEntity): HTMLElement {
  const card = div('entity-card')
  if (!e.isAvailable) card.classList.add('unavailable')
  const domain = e.entityId.startsWith('input_boolean') ? 'input_boolean' : 'switch'
  const row    = makeRow('🔌', e.name)
  const toggle = makeToggle(e.isOn, v => call(domain, v ? 'turn_on' : 'turn_off', e.entityId))
  row.querySelector('.entity-right')!.appendChild(toggle)
  card.appendChild(row)
  return card
}

// ─── CLIMATE ──────────────────────────────────────────────────────────────────
function buildClimateCard(e: ClimateEntity): HTMLElement {
  const card = div('entity-card')
  if (!e.isAvailable) card.classList.add('unavailable')

  const tempDisplay = e.currentTemp != null
    ? `${e.currentTemp.toFixed(1)}${menuData?.tempUnit ?? '°C'}`
    : e.hvacMode
  const row = makeRow('🌡️', e.name)
  row.querySelector('.entity-right')!.innerHTML = `<span class="entity-state">${esc(tempDisplay)}</span>`
  card.appendChild(row)

  if (e.hvacModes.length) {
    const pills = div('mode-pills')
    e.hvacModes.forEach(mode => {
      const btn = document.createElement('button')
      btn.className = 'mode-pill' + (mode === e.hvacMode ? ' active' : '')
      btn.textContent = modeLabel(mode)
      btn.addEventListener('click', () => {
        call('climate', 'set_hvac_mode', e.entityId, { hvac_mode: mode })
        pills.querySelectorAll('.mode-pill').forEach(p => p.classList.remove('active'))
        btn.classList.add('active')
      })
      pills.appendChild(btn)
    })
    card.appendChild(pills)
  }

  if (e.hvacMode !== 'off' && e.targetTemp != null) {
    const controls = div('controls')
    controls.appendChild(makeSlider('Target',
      e.targetTemp, e.minTemp, e.maxTemp,
      menuData?.tempUnit ?? '°C',
      val => call('climate', 'set_temperature', e.entityId, { temperature: val }),
      false, e.tempStep))
    card.appendChild(controls)
  }
  return card
}

// ─── COVER ────────────────────────────────────────────────────────────────────
function buildCoverCard(e: CoverEntity): HTMLElement {
  const card = div('entity-card')
  if (!e.isAvailable) card.classList.add('unavailable')
  const icon = e.deviceClass === 'garage_door' ? '🚗' : e.deviceClass === 'door' ? '🚪' : '🪟'
  const row  = makeRow(icon, e.name)
  const right = row.querySelector('.entity-right')!
  // Open / Stop / Close buttons
  right.appendChild(coverBtn('▲', () => call('cover', 'open_cover',  e.entityId)))
  right.appendChild(coverBtn('■', () => call('cover', 'stop_cover',  e.entityId)))
  right.appendChild(coverBtn('▼', () => call('cover', 'close_cover', e.entityId)))
  card.appendChild(row)

  const controls = div('controls')
  if (e.supportsPosition && e.position != null) {
    controls.appendChild(makeSlider('Position', e.position, 0, 100, '%',
      val => call('cover', 'set_cover_position', e.entityId, { position: val })))
  }
  if (e.supportsTilt && e.tilt != null) {
    controls.appendChild(makeSlider('Tilt', e.tilt, 0, 100, '%',
      val => call('cover', 'set_cover_tilt_position', e.entityId, { tilt_position: val })))
  }
  if (controls.children.length) card.appendChild(controls)
  return card
}

// ─── LOCK ─────────────────────────────────────────────────────────────────────
function buildLockCard(e: LockEntity): HTMLElement {
  const card = div('entity-card')
  if (!e.isAvailable) card.classList.add('unavailable')
  const row   = makeRow('🔒', e.name)
  const right = row.querySelector('.entity-right')!
  const stateEl = document.createElement('span')
  stateEl.className = `lock-state ${e.lockState}`
  stateEl.textContent = lockLabel(e.lockState)
  right.appendChild(stateEl)

  if (e.lockState === 'locked' || e.lockState === 'unlocked' || e.lockState === 'unknown') {
    right.appendChild(makeToggle(e.lockState === 'locked',
      v => call('lock', v ? 'lock' : 'unlock', e.entityId)))
  }
  card.appendChild(row)
  return card
}

// ─── FAN ──────────────────────────────────────────────────────────────────────
function buildFanCard(e: FanEntity): HTMLElement {
  const card = div('entity-card')
  if (!e.isAvailable) card.classList.add('unavailable')
  const row    = makeRow('💨', e.name)
  const toggle = makeToggle(e.isOn, v => call('fan', v ? 'turn_on' : 'turn_off', e.entityId))
  row.querySelector('.entity-right')!.appendChild(toggle)
  card.appendChild(row)

  const controls = div('controls')
  if (e.supportsPercentage && e.percentage != null) {
    controls.appendChild(makeSlider('Speed', e.percentage, 0, 100, '%',
      val => call('fan', 'set_percentage', e.entityId, { percentage: val })))
  }
  if (e.supportsOscillation && e.oscillating != null) {
    const cr = div('control-row')
    cr.innerHTML = `<span class="control-label">Oscillate</span>`
    cr.appendChild(makeToggle(e.oscillating,
      v => call('fan', 'oscillate', e.entityId, { oscillating: v })))
    controls.appendChild(cr)
  }
  // Preset mode pills
  if (e.presetModes.length > 0) {
    const pills = div('mode-pills')
    e.presetModes.forEach(mode => {
      const btn = document.createElement('button')
      btn.className = 'mode-pill' + (mode === e.presetMode ? ' active' : '')
      btn.textContent = mode
      btn.addEventListener('click', () => {
        call('fan', 'set_preset_mode', e.entityId, { preset_mode: mode })
        pills.querySelectorAll('.mode-pill').forEach(p => p.classList.remove('active'))
        btn.classList.add('active')
      })
      pills.appendChild(btn)
    })
    controls.appendChild(pills)
  }
  if (controls.children.length) card.appendChild(controls)
  return card
}

// ─── HUMIDIFIER ───────────────────────────────────────────────────────────────
function buildHumidifierCard(e: HumidifierEntity): HTMLElement {
  const card = div('entity-card')
  if (!e.isAvailable) card.classList.add('unavailable')
  const row    = makeRow('💧', e.name)
  const toggle = makeToggle(e.isOn, v => call('humidifier', v ? 'turn_on' : 'turn_off', e.entityId))
  row.querySelector('.entity-right')!.appendChild(toggle)
  card.appendChild(row)

  const controls = div('controls')
  if (e.isOn && e.targetHumidity != null) {
    controls.appendChild(makeSlider('Humidity', e.targetHumidity, 0, 100, '%',
      val => call('humidifier', 'set_humidity', e.entityId, { humidity: val })))
  }
  // Mode selection
  if (e.modes.length > 0) {
    const pills = div('mode-pills')
    e.modes.forEach(mode => {
      const btn = document.createElement('button')
      btn.className = 'mode-pill' + (mode === e.mode ? ' active' : '')
      btn.textContent = mode
      btn.addEventListener('click', () => {
        call('humidifier', 'set_mode', e.entityId, { mode })
        pills.querySelectorAll('.mode-pill').forEach(p => p.classList.remove('active'))
        btn.classList.add('active')
      })
      pills.appendChild(btn)
    })
    controls.appendChild(pills)
  }
  if (controls.children.length) card.appendChild(controls)
  return card
}

// ─── VALVE ────────────────────────────────────────────────────────────────────
function buildValveCard(e: ValveEntity): HTMLElement {
  const card  = div('entity-card')
  if (!e.isAvailable) card.classList.add('unavailable')
  const row   = makeRow('🚰', e.name)
  row.querySelector('.entity-right')!.appendChild(makeToggle(e.isOpen,
    v => call('valve', v ? 'open_valve' : 'close_valve', e.entityId)))
  card.appendChild(row)

  if (e.supportsPosition && e.position != null) {
    let expanded = cardExpanded.get(e.entityId) ?? false
    const controls = div('controls')
    controls.appendChild(makeSlider('Position', e.position, 0, 100, '%',
      val => call('valve', 'set_valve_position', e.entityId, { position: val })))
    controls.style.display = expanded ? '' : 'none'
    card.appendChild(controls)
    row.addEventListener('click', ev => {
      if ((ev.target as HTMLElement).closest('.toggle-btn, .fav-btn')) return
      expanded = !expanded
      cardExpanded.set(e.entityId, expanded)
      controls.style.display = expanded ? '' : 'none'
      resize()
    })
  }
  return card
}

// ─── SENSOR (temp/humidity only) ──────────────────────────────────────────────
function buildSensorCard(e: SensorEntity): HTMLElement {
  const card = div('entity-card')
  if (!e.isAvailable) card.classList.add('unavailable')
  const icon  = e.deviceClass === 'humidity' ? '💧' : '🌡️'
  const row   = makeRow(icon, e.name)
  const valEl = document.createElement('span')
  valEl.className = 'sensor-value'
  valEl.textContent = e.unit ? `${e.value} ${e.unit}` : e.value
  row.querySelector('.entity-right')!.appendChild(valEl)
  card.appendChild(row)
  return card
}

// ─── ALARM ────────────────────────────────────────────────────────────────────
function buildAlarmCard(e: AlarmEntity): HTMLElement {
  const card = div('entity-card')
  if (!e.isAvailable) card.classList.add('unavailable')
  const row = makeRow('🔐', e.name)
  row.querySelector('.entity-right')!.innerHTML = `<span class="entity-state">${esc(alarmLabel(e.alarmState))}</span>`
  card.appendChild(row)

  const ALARM_MODES: { mode: string; label: string; service: string }[] = [
    { mode: 'disarmed',           label: 'Disarm',   service: 'alarm_disarm'            },
    { mode: 'armed_home',         label: 'Home',     service: 'alarm_arm_home'          },
    { mode: 'armed_away',         label: 'Away',     service: 'alarm_arm_away'          },
    { mode: 'armed_night',        label: 'Night',    service: 'alarm_arm_night'         },
    { mode: 'armed_vacation',     label: 'Vacation', service: 'alarm_arm_vacation'      },
    { mode: 'armed_custom_bypass',label: 'Custom',   service: 'alarm_arm_custom_bypass' },
  ]
  const available = ALARM_MODES.filter(m => m.mode === 'disarmed' || e.supportedModes.includes(m.mode))
  if (available.length) {
    const pills = div('mode-pills')
    available.forEach(m => {
      const btn = document.createElement('button')
      btn.className = 'mode-pill' + (e.alarmState === m.mode ? ' active' : '')
      btn.textContent = m.label
      btn.addEventListener('click', () => {
        call('alarm_control_panel', m.service, e.entityId)
        pills.querySelectorAll('.mode-pill').forEach(p => p.classList.remove('active'))
        btn.classList.add('active')
      })
      pills.appendChild(btn)
    })
    card.appendChild(pills)
  }
  return card
}

// ─── CAMERA ───────────────────────────────────────────────────────────────────
function buildCameraCard(e: CameraEntity): HTMLElement {
  const card = div('entity-card')
  if (!e.isAvailable) card.classList.add('unavailable')
  const row = makeRow('📷', e.name)
  const stateEl = document.createElement('span')
  stateEl.className = 'entity-state'
  stateEl.textContent = e.state
  row.querySelector('.entity-right')!.appendChild(stateEl)
  card.appendChild(row)

  const snapBox = div('camera-snapshot')
  const img     = document.createElement('img')
  const placeholder = div('snap-placeholder')
  placeholder.textContent = '▸ Click to load snapshot'
  snapBox.appendChild(placeholder)

  const snapKey = e.entityId + ':snap'
  let expanded  = cardExpanded.get(snapKey) ?? false
  snapBox.style.display = expanded ? '' : 'none'
  card.appendChild(snapBox)

  let refreshTimer = 0

  function startRefresh() {
    placeholder.textContent = 'Loading…'
    loadSnapshot()
    refreshTimer = window.setInterval(() => loadSnapshot(), 5000)
  }

  row.addEventListener('click', () => {
    expanded = !expanded
    cardExpanded.set(snapKey, expanded)
    snapBox.style.display = expanded ? '' : 'none'
    if (expanded) { startRefresh() } else { clearInterval(refreshTimer) }
    resize()
  })

  // Restore snapshot if it was previously expanded
  if (expanded) startRefresh()

  async function loadSnapshot() {
    if (!snapBox.isConnected) { clearInterval(refreshTimer); return }
    const src = await window.api.cameras.getSnapshot(e.entityId)
    if (src) {
      snapBox.innerHTML = ''
      img.src = src
      snapBox.appendChild(img)
    } else {
      placeholder.textContent = 'Snapshot unavailable'
      if (!snapBox.contains(placeholder)) { snapBox.innerHTML = ''; snapBox.appendChild(placeholder) }
    }
  }
  return card
}

// ─── SCENE ────────────────────────────────────────────────────────────────────
function buildSceneCard(e: SceneEntity): HTMLElement {
  const card = div('entity-card')
  if (!e.isAvailable) card.classList.add('unavailable')
  const row = makeRow('✨', e.name)
  const btn = document.createElement('button')
  btn.className = 'scene-btn'
  btn.textContent = '▶'
  btn.title = 'Activate scene'
  btn.addEventListener('click', ev => {
    ev.stopPropagation()
    call('scene', 'turn_on', e.entityId)
    btn.classList.add('fired')
    btn.textContent = '✓'
    setTimeout(() => { btn.classList.remove('fired'); btn.textContent = '▶' }, 1500)
  })
  row.querySelector('.entity-right')!.appendChild(btn)
  card.appendChild(row)
  return card
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

function makeRow(icon: string, name: string): HTMLElement {
  const row = div('entity-row')
  row.innerHTML = `
    <span class="entity-icon">${icon}</span>
    <span class="entity-name" title="${esc(name)}">${esc(name)}</span>
    <span class="entity-right"></span>`
  return row
}

function makeToggle(isOn: boolean, onChange: (v: boolean) => void): HTMLElement {
  const btn = document.createElement('button')
  btn.className = 'toggle-btn' + (isOn ? ' on' : '')
  btn.addEventListener('click', ev => {
    ev.stopPropagation()
    const next = !btn.classList.contains('on')
    btn.classList.toggle('on', next)
    onChange(next)
  })
  return btn
}

function makeSlider(
  label: string, value: number, min: number, max: number, unit: string,
  onChange: (v: number) => void, reverse = false, step = 1
): HTMLElement {
  const row   = div('control-row')
  const lbl   = document.createElement('span')
  lbl.className = 'control-label'
  lbl.textContent = label

  const track = div('slider-track')
  const input = document.createElement('input')
  input.type  = 'range'
  input.min   = String(reverse ? -max : min)
  input.max   = String(reverse ? -min : max)
  input.step  = String(step)
  input.value = String(reverse ? -value : value)

  const valLabel = document.createElement('span')
  valLabel.className = 'slider-value'
  valLabel.textContent = `${value}${unit}`

  let timer = 0
  input.addEventListener('input', () => {
    const raw = Number(input.value)
    const v   = reverse ? -raw : raw
    valLabel.textContent = `${Math.round(v)}${unit}`
    clearTimeout(timer)
    timer = window.setTimeout(() => onChange(v), 120)
  })

  track.appendChild(input)
  row.appendChild(lbl); row.appendChild(track); row.appendChild(valLabel)
  return row
}

function coverBtn(label: string, onClick: () => void): HTMLElement {
  const btn = document.createElement('button')
  btn.className = 'cover-btn'
  btn.textContent = label
  btn.addEventListener('click', ev => { ev.stopPropagation(); onClick() })
  return btn
}

function div(cls: string): HTMLDivElement {
  const el = document.createElement('div')
  el.className = cls
  return el
}

function esc(s: string): string {
  return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;')
}

function modeLabel(mode: string): string {
  const MAP: Record<string,string> = {
    off:'Off', heat:'Heat', cool:'Cool', auto:'Auto',
    heat_cool:'Heat/Cool', dry:'Dry', fan_only:'Fan'
  }
  return MAP[mode] ?? mode
}

function lockLabel(s: string): string {
  const MAP: Record<string,string> = {
    locked:'Locked', unlocked:'Unlocked', locking:'Locking…',
    unlocking:'Unlocking…', jammed:'Jammed!', unknown:'Unknown'
  }
  return MAP[s] ?? s
}

function alarmLabel(s: string): string {
  const MAP: Record<string,string> = {
    disarmed:'Disarmed', armed_home:'Home', armed_away:'Away',
    armed_night:'Night', armed_vacation:'Vacation',
    armed_custom_bypass:'Custom', pending:'Pending…',
    triggered:'TRIGGERED', arming:'Arming…'
  }
  return MAP[s] ?? s
}

async function call(domain: string, service: string, entityId: string, data?: Record<string,unknown>) {
  try { await window.api.ha.callService(domain, service, entityId, data) }
  catch (e) { console.warn('[popup] callService failed', e) }
}

// ─── Top bar ─────────────────────────────────────────────────────────────────
function setupTopBar() {
  document.getElementById('btn-back')!.addEventListener('click', () => closeDetailPanel())
  document.getElementById('btn-settings')!.addEventListener('click', () => window.api.window.openSettings())
  document.getElementById('btn-reconnect')!.addEventListener('click', () => window.api.ha.reconnect())
  document.getElementById('btn-hide')!.addEventListener('click', () => window.api.window.hide())
  document.getElementById('btn-open-ha')!.addEventListener('click', () => window.api.window.openHaUrl())

  // Entrance animation + reload area icons each time the popup gains focus
  window.addEventListener('focus', async () => {
    document.body.classList.remove('popup-enter')
    requestAnimationFrame(() => document.body.classList.add('popup-enter'))
    areaIcons = await window.api.accessories.getAreaIcons()
    if (menuData) render()
  })
  document.getElementById('btn-cameras')!.addEventListener('click', () => {
    if (!menuData?.cameras.length) return
    if (selectedRoomKey === 'cameras') closeDetailPanel()
    else openDetailPanel('cameras', '📷', 'Telecamere', menuData.cameras)
  })
}

function updateTopBar() {
  const dot    = document.getElementById('top-dot')
  const nameEl = document.getElementById('top-env-name')
  if (dot)    dot.className = status
  if (nameEl) nameEl.textContent = envList.find(e => e.id === activeEnvId)?.name ?? '–'
  const camBtn = document.getElementById('btn-cameras')
  if (camBtn) camBtn.classList.toggle('cam-active', selectedRoomKey === 'cameras')

  const notifBadge  = document.getElementById('badge-notifications')
  const updateBadge = document.getElementById('badge-updates')
  const nc = menuData?.notificationCount ?? 0
  const uc = menuData?.updateCount       ?? 0
  if (notifBadge)  { notifBadge.textContent  = `🔔 ${nc}`; notifBadge.classList.toggle('visible',  nc > 0) }
  if (updateBadge) { updateBadge.textContent = `⬆ ${uc}`;  updateBadge.classList.toggle('visible', uc > 0) }
}
