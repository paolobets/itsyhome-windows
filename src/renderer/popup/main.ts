/**
 * Popup renderer – Tauri 2.0 edition.
 *
 * IPC is accessed exclusively through src/lib/api.ts.
 * Entity card rendering is in entities.ts.
 * Section building is in sections.ts.
 *
 * Phase 2 additions:
 *  - Toast notification system
 *  - Search / filter entities
 *  - Keyboard shortcuts (Escape, Ctrl+F)
 *  - callService with error toast
 */

import type {
  MenuData, AppEntity, ConnectionStatus, Room, HAEnvironment,
} from '@shared/types'
import { roomIcon } from '@shared/roomIcons'
import { api } from '@lib/api'
import {
  buildCard,
  setSharedMenuData,
  setResizeCallback,
  div,
  esc,
} from './entities'
import {
  buildEnvSection,
  buildRowSection,
  buildRoomRow,
} from './sections'
import type { SectionsState } from './sections'

// ─── State ────────────────────────────────────────────────────────────────────

let menuData:    MenuData | null  = null
let favorites:   Set<string>      = new Set()
let status:      ConnectionStatus = 'disconnected'
let envList:     HAEnvironment[]  = []
let activeEnvId: string | null    = null
let areaIcons:   Record<string, string> = {}

// Search
let searchQuery = ''

// Preserved UI state
const sectionCollapsed = new Map<string, boolean>()
const cardExpanded     = new Map<string, boolean>()   // re-exported to entities via module

// Detail panel
let detailOpen:         boolean             = false
let selectedRoomKey:    string | null       = null
let detailCloseHandler: (() => void) | null = null

// Resize dedup
let resizeRafId = 0

// ─── Startup ──────────────────────────────────────────────────────────────────

window.addEventListener('DOMContentLoaded', () => {
  // Wire resize callback into entities module
  setResizeCallback(resize)

  setupTopBar()
  setupSearch()
  setupKeyboard()
  load()

  api.menu.onUpdate(data => {
    menuData = data
    setSharedMenuData(data)
    render()
  })

  api.ha.onStatusChange(async s => {
    status = s
    if (s === 'connected') {
      const [envs, envId] = await Promise.all([
        api.environments.getAll(),
        api.environments.getActiveId(),
      ])
      envList     = envs
      activeEnvId = envId
    }
    updateTopBar()
  })
})

async function load() {
  const [data, fav, s, envs, envId, icons] = await Promise.all([
    api.menu.getData(),
    api.favorites.get(),
    api.ha.getStatus(),
    api.environments.getAll(),
    api.environments.getActiveId(),
    api.accessories.getAreaIcons(),
  ])
  menuData    = data
  favorites   = new Set(fav)
  status      = s
  envList     = envs
  activeEnvId = envId
  areaIcons   = icons
  setSharedMenuData(data)
  updateTopBar()
  render()
}

// ─── callService with toast feedback ─────────────────────────────────────────

function callService(
  domain:   string,
  service:  string,
  entityId: string,
  data?:    Record<string, unknown>,
): void {
  api.ha.callService(domain, service, entityId, data).catch(err => {
    console.warn('[popup] callService failed', err)
    showToast(`Failed: ${String(err)}`, 'error')
  })
}

// ─── Toast notifications (Phase 2) ───────────────────────────────────────────

function showToast(
  message:  string,
  type:     'success' | 'error' | 'info' = 'info',
  duration  = 3000,
): void {
  const container = document.getElementById('toast-container')
  if (!container) return
  const toast = document.createElement('div')
  toast.className   = `toast toast-${type}`
  toast.textContent = message
  container.appendChild(toast)
  requestAnimationFrame(() => toast.classList.add('show'))
  setTimeout(() => {
    toast.classList.remove('show')
    setTimeout(() => toast.remove(), 300)
  }, duration)
}

// ─── Search (Phase 2) ─────────────────────────────────────────────────────────

