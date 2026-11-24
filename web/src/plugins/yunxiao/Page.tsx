import { ApiError } from '../../api'
import { useI18n } from '../../i18n'
import { useLocation, useNavigate } from 'react-router-dom'
import { useEffect, useState } from 'react'
import { Button, Card, Input, Select, Space, Switch, Table, Tabs, Tag } from 'antd'
import { ReloadOutlined } from '@ant-design/icons'
import ManagementOverview from './ManagementOverview'
import RequirementOutput from './RequirementOutput'
import BugStatistics from './BugStatistics'
import MemberMetrics from './MemberMetrics'
import StatusDwell from './StatusDwell'
import StatisticsNotes from './StatisticsNotes'
import NotificationSettings from './NotificationSettings'
import CodeAuthors from './TeamActivity'
import { AccessDenied, PageBody, PageContainer, ReadError } from '../../components/Page'
import { devopsApi, type DevopsDashboard, type DevopsRow, type DevopsSource } from './api'
const syncTime = new Intl.DateTimeFormat('zh-CN', {
  timeZone: 'Asia/Shanghai', year: 'numeric', month: 'numeric', day: 'numeric',
  hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false,
})
const when = (v?: string) => v ? new Date(v).toLocaleString('zh-CN', { hour12: false }) : '—'
const text = (v: unknown) => v == null || v === '' ? '—' : String(v)
export default function Devops() {
  const { t } = useI18n()
  const kinds = [{ key: 'requirement', label: t("engineering.requirements") }, { key: 'bug', label: t("engineering.defects") }, { key: 'sprint', label: t("engineering.sprint") }, { key: 'testcase', label: t("engineering.testCases") }, { key: 'commit', label: t("engineering.actualCommits") }, { key: 'history', label: t("engineering.statusChanges") }, { key: 'runs', label: t("engineering.syncHistory") }]
  const location = useLocation()
  const navigate = useNavigate()
  const [query] = useState(() => new URLSearchParams(window.location.search))
  const [view, setView] = useState(query.has('reference') ? 'delivery' : query.has('kind') ? 'records' : ['overview','radar','delivery','bugs','authors','dwell','records'].includes(query.get('view') || '') ? query.get('view')! : 'overview')
  const [reference, setReference] = useState(query.get('reference') || '')
  const [metric, setMetric] = useState(query.get('metric') || '')
  const [eventDate, setEventDate] = useState(query.get('date') || '')
  const [sources, setSources] = useState<DevopsSource[]>([]), [selectedSource, setSource] = useState(query.get('source') || '')
  const source = sources.some(item => item.id === selectedSource) ? selectedSource : ''
  const [dashboard, setDashboard] = useState<DevopsDashboard | null>(null), [rows, setRows] = useState<DevopsRow[]>([])
  const [total, setTotal] = useState(0), [kind, setKind] = useState(kinds.some(item => item.key === query.get('kind')) ? query.get('kind')! : 'requirement'), [days, setDays] = useState(Math.min(366,Math.max(1,Number(query.get('days')) || 7)))
  const [status, setStatus] = useState(query.get('status') || '')
  const [group, setGroup] = useState(query.get('group') || ''), [search, setSearch] = useState(query.get('search') || ''), [delayed, setDelayed] = useState(query.get('delayedOnly') === 'true')
  const [page, setPage] = useState(1), [revision, setRevision] = useState(0), [loading, setLoading] = useState(false), [error, setError] = useState<unknown>()
  const [denied, setDenied] = useState(false)
  useEffect(() => {
    const next = new URLSearchParams(location.search)
    if (next.get('source')) setSource(next.get('source')!)
    if (kinds.some(item => item.key === next.get('kind'))) { setKind(next.get('kind')!); setView('records') }
    if (['overview','radar','delivery','bugs','authors','dwell','records'].includes(next.get('view') || '')) setView(next.get('view')!)
    if (next.get('reference')) { setReference(next.get('reference')!); setView('delivery') }
    setSearch(next.get('search') || ''); setStatus(next.get('status') || ''); setGroup(next.get('group') || ''); setDelayed(next.get('delayedOnly') === 'true'); if (next.get('days')) setDays(Math.min(366,Math.max(1,Number(next.get('days')) || 7)))
    setMetric(next.get('metric') || ''); setEventDate(next.get('date') || ''); setPage(1)
  }, [location.search])
  useEffect(() => {
    let live = true
    devopsApi.sources().then(d => {
      if (!live) return
      setDenied(false)
      setSources(d.items)
      setSource(s => d.items.some(item => item.id === s) ? s : d.items[0]?.id || '')
    }).catch(e => {
      if (!live) return
      if (e instanceof ApiError && e.status === 403) {
        setDenied(true)
        setSources([])
        setSource('')
      } else {
        setError(e)
      }
    })
    return () => { live = false }
  }, [revision])
  useEffect(() => {
    if (!source || (view !== 'overview' && (view !== 'records' || metric))) return
    let live = true
    setDashboard(null)
    devopsApi.dashboard(source, days).then(d => { if (live) setDashboard(d) }).catch(e => { if (live) setError(e) })
    return () => { live = false }
  }, [source, days, revision, view, metric])
  useEffect(() => {
    if (!source || view !== 'records') return
    let live = true
    setLoading(true); setError(undefined)
    devopsApi.records(source, { kind, days, group, search, status, page, pageSize: 30, delayedOnly: kind === 'requirement' && delayed, ...(metric && eventDate ? { metric, date: eventDate } : metric && new URLSearchParams(location.search).get('period') === 'true' ? { metric, period: true } : {}) }).then(d => {
      if (live) { setRows(d.items); setTotal(d.total) }
    }).catch(e => { if (live) { setRows([]); setTotal(0); setError(e) } }).finally(() => { if (live) setLoading(false) })
    return () => { live = false }
  }, [source, kind, days, group, search, status, page, delayed, revision, metric, eventDate, view, location.search])
  const groups = Array.from(new Set([...(dashboard?.groups || []),...(dashboard?.authors?.map(x => x.group) || []),...(group ? [group] : [])]))
  const info = sources.find(s => s.id === source)
  const openRequirement = (reference: string) => navigate(`/devops?${new URLSearchParams({ source,reference })}`)
  const requirementLinks = (row: DevopsRow) => row.requirements?.length
    ? <Space size={[4,0]} wrap>{row.requirements.map(item => <Button type="link" size="small" key={item.externalId} onClick={() => openRequirement(item.externalId)}>{item.serial || item.externalId} {item.title || ''}</Button>)}</Space>
    : <Tag>{t("engineering.noLinkedRequirements")}</Tag>
  const columns = kind === 'testcase' ? [
    { title: t("engineering.yunxiaoNumber"), dataIndex: 'serial', width: 140 }, { title: t("engineering.testCaseName"), dataIndex: 'title' }, { title: t("engineering.testCaseLibrary"), dataIndex: 'repo', width: 140 }, { title: t("engineering.folder"), dataIndex: 'directory', width: 200 }, { title: t("engineering.owner"), dataIndex: 'owner', width: 110 }, { title: t("engineering.priority"), dataIndex: 'priority', width: 90 }, { title: t("engineering.steps"), dataIndex: 'steps', width: 90, render: (v?: unknown[]) => v?.length || 0 },
  ] : kind === 'commit' ? [
    { title: t("engineering.commit"), dataIndex: 'sha', width: 115, render: (v: string, r: DevopsRow) => r.url?.startsWith('https://') ? <a href={r.url} target="_blank" rel="noreferrer">{v?.slice(0, 10)}</a> : v?.slice(0, 10) },
    { title: t("engineering.description"), dataIndex: 'title' }, { title: t("engineering.actualAuthor"), dataIndex: 'author', width: 120 }, { title: t("engineering.group"), dataIndex: 'group', width: 140 }, { title: t("engineering.repository"), dataIndex: 'repo' },
    { title: t("engineering.authorDate"), dataIndex: 'authoredAt', width: 175, render: when },
    { title: t("engineering.linesAddedDeleted"), key: 'lines', width: 125, render: (_: unknown, r: DevopsRow) => r.additions == null ? t("engineering.unknownLineCount") : `+${r.additions} / -${r.deletions}` },
    { title: t("engineering.requirementLinks"), key: 'requirements', render: (_: unknown, row: DevopsRow) => requirementLinks(row) },
  ] : kind === 'history' ? [
    { title: t("engineering.type"), dataIndex: 'kind' }, { title: t("engineering.externalId"), dataIndex: 'external_id' }, { title: t("engineering.previousStatus"), dataIndex: 'old_status' }, { title: t("engineering.newStatus"), dataIndex: 'new_status' },
    { title: t("engineering.yunxiaoStatusDate"), dataIndex: 'source_changed_at', render: when }, { title: t("engineering.observedAtSync"), dataIndex: 'observed_at', render: when },
  ] : kind === 'runs' ? [
    { title: t("engineering.status"), dataIndex: 'status', render: (v: string) => <Tag color={v === 'SUCCESS' ? 'green' : v === 'FAILED' ? 'red' : 'blue'}>{v}</Tag> },
    { title: t("engineering.start"), dataIndex: 'started_at', render: when }, { title: t("engineering.end"), dataIndex: 'finished_at', render: when },
    { title: t("engineering.syncedItems"), dataIndex: 'counts', render: (v: unknown) => JSON.stringify(v) }, { title: t("engineering.error"), dataIndex: 'error' },
  ] : [
    { title: t("engineering.yunxiaoNumberId"), key: 'serial', width: 155, render: (_: unknown, r: DevopsRow) => text(r.serial || r.externalId) },
    { title: t("engineering.title"), dataIndex: 'title', width: 400, render: (value: string, row: DevopsRow) => kind === 'requirement' ? <Button type="link" size="small" onClick={() => openRequirement(String(row.externalId))}>{value}</Button> : value }, { title: t("engineering.status"), dataIndex: 'status', width: 110 },
    ...(kind === 'sprint' ? [{ title: t("engineering.start"), dataIndex: 'startAt', render: when }, { title: t("engineering.end"), dataIndex: 'endAt', render: when }] : [
      ...(kind === 'bug' ? [{ title: t("engineering.linkedRequirements"), key: 'requirements', width: 240, render: (_: unknown, row: DevopsRow) => requirementLinks(row) }] : []),
      { title: t("engineering.priority"), dataIndex: 'priority', width: 90, render: (v: string) => <Tag color={v === '紧急' ? 'red' : v === '高' ? 'orange' : undefined}>{text(v)}</Tag> },
      { title: t("engineering.owner"), dataIndex: 'owner', width: 110, render: text }, { title: t("engineering.group"), dataIndex: 'group', width: 130 },
      { title: t("engineering.sprint"), dataIndex: 'sprint', width: 140, render: text }, { title: t("engineering.dueDate"), dataIndex: 'due', width: 120, render: text },
      { title: t("engineering.statusChange"), dataIndex: 'statusChangedAt', width: 175, render: when },
    ]),
    ...(['requirement', 'bug'].includes(kind) ? [{
      title: t("engineering.prQuality"), key: 'quality', width: 160,
      render: (_: unknown, row: DevopsRow) => row.quality?.analyses
        ? <Tag color={row.quality.failed ? 'red' : 'blue'}>{row.quality.analyses} 个 PR · {row.quality.failed} 个未通过</Tag>
        : <Tag>{t("engineering.noLinkedAnalysis")}</Tag>,
    }] : []),
  ]
  if (denied) return <PageContainer>
    <PageBody testId="devops-scroll">
      <Card size="small"><AccessDenied title={t('engineering.devopsReadPermissionRequired')}
        description={t('engineering.askForDevopsReadPermission')} /></Card>
    </PageBody>
  </PageContainer>
  const showPeriodControls = view === 'overview' || view === 'records'
  const lastSynced = info?.last_success_at ? new Date(info.last_success_at) : null
  const sourceControls = <Space size={8} wrap>
      <Select aria-label={t("engineering.yunxiaoProject")} style={{ minWidth: 180 }} value={source || undefined} onChange={v => { setSource(v); setPage(1); setGroup(''); setReference(''); const next = new URLSearchParams(location.search); next.set('source',v); next.delete('bugPerson'); next.delete('bugGroup'); next.delete('reference'); next.delete('group'); next.delete('activityGroup'); next.delete('perspective'); next.delete('chart'); next.delete('compare'); navigate({ pathname:location.pathname,search:next.toString() }) }} options={sources.map(s => ({ value: s.id, label: s.name }))} placeholder={t("engineering.selectDataSource")} />
      <span style={{ visibility: showPeriodControls ? 'visible' : 'hidden' }} aria-hidden={!showPeriodControls}>
        <Select aria-label={t("engineering.dateRange")} value={days} onChange={v => { setDays(v); setPage(1) }}
          options={[7,14,30,90,365].map(v => ({ value:v, label:t("engineering.lastV0Days", undefined, { v0: v }) }))} />
      </span>
      <NotificationSettings source={source} />
      <Button aria-label={t("engineering.refreshData")} icon={<ReloadOutlined />} onClick={() => setRevision(x => x + 1)} />
      <span style={{ visibility: showPeriodControls ? 'visible' : 'hidden' }} aria-hidden={!showPeriodControls}>
        <StatisticsNotes info={info} data={dashboard} />
      </span>
    </Space>
  return <PageContainer>
    <PageBody testId="devops-scroll">
      {error != null && <ReadError error={error} title={t('engineering.devopsReadFailed', '云效研发统计读取失败')} />}
      <Tabs tabBarExtraContent={sourceControls} activeKey={view} onChange={value => { setView(value); setError(undefined); const next = new URLSearchParams(location.search); next.set('view', value); if (source) next.set('source', source); if (value !== 'delivery') next.delete('reference'); if (value !== 'records') { next.delete('kind'); next.delete('metric'); next.delete('date'); next.delete('period') } navigate({ pathname: location.pathname, search: next.toString() }) }} items={[
        { key:'overview', label:t("engineering.overview"), children: <ManagementOverview data={dashboard} sourceId={source} days={days} /> },
        { key:'radar', label:t("engineering.individualMetrics"), children: source && view === 'radar' ? <MemberMetrics key={source} sourceId={source} revision={revision} /> : null },
        { key:'delivery', label:t("engineering.requirementDelivery"), children: source ? <RequirementOutput key={source} sourceId={source} reference={reference} revision={revision} /> : null },
        { key:'bugs', label:t("engineering.defectStatistics"), children: source && view === 'bugs' ? <BugStatistics key={source} sourceId={source} revision={revision} /> : null },
        { key:'authors', label:t("engineering.teamActivity"), children: source && view === 'authors' ? <CodeAuthors key={source} sourceId={source} revision={revision} /> : null },
        { key:'dwell', label:t("engineering.timeInStatus"), children: source && view === 'dwell' ? <StatusDwell key={source} sourceId={source} revision={revision} /> : null },
        { key:'records', label:t("engineering.records"), children: <Card size="small">
      <Tabs activeKey={kind} items={kinds} onChange={v => { setKind(v); setPage(1); setSearch(''); setGroup(''); setStatus(''); setDelayed(false); setMetric(''); setEventDate('') }} />
      {metric && <Tag closable onClose={() => { const next=new URLSearchParams(location.search);next.delete('metric');next.delete('date');next.delete('period');navigate({pathname:location.pathname,search:next.toString()},{replace:true});setEventDate('');setMetric('');setPage(1) }}>{({requirements:t("engineering.newRequirements"),delivered:t("engineering.releasedRequirements"),bugs:t("engineering.newDefects"),testcases:t("engineering.newTestCases"),commits:t("engineering.commits"),linkedCommits:t("engineering.linkedCommits")} as Record<string,string>)[metric] || metric} · {eventDate || t("engineering.lastV0Days", undefined, { v0: days })}</Tag>}
      {!['history', 'runs'].includes(kind) && <Space wrap style={{ marginBottom: 16 }}>
        <Input.Search aria-label={t("engineering.searchYunxiaoRecords")} placeholder={t("engineering.numberTitleOwnerOrAuthor")} allowClear onSearch={v => { setSearch(v); setPage(1) }} style={{ width: 320 }} />
        {!metric && kind !== 'sprint' && kind !== 'testcase' && <Select aria-label={t("engineering.organizationGroup")} style={{ width: 180 }} value={group} onChange={v => { setGroup(v); setPage(1) }} options={[{ value: '', label: t("engineering.allGroups") }, ...groups.map(g => ({ value: g, label: g }))]} />}
        {!metric && ['requirement','bug'].includes(kind) && <Select aria-label={t("engineering.recordStatus")} allowClear showSearch placeholder={t("engineering.allStatuses")} value={status || undefined} style={{ width:150 }} onChange={v => {setStatus(v || '');setPage(1)}} options={[...new Set([...(dashboard?.statuses?.filter(s => s.kind === kind).map(s => s.status) || []),...(status ? [status] : [])])].map(value => ({value,label:value}))} />}
        {!metric && kind === 'requirement' && <Space><Switch checked={delayed} onChange={v => { setDelayed(v); setPage(1) }} />{t("engineering.overdueOnlyUrgentHighMedium")}</Space>}
      </Space>}
      <div className="yx-management-scope"><span>{loading ? t("engineering.updatingResults") : t("engineering.v0Records", undefined, { v0: total.toLocaleString() })}</span></div>
      <Table<DevopsRow> rowKey={r => String(r.externalId || r.id)} loading={loading} dataSource={rows} columns={columns} scroll={{ x: 1300 }} pagination={{ current: page, pageSize: 30, total, showSizeChanger: false, showTotal: n => t("engineering.v0Records", undefined, { v0: n }), onChange: setPage }} />
    </Card> },
      ]} />
    </PageBody>
    <footer className="yx-devops-sync" data-testid="devops-sync-footer">
      {lastSynced && !Number.isNaN(lastSynced.getTime())
        ? t('engineering.lastSyncedShanghai', undefined, { v0: syncTime.format(lastSynced) })
        : info ? t('engineering.awaitingFirstSync') : '—'}
    </footer>
  </PageContainer>
}
