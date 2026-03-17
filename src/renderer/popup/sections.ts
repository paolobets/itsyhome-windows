/**
 * Section builders for the popup list view.
 *
 * Covers:
 *  - Environment switcher section
 *  - Generic collapsible row section (scenes)
 *  - Room rows that open the detail panel
 */

import type { AppEntity, HAEnvironment, Room } from '@shared/types'
import { api } from '@lib/api'
import { div, esc, buildCard } from './entities'
import type { CallService } from './entities'

// ─── State references injected from main.ts ──────────────────────────────────

export interface SectionsState {
  sectionCollapsed:  Map<string, boolean>
  selectedRoomKey:   string | null
  envList:           HAEnvironment[]
  activeEnvId:       string | null
  areaIcons:         Record<string, string>
  favorites:         Set<string>
}

export type OpenDetailFn  = (key: string, icon: string, name: string, entities: AppEntity[]) => void
export type CloseDetailFn = () => void
export type ResizeFn      = () => void
export type RoomIconFn    = (name: string) => string

// ─── Environment section ─────────────────────────────────────────────────────

export function buildEnvSection(
  state:         SectionsState,
  updateTopBar:  () => void,
  resize:        ResizeFn,
): HTMLElement {
  const sec        = div('section')
  const activeName = state.envList.find(e => e.id === state.activeEnvId)?.name ?? '–'
  const isExpandable = state.envList.length > 1

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

  let collapsed = state.sectionCollapsed.get('_env') ?? true
  if (!collapsed) head.classList.remove('collapsed')

  const body = div('section-content')
  body.style.display = collapsed ? 'none' : ''

  function renderEnvBody() {
    body.innerHTML = ''
    for (const env of state.envList) {
      const isActive = env.id === state.activeEnvId
      const card   = div('entity-card')
      const row    = div('entity-row')

      const iconEl = document.createElement('span')
      iconEl.className   = 'entity-icon'
      iconEl.textContent = isActive ? '🔗' : '○'

      const nameEl = document.createElement('span')
      nameEl.className   = 'entity-name'
      nameEl.textContent = env.name

      const right = document.createElement('span')
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
          await api.environments.connect(env.id)
          state.activeEnvId = env.id
          updateTopBar()
          renderEnvBody()
        })
        right.appendChild(btn)
      }

      row.appendChild(iconEl); row.appendChild(nameEl); row.appendChild(right)
      card.appendChild(row)
      body.appendChild(card)
    }
  }

  if (!collapsed) renderEnvBody()

  head.addEventListener('click', () => {
    collapsed = !collapsed
    state.sectionCollapsed.set('_env', collapsed)
    if (!collapsed && !body.children.length) renderEnvBody()
    body.style.display = collapsed ? 'none' : ''
    head.classList.toggle('collapsed', collapsed)
    resize()
  })

  sec.appendChild(head)
  sec.appendChild(body)
  return sec
}

// ─── Generic collapsible row section ─────────────────────────────────────────

export function buildRowSection(
  key:          string,
  icon:         string,
  name:         string,
  buildContent: () => HTMLElement,
  state:        Pick<SectionsState, 'sectionCollapsed'>,
  resize:       ResizeFn,
): HTMLElement {
  const sec = div('section')
  let collapsed = state.sectionCollapsed.get(key) ?? true
  let rendered  = false

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
    state.sectionCollapsed.set(key, collapsed)
    if (!collapsed && !rendered) {
      body.appendChild(buildContent())
      rendered = true
    }
    body.style.display = collapsed ? 'none' : ''
    head.classList.toggle('collapsed', collapsed)
    resize()
  })

  if (!collapsed) {
    body.appendChild(buildContent())
    rendered = true
  }

  sec.appendChild(head)
  sec.appendChild(body)
  return sec
}

// ─── Detail-panel row (rooms) ─────────────────────────────────────────────────

export function buildRoomRow(
  room:         Room,
  state:        SectionsState,
  openDetail:   OpenDetailFn,
  closeDetail:  CloseDetailFn,
  roomIconFn:   RoomIconFn,
): HTMLElement {
  const icon = state.areaIcons[room.areaId] ?? roomIconFn(room.name)
  return buildDetailRow(`room:${room.areaId}`, icon, room.name, room.entities, state, openDetail, closeDetail)
}

function buildDetailRow(
  key:         string,
  icon:        string,
  name:        string,
  entities:    AppEntity[],
  state:       Pick<SectionsState, 'selectedRoomKey'>,
  openDetail:  OpenDetailFn,
  closeDetail: CloseDetailFn,
): HTMLElement {
  const sec  = div('section')
  const head = div('section-header row-header')
  head.dataset.roomKey = key
  head.innerHTML = `
    <span class="row-icon">${icon}</span>
    <span class="row-name">${esc(name)}</span>
    <span class="row-chevron">›</span>`

  if (state.selectedRoomKey === key) head.classList.add('row-selected')

  head.addEventListener('click', () => {
    if (state.selectedRoomKey === key) closeDetail()
    else openDetail(key, icon, name, entities)
  })

  sec.appendChild(head)
  return sec
}

// ─── Favorites section ────────────────────────────────────────────────────────

export function buildFavSection(
  favorites:   AppEntity[],
  favSet:      Set<string>,
  cs:          CallService,
  onFavToggle: (e: AppEntity) => Promise<void>,
): HTMLElement {
  const wrap = div('fav-section')
  favorites.forEach(e => wrap.appendChild(buildCard(e, cs, favSet, onFavToggle)))
  return wrap
}
