import { useI18n } from '../../i18n'
import { useEffect, useMemo, useState } from 'react'
import { Alert, Button, Card, Empty, Progress, Segmented, Select, Space, Statistic, Typography } from 'antd'
import { useSearchParams } from 'react-router-dom'
import { ApiError, request } from '../../api'
import { AccessDenied } from '../../components/Page'
import GroupedBars from '../../components/GroupedBars'
import NotificationSettings from './NotificationSettings'
import QualityTrend from './QualityTrend'
import RepositoryAnalyses from './RepositoryAnalyses'
import type { SonarAnalysis } from '../yunxiao/api'
import './quality-dashboard.css'

type Dashboard = {
  source: string | null
  sources: { id: string; name: string }[]
  items: SonarAnalysis[]
  truncated: boolean
  canManageNotifications?: boolean
}

const shanghaiDate = new Intl.DateTimeFormat('sv-SE', { timeZone: 'Asia/Shanghai' })
const gateType = (row: SonarAnalysis) => row.gate === 'OK' ? 'passed' : row.gate === 'ERROR' ? 'failed' : 'other'
const repositoryLabel = (row: SonarAnalysis) => row.repositoryName || row.projectKey || row.repositoryId
const personLabel = (row: SonarAnalysis) => row.personName || (row.scope === 'ci_pull_request' ? row.author : row.push?.author || row.commitAuthor)
const options = (entries: Map<string, string>) =>
  [...entries].sort((a, b) => a[1].localeCompare(b[1], 'zh-CN')).map(([value, label]) => ({ value, label }))

function periodDate(date: string, weekly: boolean) {
  if (!weekly) return date
  const monday = new Date(date + 'T00:00:00Z')
  monday.setUTCDate(monday.getUTCDate() - (monday.getUTCDay() + 6) % 7)
  return monday.toISOString().slice(0, 10)
}

