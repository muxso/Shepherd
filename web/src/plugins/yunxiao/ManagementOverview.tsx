import { useI18n } from '../../i18n'
import { useState } from 'react'
import { Card, Empty, Segmented, Tag } from 'antd'
import { useNavigate } from 'react-router-dom'
import type { DevopsDashboard } from './api'
import { ManagementBars, ManagementTrend } from './ManagementCharts'
import { dateKey } from './codeActivity'
import './management.css'
export default function ManagementOverview({ data, sourceId, days }: { data: DevopsDashboard | null; sourceId: string; days: number }) {
  const { t } = useI18n()
  const [statusScope,setStatusScope] = useState('active')
  const navigate = useNavigate(), metrics = data?.productivity?.metrics
  const records = (kind: string, extra: Record<string, string> = {}) => navigate(`/devops?${new URLSearchParams({ view: 'records', source: sourceId, kind, days: String(days), ...extra })}`)
  const metricRoute = (metric: string, date: string) => records(metric === 'bugs' ? 'bug' : 'requirement', { metric, date })
  const groups = new Map<string, number>(); for (const author of data?.authors || []) groups.set(author.group || '待归属', (groups.get(author.group || '待归属') || 0) + author.commits)
  const status = (kind: string) => (data?.statuses || []).filter(s => s.kind === kind && (statusScope === 'all' || !['已发布','已交付','已完成','已关闭','已取消','已拒绝','已验收','完成'].includes(s.status))).sort((a, b) => b.count - a.count).map(s => ({ name: s.status || '未记录', value: s.count }))
  const capture = data?.source?.last_success_at, cutoff = capture && Number.isFinite(Date.parse(capture)) ? dateKey(capture) : undefined
  const series = (keys: string[], colors: string[]) => keys.map((key, i) => ({ key, label: ({ requirements: t("engineering.newRequirements"), delivered: t("engineering.releasedRequirements"), bugs: t("engineering.newDefects") } as Record<string, string>)[key], color: colors[i], points: (metrics?.[key]?.trend || []).filter(p => !cutoff || p.date <= cutoff) }))
  if (!data) return <Empty description={t("engineering.loadingTheEngineeringOverview")} />
  return <section aria-label={t("engineering.engineeringOverview")}>
    <div className="yx-management-scope"><span>{data.productivity ? t("engineering.v0V1Shanghai", undefined, { v0: data.productivity.from, v1: data.productivity.through }) : t("engineering.lastV0Days", undefined, { v0: days })}</span><span>{t("engineering.reviewDeliveryTrendsAndCurrentRisks")}</span></div>
    <div className="yx-management-summary ms-summary-motion">{[
      { label: t("engineering.releasedThisPeriod"), value: metrics?.delivered?.current ?? data.totals.delivered, note: t("engineering.byReleaseStatusDate"), action: () => records('requirement', { metric: 'delivered', period: 'true' }) },
      { label: t("engineering.newRequirementsThisPeriod"), value: metrics?.requirements?.current, note: t("engineering.byCreationDate"), action: () => records('requirement', { metric: 'requirements', period: 'true' }) },
      { label: t("engineering.newDefectsThisPeriod"), value: metrics?.bugs?.current, note: t("engineering.openDefectStatisticsForDetails"), action: () => navigate(`/devops?${new URLSearchParams({ view: 'bugs', source: sourceId, days:String(days) })}`) },
      { label: t("engineering.currentlyOverdue"), value: data.totals.delayed, note: t("engineering.urgentHighAndMediumExcludingPendingRelease"), action: () => records('requirement', { delayedOnly: 'true' }) },
    ].map(item => <button type="button" key={item.label} onClick={item.action}><span>{item.label}</span><strong>{item.value?.toLocaleString() ?? t("engineering.awaitingCollection")}</strong><small>{item.note}</small></button>)}</div>
    <div className="yx-management-grid-layout ms-summary-motion">
      <Card size="small" title={t("engineering.requirementDeliveryTrends")} extra={<Tag>{t("engineering.shanghaiCalendarDays")}</Tag>}><ManagementTrend title={t("engineering.newAndReleasedRequirements")} series={series(['requirements', 'delivered'], ['var(--brand)', '#65a89c'])} partialDate={cutoff} onSelect={metricRoute} /></Card>
      <Card size="small" title={statusScope === 'active' ? t("engineering.requirementsInProgress") : t("engineering.allRequirementsByStatus")} extra={<Segmented size="small" value={statusScope} onChange={v=>setStatusScope(String(v))} options={[{value:'active',label:t("engineering.inProgress")},{value:'all',label:t("engineering.all")}]} />}><ManagementBars items={status('requirement')} unit=" 项" onSelect={value => records('requirement', { status: value })} limit={7} /></Card>
      <Card size="small" title={t("engineering.defectIntakeTrends")}><ManagementTrend title={t("engineering.newDefectsOverTime")} series={series(['bugs'], ['#d69660'])} partialDate={cutoff} onSelect={metricRoute} /></Card>
      <Card size="small" title={t("engineering.codeActivityByGroup")} extra={<Tag>滚动 {days} 天</Tag>}><ManagementBars items={[...groups].map(([name, value]) => ({ name, value })).sort((a, b) => b.value - a.value)} unit=" 次" onSelect={group => records('commit', { group })} limit={7} /></Card>
    </div>
  </section>
}
