// 后端暂无「列表」端点的资源(测试计划 / 压测报告),用 localStorage 维护一份
// 按项目隔离的本地注册表,让前端有可浏览的列表。后端补 list 端点后可平滑替换。

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
