// 测试计划页的本地口径:计划本体后端只有 create/run/stats 等端点,列表来自
// registry(localStorage)。模块 / 分组 / 标签后端无字段,统一放这里:
// - 计划模块树:shepherd.planModules.<projectId>,节点 {id,name,parentId},本地持久化。
// - 计划扩展字段:registry 条目 meta(值均为字符串):
//   createdBy 创建人;kind='group' 表示计划组条目;groupId 计划所属计划组的 id;
//   module 所属模块 id(空/缺省 = 未规划);tags 逗号拼接的标签串。
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

/** 删除模块及其整棵子树;返回被删 id 集,调用方据此把归属计划改回未规划。 */
export function planModuleRemove(projectId: string, id: string): { modules: PlanModule[]; removedIds: string[] } {
  const all = planModules(projectId)
  const removedIds = moduleSubtreeIds(all, id)
  return { modules: saveModules(projectId, all.filter((m) => !removedIds.includes(m.id))), removedIds }
}

/** 模块自身 + 所有后代 id(「选父含子」过滤与计数共用同一口径)。 */
export function moduleSubtreeIds(modules: PlanModule[], id: string): string[] {
  const walk = (mid: string): string[] => [mid, ...modules.filter((m) => (m.parentId || null) === mid).flatMap((c) => walk(c.id))]
  return walk(id)
}

/** 条目是否落在选中模块键下(ALL=全部;UNFILED=未规划;<id>=该模块整棵子树)。 */
export function inPlanModule(modules: PlanModule[], selectedKey: string, moduleId: string): boolean {
  if (selectedKey === 'ALL') return true
  if (selectedKey === 'UNFILED') return !moduleId
  return moduleSubtreeIds(modules, selectedKey).includes(moduleId)
}

export function moduleNameOf(modules: PlanModule[], id: string): string {
  return modules.find((m) => m.id === id)?.name || ''
}

// registry 条目的原位更新(regAdd 是置顶 upsert,编辑用这里保持列表原顺序)。
// 存储 key 与 src/registry.ts 的口径一致(kind 固定 'plan')。
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
