import { lazy, Suspense, useEffect, useMemo, useState } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { Alert, Button, Dropdown, Empty, Input, InputNumber, Popover, Segmented, Select, Space, Spin, Tooltip } from 'antd'
import { activityApi, type ActivityKind, type ActivityMember, type ActivityRole, type TeamActivityData } from './api'
import { activityRoles, datesFor, dayFacts, lineCount, memberTotals } from './activityModel'
import { dateTime } from './codeActivity'
import AuthorIdentities from './AuthorIdentities'
import ActivityRoster from './ActivityRoster'
import ActivityDetails, { type ActivitySelection } from './ActivityDetails'
import './team-activity.css'
const RequirementActivityAnalysis = lazy(() => import('./RequirementActivityAnalysis'))
function roleFrom(search: string): ActivityRole { const value = new URLSearchParams(search).get('role'); return value === 'product' || value === 'test' ? value : 'code' }
function daysFrom(search: string) { const value = Number(new URLSearchParams(search).get('days') || 7); return Number.isInteger(value) && value >= 1 && value <= 365 ? value : 7 }
const compactNumber = (value: number) => value.toLocaleString()
export default function TeamActivity({ sourceId, revision }: { sourceId: string; revision: number }) {
  const location = useLocation(), navigate = useNavigate()
  const [role, setRole] = useState<ActivityRole>(() => roleFrom(location.search)), [days, setDays] = useState(() => daysFrom(location.search))
  const [metric, setMetric] = useState<ActivityKind>(() => activityRoles.find(r => r.value === roleFrom(location.search))!.metrics[0].value)
  const [view, setView] = useState(new URLSearchParams(location.search).get('activity') === 'month' ? 'month' : 'week')
  const [data, setData] = useState<TeamActivityData | null>(null), [loading, setLoading] = useState(false), [error, setError] = useState('')
  const [refresh, setRefresh] = useState(0), [group, setGroup] = useState(''), [search, setSearch] = useState(''), [absence, setAbsence] = useState(false), [expanded, setExpanded] = useState(false)
  const [manage, setManage] = useState<'aliases' | 'roster' | null>(null), [selection, setSelection] = useState<ActivitySelection | null>(null), [analysis, setAnalysis] = useState(false)
  const updateLocation = (values: Record<string, string>) => { const q = new URLSearchParams(location.search); q.set('source', sourceId); q.set('view', 'authors'); for (const [key, value] of Object.entries(values)) q.set(key, value); navigate({ pathname: location.pathname, search: q.toString() }, { replace: true }) }
  useEffect(() => {
    const nextRole = roleFrom(location.search)
    setRole(current => { if (current !== nextRole) { setMetric(activityRoles.find(r => r.value === nextRole)!.metrics[0].value); setGroup(''); setSearch(''); setSelection(null) } return nextRole })
    setDays(daysFrom(location.search))
    setView(new URLSearchParams(location.search).get('activity') === 'month' ? 'month' : 'week')
  }, [location.search])
  useEffect(() => {
    let live = true
    setLoading(true); setError(''); setData(null); setSelection(null)
    const timer = window.setTimeout(() => activityApi.activity(sourceId, role, days).then(value => { if (live) setData(value) }).catch(e => { if (live) setError(String(e.message || e)) }).finally(() => { if (live) setLoading(false) }), 120)
    return () => { live = false; window.clearTimeout(timer) }
  }, [sourceId, role, days, revision, refresh])
  const roleInfo = activityRoles.find(r => r.value === role)!
  const memberEvents = useMemo(() => new Map((data?.members || []).map(p => [p.name, data!.events.filter(e => e.actor === p.name && e.kind === metric)])), [data, metric])
  const filtered = useMemo(() => (data?.members || []).filter(p => (!group || p.group === group) && (!search || `${p.name} ${data?.events.filter(e => e.actor === p.name).map(e => e.authorRaw).join(' ')}`.toLowerCase().includes(search.toLowerCase()))).sort((a, b) => (memberEvents.get(b.name)?.length || 0) - (memberEvents.get(a.name)?.length || 0) || a.name.localeCompare(b.name, 'zh-CN')), [data, group, search, memberEvents])
  const members = filtered.filter(p => !absence || p.status !== 'active')
  const scope = useMemo(() => filtered.map(p => p.name), [filtered])
  const dates = data ? datesFor(data, view) : []
  const noRecords = filtered.filter(p => p.status === 'noRecords').length, unverified = filtered.filter(p => p.status === 'unverified').length
  const refreshData = () => { setSelection(null); setGroup(''); setRefresh(v => v + 1) }
  const openRequirement = (id: string) => { setSelection(null); navigate(`/devops?${new URLSearchParams({ view: 'delivery', source: sourceId, reference: id })}`) }
  const totalHeaders = role === 'code' ? ['提交', '需求', 'Bug'] : role === 'product' ? ['新增', '交付', '需求'] : ['用例', 'Bug', '活动']
  const totals = (p: ActivityMember) => data ? memberTotals(data, p) : [0, 0, 0]
  const showAnalysis = () => { setAnalysis(true); if (role !== 'code') { setRole('code'); setMetric('commit'); updateLocation({ role: 'code' }) } }
  return <section className="yx-activity-board" aria-label="团队产出与活跃情况">
    <div className="yx-activity-toolbar">
      <Segmented aria-label="岗位视角" value={role} onChange={v => { const next = v as ActivityRole; setRole(next); setMetric(activityRoles.find(r => r.value === next)!.metrics[0].value); setGroup(''); setSearch(''); setAbsence(false); setAnalysis(false); updateLocation({ role: next }) }} options={activityRoles.map(r => ({ value: r.value, label: r.label }))} />
      <Space size={8} wrap><Button onClick={showAnalysis}>需求投入与质量</Button><Dropdown menu={{ items: [{ key: 'aliases', label: '账号归并管理' }, { key: 'roster', label: '统计人员名单' }], onClick: e => setManage(e.key as 'aliases' | 'roster') }}><Button>人员管理 ⌄</Button></Dropdown></Space>
    </div>
    <div className="yx-activity-filters"><Space size={[8, 8]} wrap>
      <label className="yx-activity-days">最近 <InputNumber aria-label="最近天数" min={1} max={365} precision={0} value={days} onChange={value => { if (value != null) { const next = Math.min(365, Math.max(1, Number(value))); setDays(next); updateLocation({ days: String(next) }) } }} /> 天</label>
      <Select aria-label="活动指标" value={metric} onChange={setMetric} options={roleInfo.metrics} style={{ width: 150 }} />
      <Select aria-label="组织分组" value={group} onChange={setGroup} options={[{ value: '', label: '全部分组' }, ...[...new Set(data?.members.map(p => p.group) || [])].sort().map(value => ({ value, label: value }))]} style={{ width: 155 }} />
      <Input.Search aria-label="搜索成员" placeholder="搜索姓名或账号" value={search} allowClear onChange={e => setSearch(e.target.value)} style={{ width: 190 }} />
      {data && <span className="yx-activity-date-range">{data.from.slice(5).replace('-', '/')}–{data.through.slice(5).replace('-', '/')} · 上海</span>}
    </Space><Button className="yx-activity-no-records" onClick={() => { setAbsence(v => !v); setAnalysis(false) }}>{absence ? '返回全部人员' : `查看未记录产出人员${data ? `（${noRecords}）` : ''}`}</Button></div>
    <div className="yx-activity-stat-strip" aria-label="统计概览">{[
      ['统计人员', filtered.length, data?.coverage.rosterConfigured ? '已配置名单及观察到的人员' : '仅覆盖已采集记录中的人员，尚无完整统计名单'],
      ['有产出记录', filtered.filter(p => p.status === 'active').length, '本期任一对应岗位活动有记录'],
      ['未记录产出', noRecords, '已纳入名单、采集完整且本期无记录'],
      ['采集待核验', unverified, '未纳入名单、采集不完整或事件缺少时间，不直接判零'],
    ].map(([label, count, note]) => <Tooltip key={label} title={note}><span>{label}<strong>{data ? count : '—'}</strong></span></Tooltip>)}</div>
    {error && <Alert type="error" showIcon title={error} style={{ marginBottom: 12 }} />}
    <Spin spinning={loading}>
      {data && analysis ? <Suspense fallback={<Spin />}><RequirementActivityAnalysis data={data} onBack={() => setAnalysis(false)} openRequirement={openRequirement} /></Suspense> : <div className="yx-activity-chart">
        <div className="yx-activity-chart-heading"><h3>{absence ? `最近 ${days} 天未记录产出人员` : `${roleInfo.metrics.find(m => m.value === metric)?.label} · 成员热力图`}</h3><Space size={6} wrap>{!absence && <><Button size="small" type={expanded ? 'primary' : 'default'} onClick={() => setExpanded(v => !v)}>{expanded ? '收起格子指标' : '展开格子指标'}</Button><Segmented size="small" aria-label="热力图时间视图" value={view} onChange={value => { setView(String(value)); updateLocation({ activity: String(value) }) }} options={[{ value: 'week', label: '近 7 天' }, { value: 'month', label: '月度概览' }]} /></>}</Space></div>
        {absence && data && !data.coverage.rosterConfigured && <Alert type="info" showIcon title="尚未配置完整统计名单，无法判断组织全员是否无记录" action={<Button size="small" onClick={() => setManage('roster')}>维护名单</Button>} style={{ marginBottom: 12 }} />}
        {absence && data && !data.coverage.complete && <Alert type="info" showIcon title="所选区间采集尚未覆盖完整，无记录者保留待核验" style={{ marginBottom: 12 }} />}
        {!members.length ? <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={loading ? '正在读取已采集数据' : absence ? '没有可确认的未记录产出人员' : '所选范围暂无已同步的真实人员或活动'} /> : absence ? <div className="yx-activity-table-scroll"><table className="yx-activity-absence"><thead><tr><th>成员</th><th>分组</th><th>最近记录</th><th>本期记录</th><th>统计状态</th><th /></tr></thead><tbody>{members.map(p => <tr key={p.name}><td>{p.name}</td><td>{p.group}</td><td>{p.lastActivityAt ? dateTime(p.lastActivityAt) : '采集范围内暂无记录'}</td><td>0 条</td><td>{p.status === 'noRecords' ? `最近 ${days} 天未记录产出` : '待核验'}</td><td><Button type="link" size="small" onClick={() => setSelection({ person: p.name, history: true })}>最近记录</Button></td></tr>)}</tbody></table></div> : data && <div className="yx-activity-table-scroll"><table className={`yx-activity-matrix ${view === 'month' ? 'is-month' : ''} ${expanded ? 'is-expanded' : ''}`} aria-label="成员每日活动热力图"><thead><tr><th className="yx-activity-name">成员</th>{dates.map(date => { const weekday = new Date(`${date}T00:00:00Z`).getUTCDay(); return <th key={date} className={weekday === 0 || weekday === 6 ? 'is-weekend' : ''}><button type="button" aria-label={`查看 ${date} 全员明细`} onClick={() => setSelection({ date })}><time dateTime={date}>{date.slice(5).replace('-', '/')}</time><small>周{'日一二三四五六'[weekday]}</small></button></th> })}{totalHeaders.map((label, i) => <th key={label} className="yx-activity-total"><Tooltip title={i === 1 && role === 'code' ? '统计期关联需求，按需求 ID 去重' : label === 'Bug' && role === 'code' ? '统计期新增责任 Bug，按 Bug ID 去重' : '所选统计期累计'}>{label}</Tooltip></th>)}</tr></thead><tbody>{members.map(p => <tr key={p.name}><td className="yx-activity-name"><Tooltip title={`${p.name} · ${p.group}${p.inRoster ? '' : ' · 未纳入统计名单'}`}><span>{p.name}</span></Tooltip></td>{dates.map(date => {
          const facts = dayFacts(data, p.name, date, metric), count = facts.events.length, future = date > data.through, outside = date < data.from, weekend = [0, 6].includes(new Date(`${date}T00:00:00Z`).getUTCDay())
          const level = count > 9 ? 4 : count > 5 ? 3 : count > 2 ? 2 : count ? 1 : 0
          const time = facts.last ? dateTime(facts.last).slice(11, 16) : ''
          const tip = <div className="yx-activity-hover"><strong>{p.name} · {date}</strong><span>{count} 次{roleInfo.metrics.find(m => m.value === metric)?.label}{facts.last ? ` · 最后 ${dateTime(facts.last).slice(11)}` : ''}</span>{role === 'code' && <><span>新增 +{lineCount(facts.events, 'additions')} / 删除 −{lineCount(facts.events, 'deletions')} 行</span>{facts.bugs.length > 0 && <span>新增责任 Bug {facts.bugs.length}</span>}<span>{facts.requirements.length ? `关联 ${facts.requirements.length} 项需求` : '没有明确关联需求'}</span>{facts.requirements.slice(0, 3).map(r => <span key={r.externalId}>{r.serial || r.externalId} {r.title || ''}</span>)}{facts.requirements.length > 3 && <small>另 {facts.requirements.length - 3} 项，点击查看</small>}</>}</div>
          return <td key={date} className={weekend ? 'is-weekend' : ''}><Popover content={tip} trigger={['hover', 'focus']} mouseEnterDelay={0.15}><button type="button" disabled={future || outside} className={`yx-activity-cell level-${level} ${future ? 'is-future' : ''} ${outside ? 'is-outside' : ''}`} aria-label={`${p.name} ${date} ${count} 次活动${time ? `，最后 ${time}` : ''}${facts.bugs.length ? `，Bug ${facts.bugs.length}` : ''}`} onClick={() => setSelection({ date, person: p.name })}>{future ? <span className="yx-activity-dot">·</span> : view === 'month' ? <span>{role === 'code' ? facts.bugs.length || '' : count || ''}</span> : <>{time && <time dateTime={facts.last}>{time}</time>}{facts.bugs.length > 0 && <span className="yx-activity-bug">Bug {facts.bugs.length}</span>}{!time && !facts.bugs.length && <span className="yx-activity-dot">·</span>}{expanded && role === 'code' && count > 0 && <><span className="yx-activity-lines">+{lineCount(facts.events, 'additions')} / −{lineCount(facts.events, 'deletions')}</span>{facts.requirements.length > 0 && <small>关联需求 {facts.requirements.length}</small>}</>}{expanded && role !== 'code' && count > 0 && <small>{count} 条记录</small>}</>}</button></Popover></td>
        })}{totals(p).map((value, i) => <td key={i} className={`yx-activity-total ${totalHeaders[i] === 'Bug' && value > 0 ? 'has-bugs' : ''}`}>{totalHeaders[i] === 'Bug' && (!value || role === 'code' && !data.permissions.bugs) ? '—' : compactNumber(value)}</td>)}</tr>)}</tbody></table></div>}
        {data && <div className="yx-activity-chart-footer"><span><i />深浅表示当前指标次数 · 时间为当天最后活动 <em>周六 / 周日</em></span><span>共 {members.length} 位 · 全部展示</span></div>}
      </div>}
    </Spin>
    <details className="yx-activity-notes"><summary>统计口径与数据范围{data?.observedAt ? ` · 最近同步 ${dateTime(data.observedAt)}` : ''}</summary><p>仅展示已同步的真实记录；岗位是指标视角，不自动认定人员岗位。研发按 Git 作者时间、每仓库 SHA 去重，排除合并提交。产品新增按创建时间与创建人，交付按已发布状态、状态时间及当前负责人；历史交付归属来自当前快照。测试按用例创建人和有效缺陷发现人；缺少真实验证事件时不推算验证人数。</p><p>Bug 按本期创建时间和源缺陷责任人归属；不是发现人或本次提交作者，缺少责任人时待归属。多人责任可分别关联，不把个人数量相加当团队去重总数。无记录判断要求已纳入统计名单、时间字段可用且区间采集完整；空白不代表未工作。</p>{data && <p>事件缺少创建人：{data.coverage.unattributedEvents} 条；采集范围{data.coverage.complete ? '已覆盖' : '待核验'}。{role === 'test' && !data.coverage.testcasesCollected && '最新同步尚未确认测试用例完整采集。'}</p>}</details>
    <AuthorIdentities sourceId={sourceId} commits={(data?.events || []).filter(e => e.kind === 'commit').map(e => ({ author: e.actor, authorRaw: e.authorRaw }))} open={manage === 'aliases'} onOpenChange={open => setManage(open ? 'aliases' : null)} onSaved={refreshData} />
    <ActivityRoster sourceId={sourceId} candidates={data?.members || []} open={manage === 'roster'} onClose={() => setManage(null)} onSaved={refreshData} />
    {data && <ActivityDetails data={data} selection={selection} onClose={() => setSelection(null)} metric={metric} scope={scope} openRequirement={openRequirement} />}
  </section>
}
