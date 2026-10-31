import type { ApiModule } from '../api'

// Build counts once, rather than scanning all cases for every ancestor node.
export function moduleIndex<T>(modules: ApiModule[], items: T[], getModuleId: (item: T) => string) {
  const children = new Map<string | null, ApiModule[]>()
  for (const m of modules) {
    const parent = m.parentId || null
    const siblings = children.get(parent) || []
    siblings.push(m); children.set(parent, siblings)
  }
  const direct = new Map<string, number>()
  let unfiled = 0
  for (const item of items) {
    const id = getModuleId(item)
    if (!id) unfiled++
    else direct.set(id, (direct.get(id) || 0) + 1)
  }
  const counts = new Map<string, number>()
  const visiting = new Set<string>()
  const total = (id: string): number => {
    if (counts.has(id)) return counts.get(id)!
    if (visiting.has(id)) return 0
    visiting.add(id)
    const n = (direct.get(id) || 0) + (children.get(id) || []).reduce((sum, c) => sum + total(c.id), 0)
    visiting.delete(id); counts.set(id, n)
    return n
  }
  for (const m of modules) total(m.id)
  return { children, counts, unfiled }
}
