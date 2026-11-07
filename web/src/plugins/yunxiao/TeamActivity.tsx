import { lazy, Suspense, useEffect, useMemo, useState } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { Alert, Button, Dropdown, Empty, Input, InputNumber, Segmented, Select, Space, Spin, Tooltip } from 'antd'
import { activityApi, type ActivityKind, type ActivityMember, type ActivityRole, type TeamActivityData } from './api'
import { activityRoles, activitySortValue, type ActivitySort } from './activityModel'
import { attribution, memberLabel, reviewMember, type ActivityReview } from './activityReview'
import { dateTime } from './codeActivity'
import AuthorIdentities from './AuthorIdentities'
import ActivityRoster from './ActivityRoster'
import ActivityReviewSettings from './ActivityReviewSettings'
import ActivityMatrix from './ActivityMatrix'
import ActivityDetails, { type ActivitySelection } from './ActivityDetails'
import './team-activity.css'
const RequirementActivityAnalysis = lazy(() => import('./RequirementActivityAnalysis'))
const allMetrics = activityRoles.flatMap(r => r.metrics)
function roleFrom(search: string): ActivityRole { const value = new URLSearchParams(search).get('role'); return value === 'product' || value === 'test' ? value : 'code' }
function daysFrom(search: string) { const value = Number(new URLSearchParams(search).get('days') || 7); return Number.isInteger(value) && value >= 1 && value <= 365 ? value : 7 }
function metricFrom(search: string): ActivityKind { const q = new URLSearchParams(search), info = activityRoles.find(r => r.value === roleFrom(search))!; return info.metrics.find(m => m.value === q.get('activityMetric'))?.value || info.metrics[0].value }
const sortLabels: Record<ActivitySort, string> = { commits: '按提交次数', bugs: '按 Bug 数', last: '按最晚提交时间', repositories: '按变动仓库数', requirements: '按关联需求数', lines: '按变动行数' }
type ListMode = 'all' | 'absence' | 'review' | 'unmapped'
export default function TeamActivity({ sourceId, revision }: { sourceId: string; revision: number }) {
  const location = useLocation(), navigate = useNavigate(), initial = new URLSearchParams(location.search)
  const [role, setRole] = useState<ActivityRole>(() => roleFrom(location.search)), [days, setDays] = useState(() => daysFrom(location.search)), [metric, setMetric] = useState<ActivityKind>(() => metricFrom(location.search))
  const [view, setView] = useState(initial.get('activity') === 'month' ? 'month' : 'week'), [perspective, setPerspective] = useState(initial.get('perspective') === 'group' ? 'group' : 'role'), [group, setGroup] = useState(initial.get('activityGroup') || '')
  const [data, setData] = useState<TeamActivityData | null>(null), [loading, setLoading] = useState(false), [error, setError] = useState(''), [refresh, setRefresh] = useState(0)
  const [search, setSearch] = useState(''), [listMode, setListMode] = useState<ListMode>('all'), [expanded, setExpanded] = useState(false), [showHistory, setShowHistory] = useState(false)
  const [sort, setSort] = useState<ActivitySort>((initial.get('activitySort') as ActivitySort) in sortLabels ? initial.get('activitySort') as ActivitySort : 'commits'), [order, setOrder] = useState(initial.get('activityOrder') === 'asc' ? 'asc' : 'desc')
  const [manage, setManage] = useState<'aliases' | 'roster' | 'review' | null>(null), [selection, setSelection] = useState<ActivitySelection | null>(null), [analysis, setAnalysis] = useState(false)
  const updateLocation = (values: Record<string, string>) => { const q = new URLSearchParams(location.search); q.set('source', sourceId); q.set('view', 'authors'); for (const [key, value] of Object.entries(values)) { if (value) q.set(key, value); else q.delete(key) } navigate({ pathname: location.pathname, search: q.toString() }, { replace: true }) }
  useEffect(() => {
    const q = new URLSearchParams(location.search)
    setRole(roleFrom(location.search)); setDays(daysFrom(location.search)); setMetric(metricFrom(location.search)); setView(q.get('activity') === 'month' ? 'month' : 'week')
    setPerspective(q.get('perspective') === 'group' && q.get('activityGroup') ? 'group' : 'role'); setGroup(q.get('perspective') === 'group' ? q.get('activityGroup') || '' : '')
    const nextSort = q.get('activitySort') as ActivitySort; setSort(nextSort in sortLabels ? nextSort : 'commits'); setOrder(q.get('activityOrder') === 'asc' ? 'asc' : 'desc')
  }, [location.search])
  useEffect(() => {
    let live = true; setLoading(true); setError(''); setData(null); setSelection(null)
    const timer = window.setTimeout(() => activityApi.activity(sourceId, role, days).then(value => { if (live) setData(value) }).catch(e => { if (live) setError(String(e.message || e)) }).finally(() => { if (live) setLoading(false) }), 120)
    return () => { live = false; window.clearTimeout(timer) }
  }, [sourceId, role, days, revision, refresh])
  const roleInfo = activityRoles.find(r => r.value === role)!
  const reviews = useMemo(() => new Map<string, ActivityReview>((data?.members || []).flatMap(person => { const review = data && reviewMember(data, person); return review ? [[person.name, review] as const] : [] })), [data])
  const filtered = useMemo(() => (data?.members || []).filter(person => (!group || person.group === group) && (!search || `${memberLabel(person)} ${data?.events.filter(e => e.actor === person.name).map(e => e.authorRaw).join(' ')}`.toLowerCase().includes(search.toLowerCase()))).sort((a, b) => {
    const difference = data ? activitySortValue(data, b, metric, sort) - activitySortValue(data, a, metric, sort) : 0
    return difference * (order === 'desc' ? 1 : -1) || memberLabel(a).localeCompare(memberLabel(b), 'zh-CN')
  }), [data, group, search, metric, sort, order])
  const currentBugActors = new Set(data?.bugs.flatMap(b => b.responsibles) || [])
  const historical = (person: ActivityMember) => !person.inRoster && person.status !== 'active' && !currentBugActors.has(person.name)
  const members = filtered.filter(person => listMode === 'absence' ? person.status !== 'active' && attribution(person) !== 'unknownAuthor' : listMode === 'review' ? reviews.has(person.name) : listMode === 'unmapped' ? attribution(person) !== 'person' && person.eventCount > 0 : showHistory || !!search || !historical(person))
  const subjects = filtered.filter(p => attribution(p) !== 'unknownAuthor'), noRecords = subjects.filter(p => p.status === 'noRecords').length, unverified = subjects.filter(p => p.status === 'unverified').length
  const reviewCount = filtered.filter(p => reviews.has(p.name)).length, unmappedCount = filtered.filter(p => attribution(p) === 'unmappedAccount' && p.eventCount > 0).length, unassignedCount = filtered.filter(p => attribution(p) !== 'person' && p.eventCount > 0).length, hiddenHistory = filtered.filter(historical).length
  const scope = members.map(person => person.name), groups = [...new Set([...(data?.members.map(p => p.group) || []), ...(group ? [group] : [])])].sort()
  const refreshData = () => { setSelection(null); setRefresh(v => v + 1) }
  const openRequirement = (id: string) => { setSelection(null); navigate(`/devops?${new URLSearchParams({ view: 'delivery', source: sourceId, reference: id })}`) }
  const chooseRole = (next: ActivityRole) => { setRole(next); setMetric(activityRoles.find(r => r.value === next)!.metrics[0].value); setPerspective('role'); setGroup(''); setSearch(''); setListMode('all'); setAnalysis(false); setShowHistory(false); updateLocation({ role: next, perspective: 'role', activityGroup: '', activityMetric: '', activitySort: 'commits' }) }
  const chooseGroup = (value?: string) => { setGroup(value || ''); setPerspective(value ? 'group' : 'role'); setListMode('all'); updateLocation({ perspective: value ? 'group' : 'role', activityGroup: value || '' }) }
  const chooseMetric = (value: ActivityKind) => { const next = activityRoles.find(r => r.metrics.some(m => m.value === value))!.value; setMetric(value); setRole(next); setSelection(null); setListMode('all'); setAnalysis(false); setShowHistory(false); updateLocation({ role: next, activityMetric: value, activitySort: next === role ? sort : 'commits' }) }
  const showAnalysis = () => { setAnalysis(true); setListMode('all'); if (role !== 'code') { setRole('code'); setMetric('commit'); updateLocation({ role: 'code', activityMetric: 'commit' }) } }
  const toggleList = (value: ListMode) => { setListMode(current => current === value ? 'all' : value); setAnalysis(false) }
  const sortOptions = (role === 'code' ? Object.keys(sortLabels) as ActivitySort[] : role === 'product' ? ['commits', 'last', 'requirements'] as ActivitySort[] : ['commits', 'last', 'bugs'] as ActivitySort[]).filter(key => key !== 'bugs' || !data || role !== 'code' || data.permissions.bugs).map(value => ({ value, label: value === 'commits' && role !== 'code' ? '按活动次数' : sortLabels[value] }))
  const analysisData = data && (group || search) ? { ...data, events: data.events.filter(e => filtered.some(p => p.name === e.actor)), bugs: data.bugs.filter(b => b.responsibles.some(name => filtered.some(p => p.name === name))) } : data
  return <section className="yx-activity-board" aria-label="团队产出与活跃情况">
    <div className="yx-activity-toolbar"><Segmented aria-label="岗位视角" value={perspective === 'role' ? role : ''} onChange={v => chooseRole(v as ActivityRole)} options={activityRoles.map(r => ({ value: r.value, label: r.label }))} /><Space size={8} wrap><Button onClick={showAnalysis}>需求投入与质量</Button><Dropdown menu={{ items: [{ key: 'aliases', label: '账号归并管理' }, { key: 'roster', label: '统计人员名单' }, { key: 'review', label: '产出核验规则' }], onClick: e => setManage(e.key as 'aliases' | 'roster' | 'review') }}><Button>人员管理 ⌄</Button></Dropdown></Space></div>
    <div className="yx-activity-filters"><Space size={[8, 8]} wrap>
      <label className="yx-activity-days">最近 <InputNumber aria-label="最近天数" min={1} max={365} precision={0} value={days} onChange={value => { if (value != null) { const next = Math.min(365, Math.max(1, Number(value))); setDays(next); updateLocation({ days: String(next) }) } }} /> 天</label>
      <Select aria-label="活动指标" value={metric} onChange={chooseMetric} options={perspective === 'group' ? allMetrics : roleInfo.metrics} style={{ width: 150 }} />
      <Tooltip title="与岗位视角互斥；选择具体分组后按当前活动指标查看"><Select aria-label="组织分组" allowClear placeholder="按组织分组查看" value={perspective === 'group' ? group : undefined} onChange={chooseGroup} options={groups.map(value => ({ value, label: value }))} style={{ width: 165 }} /></Tooltip>
      <Input.Search aria-label="搜索成员" placeholder="搜索姓名或原始账号" value={search} allowClear onChange={e => setSearch(e.target.value)} style={{ width: 190 }} />
      {data && <span className="yx-activity-date-range">{data.from.slice(5).replace('-', '/')}–{data.through.slice(5).replace('-', '/')} · 上海{perspective === 'group' ? ' · 分组视角' : ''}</span>}
    </Space><Space size={6} wrap><Button className="yx-activity-no-records" type={listMode === 'review' ? 'primary' : 'default'} onClick={() => toggleList('review')}>需核验{data ? `（${reviewCount}）` : ''}</Button><Button className="yx-activity-no-records" onClick={() => toggleList('absence')}>{listMode === 'absence' ? '返回全部人员' : `查看未记录产出人员${data ? `（${noRecords}）` : ''}`}</Button>{role === 'code' && <Button type={listMode === 'unmapped' ? 'primary' : 'default'} onClick={() => toggleList('unmapped')}>未关联{data ? `（${unassignedCount}）` : ''}</Button>}</Space></div>
    <div className="yx-activity-stat-strip" aria-label="统计概览">{[
      ['统计对象', subjects.length, '含已归属成员和原始 Git 账号；未识别作者的提交另行汇总'], ['有产出记录', subjects.filter(p => p.status === 'active').length, '本期对应岗位活动有记录，低活跃记录另列需核验'], ['未记录产出', noRecords, '已纳入名单、采集完整且本期无记录'], ['采集待核验', unverified, '名单或时间范围不完整，不直接判零'],
    ].map(([label, count, note]) => <Tooltip key={label} title={note}><span>{label}<strong>{data ? count : '—'}</strong></span></Tooltip>)}{role === 'code' && <Tooltip title="原始 Git 作者与未识别作者的记录均计入提交和变更行数"><span>未关联账号<strong>{data ? unmappedCount : '—'}</strong></span></Tooltip>}{role === 'code' && !!data?.coverage.unattributedEvents && <span>未识别作者提交<strong>{data.coverage.unattributedEvents}</strong></span>}</div>
    {error && <Alert type="error" showIcon title={error} style={{ marginBottom: 12 }} />}
    <Spin spinning={loading}>
      {data && analysis ? <Suspense fallback={<Spin />}><RequirementActivityAnalysis data={analysisData!} onBack={() => setAnalysis(false)} openRequirement={openRequirement} /></Suspense> : <div className="yx-activity-chart">
        <div className="yx-activity-chart-heading"><h3>{listMode === 'absence' ? `最近 ${days} 天未记录产出人员` : listMode === 'review' ? '产出记录 · 需核验' : listMode === 'unmapped' ? '未关联人员的 Git 记录' : `${roleInfo.metrics.find(m => m.value === metric)?.label} · 成员热力图`}</h3><Space size={6} wrap>{listMode !== 'absence' && <><Select size="small" aria-label="成员排序" value={sort} onChange={value => { setSort(value); updateLocation({ activitySort: value }) }} options={sortOptions} style={{ width: 155 }} /><Button size="small" aria-label="切换排序方向" onClick={() => { const next = order === 'desc' ? 'asc' : 'desc'; setOrder(next); updateLocation({ activityOrder: next }) }}>{order === 'desc' ? '降序 ↓' : '升序 ↑'}</Button>{hiddenHistory > 0 && <Button size="small" onClick={() => setShowHistory(v => !v)}>{showHistory ? '收起历史人员' : `历史人员 ${hiddenHistory}`}</Button>}<Button size="small" type={expanded ? 'primary' : 'default'} onClick={() => setExpanded(v => !v)}>{expanded ? '收起格子指标' : '展开格子指标'}</Button><Segmented size="small" aria-label="热力图时间视图" value={view} onChange={value => { setView(String(value)); updateLocation({ activity: String(value) }) }} options={[{ value: 'week', label: '近 7 天' }, { value: 'month', label: '月度概览' }]} /></>}</Space></div>
        {listMode === 'review' && <p className="yx-muted">按岗位活动及已保存规则筛查；点击标记成员查看活跃天数、提交量、行数和原始证据。</p>}
        {listMode === 'absence' && data && !data.coverage.rosterConfigured && <Alert type="info" showIcon title="尚未配置完整统计名单，无法判断组织全员是否无记录" action={<Button size="small" onClick={() => setManage('roster')}>维护名单</Button>} style={{ marginBottom: 12 }} />}
        {listMode === 'absence' && data && !data.coverage.complete && <Alert type="info" showIcon title="所选区间采集尚未覆盖完整，无记录者保留待核验" style={{ marginBottom: 12 }} />}
        {!members.length ? <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={loading ? '正在读取已采集数据' : listMode === 'absence' ? '没有可确认的未记录产出人员' : listMode === 'review' ? '所选范围没有触发核验规则的记录' : '所选范围暂无已同步的真实人员或活动'} /> : listMode === 'absence' ? <div className="yx-activity-table-scroll"><table className="yx-activity-absence"><thead><tr><th>成员</th><th>分组</th><th>最近记录</th><th>本期记录</th><th>统计状态</th><th /></tr></thead><tbody>{members.map(p => <tr key={p.name}><td>{memberLabel(p)}</td><td>{p.group}</td><td>{p.lastActivityAt ? dateTime(p.lastActivityAt) : '采集范围内暂无记录'}</td><td>0 条</td><td>{p.status === 'noRecords' ? `最近 ${days} 天未记录产出` : '待核验'}</td><td><Button type="link" size="small" onClick={() => setSelection({ person: p.name, history: true })}>最近记录</Button></td></tr>)}</tbody></table></div> : data && <ActivityMatrix data={data} members={members} metric={metric} view={view} expanded={expanded} reviews={reviews} select={setSelection} associate={() => setManage('aliases')} />}
        {data && <div className="yx-activity-chart-footer"><span><i />深浅表示当前指标次数 · 时间为当天最后活动 <em>周六 / 周日</em></span><span>当前 {members.length} 项{!showHistory && hiddenHistory > 0 && listMode === 'all' ? ` · 历史 ${hiddenHistory} 项已收起` : ' · 全部展示'}</span></div>}
      </div>}
    </Spin>
    <details className="yx-activity-notes"><summary>统计口径与数据范围{data?.observedAt ? ` · 最近同步 ${dateTime(data.observedAt)}` : ''}</summary><p>仅展示已同步记录；岗位是指标视角，不自动认定人员岗位。研发按 Git 作者时间、每仓库 SHA 去重，排除合并提交。产品新增按创建时间与创建人，交付按已发布状态、状态时间及当前负责人；历史交付归属来自当前快照。测试按用例创建人和有效缺陷发现人；没有真实验证事件时不推算验证人数。</p><p>未关联的 Git 账号及未识别作者提交仍计入提交、仓库和行数。未识别作者汇总不是一个人员。Bug 按本期创建时间及源缺陷责任人归属；不是发现人或本次提交作者。多人责任不相加当团队去重总数。未记录产出判断要求名单及采集完整；“需核验”是记录信号，需结合实际需求与人员安排复核。</p>{data && <p>{role === 'code' ? '未识别作者提交' : '事件缺少创建人'}：{data.coverage.unattributedEvents} 条；采集范围{data.coverage.complete ? '已覆盖' : '待核验'}。{role === 'test' && !data.coverage.testcasesCollected && '最新同步尚未确认测试用例完整采集。'}</p>}</details>
    <AuthorIdentities sourceId={sourceId} commits={(data?.events || []).filter(e => e.kind === 'commit' && e.actor).map(e => ({ author: e.actor, authorRaw: e.authorRaw }))} open={manage === 'aliases'} onOpenChange={open => setManage(open ? 'aliases' : null)} onSaved={refreshData} />
    <ActivityRoster sourceId={sourceId} candidates={(data?.members || []).filter(p => p.name)} open={manage === 'roster'} onClose={() => setManage(null)} onSaved={refreshData} />
    <ActivityReviewSettings sourceId={sourceId} open={manage === 'review'} onClose={() => setManage(null)} onSaved={refreshData} />
    {data && <ActivityDetails data={data} selection={selection} onClose={() => setSelection(null)} metric={metric} scope={scope} openRequirement={openRequirement} review={selection?.person !== undefined ? reviews.get(selection.person) : undefined} />}
  </section>
}
