import type { ActivityMember, ActivityReviewPolicy, TeamActivityData } from './api'
import { dateKey } from './codeActivity'
import { lineCount } from './activityModel'
export const defaultReviewPolicy: ActivityReviewPolicy = Object.fromEntries(['code', 'product', 'test'].map(role => [role, { enabled: true, inactiveDays: 3, activeWindowDays: 7, minActiveDays: 2 }])) as ActivityReviewPolicy
export const memberLabel = (member: ActivityMember) => member.displayName || member.name || '未识别作者'
export function attribution(member: ActivityMember) { return member.attribution || (member.name ? 'person' : 'unknownAuthor') }
export interface ActivityReview { reasons: string[]; summary: string; activeDays: number; count: number; changedLines: number }
export function reviewMember(data: TeamActivityData, member: ActivityMember): ActivityReview | undefined {
  const rule = (data.reviewPolicy || defaultReviewPolicy)[data.role]
  if (!rule.enabled || !data.coverage.complete || member.unknownActivityTimes || attribution(member) === 'unknownAuthor') return
  const events = data.events.filter(e => e.actor === member.name)
  if (!member.inRoster && attribution(member) !== 'person' && !events.length) return
  const last = member.lastActivityAt ? dateKey(member.lastActivityAt) : null
  const gap = last ? Math.max(0, Math.floor((Date.parse(`${data.through}T00:00:00Z`) - Date.parse(`${last}T00:00:00Z`)) / 86400000)) : data.days
  const sparseFrom = new Date(Date.parse(`${data.through}T00:00:00Z`) - (rule.activeWindowDays - 1) * 86400000).toISOString().slice(0, 10)
  const sparseDays = new Set(events.filter(e => e.date >= sparseFrom).map(e => e.date)).size
  const activity = data.role === 'code' ? '代码提交' : data.role === 'product' ? '需求活动' : '测试活动'
  const reasons: string[] = []
  if (rule.inactiveDays > 0 && data.days >= rule.inactiveDays && gap >= rule.inactiveDays) reasons.push(last ? `最近一次已采集${activity}距今 ${gap} 天` : `最近 ${rule.inactiveDays} 天无已采集${activity}`)
  if (rule.minActiveDays > 0 && data.days >= rule.activeWindowDays && sparseDays < rule.minActiveDays) reasons.push(`最近 ${rule.activeWindowDays} 天仅 ${sparseDays} 个活跃日（规则至少 ${rule.minActiveDays} 天）`)
  if (!reasons.length) return
  const activeDays = new Set(events.map(e => e.date)).size
  const summary = `本期 ${events.length} 条记录 · ${activeDays} 个活跃日${data.role === 'code' ? ` · 新增 +${lineCount(events, 'additions')} / 删除 −${lineCount(events, 'deletions')} 行` : ''}`
  return { reasons, summary, activeDays, count: events.length, changedLines: events.reduce((n, e) => n + (e.additions || 0) + (e.deletions || 0), 0) }
}
