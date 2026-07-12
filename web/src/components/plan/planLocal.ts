// Local-state layer for the test-plan page: the backend only has create/run/stats
// endpoints, so the plan list comes from the registry (localStorage). Modules /
// groups / tags have no backend fields and live here:
// - plan module tree: shepherd.planModules.<projectId>, nodes {id,name,parentId}, persisted locally.
// - plan extension fields: registry item meta (all string values):
//   createdBy; kind='group' marks a plan-group entry; groupId = owning plan group;
//   module = owning module id (empty/absent = unfiled); tags = comma-joined tag string.
import type { RegItem } from '../../registry'
import { regList } from '../../registry'

export interface PlanModule {
  id: string
  name: string
  parentId: string | null
}

const modKey = (projectId: string) => `shepherd.planModules.${projectId || 'global'}`

export function planModules(projectId: string): PlanModule[] {
  try {
    const raw = localStorage.getItem(modKey(projectId))
    return raw ? (JSON.parse(raw) as PlanModule[]) : []
  } catch {
    return []
  }
}

function saveModules(projectId: string, list: PlanModule[]): PlanModule[] {
  localStorage.setItem(modKey(projectId), JSON.stringify(list))
  return list
}

export function planModuleAdd(projectId: string, name: string, parentId: string | null): PlanModule[] {
  const m: PlanModule = { id: `m_${Date.now().toString(36)}${Math.random().toString(36).slice(2, 7)}`, name, parentId }
  return saveModules(projectId, [...planModules(projectId), m])
}

export function planModuleRename(projectId: string, id: string, name: string): PlanModule[] {
  return saveModules(projectId, planModules(projectId).map((m) => (m.id === id ? { ...m, name } : m)))
}

/** Remove a module and its whole subtree; returns removed ids so callers can move owned plans back to unfiled. */
export function planModuleRemove(projectId: string, id: string): { modules: PlanModule[]; removedIds: string[] } {
  const all = planModules(projectId)
  const removedIds = moduleSubtreeIds(all, id)
  return { modules: saveModules(projectId, all.filter((m) => !removedIds.includes(m.id))), removedIds }
}

/** Module id + all descendant ids ("select parent includes children" — same rule for filtering and counts). */
export function moduleSubtreeIds(modules: PlanModule[], id: string): string[] {
  const walk = (mid: string): string[] => [mid, ...modules.filter((m) => (m.parentId || null) === mid).flatMap((c) => walk(c.id))]
  return walk(id)
}

/** Whether an item falls under the selected module key (ALL; UNFILED; <id> = that module's whole subtree). */
export function inPlanModule(modules: PlanModule[], selectedKey: string, moduleId: string): boolean {
  if (selectedKey === 'ALL') return true
  if (selectedKey === 'UNFILED') return !moduleId
  return moduleSubtreeIds(modules, selectedKey).includes(moduleId)
}

export function moduleNameOf(modules: PlanModule[], id: string): string {
  return modules.find((m) => m.id === id)?.name || ''
}

// In-place registry item update (regAdd is a move-to-top upsert; edits use this to keep list order).
// Storage key must match src/registry.ts (kind fixed to 'plan').
const regKey = (projectId: string) => `shepherd.reg.plan.${projectId || 'global'}`

export function planRegUpdate(
  projectId: string,
  id: string,
  patch: { label?: string; meta?: Record<string, string> },
): RegItem[] {
  const list = regList('plan', projectId).map((it) =>
    it.id === id ? { ...it, label: patch.label ?? it.label, meta: { ...it.meta, ...patch.meta } } : it,
  )
  localStorage.setItem(regKey(projectId), JSON.stringify(list))
  return list
}

export const isGroup = (p: RegItem) => p.meta?.kind === 'group'
export const moduleOf = (p: RegItem) => p.meta?.module || ''
export const groupIdOf = (p: RegItem) => p.meta?.groupId || ''
export const tagsOf = (p: RegItem): string[] =>
  (p.meta?.tags || '').split(',').map((s) => s.trim()).filter(Boolean)
export const joinTags = (tags: string[]) => tags.map((s) => s.trim()).filter(Boolean).join(',')
