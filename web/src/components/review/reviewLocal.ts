// Local module tree for case reviews: modules have no backend table, so they live in
// localStorage (shepherd.reviewModules.<projectId>), while each review stores its
// moduleId backend-side (ms_case_review.module_id).

export interface ReviewModule {
  id: string
  name: string
  parentId: string | null
}

const modKey = (projectId: string) => `shepherd.reviewModules.${projectId || 'global'}`

export function reviewModules(projectId: string): ReviewModule[] {
  try {
    const raw = localStorage.getItem(modKey(projectId))
    return raw ? (JSON.parse(raw) as ReviewModule[]) : []
  } catch {
    return []
  }
}

function saveModules(projectId: string, list: ReviewModule[]): ReviewModule[] {
  localStorage.setItem(modKey(projectId), JSON.stringify(list))
  return list
}

export function reviewModuleAdd(projectId: string, name: string, parentId: string | null): ReviewModule[] {
  const m: ReviewModule = { id: `m_${Date.now().toString(36)}${Math.random().toString(36).slice(2, 7)}`, name, parentId }
  return saveModules(projectId, [...reviewModules(projectId), m])
}

export function reviewModuleRename(projectId: string, id: string, name: string): ReviewModule[] {
  return saveModules(projectId, reviewModules(projectId).map((m) => (m.id === id ? { ...m, name } : m)))
}

/** Remove a module and its whole subtree; returns removed ids so the caller can treat owned reviews as unfiled. */
export function reviewModuleRemove(projectId: string, id: string): { modules: ReviewModule[]; removedIds: string[] } {
  const all = reviewModules(projectId)
  const removedIds = moduleSubtreeIds(all, id)
  return { modules: saveModules(projectId, all.filter((m) => !removedIds.includes(m.id))), removedIds }
}

/** Module id + all descendant ids ("select parent includes children" — same rule for filtering and counts). */
export function moduleSubtreeIds(modules: ReviewModule[], id: string): string[] {
  const walk = (mid: string): string[] => [mid, ...modules.filter((m) => (m.parentId || null) === mid).flatMap((c) => walk(c.id))]
  return walk(id)
}

/** Whether a review falls under the selected tree key (ALL; UNFILED; <id> = that module's whole subtree). */
export function inReviewModule(modules: ReviewModule[], selectedKey: string, moduleId?: string | null): boolean {
  const known = moduleId && modules.some((m) => m.id === moduleId)
  if (selectedKey === 'ALL') return true
  if (selectedKey === 'UNFILED') return !known
  return known ? moduleSubtreeIds(modules, selectedKey).includes(moduleId) : false
}

/** Slash path from root to the module ('' when unknown → caller renders /未规划评审). */
export function modulePathOf(modules: ReviewModule[], id?: string | null): string {
  let cur = id ? modules.find((m) => m.id === id) : undefined
  const parts: string[] = []
  while (cur) {
    parts.unshift(cur.name)
    cur = cur.parentId ? modules.find((m) => m.id === cur!.parentId) : undefined
  }
  return parts.length ? `/${parts.join('/')}` : ''
}
