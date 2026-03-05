/**
 * Shared types – mirrors the original Swift data model.
 * Entity types match exactly what HomeAssistantPlatform / EntityMapper supported.
 */

// ─── Entity types ─────────────────────────────────────────────────────────────

export type EntityType =
  | 'light' | 'switch' | 'climate' | 'cover' | 'lock' | 'fan'
  | 'humidifier' | 'valve' | 'sensor' | 'alarm' | 'camera' | 'scene'

export interface BaseEntity {
  entityId: string
  name:     string
  type:     EntityType
  areaId:   string | null
  state:    string
  isAvailable: boolean
}

export interface LightEntity extends BaseEntity {
  type: 'light'
  isOn:            boolean
  brightness:      number | null   // 0–100 %
  hue:             number | null   // 0–360
  saturation:      number | null   // 0–100
  colorTemp:       number | null   // mireds
  minColorTemp:    number | null
  maxColorTemp:    number | null
  supportsBrightness: boolean
  supportsColorTemp:  boolean
  supportsRgb:        boolean
}

export interface SwitchEntity extends BaseEntity {
  type: 'switch'
  isOn: boolean
}

export interface ClimateEntity extends BaseEntity {
  type: 'climate'
  hvacMode:       string          // off / heat / cool / auto / heat_cool / dry / fan_only
  hvacAction:     string | null   // heating / cooling / idle / off
  hvacModes:      string[]
  currentTemp:    number | null
  targetTemp:     number | null
  targetTempHigh: number | null   // heat_cool dual setpoint
  targetTempLow:  number | null
  minTemp:        number
  maxTemp:        number
  tempStep:       number
}

export interface CoverEntity extends BaseEntity {
  type: 'cover'
  coverState:      'open' | 'closed' | 'opening' | 'closing' | 'stopped'
  position:        number | null   // 0–100
  tilt:            number | null   // 0–100
  supportsPosition: boolean
  supportsTilt:     boolean
  deviceClass:      string | null  // blind / garage_door / door / window / awning …
}

export interface LockEntity extends BaseEntity {
  type: 'lock'
  lockState: 'locked' | 'unlocked' | 'locking' | 'unlocking' | 'jammed' | 'unknown'
}

export interface FanEntity extends BaseEntity {
  type: 'fan'
  isOn:               boolean
  percentage:         number | null
  oscillating:        boolean | null
  direction:          'forward' | 'reverse' | null
  presetMode:         string | null
  presetModes:        string[]
  supportsPercentage: boolean
  supportsOscillation: boolean
  supportsDirection:  boolean
}

export interface HumidifierEntity extends BaseEntity {
  type: 'humidifier'
  isOn:            boolean
  targetHumidity:  number | null   // 0–100 %
  currentHumidity: number | null
  mode:            string | null
  modes:           string[]
}

export interface ValveEntity extends BaseEntity {
  type: 'valve'
  isOpen:          boolean
  position:        number | null
  supportsPosition: boolean
}

export interface SensorEntity extends BaseEntity {
  type:        'sensor'
  value:       string
  unit:        string | null
  deviceClass: 'temperature' | 'humidity' | null
}

export interface AlarmEntity extends BaseEntity {
  type:           'alarm'
  alarmState:     string    // disarmed / armed_home / armed_away / armed_night / pending / triggered
  supportedModes: string[]  // subset of the 5 arm modes based on supported_features bitmask
}

export interface CameraEntity extends BaseEntity {
  type: 'camera'
}

export interface SceneEntity extends BaseEntity {
  type: 'scene'
}

export type AppEntity =
  | LightEntity | SwitchEntity | ClimateEntity | CoverEntity | LockEntity
  | FanEntity | HumidifierEntity | ValveEntity | SensorEntity | AlarmEntity
  | CameraEntity | SceneEntity

// ─── Menu data ────────────────────────────────────────────────────────────────

export interface Room {
  areaId:   string
  name:     string
  entities: AppEntity[]
}

