import { useMemo, useState } from 'react'
import { Card, Empty, Typography } from 'antd'
import GroupedBars from '../../components/GroupedBars'
import { useI18n } from '../../i18n'
import type { SonarAnalysis } from '../yunxiao/api'

const metrics = [
  { key: 'issues', percentage: false },
  { key: 'acceptedIssues', percentage: false },
  { key: 'coverage', percentage: true },
  { key: 'duplication', percentage: true },
  { key: 'securityHotspots', percentage: false },
] as const
type Metric = typeof metrics[number]['key']
const dateFormat = new Intl.DateTimeFormat('sv-SE', { timeZone: 'Asia/Shanghai' })

function summarize(items: SonarAnalysis[]) {
  const days = new Map<string, Map<string, SonarAnalysis>>()
  for (const row of items) {
    if (row.scope !== 'sonar_repository' || !row.overallQuality) continue
    const timestamp = Date.parse(row.analyzedAt)
    if (!Number.isFinite(timestamp)) continue
    const day = dateFormat.format(timestamp)
    let repositories = days.get(day)
    if (!repositories) days.set(day, repositories = new Map())
    const previous = repositories.get(row.repositoryId)
    if (!previous || timestamp > Date.parse(previous.analyzedAt)) repositories.set(row.repositoryId, row)
  }
  return [...days].sort(([a], [b]) => a.localeCompare(b)).map(([name, repositories]) => {
    const values: Record<string, number> = {}
    for (const { key, percentage } of metrics) {
      const samples = Array.from(repositories.values(), row => row.overallQuality?.[key])
        .filter((value): value is number => typeof value === 'number' && Number.isFinite(value))
      if (!samples.length) continue
      const total = samples.reduce((sum, value) => sum + value, 0)
      values[key] = percentage ? Math.round(total * 10 / samples.length) / 10 : total
    }
    return { name, values }
  })
}

export default function QualityTrend({ items, loading }: { items: SonarAnalysis[]; loading: boolean }) {
  const { t, lang } = useI18n()
  const [selected, setSelected] = useState<Metric>('issues')
  const rows = useMemo(() => summarize(items), [items])
  const latest = rows.at(-1)
  const metric = metrics.find(item => item.key === selected) ?? metrics[0]
  const format = new Intl.NumberFormat(lang, { maximumFractionDigits: 1 })
  const label = (key: Metric) => t('sonar.trend.' + key)
  const display = (value: number, percentage: boolean) => format.format(value) + (percentage ? '%' : '')
  return <Card title={t('sonar.trend.title')} size="small" loading={loading}>
    <Typography.Paragraph type="secondary" className="sonar-trend-note">
      {t('sonar.trend.method')}
    </Typography.Paragraph>
    {latest && <Typography.Paragraph type="secondary" className="sonar-trend-note">
      {t('sonar.trend.latest', undefined, { date: latest.name })}
    </Typography.Paragraph>}
    <div className="sonar-trend-metrics">
      {metrics.map(({ key, percentage }) => <button key={key} type="button" aria-pressed={selected === key} onClick={() => setSelected(key)}>
        <span>{label(key)}</span>
        <strong>{latest?.values[key] == null ? '—' : display(latest.values[key], percentage)}</strong>
      </button>)}
    </div>
    {rows.some(row => row.values[selected] != null)
      ? <GroupedBars key={selected} series={[{ key: selected, label: label(selected), color: 'var(--success)', gradientEnd: 'var(--success-soft)' }]}
        rows={rows} height={220} maxValue={metric.percentage ? 100 : undefined}
        formatValue={value => display(value, metric.percentage)} />
      : <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('sonar.trend.empty')} />}
  </Card>
}
