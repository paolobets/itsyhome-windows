/**
 * Shared room icon utilities – used by both popup and settings renderers.
 * Keeping the logic in one place ensures Settings and the popup always show
 * the same default icon for a given area name.
 */

// ─── Icon palette (60 icons, 6 per category) ─────────────────────────────────
export const AREA_ICONS: string[] = [
  // Living areas
  '🛋️', '🏠', '🏡', '🏘️', '🏰', '🏖️',
  // Kitchen & dining
  '🍳', '🍽️', '☕', '🥂', '🫕', '🍪',
  // Bedroom & sleep
  '🛏️', '🌙', '🧸', '👶', '😴', '🌟',
  // Bathroom
  '🚿', '🛁', '🪥', '🪞', '🧴', '🧼',
  // Office & study
  '🖥️', '💻', '📚', '📝', '🎓', '🔬',
  // Garden & outdoor
  '🌳', '🌿', '🪴', '☀️', '🌺', '🌻',
  // Entertainment
  '🎮', '📺', '🎬', '🎭', '🎵', '🎸',
  // Exercise & wellness
  '💪', '🏋️', '🧘', '🏊', '🚴', '⚽',
  // Utility & storage
  '🚗', '🚪', '📦', '🫧', '🧹', '🔧',
  // Misc / special
  '🔒', '💡', '🔑', '🕯️', '🌡️', '💧',
]

// ─── Default icon by area name ────────────────────────────────────────────────
/**
 * Returns a default emoji icon based on the area name.
 * Italian and English names are both recognised.
 * Note: "studio" is treated as the Italian "studio/ufficio" (🖥️), not a
 * music studio, since this app targets Italian Home Assistant setups.
 */
export function roomIcon(name: string): string {
  const n = name.toLowerCase()

  // Living room
  if (n.includes('living') || n.includes('soggiorno') || n.includes('salotto')) return '🛋️'

  // Kitchen
  if (n.includes('kitchen') || n.includes('cucina')) return '🍳'

  // Bedroom – specific first
  if (n.includes('master') || n.includes('main bedroom'))   return '🌙'
  if (n.includes('cameretta'))                               return '🧸'   // kids' room
  if (n.includes('nursery') || n.includes('baby'))          return '🧸'
  if (n.includes('bedroom') || n.includes('guest') ||
      n.includes('camera da letto') || n.includes('camera')) return '🛏️'

  // Bathroom
  if (n.includes('bath') || n.includes('bagno') || n.includes('bagnetto') ||
      n.includes('wc')   || n.includes('toilette'))          return '🚿'

  // Office / study  ("studio" = Italian for study, maps to 🖥️)
  if (n.includes('office') || n.includes('ufficio') || n.includes('studio')) return '🖥️'

  // Hallway / entrance
  if (n.includes('hall') || n.includes('corridor') || n.includes('entryway') ||
      n.includes('ingresso') || n.includes('corridoio') ||
      n.includes('disimpegno') || n.includes('entrata'))     return '🚪'

  // Garage
  if (n.includes('garage') || n.includes('box'))             return '🚗'

  // Garden / outdoor
  if (n.includes('garden') || n.includes('yard') ||
      n.includes('giardino') || n.includes('esterno'))        return '🌳'

  // Terrace / balcony
  if (n.includes('terrace') || n.includes('balcony') ||
      n.includes('patio')   || n.includes('terrazzo') ||
      n.includes('terrazza') || n.includes('balcone'))        return '☀️'

  // Gym / fitness
  if (n.includes('gym') || n.includes('fitness') || n.includes('palestra')) return '💪'

  // Pool / spa
  if (n.includes('pool') || n.includes('spa') || n.includes('piscina'))     return '🏊'

  // Laundry
  if (n.includes('laundry') || n.includes('lavanderia'))     return '🫧'

  // Dining room
  if (n.includes('dining')      || n.includes('pranzo') ||
      n.includes('sala pranzo') || n.includes('tinello'))    return '🍽️'

  // Attic / basement / storage
  if (n.includes('attic') || n.includes('basement') || n.includes('cantina') ||
      n.includes('ripostiglio') || n.includes('mansarda') || n.includes('soffitta')) return '📦'

  return '🏠'
}
