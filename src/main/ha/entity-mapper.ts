/**
 * EntityMapper – converts raw HA states into typed AppEntity objects.
 * Mirrors EntityMapper.swift from the original macOS project.
 *
 * Supported domains (same as original):
 *   light, switch, input_boolean, climate, cover, lock, fan,
 *   humidifier, valve, sensor (temp/humidity only), alarm_control_panel,
 *   camera, scene
 *
 * Excluded domains (same as original):
 *   binary_sensor, button, input_button, number, input_number,
 *   select, input_select, device_tracker, person, sun, weather,
 *   update, automation, script, media_player, vacuum, and all
 *   entities with entity_category (config / diagnostic)
 */

import type { HAState, HAArea, HADevice, HAEntityEntry, HAConfig } from './models'
import type {
  AppEntity, Room, MenuData, LightEntity, SwitchEntity, ClimateEntity,
  CoverEntity, LockEntity, FanEntity, HumidifierEntity, ValveEntity,
  SensorEntity, AlarmEntity, CameraEntity, SceneEntity, BaseEntity
} from '@shared/types'
import type { Store } from '../store/store'

const ALARM_MODE_BITS: { mode: string; bit: number }[] = [
  { mode: 'armed_home',          bit: 1  },
  { mode: 'armed_away',          bit: 2  },
  { mode: 'armed_night',         bit: 4  },
  { mode: 'armed_vacation',      bit: 16 },
  { mode: 'armed_custom_bypass', bit: 32 },
]

const EXCLUDED_DOMAINS = new Set([
  'binary_sensor', 'button', 'input_button', 'number', 'input_number',
  'select', 'input_select', 'device_tracker', 'person', 'sun', 'weather',
  'update', 'automation', 'script', 'media_player', 'vacuum', 'remote',
  'todo', 'calendar', 'event', 'image', 'siren', 'notify', 'persistent_notification'
])

export class EntityMapper {
  private store:    Store
  private tempUnit: '°C' | '°F' = '°C'

  constructor(store: Store) { this.store = store }

  setTempUnit(unit: string): void {
    this.tempUnit = unit.trim().toUpperCase().startsWith('F') ? '°F' : '°C'
  }

  map(
    states:    HAState[],
    areas:     HAArea[],
    devices:   HADevice[],
    entries:   HAEntityEntry[],
    haConfig?: HAConfig
  ): MenuData {
    if (haConfig?.unit_system?.temperature) this.setTempUnit(haConfig.unit_system.temperature)

    const deviceMap = new Map(devices.map(d => [d.id, d]))
    const entryMap  = new Map(entries.map(e => [e.entity_id, e]))
    const areaMap   = new Map(areas.map(a => [a.area_id, a]))

    const getAreaId = (state: HAState): string | null => {
      const entry = entryMap.get(state.entity_id)
      if (entry?.area_id)   return entry.area_id
      if (entry?.device_id) return deviceMap.get(entry.device_id)?.area_id ?? null
      return null
    }

    // Single pass: count updates + map entities
    let notificationCount = 0   // placeholder; overridden in refreshMenuData()
    let updateCount       = 0
    const allEntities: AppEntity[] = []
    const scenes:   SceneEntity[]  = []
    const cameras:  CameraEntity[] = []

    for (const state of states) {
      const domain = state.entity_id.split('.')[0]
      if (domain === 'update' && state.state === 'on') updateCount++
      if (EXCLUDED_DOMAINS.has(domain)) continue

      const entry = entryMap.get(state.entity_id)
      if (entry?.disabled_by || entry?.hidden_by) continue
      if (entry?.entity_category)                 continue   // config/diagnostic

      const areaId = getAreaId(state)
      const entity = this.mapEntity(state, entry, areaId)
      if (!entity) continue

      if (domain === 'scene')  scenes.push(entity as SceneEntity)
      else if (domain === 'camera') cameras.push(entity as CameraEntity)
      else allEntities.push(entity)
    }

    // Apply hidden filter
    const hiddenSet   = new Set(this.store.getHiddenEntities())
    const favorites   = this.store.getFavorites()
    const favSet      = new Set(favorites)

    const visible = allEntities.filter(e => !hiddenSet.has(e.entityId))
    const favList = visible.filter(e => favSet.has(e.entityId))

    // Group by area
    const roomMap    = new Map<string, AppEntity[]>()
    const noAreaList: AppEntity[] = []

    for (const entity of visible) {
      if (entity.areaId) {
        if (!roomMap.has(entity.areaId)) roomMap.set(entity.areaId, [])
        roomMap.get(entity.areaId)!.push(entity)
      } else {
        noAreaList.push(entity)
      }
    }

    // Build ordered rooms
    const roomOrder    = this.store.getRoomOrder()
    const roomOrderSet = new Set(roomOrder)
    const allAreaIds   = [...roomMap.keys()]
    const orderedAreaIds = [
      ...roomOrder.filter(id => roomMap.has(id)),
      ...allAreaIds.filter(id => !roomOrderSet.has(id)).sort((a, b) => {
        const na = areaMap.get(a)?.name ?? a
        const nb = areaMap.get(b)?.name ?? b
        return na.localeCompare(nb)
      })
    ]

    const rooms: Room[] = []
    for (const areaId of orderedAreaIds) {
      const area = areaMap.get(areaId)
      if (!area) continue
      const entities = this.orderedEntities(areaId, roomMap.get(areaId) ?? [])
      if (entities.length > 0) rooms.push({ areaId, name: area.name, entities })
    }

    if (noAreaList.length > 0) {
      const sorted = this.orderedEntities('', noAreaList)
      rooms.push({ areaId: '', name: 'Other', entities: sorted })
    }

    return {
      favorites: favList,
      rooms,
      scenes:  scenes.filter(s => !hiddenSet.has(s.entityId)),
      cameras: this.store.getCamerasEnabled() ? cameras.filter(c => !hiddenSet.has(c.entityId)) : [],
      tempUnit: this.tempUnit,
      notificationCount,
      updateCount,
    }
  }