function setupSearch() {
  const btnSearch = document.getElementById('btn-search')
  const searchBar = document.getElementById('search-bar')
  const input     = document.getElementById('search-input') as HTMLInputElement | null
  const btnClear  = document.getElementById('btn-search-clear')

  if (!btnSearch || !searchBar || !input || !btnClear) return

  btnSearch.addEventListener('click', () => toggleSearch())
  btnClear.addEventListener('click',  () => clearSearch())

  input.addEventListener('input', () => {
    searchQuery = input.value.toLowerCase().trim()
    render()
  })
}

function toggleSearch(): void {
  const searchBar = document.getElementById('search-bar')
  const input     = document.getElementById('search-input') as HTMLInputElement | null
  if (!searchBar) return
  const hidden = searchBar.classList.toggle('hidden')
  if (!hidden) {
    input?.focus()
  } else {
    clearSearch()
  }
}

function clearSearch(): void {
  const searchBar = document.getElementById('search-bar')
  const input     = document.getElementById('search-input') as HTMLInputElement | null
  searchQuery = ''
  if (input) input.value = ''
  searchBar?.classList.add('hidden')
  render()
}

function entityMatchesSearch(entity: AppEntity): boolean {
  if (!searchQuery) return true
  return entity.name.toLowerCase().includes(searchQuery)
}

// ─── Keyboard shortcuts (Phase 2) ────────────────────────────────────────────

function setupKeyboard(): void {
  document.addEventListener('keydown', (e: KeyboardEvent) => {
    if (e.key === 'Escape') {
      if (searchQuery) { clearSearch(); return }
      if (detailOpen)  { closeDetailPanel(); return }
      api.window.hide().catch(() => { /* ignore */ })
      return
    }
    if ((e.ctrlKey || e.metaKey) && e.key === 'f') {
      e.preventDefault()
      toggleSearch()
    }
  })
}

// ─── Render ───────────────────────────────────────────────────────────────────

function render(): void {
  const list = document.getElementById('menu-list')!
  list.innerHTML = ''

  updateTopBar()

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

  const sectionsState: SectionsState = {
    sectionCollapsed,
    selectedRoomKey,
    envList,
    activeEnvId,
    areaIcons,
    favorites,
  }

  // 1. Environment section (always shown, unaffected by search)
  if (!searchQuery) {
    frag.appendChild(buildEnvSection(sectionsState, updateTopBar, resize))
  }

  // Filter helpers
  const filterEntities = (entities: AppEntity[]) =>
    searchQuery ? entities.filter(entityMatchesSearch) : entities

  // 2. Favorites
  const filteredFavs = filterEntities(menuData.favorites)
  if (filteredFavs.length) {
    frag.appendChild(div('menu-separator'))
    const wrap = div('fav-section')
    filteredFavs.forEach(e => wrap.appendChild(buildCard(e, callService, favorites, toggleFavorite)))
    frag.appendChild(wrap)
  }

  // 3. Rooms
  const matchingRooms: Room[] = searchQuery
    ? menuData.rooms
        .map(r => ({ ...r, entities: filterEntities(r.entities) }))
        .filter(r => r.entities.length > 0)
    : menuData.rooms.filter(r => r.entities.length > 0)

  if (matchingRooms.length) {
    frag.appendChild(div('menu-separator'))
    for (const room of matchingRooms) {
      frag.appendChild(buildRoomRow(room, sectionsState, openDetailPanel, closeDetailPanel, roomIcon))
    }
  }

  // 4. Scenes (collapsible row)
  const filteredScenes = filterEntities(menuData.scenes)
  if (filteredScenes.length) {
    const autoExpand = !!searchQuery
    if (autoExpand) sectionCollapsed.set('scenes', false)
    frag.appendChild(buildRowSection(
      'scenes', '✨', 'Scene',
      () => {
        const w = div('section-body')
        filteredScenes.forEach(e => w.appendChild(buildCard(e, callService, favorites, toggleFavorite)))
        return w
      },
      sectionsState,
      resize,
    ))
    if (autoExpand) sectionCollapsed.delete('scenes')   // reset for next non-search render
  }

  list.appendChild(frag)
  resize()
}

