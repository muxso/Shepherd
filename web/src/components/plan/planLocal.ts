// Plan-page state layer. Plans (incl. plan groups) come from GET /test-plan?projectId=
// and are mapped into the RegItem shape the page components consume; edits persist
// via PUT /test-plan/{id}. Only the module tree is still frontend-local:
// - plan module tree: shepherd.planModules.<projectId>, nodes {id,name,parentId}, in localStorage.
// - RegItem meta (all string values): createdBy; kind='group' marks a plan-group row;
//   groupId = owning plan group; module = owning module id (empty = unfiled);
//   tags = comma-joined tag string.
import { api, type PlanListItem } from '../../api'
import type { RegItem } from '../../registry'

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

/** Server plan row → the RegItem shape the plan components render. */
export function planToRegItem(p: PlanListItem): RegItem {
  return {
    id: p.id,
    label: p.name,
    createdAt: p.createdAt,
    meta: {
      createdBy: p.createdBy || '',
      module: p.moduleId || '',
      groupId: p.groupId && p.groupId !== 'NONE' ? p.groupId : '',
      tags: joinTags(p.tags || []),
      ...(p.type === 'GROUP' ? { kind: 'group' as const } : {}),
    },
  }
}

/** Non-archived plans + plan groups of a project, newest first. */
export async function fetchPlanItems(projectId: string): Promise<RegItem[]> {
  const list = await api.listPlans(projectId)
  return list.map(planToRegItem)
}

export const isGroup = (p: RegItem) => p.meta?.kind === 'group'
export const moduleOf = (p: RegItem) => p.meta?.module || ''
export const groupIdOf = (p: RegItem) => p.meta?.groupId || ''
export const tagsOf = (p: RegItem): string[] =>
  (p.meta?.tags || '').split(',').map((s) => s.trim()).filter(Boolean)
export const joinTags = (tags: string[]) => tags.map((s) => s.trim()).filter(Boolean).join(',')