  private orderedEntities(areaId: string, entities: AppEntity[]): AppEntity[] {
    const saved    = this.store.getDeviceOrder(areaId)
    const entityMap = new Map(entities.map(e => [e.entityId, e]))
    const ordered  = saved.filter(id => entityMap.has(id)).map(id => entityMap.get(id)!)
    const savedSet = new Set(saved)
    const rest     = entities.filter(e => !savedSet.has(e.entityId)).sort((a, b) => a.name.localeCompare(b.name))
    return [...ordered, ...rest]
  }

  private mapEntity(state: HAState, entry: HAEntityEntry | undefined, areaId: string | null): AppEntity | null {
    const domain      = state.entity_id.split('.')[0]
    const name        = (entry?.name ?? state.attributes.friendly_name as string | undefined ?? state.entity_id)
    const isAvailable = state.state !== 'unavailable' && state.state !== 'unknown'

    const base: BaseEntity = { entityId: state.entity_id, name, type: 'switch', areaId, state: state.state, isAvailable }

    switch (domain) {
      case 'light':                return this.mapLight(state,      { ...base, type: 'light'      })
      case 'switch':
      case 'input_boolean':        return this.mapSwitch(state,     { ...base, type: 'switch'     })
      case 'climate':              return this.mapClimate(state,    { ...base, type: 'climate'    })
      case 'cover':                return this.mapCover(state,      { ...base, type: 'cover'      })
      case 'lock':                 return this.mapLock(state,       { ...base, type: 'lock'       })
      case 'fan':                  return this.mapFan(state,        { ...base, type: 'fan'        })
      case 'humidifier':           return this.mapHumidifier(state, { ...base, type: 'humidifier' })
      case 'valve':                return this.mapValve(state,      { ...base, type: 'valve'      })
      case 'sensor':               return this.mapSensor(state,     { ...base, type: 'sensor'     })
      case 'alarm_control_panel':  return this.mapAlarm(state,      { ...base, type: 'alarm'      })
      case 'camera':               return { ...base, type: 'camera'  } as CameraEntity
      case 'scene':                return { ...base, type: 'scene'   } as SceneEntity
      default:                     return null
    }
  }

  // ─── Per-domain mappers ───────────────────────────────────────────────────

  private mapLight(s: HAState, base: LightEntity): LightEntity {
    const a   = s.attributes
    const cms = (a.supported_color_modes as string[] | undefined) ?? []
    const hs  = a.hs_color as [number, number] | undefined

    const supportsRgb       = ['hs','rgb','rgbw','rgbww','xy'].some(m => cms.includes(m))
    const supportsColorTemp = cms.some(m => ['color_temp','rgbww','rgbw','white'].includes(m))
    const supportsBrightness = !cms.includes('onoff') || cms.length > 1 || a.brightness !== undefined

    const minCT = (a.min_mireds as number | undefined)
      ?? (a.min_color_temp_kelvin != null ? Math.round(1e6 / (a.min_color_temp_kelvin as number)) : null)
    const maxCT = (a.max_mireds as number | undefined)
      ?? (a.max_color_temp_kelvin != null ? Math.round(1e6 / (a.max_color_temp_kelvin as number)) : null)

    return {
      ...base, isOn: s.state === 'on',
      brightness:   s.state === 'on' && a.brightness != null ? Math.round((a.brightness as number) / 255 * 100) : null,
      hue:          hs?.[0] ?? null,
      saturation:   hs?.[1] ?? null,
      colorTemp:    a.color_temp as number | null ?? null,
      minColorTemp: minCT ?? null,
      maxColorTemp: maxCT ?? null,
      supportsBrightness, supportsColorTemp, supportsRgb
    }
  }

