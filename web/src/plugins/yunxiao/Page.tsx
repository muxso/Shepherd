import { useLocation, useNavigate } from 'react-router-dom'
import { useEffect, useState } from 'react'
import { Alert, Button, Card, Input, Select, Space, Switch, Table, Tabs, Tag } from 'antd'
import { ReloadOutlined } from '@ant-design/icons'
import ManagementOverview from './ManagementOverview'
import RequirementOutput from './RequirementOutput'
import BugStatistics from './BugStatistics'
import MemberMetrics from './MemberMetrics'
import StatisticsNotes from './StatisticsNotes'
import CodeAuthors from './TeamActivity'
import { PageBody, PageContainer } from '../../components/Page'
import { devopsApi, type DevopsDashboard, type DevopsRow, type DevopsSource } from './api'
const kinds = [{ key: 'requirement', label: '需求' }, { key: 'bug', label: '缺陷' }, { key: 'sprint', label: '迭代' }, { key: 'testcase', label: '测试用例' }, { key: 'commit', label: '实际提交' }, { key: 'history', label: '状态变化' }, { key: 'runs', label: '同步记录' }]
const when = (v?: string) => v ? new Date(v).toLocaleString('zh-CN', { hour12: false }) : '—'
const text = (v: unknown) => v == null || v === '' ? '—' : String(v)
export default function Devops() {
  const location = useLocation()
  const navigate = useNavigate()
  const [query] = useState(() => new URLSearchParams(window.location.search))
  const [view, setView] = useState(query.has('reference') ? 'delivery' : query.has('kind') ? 'records' : ['overview','radar','delivery','bugs','authors','records'].includes(query.get('view') || '') ? query.get('view')! : 'overview')
  const [reference, setReference] = useState(query.get('reference') || '')
  const [metric, setMetric] = useState(query.get('metric') || '')
  const [eventDate, setEventDate] = useState(query.get('date') || '')
  const [sources, setSources] = useState<DevopsSource[]>([]), [source, setSource] = useState(query.get('source') || '')
  const [dashboard, setDashboard] = useState<DevopsDashboard | null>(null), [rows, setRows] = useState<DevopsRow[]>([])
  const [total, setTotal] = useState(0), [kind, setKind] = useState(kinds.some(item => item.key === query.get('kind')) ? query.get('kind')! : 'requirement'), [days, setDays] = useState(Math.min(366,Math.max(1,Number(query.get('days')) || 7)))
  const [status, setStatus] = useState(query.get('status') || '')
  const [group, setGroup] = useState(query.get('group') || ''), [search, setSearch] = useState(query.get('search') || ''), [delayed, setDelayed] = useState(query.get('delayedOnly') === 'true')
  const [page, setPage] = useState(1), [revision, setRevision] = useState(0), [loading, setLoading] = useState(false), [error, setError] = useState('')
  useEffect(() => {
    const next = new URLSearchParams(location.search)
    if (next.get('source')) setSource(next.get('source')!)
    if (kinds.some(item => item.key === next.get('kind'))) { setKind(next.get('kind')!); setView('records') }
    if (['overview','radar','delivery','bugs','authors','records'].includes(next.get('view') || '')) setView(next.get('view')!)
    if (next.get('reference')) { setReference(next.get('reference')!); setView('delivery') }
    setSearch(next.get('search') || ''); setStatus(next.get('status') || ''); setGroup(next.get('group') || ''); setDelayed(next.get('delayedOnly') === 'true'); if (next.get('days')) setDays(Math.min(366,Math.max(1,Number(next.get('days')) || 7)))
    setMetric(next.get('metric') || ''); setEventDate(next.get('date') || ''); setPage(1)
  }, [location.search])
  useEffect(() => {
    let live = true
    devopsApi.sources().then(d => { if (live) { setSources(d.items); setSource(s => d.items.some(item => item.id === s) ? s : d.items[0]?.id || '') } }).catch(e => { if (live) setError(String(e.message || e)) })
    return () => { live = false }
  }, [revision])
  useEffect(() => {
    if (!source || view === 'delivery' || view === 'radar' || view === 'authors' || view === 'bugs') return
    let live = true
    setDashboard(null)
    devopsApi.dashboard(source, days).then(d => { if (live) setDashboard(d) }).catch(e => { if (live) setError(String(e.message || e)) })
    return () => { live = false }
  }, [source, days, revision, view])
  useEffect(() => {
    if (!source || view !== 'records') return
    let live = true
    setLoading(true); setError('')
    devopsApi.records(source, { kind, days, group, search, status, page, pageSize: 30, delayedOnly: kind === 'requirement' && delayed, ...(metric && eventDate ? { metric, date: eventDate } : metric && new URLSearchParams(location.search).get('period') === 'true' ? { metric, period: true } : {}) }).then(d => {
      if (live) { setRows(d.items); setTotal(d.total) }
    }).catch(e => { if (live) { setRows([]); setTotal(0); setError(String(e.message || e)) } }).finally(() => { if (live) setLoading(false) })
    return () => { live = false }
  }, [source, kind, days, group, search, status, page, delayed, revision, metric, eventDate, view, location.search])
  const groups = Array.from(new Set([...(dashboard?.groups || []),...(dashboard?.authors?.map(x => x.group) || []),...(group ? [group] : [])]))
  const info = sources.find(s => s.id === source)
  const openRequirement = (reference: string) => navigate(`/devops?${new URLSearchParams({ source,reference })}`)
  const requirementLinks = (row: DevopsRow) => row.requirements?.length
    ? <Space size={[4,0]} wrap>{row.requirements.map(item => <Button type="link" size="small" key={item.externalId} onClick={() => openRequirement(item.externalId)}>{item.serial || item.externalId} {item.title || ''}</Button>)}</Space>
    : <Tag>未关联需求</Tag>
  const columns = kind === 'testcase' ? [
    { title: '云效编号', dataIndex: 'serial', width: 140 }, { title: '用例名称', dataIndex: 'title' }, { title: '用例库', dataIndex: 'repo', width: 140 }, { title: '目录', dataIndex: 'directory', width: 200 }, { title: '负责人', dataIndex: 'owner', width: 110 }, { title: '优先级', dataIndex: 'priority', width: 90 }, { title: '步骤数', dataIndex: 'steps', width: 90, render: (v?: unknown[]) => v?.length || 0 },
  ] : kind === 'commit' ? [
    { title: '提交', dataIndex: 'sha', width: 115, render: (v: string, r: DevopsRow) => r.url?.startsWith('https://') ? <a href={r.url} target="_blank" rel="noreferrer">{v?.slice(0, 10)}</a> : v?.slice(0, 10) },
    { title: '说明', dataIndex: 'title' }, { title: '实际作者', dataIndex: 'author', width: 120 }, { title: '分组', dataIndex: 'group', width: 140 }, { title: '仓库', dataIndex: 'repo' },
    { title: '作者时间', dataIndex: 'authoredAt', width: 175, render: when },
    { title: '增 / 删行', key: 'lines', width: 125, render: (_: unknown, r: DevopsRow) => r.additions == null ? '行数未知' : `+${r.additions} / -${r.deletions}` },
    { title: '需求关联', key: 'requirements', render: (_: unknown, row: DevopsRow) => requirementLinks(row) },
  ] : kind === 'history' ? [
    { title: '类型', dataIndex: 'kind' }, { title: '外部 ID', dataIndex: 'external_id' }, { title: '原状态', dataIndex: 'old_status' }, { title: '新状态', dataIndex: 'new_status' },
    { title: '云效状态时间', dataIndex: 'source_changed_at', render: when }, { title: '同步观察时间', dataIndex: 'observed_at', render: when },
  ] : kind === 'runs' ? [
    { title: '状态', dataIndex: 'status', render: (v: string) => <Tag color={v === 'SUCCESS' ? 'green' : v === 'FAILED' ? 'red' : 'blue'}>{v}</Tag> },
    { title: '开始', dataIndex: 'started_at', render: when }, { title: '结束', dataIndex: 'finished_at', render: when },
    { title: '同步数量', dataIndex: 'counts', render: (v: unknown) => JSON.stringify(v) }, { title: '异常', dataIndex: 'error' },
  ] : [
    { title: '云效编号 / ID', key: 'serial', width: 155, render: (_: unknown, r: DevopsRow) => text(r.serial || r.externalId) },
    { title: '标题', dataIndex: 'title', width: 400, render: (value: string, row: DevopsRow) => kind === 'requirement' ? <Button type="link" size="small" onClick={() => openRequirement(String(row.externalId))}>{value}</Button> : value }, { title: '状态', dataIndex: 'status', width: 110 },
    ...(kind === 'sprint' ? [{ title: '开始', dataIndex: 'startAt', render: when }, { title: '结束', dataIndex: 'endAt', render: when }] : [
      ...(kind === 'bug' ? [{ title: '关联需求', key: 'requirements', width: 240, render: (_: unknown, row: DevopsRow) => requirementLinks(row) }] : []),
      { title: '优先级', dataIndex: 'priority', width: 90, render: (v: string) => <Tag color={v === '紧急' ? 'red' : v === '高' ? 'orange' : undefined}>{text(v)}</Tag> },
      { title: '负责人', dataIndex: 'owner', width: 110, render: text }, { title: '分组', dataIndex: 'group', width: 130 },
      { title: '迭代', dataIndex: 'sprint', width: 140, render: text }, { title: '截止日', dataIndex: 'due', width: 120, render: text },
      { title: '状态变更', dataIndex: 'statusChangedAt', width: 175, render: when },
    ]),
  ]
  const sourceControls = <Space size={8} wrap>
      <Select aria-label="云效项目" style={{ minWidth: 180 }} value={source || undefined} onChange={v => { setSource(v); setPage(1); setGroup(''); setReference(''); const next = new URLSearchParams(location.search); next.set('source',v); next.delete('bugPerson'); next.delete('bugGroup'); next.delete('reference'); next.delete('group'); next.delete('activityGroup'); next.delete('perspective'); next.delete('chart'); next.delete('compare'); navigate({ pathname:location.pathname,search:next.toString() }) }} options={sources.map(s => ({ value: s.id, label: s.name }))} placeholder="选择数据源" />
      {!['delivery','radar','authors','bugs'].includes(view) && <Select aria-label="统计区间" value={days} onChange={v => { setDays(v); setPage(1) }} options={[7,14,30,90,365].map(v => ({ value:v, label:`最近 ${v} 天` }))} />}
      <Button aria-label="刷新数据" icon={<ReloadOutlined />} onClick={() => setRevision(x => x + 1)} />
      {!['delivery','radar','authors','bugs'].includes(view) && <StatisticsNotes info={info} data={dashboard} />}
    </Space>
  return <PageContainer>
    <PageBody testId="devops-scroll">
      {error && <Alert showIcon type="error" message={error} style={{ marginBottom: 12 }} />}
      {view !== 'authors' && <div className="yx-page-status"><Tag>云效插件</Tag><span>{info?.last_success_at ? `最近同步 ${when(info.last_success_at)}` : '等待首次同步'}</span></div>}
      <Tabs tabBarExtraContent={sourceControls} activeKey={view} onChange={value => { setView(value); setError(''); const next = new URLSearchParams(location.search); next.set('view', value); if (source) next.set('source', source); if (value !== 'delivery') next.delete('reference'); if (value !== 'records') { next.delete('kind'); next.delete('metric'); next.delete('date'); next.delete('period') } navigate({ pathname: location.pathname, search: next.toString() }) }} items={[
        { key:'overview', label:'概览', children: <ManagementOverview data={dashboard} sourceId={source} days={days} /> },
        { key:'radar', label:'个人雷达指标', children: source && view === 'radar' ? <MemberMetrics key={source} sourceId={source} revision={revision} /> : null },
        { key:'delivery', label:'需求产出', children: source ? <RequirementOutput key={source} sourceId={source} reference={reference} revision={revision} /> : null },
        { key:'bugs', label:'缺陷统计', children: source && view === 'bugs' ? <BugStatistics key={source} sourceId={source} revision={revision} /> : null },
        { key:'authors', label:'团队产出', children: source && view === 'authors' ? <CodeAuthors key={source} sourceId={source} revision={revision} /> : null },
        { key:'records', label:'明细记录', children: <Card size="small">
      <Tabs activeKey={kind} items={kinds} onChange={v => { setKind(v); setPage(1); setSearch(''); setGroup(''); setStatus(''); setDelayed(false); setMetric(''); setEventDate('') }} />
      {metric && <Tag closable onClose={() => { const next=new URLSearchParams(location.search);next.delete('metric');next.delete('date');next.delete('period');navigate({pathname:location.pathname,search:next.toString()},{replace:true});setEventDate('');setMetric('');setPage(1) }}>{({requirements:'新增需求',delivered:'已发布需求',bugs:'新增缺陷',testcases:'新增用例',commits:'代码提交',linkedCommits:'关联提交'} as Record<string,string>)[metric] || metric} · {eventDate || `最近 ${days} 天`}</Tag>}
      {!['history', 'runs'].includes(kind) && <Space wrap style={{ marginBottom: 16 }}>
        <Input.Search aria-label="搜索云效记录" placeholder="编号、标题、负责人或作者" allowClear onSearch={v => { setSearch(v); setPage(1) }} style={{ width: 320 }} />
        {!metric && kind !== 'sprint' && kind !== 'testcase' && <Select aria-label="组织分组" style={{ width: 180 }} value={group} onChange={v => { setGroup(v); setPage(1) }} options={[{ value: '', label: '全部分组' }, ...groups.map(g => ({ value: g, label: g }))]} />}
        {!metric && ['requirement','bug'].includes(kind) && <Select aria-label="记录状态" allowClear showSearch placeholder="全部状态" value={status || undefined} style={{ width:150 }} onChange={v => {setStatus(v || '');setPage(1)}} options={[...new Set([...(dashboard?.statuses?.filter(s => s.kind === kind).map(s => s.status) || []),...(status ? [status] : [])])].map(value => ({value,label:value}))} />}
        {!metric && kind === 'requirement' && <Space><Switch checked={delayed} onChange={v => { setDelayed(v); setPage(1) }} />只看延期（紧急 / 高 / 中）</Space>}
      </Space>}
      <div className="yx-management-scope"><span>{loading ? '正在更新筛选结果…' : `共 ${total.toLocaleString()} 条记录`}</span><span>保留原始来源，按当前筛选范围核对</span></div>
      <Table<DevopsRow> rowKey={r => String(r.externalId || r.id)} loading={loading} dataSource={rows} columns={columns} scroll={{ x: 1300 }} pagination={{ current: page, pageSize: 30, total, showSizeChanger: false, showTotal: n => `${n} 条`, onChange: setPage }} />
    </Card> },
      ]} />
    </PageBody>
  </PageContainer>
}
