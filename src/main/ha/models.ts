/** Raw types returned by the Home Assistant REST/WS API. */

export interface HAState {
  entity_id:    string
  state:        string
  attributes:   Record<string, unknown>
  last_changed: string
  last_updated: string
  context:      { id: string; parent_id: string | null; user_id: string | null }
}

export interface HAArea {
  area_id: string
  name:    string
}

export interface HADevice {
  id:           string
  name:         string
  name_by_user: string | null
  area_id:      string | null
  disabled_by:  string | null
}

export interface HAEntityEntry {
  entity_id:       string
  name:            string | null
  platform:        string
  device_id:       string | null
  area_id:         string | null
  disabled_by:     string | null
  hidden_by:       string | null
  icon:            string | null
  entity_category: string | null
}

export interface HAConfig {
  unit_system:   { temperature: string }
  location_name: string
}

// WebSocket message types
export type WSMessage =
  | { type: 'auth_required'; ha_version: string }
  | { type: 'auth_ok' }
  | { type: 'auth_invalid'; message: string }
  | { type: 'result'; id: number; success: boolean; result: unknown; error?: { code: string; message: string } }
  | { type: 'event'; id: number; event: { event_type: string; data: unknown } }
  | { type: 'pong'; id: number }