  private mapSwitch(s: HAState, base: SwitchEntity): SwitchEntity {
    return { ...base, isOn: s.state === 'on' }
  }

  private mapClimate(s: HAState, base: ClimateEntity): ClimateEntity {
    const a = s.attributes
    return {
      ...base,
      hvacMode:       s.state,
      hvacAction:     a.hvac_action as string | null ?? null,
      hvacModes:      (a.hvac_modes as string[] | undefined) ?? [],
      currentTemp:    a.current_temperature as number | null ?? null,
      targetTemp:     a.temperature as number | null ?? null,
      targetTempHigh: a.target_temp_high as number | null ?? null,
      targetTempLow:  a.target_temp_low  as number | null ?? null,
      minTemp:        (a.min_temp as number | undefined) ?? 7,
      maxTemp:        (a.max_temp as number | undefined) ?? 35,
      tempStep:       (a.target_temp_step as number | undefined) ?? 0.5
    }
  }

  private mapCover(s: HAState, base: CoverEntity): CoverEntity {
    const a    = s.attributes
    const feat = (a.supported_features as number | undefined) ?? 0
    const VALID_STATES = ['open','closed','opening','closing','stopped']
    return {
      ...base,
      coverState:      (VALID_STATES.includes(s.state) ? s.state : 'stopped') as CoverEntity['coverState'],
      position:        a.current_position as number | null ?? null,
      tilt:            a.current_tilt_position as number | null ?? null,
      supportsPosition: (feat & 4)   !== 0,
      supportsTilt:     (feat & 128) !== 0,
      deviceClass:     a.device_class as string | null ?? null
    }
  }

  private mapLock(s: HAState, base: LockEntity): LockEntity {
    const valid = ['locked','unlocked','locking','unlocking','jammed']
    return { ...base, lockState: (valid.includes(s.state) ? s.state : 'unknown') as LockEntity['lockState'] }
  }

  private mapFan(s: HAState, base: FanEntity): FanEntity {
    const a    = s.attributes
    const feat = (a.supported_features as number | undefined) ?? 0
    return {
      ...base,
      isOn:               s.state === 'on',
      percentage:         a.percentage         as number | null ?? null,
      oscillating:        a.oscillating        as boolean | null ?? null,
      direction:          a.direction          as 'forward' | 'reverse' | null ?? null,
      presetMode:         a.preset_mode        as string | null ?? null,
      presetModes:        (a.preset_modes as string[] | undefined) ?? [],
      supportsPercentage:  a.percentage !== undefined,
      supportsOscillation: (feat & 1) !== 0,
      supportsDirection:   (feat & 4) !== 0
    }
  }

  private mapHumidifier(s: HAState, base: HumidifierEntity): HumidifierEntity {
    const a = s.attributes
    return {
      ...base,
      isOn:            s.state === 'on',
      targetHumidity:  a.humidity         as number | null ?? null,
      currentHumidity: a.current_humidity as number | null ?? null,
      mode:            a.mode             as string | null ?? null,
      modes:           (a.available_modes as string[] | undefined) ?? []
    }
  }

  private mapValve(s: HAState, base: ValveEntity): ValveEntity {
    const a = s.attributes
    return {
      ...base,
      isOpen:          s.state === 'open' || s.state === 'opening',
      position:        a.current_position as number | null ?? null,
      supportsPosition: (((a.supported_features as number | undefined) ?? 0) & 4) !== 0
    }
  }

  private mapSensor(s: HAState, base: SensorEntity): SensorEntity | null {
    const dc = s.attributes.device_class as string | null ?? null
    // Original only displays temperature and humidity sensors
    if (dc !== 'temperature' && dc !== 'humidity') return null
    return {
      ...base,
      value:       s.state,
      unit:        s.attributes.unit_of_measurement as string | null ?? null,
      deviceClass: dc as 'temperature' | 'humidity'
    }
  }

  private mapAlarm(s: HAState, base: AlarmEntity): AlarmEntity {
    const feat = (s.attributes.supported_features as number | undefined) ?? 0
    const supportedModes = ALARM_MODE_BITS.filter(m => (feat & m.bit) !== 0).map(m => m.mode)
    return { ...base, alarmState: s.state, supportedModes }
  }
}
