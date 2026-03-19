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
  cameras:           AppEntity[]
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

// IPC API interfaces removed – use src/lib/api.ts (Tauri adapter) directly.
