// Resources with no backend list endpoint yet (test plans / perf reports): keep a per-project
// local registry in localStorage so the frontend has a browsable list. Swap out once the
// backend adds list endpoints.

export interface RegItem {
  id: string
  label: string
  createdAt: number
  meta?: Record<string, string>
}

function key(kind: string, projectId: string) {
  return `shepherd.reg.${kind}.${projectId || 'global'}`
}

export function regList(kind: string, projectId: string): RegItem[] {
  try {
    const raw = localStorage.getItem(key(kind, projectId))
    return raw ? (JSON.parse(raw) as RegItem[]) : []
  } catch {
    return []
  }
}

export function regAdd(kind: string, projectId: string, item: RegItem): RegItem[] {
  const list = [item, ...regList(kind, projectId).filter((x) => x.id !== item.id)]
  localStorage.setItem(key(kind, projectId), JSON.stringify(list))
  return list
}

export function regRemove(kind: string, projectId: string, id: string): RegItem[] {
  const list = regList(kind, projectId).filter((x) => x.id !== id)
  localStorage.setItem(key(kind, projectId), JSON.stringify(list))
  return list
}
