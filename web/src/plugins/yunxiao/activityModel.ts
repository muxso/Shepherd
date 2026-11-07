import type { ActivityBug, ActivityEvent, ActivityKind, ActivityMember, ActivityRole, TeamActivityData } from './api'
export const activityRoles: { value: ActivityRole; label: string; metrics: { value: ActivityKind; label: string }[] }[] = [
  { value: 'code', label: '研发 · 代码', metrics: [{ value: 'commit', label: '代码提交' }] },
  { value: 'product', label: '产品 · 需求', metrics: [{ value: 'requirementCreated', label: '新增需求' }, { value: 'requirementDelivered', label: '交付需求' }] },
  { value: 'test', label: '测试 · 质量', metrics: [{ value: 'bugReported', label: '发现有效 Bug' }, { value: 'testcaseCreated', label: '新增用例' }] },
]
export const uniqueRequirements = (events: ActivityEvent[]) => [...new Map(events.flatMap(e => e.requirements).map(r => [r.externalId, r])).values()]
export const repositoryKey = (e: ActivityEvent) => String(e.repoId ?? e.repo ?? e.externalId)
export function lineCount(events: ActivityEvent[], field: 'additions' | 'deletions') {
  const known = events.reduce((n, e) => n + (e[field] ?? 0), 0)
  return `${events.some(e => e[field] == null) ? '≥' : ''}${known.toLocaleString()}`
}
export function dayFacts(data: TeamActivityData, member: string, date: string, metric: ActivityKind) {
  const events = data.events.filter(e => e.actor === member && e.date === date && e.kind === metric)
  const bugs = data.bugs.filter(b => b.date === date && b.responsibles.includes(member))
  return { events, bugs, last: events.map(e => e.at).sort((a, b) => Date.parse(a) - Date.parse(b)).at(-1), requirements: uniqueRequirements(events) }
}
export function memberTotals(data: TeamActivityData, member: ActivityMember) {
  const events = data.events.filter(e => e.actor === member.name)
  if (data.role === 'code') return [events.length, uniqueRequirements(events).length, new Set(data.bugs.filter(b => b.responsibles.includes(member.name)).map(b => b.id)).size]
  if (data.role === 'product') return [events.filter(e => e.kind === 'requirementCreated').length, events.filter(e => e.kind === 'requirementDelivered').length, new Set(events.map(e => e.externalId)).size]
  return [events.filter(e => e.kind === 'testcaseCreated').length, events.filter(e => e.kind === 'bugReported').length, events.length]
}
export function shiftDate(date: string, days: number) { const d = new Date(`${date}T00:00:00Z`); d.setUTCDate(d.getUTCDate() + days); return d.toISOString().slice(0, 10) }
export function datesFor(data: TeamActivityData, view: string) {
  if (view === 'month') { const first = data.through.slice(0, 7) + '-01'; const end = new Date(`${first}T00:00:00Z`); end.setUTCMonth(end.getUTCMonth() + 1); end.setUTCDate(0); return Array.from({ length: end.getUTCDate() }, (_, i) => shiftDate(first, i)) }
  const count = Math.min(7, data.days)
  return Array.from({ length: count }, (_, i) => shiftDate(data.through, i - count + 1))
}
export interface RequirementFocus { id: string; serial: string | null; title: string | null; url?: string | null; events: ActivityEvent[]; people: number; lines: number; unknownLines: number; repositories: number; bugs: ActivityBug[] }
export function requirementFocus(data: TeamActivityData): RequirementFocus[] {
  const byId = new Map<string, RequirementFocus>()
  for (const e of data.events.filter(e => e.kind === 'commit')) for (const r of e.requirements) {
    if (!byId.has(r.externalId)) byId.set(r.externalId, { id: r.externalId, serial: r.serial, title: r.title, url: r.url, events: [], people: 0, lines: 0, unknownLines: 0, repositories: 0, bugs: [] })
    byId.get(r.externalId)!.events.push(e)
  }
  for (const r of byId.values()) {
    r.people = new Set(r.events.map(e => e.actor)).size
    r.lines = r.events.reduce((n, e) => n + (e.additions ?? 0) + (e.deletions ?? 0), 0)
    r.unknownLines = r.events.filter(e => e.additions == null || e.deletions == null).length
    r.repositories = new Set(r.events.map(repositoryKey)).size
    r.bugs = data.bugs.filter(b => b.requirements.some(link => link.externalId === r.id))
  }
  return [...byId.values()]
}

export function sourceHref(value?: string | null): string | undefined {
  if (!value || /[\u0000-\u001f\u007f]/.test(value)) return undefined
  try { const url = new URL(value); return ['https:', 'http:'].includes(url.protocol) && !url.username && !url.password ? url.href : undefined } catch { return undefined }
}