export interface MenuData {
  favorites:         AppEntity[]
  rooms:             Room[]
  scenes:            SceneEntity[]
  cameras:           CameraEntity[]
  tempUnit:          '°C' | '°F'
  notificationCount: number
  updateCount:       number
}

// ─── App config ───────────────────────────────────────────────────────────────

export interface HAEnvironment {
  id:      string
  name:    string
  haUrl:   string
  haToken: string
}

export interface AppConfig {
  haUrl:          string
  haToken:        string
  camerasEnabled: boolean
  launchAtLogin:  boolean
  activeEnvId:    string | null
}

export type ConnectionStatus = 'connected' | 'connecting' | 'disconnected' | 'error'

// ─── IPC API surfaces ─────────────────────────────────────────────────────────

export interface PopupAPI {
  menu: {
    getData:  ()                                         => Promise<MenuData | null>
    onUpdate: (cb: (data: MenuData) => void)             => () => void
  }
  ha: {
    callService: (domain: string, service: string, entityId: string,
                  data?: Record<string, unknown>)        => Promise<void>
    reconnect:   ()                                      => void
    getStatus:   ()                                      => Promise<ConnectionStatus>
    onStatusChange: (cb: (s: ConnectionStatus) => void) => () => void
  }
  favorites: {
    get:    ()                    => Promise<string[]>
    add:    (entityId: string)    => Promise<void>
    remove: (entityId: string)    => Promise<void>
  }
  cameras: {
    getSnapshot: (entityId: string) => Promise<string | null>
  }
  accessories: {
    getAreaIcons: () => Promise<Record<string, string>>
  }
  environments: {
    getAll:      () => Promise<HAEnvironment[]>
    getActiveId: () => Promise<string | null>
    connect:     (id: string) => Promise<void>
  }
  window: {
    hide:         ()                        => void
    quit:         ()                        => void
    openSettings: ()                        => void
    openHaUrl:    ()                        => void
    resize:       (h: number, w?: number)   => void
  }
}

export interface SettingsAPI {
  ha: {
    onStatusChange: (cb: (s: ConnectionStatus) => void) => () => void
  }
  config: {
    get:               ()                                         => Promise<AppConfig>
    setHACredentials:  (url: string, token: string)               => Promise<void>
    testConnection:    (url: string, token: string)
      => Promise<{ ok: boolean; entityCount?: number; error?: string }>
    setLaunchAtLogin:  (enabled: boolean)                         => Promise<void>
    setCamerasEnabled: (enabled: boolean)                         => Promise<void>
    disconnect:        ()                                         => Promise<void>
  }
  environments: {
    getAll:      () => Promise<HAEnvironment[]>
    getActiveId: () => Promise<string | null>
    add:         (name: string, url: string, token: string) => Promise<HAEnvironment>
    update:      (env: HAEnvironment)                       => Promise<void>
    remove:      (id: string)                               => Promise<void>
    connect:     (id: string)                               => Promise<void>
    test:        (url: string, token: string)
      => Promise<{ ok: boolean; entityCount?: number; error?: string }>
  }
  accessories: {
    getAll:         ()                                  => Promise<AppEntity[]>
    getRooms:       ()                                  => Promise<Array<{ areaId: string; name: string }>>
    getHidden:      ()                                  => Promise<string[]>
    setHidden:      (ids: string[])                     => Promise<void>
    getRoomOrder:   ()                                  => Promise<string[]>
    setRoomOrder:   (areaIds: string[])                 => Promise<void>
    getDeviceOrder: (areaId: string)                    => Promise<string[]>
    setDeviceOrder: (areaId: string, ids: string[])     => Promise<void>
    getAreaIcons:      ()                                  => Promise<Record<string, string>>
    setAreaIcon:       (areaId: string, icon: string|null) => Promise<void>
    getFavorites:      ()                                  => Promise<string[]>
    setFavoritesOrder: (ids: string[])                     => Promise<void>
  }
  window: {
    close: () => void
  }
}