// ─── Resize ───────────────────────────────────────────────────────────────────

function resize(): void {
  if (resizeRafId) cancelAnimationFrame(resizeRafId)
  resizeRafId = requestAnimationFrame(() => {
    resizeRafId = 0
    const topBar   = document.getElementById('top-bar')!
    const menuList = document.getElementById('menu-list')!
    const h = topBar.offsetHeight + menuList.offsetHeight
    const w = 300
    api.window.resize(h, w).catch(() => { /* ignore */ })
  })
}

// ─── Detail panel ─────────────────────────────────────────────────────────────

function openDetailPanel(key: string, icon: string, name: string, entities: AppEntity[]): void {
  selectedRoomKey = key
  detailOpen = true

  const detailIcon  = document.getElementById('detail-icon')!
  const detailTitle = document.getElementById('detail-title')!
  const detailList  = document.getElementById('detail-list')!

  detailIcon.textContent  = icon
  detailTitle.textContent = name
  detailList.innerHTML    = ''

  const entitiesToShow = searchQuery ? entities.filter(entityMatchesSearch) : entities
  entitiesToShow.forEach(e => detailList.appendChild(buildCard(e, callService, favorites, toggleFavorite)))

  const panel = document.getElementById('detail-panel')!
  if (detailCloseHandler) {
    panel.removeEventListener('animationend', detailCloseHandler)
    detailCloseHandler = null
  }
  panel.classList.remove('closing')
  panel.classList.add('open')

  document.querySelectorAll<HTMLElement>('[data-room-key]').forEach(el => {
    el.classList.toggle('row-selected', el.dataset.roomKey === key)
  })

  updateTopBar()
  resize()
}

function closeDetailPanel(): void {
  selectedRoomKey = null
  document.querySelectorAll('.row-selected').forEach(el => el.classList.remove('row-selected'))
  updateTopBar()

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

// ─── Favorite toggle ──────────────────────────────────────────────────────────

async function toggleFavorite(entity: AppEntity): Promise<void> {
  if (favorites.has(entity.entityId)) {
    favorites.delete(entity.entityId)
    await api.favorites.remove(entity.entityId)
    if (menuData) menuData.favorites = menuData.favorites.filter(e => e.entityId !== entity.entityId)
  } else {
    favorites.add(entity.entityId)
    await api.favorites.add(entity.entityId)
    if (menuData) menuData.favorites = [...menuData.favorites, entity]
  }
  render()
}

// ─── Top bar setup ────────────────────────────────────────────────────────────

function setupTopBar(): void {
  document.getElementById('btn-back')!
    .addEventListener('click', () => closeDetailPanel())
  document.getElementById('btn-settings')!
    .addEventListener('click', () => api.window.openSettings())
  document.getElementById('btn-reconnect')!
    .addEventListener('click', () => api.ha.reconnect())
  document.getElementById('btn-hide')!
    .addEventListener('click', () => api.window.hide())
  document.getElementById('btn-open-ha')!
    .addEventListener('click', () => api.window.openHaUrl())

  window.addEventListener('focus', async () => {
    document.body.classList.remove('popup-enter')
    requestAnimationFrame(() => document.body.classList.add('popup-enter'))
    areaIcons = await api.accessories.getAreaIcons()
    if (menuData) render()
  })

  document.getElementById('btn-cameras')!.addEventListener('click', () => {
    if (!menuData?.cameras.length) return
    if (selectedRoomKey === 'cameras') closeDetailPanel()
    else openDetailPanel('cameras', '📷', 'Telecamere', menuData.cameras)
  })
}

function updateTopBar(): void {
  const dot    = document.getElementById('top-dot')
  const nameEl = document.getElementById('top-env-name')
  if (dot) {
    dot.className = status
    const dotTitles: Record<string, string> = {
      connected: 'Connected', connecting: 'Connecting…',
      disconnected: 'Disconnected', error: 'Connection error',
    }
    dot.title = dotTitles[status] ?? status
  }
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

// Suppress unused-variable warning – cardExpanded is declared for future external use
void cardExpanded