export default function QualityDashboard() {
  const { t } = useI18n()
  const series = [
    { key: 'passed', label: t("engineering.passed"), color: 'var(--success)', gradientEnd: 'var(--success-soft)' },
    { key: 'failed', label: t("engineering.failed"), color: 'var(--error)', gradientEnd: 'var(--error-soft)' },
    { key: 'other', label: t("engineering.notRatedWarning"), color: 'var(--text-3)' },
  ]
  const [params, setParams] = useSearchParams()
  const source = params.get('source') ?? ''
  const days = params.get('days') === '7' ? 7 : params.get('days') === '90' ? 90 : 30
  const [data, setData] = useState<Dashboard>()
  const [loading, setLoading] = useState(true)
  const [failure, setFailure] = useState<'denied' | 'error'>()
  const [repository, setRepository] = useState<string>()
  const [person, setPerson] = useState<string>()
  const [gate, setGate] = useState<string>()
  const [notificationSource, setNotificationSource] = useState<string>()
  const [interval, setInterval] = useState<'day' | 'week'>()
  const weekly = interval ? interval === 'week' : days === 90

  const query = (key: string, value: string) => {
    setRepository(undefined)
    setPerson(undefined)
    setParams(current => {
      const next = new URLSearchParams(current)
      next.set(key, value)
      return next
    }, { replace: true })
  }

  useEffect(() => {
    let live = true
    setLoading(true)
    setFailure(undefined)
    const search = new URLSearchParams({ days: String(days), ...(source ? { source } : {}) })
    request<Dashboard>('GET', '/api/sonar/dashboard?' + search)
      .then(result => { if (live) setData(result) })
      .catch(error => {
        if (!live) return
        setData(undefined)
        setFailure(error instanceof ApiError && error.status === 403 ? 'denied' : 'error')
      })
      .finally(() => { if (live) setLoading(false) })
    return () => { live = false }
  }, [days, source])

  const repositories = useMemo(() => options(new Map(data?.items.map(row => [row.repositoryId, repositoryLabel(row)]))), [data])
  const people = useMemo(() => {
    const names = new Map<string, string>()
    data?.items.forEach(row => {
      const name = personLabel(row)
      if (name) names.set(name, name)
    })
    return options(names)
  }, [data])
  const items = useMemo(() => (data?.items ?? []).filter(row =>
    (!repository || row.repositoryId === repository) &&
    (!person || personLabel(row) === person) && (!gate || gateType(row) === gate)
  ), [data, repository, person, gate])
  const passed = items.filter(row => row.gate === 'OK').length
  const failed = items.filter(row => row.gate === 'ERROR').length
  const latestByRepository = useMemo(() => {
    const latest = new Map<string, SonarAnalysis>()
    for (const row of items) {
      if (row.scope === 'sonar_repository' && !latest.has(row.repositoryId)) latest.set(row.repositoryId, row)
    }
    return latest
  }, [items])
  const trend = useMemo(() => {
    const dates = new Map<string, Record<string, number>>()
    items.forEach(row => {
      const date = new Date(row.analyzedAt)
      if (!Number.isFinite(date.getTime())) return
      const day = periodDate(shanghaiDate.format(date), weekly)
      const values = dates.get(day) || { passed: 0, failed: 0, other: 0 }
      values[gateType(row)]++
      dates.set(day, values)
    })
    return [...dates].sort(([a], [b]) => a.localeCompare(b)).map(([date, values]) => ({ name: date.slice(5), values }))
  }, [items, weekly])
  const gateCounts = { passed, failed }
  const rated = passed + failed
  const failedRepositories = [...latestByRepository.values()].filter(row => gateType(row) === 'failed')

  return <main className="sonar-dashboard">
    <header className="sonar-dashboard-header">
      <Typography.Title level={3}>{t("engineering.codeQuality")}</Typography.Title>
      <Space wrap>
        <Segmented aria-label={t("engineering.scanDateRange")} value={days} options={[
          { label: t("engineering.last7Days"), value: 7 },
          { label: t("engineering.last30Days"), value: 30 },
          { label: t("engineering.last90Days"), value: 90 },
        ]} onChange={value => { setInterval(undefined); query('days', String(value)) }} />
        {data?.canManageNotifications && data.source && <Button onClick={() => setNotificationSource(data.source ?? undefined)}>
          {t('sonar.notify.title')}
        </Button>}
      </Space>
    </header>
    {notificationSource && <NotificationSettings key={notificationSource} source={notificationSource} onClose={() => setNotificationSource(undefined)} />}
    {failure === 'denied' ? <Card size="small"><AccessDenied title={t("engineering.codeQualityPermissionRequired")} description={t("engineering.askAnAdministratorToGrantTheCodeQualityReadPermissionToYourUserGroup")} /></Card> : <>
      {failure && <Alert type="error" showIcon message={t("engineering.couldNotLoadSonarqubeAnalyses")} />}
      <div className="sonar-dashboard-filters">
        {(data?.sources.length ?? 0) > 1 && <Select aria-label={t("engineering.sonarDataSource")} value={data?.source}
          options={data?.sources.map(option => ({ value: option.id, label: option.name }))}
          onChange={value => query('source', value || '')} />}
        <Select aria-label={t("engineering.filterByRepository")} placeholder={t("engineering.repository")} value={repository} options={repositories} onChange={setRepository} allowClear showSearch optionFilterProp="label" />
        <Select aria-label={t("engineering.filterByPerson")} placeholder={t("engineering.person")} value={person} options={people} onChange={setPerson} allowClear showSearch optionFilterProp="label" />
        <Select aria-label={t("engineering.filterByQualityGate")} placeholder={t("engineering.qualityGate")} value={gate} options={series.map(item => ({ value: item.key, label: item.label }))} onChange={setGate} allowClear />
      </div>
      {data?.truncated && <Alert type="info" showIcon message={t("engineering.showingTheLatest1000AnalysesInThisRangeShortenTheRangeToFocusOnRecentResults")} />}
      <div className="sonar-dashboard-kpis ms-summary-motion">
        <Card size="small" loading={loading}><Statistic title={t("engineering.analyses")} value={items.length} /></Card>
        <Card size="small" loading={loading}><Statistic title={t("engineering.repositories")} value={new Set(items.map(row => row.repositoryId)).size} /></Card>
        <Card className="sonar-gate-kpi" size="small" loading={loading}>
          <Statistic title={t("engineering.gatePassRate")} value={rated ? (100 * passed / rated).toFixed(1) + '%' : '—'} />
          <Typography.Text type="secondary">{t('engineering.gateSummary', undefined, gateCounts)}</Typography.Text>
        </Card>
        <Card className="sonar-failed-kpi" size="small" loading={loading}>
          <Statistic title={t('sonar.analysis.failedAnalyses')} value={failed} />
          <Typography.Text type="secondary">{t('sonar.analysis.failedRepositoriesCount', undefined, { count: failedRepositories.length })}</Typography.Text>
        </Card>
      </div>
      <Card className="sonar-dashboard-visuals" title={t('sonar.analysis.gateTrend')} size="small" loading={loading}
        extra={<Segmented<'day' | 'week'> size="small" aria-label={t('sonar.analysis.granularity')} value={weekly ? 'week' : 'day'}
          options={[{ value: 'day', label: t('sonar.analysis.byDay') }, { value: 'week', label: t('sonar.analysis.byWeek') }]}
          onChange={value => setInterval(value)} />}>
        <div className="sonar-analysis-split ms-summary-motion">
          <section className="sonar-analysis-chart">
            {trend.length ? <GroupedBars key={weekly ? 'week' : 'day'} series={series} rows={trend} height={260} /> :
              <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t("engineering.noAnalyses")} />}
          </section>
          <aside className="sonar-analysis-side">
            <Typography.Title level={5}>{t("engineering.latestAnalysisByRepository")}</Typography.Title>
            {latestByRepository.size ? <div className="sonar-repo-status">
              {series.map(item => {
                const count = [...latestByRepository.values()].filter(row => gateType(row) === item.key).length
                return <div key={item.key}><div><span>{item.label}</span><strong>{t('engineering.repositoryCount', undefined, { count })}</strong></div>
                  <Progress percent={100 * count / latestByRepository.size} showInfo={false}
                    strokeColor={item.gradientEnd ? { '0%': item.gradientEnd, '100%': item.color } : item.color} trailColor="var(--panel-2)" /></div>
              })}
            </div> : <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t("engineering.noRepositoryAnalyses")} />}
            {!!failedRepositories.length && <div className="sonar-repo-attention">
              <Typography.Text strong>{t('sonar.analysis.recentFailedRepositories')}</Typography.Text>
              <ul>
                {failedRepositories.slice(0, 4).map(row => (
                  <li key={row.repositoryId}>
                    <Button type="link" size="small" title={repositoryLabel(row)} onClick={() => setRepository(row.repositoryId)}>
                      {repositoryLabel(row)}
                    </Button>
                  </li>
                ))}
              </ul>
            </div>}
          </aside>
        </div>
      </Card>
      <QualityTrend items={items} loading={loading} />
      <RepositoryAnalyses source={data?.source ?? ''} items={items} loading={loading} repository={repository} onRepositoryChange={setRepository} />
    </>}
  </main>
}
