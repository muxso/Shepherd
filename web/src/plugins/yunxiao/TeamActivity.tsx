import { ReadError } from '../../components/Page'
import { useI18n } from '../../i18n'
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
import ActivityScreenshot from './ActivityScreenshot'
import ActivityDetails, { type ActivitySelection } from './ActivityDetails'
import './team-activity.css'
const RequirementActivityAnalysis = lazy(() => import('./RequirementActivityAnalysis'))
function roleFrom(search: string): ActivityRole { const value = new URLSearchParams(search).get('role'); return value === 'product' || value === 'test' ? value : 'code' }
function daysFrom(search: string) { const value = Number(new URLSearchParams(search).get('days') || 7); return Number.isInteger(value) && value >= 1 && value <= 365 ? value : 7 }
function currentMonthDays() {
  const day = new Intl.DateTimeFormat('en-US', { timeZone: 'Asia/Shanghai', day: 'numeric' }).format(new Date())
  return Number(day)
}
function metricFrom(search: string): ActivityKind { const q = new URLSearchParams(search), info = activityRoles.find(r => r.value === roleFrom(search))!; return info.metrics.find(m => m.value === q.get('activityMetric'))?.value || info.metrics[0].value }
const sortLabels: Record<ActivitySort, string> = { commits: 'sortByCommits', bugs: 'sortByBugs', last: 'sortByLastCommit', repositories: 'sortByRepositories', requirements: 'sortByRequirements', lines: 'sortByLines' }
type ListMode = 'all' | 'absence' | 'review' | 'unmapped'
export default function TeamActivity({ sourceId, revision }: { sourceId: string; revision: number }) {
  const { t } = useI18n()
  const location = useLocation(), navigate = useNavigate(), initial = new URLSearchParams(location.search)
  const [role, setRole] = useState<ActivityRole>(() => roleFrom(location.search)), [days, setDays] = useState(() => daysFrom(location.search)), [metric, setMetric] = useState<ActivityKind>(() => metricFrom(location.search))
  const [view, setView] = useState(initial.get('activity') === 'month' ? 'month' : 'week'), [perspective, setPerspective] = useState(initial.get('perspective') === 'group' ? 'group' : 'role'), [group, setGroup] = useState(initial.get('activityGroup') || '')
  const colorByLines = role === 'code' && metric === 'commit' && initial.get('activityColor') === 'lines'
  const queryDays = view === 'month' ? currentMonthDays() : days
  const [data, setData] = useState<TeamActivityData | null>(null), [loading, setLoading] = useState(false), [error, setError] = useState<unknown>(), [refresh, setRefresh] = useState(0)
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
    let live = true; setLoading(true); setError(undefined); setData(null); setSelection(null)
    const timer = window.setTimeout(() => {
      activityApi.activity(sourceId, role, queryDays)
        .then(value => { if (live) setData(value) })
        .catch(e => { if (live) setError(e) })
        .finally(() => { if (live) setLoading(false) })
    }, 120)
    return () => { live = false; window.clearTimeout(timer) }
  }, [sourceId, role, queryDays, revision, refresh])
  useEffect(() => {
    if (!data || data.role !== 'code' || data.regressionDaily) return
    let live = true
    activityApi.regressionActivity(data.days).then(regressionDaily => {
      if (live) setData(current => current === data ? { ...current, regressionDaily } : current)
    }).catch(() => {})
    return () => { live = false }
  }, [data])
  const roleKeys = { code: 'engineeringCode', product: 'productRequirements', test: 'testingQuality' }
  const metricKeys = { commit: 'commits', requirementCreated: 'newRequirements', requirementDelivered: 'deliveredRequirements', bugReported: 'validDefectsReported', testcaseCreated: 'newTestCases' }
  const localizedRoles = activityRoles.map(item => ({
    ...item,
    label: t('engineering.' + roleKeys[item.value]),
    metrics: item.metrics.map(metric => ({ ...metric, label: t('engineering.' + metricKeys[metric.value as keyof typeof metricKeys], metric.label) })),
  }))
  const allMetrics = localizedRoles.flatMap(item => item.metrics)
  const roleInfo = localizedRoles.find(item => item.value === role)!
  const reviews = useMemo(() => new Map<string, ActivityReview>((data?.members || []).flatMap(person => { const review = data && reviewMember(data, person); return review ? [[person.name, review] as const] : [] })), [data])
  const filtered = useMemo(() => (data?.members || []).filter(person => (!group || person.group === group) && (!search || `${memberLabel(person)} ${data?.events.filter(e => e.actor === person.name).map(e => e.authorRaw).join(' ')}`.toLowerCase().includes(search.toLowerCase()))).sort((a, b) => {
    const difference = data ? activitySortValue(data, b, metric, sort) - activitySortValue(data, a, metric, sort) : 0
    return difference * (order === 'desc' ? 1 : -1) || memberLabel(a).localeCompare(memberLabel(b), 'zh-CN')
  }), [data, group, search, metric, sort, order])
  const currentBugActors = new Set(data?.bugs.flatMap(b => b.responsibles) || [])
  const qualityMembers = new Set([...(data?.sonarDaily ?? []), ...(data?.regressionDaily ?? [])].map(day => day.member))
  const historical = (person: ActivityMember) => !person.inRoster && person.status !== 'active' && !currentBugActors.has(person.name) && !qualityMembers.has(person.name)
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
  const sortOptions = (role === 'code' ? Object.keys(sortLabels) as ActivitySort[] : role === 'product' ? ['commits', 'last', 'requirements'] as ActivitySort[] : ['commits', 'last', 'bugs'] as ActivitySort[]).filter(key => key !== 'bugs' || !data || role !== 'code' || data.permissions.bugs).map(value => ({ value, label: value === 'commits' && role !== 'code' ? t('engineering.sortByActivity') : t('engineering.' + sortLabels[value]) }))
  const analysisData = data && (group || search) ? { ...data, events: data.events.filter(e => filtered.some(p => p.name === e.actor)), bugs: data.bugs.filter(b => b.responsibles.some(name => filtered.some(p => p.name === name))) } : data
  return <section className="yx-activity-board" aria-label={t("engineering.teamActivityAndOutput")}>
    <div className="yx-activity-toolbar"><Segmented aria-label={t("engineering.roleView")} value={perspective === 'role' ? role : ''} onChange={v => chooseRole(v as ActivityRole)} options={localizedRoles.map(r => ({ value: r.value, label: r.label }))} /><Space size={8} wrap><Button onClick={showAnalysis}>{t("engineering.requirementEffortAndQuality")}</Button><Dropdown menu={{ items: [{ key: 'aliases', label: t("engineering.manageLinkedAccounts") }, { key: 'roster', label: t("engineering.reportingRoster") }, { key: 'review', label: t("engineering.activityReviewRules") }], onClick: e => setManage(e.key as 'aliases' | 'roster' | 'review') }}><Button>{t("engineering.manageMembers")}</Button></Dropdown></Space></div>
    <div className="yx-activity-filters"><Space size={[8, 8]} wrap>
      {view === 'month' ? <span>{t("engineering.monthToDate")}</span> : <label className="yx-activity-days">{t("engineering.last")} <InputNumber aria-label={t("engineering.recentDays")} min={1} max={365} precision={0} value={days} onChange={value => { if (value != null) { const next = Math.min(365, Math.max(1, Number(value))); setDays(next); updateLocation({ days: String(next) }) } }} />{t("engineering.days")}</label>}
      <Select aria-label={t("engineering.activityMetric")} value={metric} onChange={chooseMetric} options={perspective === 'group' ? allMetrics : roleInfo.metrics} style={{ width: 150 }} />
      <Tooltip title="与岗位视角互斥；选择具体分组后按当前活动指标查看"><Select aria-label={t("engineering.organizationGroup")} allowClear placeholder={t("engineering.viewByOrganizationGroup")} value={perspective === 'group' ? group : undefined} onChange={chooseGroup} options={groups.map(value => ({ value, label: value }))} style={{ width: 165 }} /></Tooltip>
      <Input.Search aria-label={t("engineering.searchMembers")} placeholder={t("engineering.searchNameOrOriginalAccount")} value={search} allowClear onChange={e => setSearch(e.target.value)} style={{ width: 190 }} />
      {data && <span className="yx-activity-date-range">{data.from.slice(5).replace('-', '/')}–{data.through.slice(5).replace('-', '/')} · 上海时间{perspective === 'group' ? ' · 分组视角' : ''}</span>}
    </Space><Space size={6} wrap><Button className="yx-activity-no-records" type={listMode === 'review' ? 'primary' : 'default'} onClick={() => toggleList('review')}>{t("engineering.reviewNeeded")}{data ? `（${reviewCount}）` : ''}</Button><Button className="yx-activity-no-records" onClick={() => toggleList('absence')}>{listMode === 'absence' ? t("engineering.backToAllMembers") : `查看未记录产出人员${data ? `（${noRecords}）` : ''}`}</Button>{role === 'code' && <Button type={listMode === 'unmapped' ? 'primary' : 'default'} onClick={() => toggleList('unmapped')}>{t("engineering.unlinked")}{data ? `（${unassignedCount}）` : ''}</Button>}</Space></div>
    <div className="yx-activity-stat-strip" aria-label={t("engineering.statisticsOverview")}>{[
      [t("engineering.membersTracked"), subjects.length, '含已归属成员和原始 Git 账号；未识别作者的提交另行汇总'], [t("engineering.activityRecorded"), subjects.filter(p => p.status === 'active').length, '本期对应岗位活动有记录，低活跃记录另列需核验'], [t("engineering.noActivityRecorded"), noRecords, '已纳入名单、采集完整且本期无记录'], [t("engineering.collectionReviewPending"), unverified, '名单或时间范围不完整，不直接判零'],
    ].map(([label, count, note]) => <Tooltip key={label} title={note}><span>{label}<strong>{data ? count : '—'}</strong></span></Tooltip>)}{role === 'code' && <Tooltip title="原始 Git 作者与未识别作者的记录均计入提交和变更行数"><span>{t("engineering.unlinkedAccounts")}<strong>{data ? unmappedCount : '—'}</strong></span></Tooltip>}{role === 'code' && !!data?.coverage.unattributedEvents && <span>{t("engineering.commitsByUnidentifiedAuthors")}<strong>{data.coverage.unattributedEvents}</strong></span>}</div>
    {error != null && <ReadError error={error} title={t("engineering.teamReadFailed", "团队活动读取失败")} />}
    <Spin spinning={loading}>
      {data && analysis ? <Suspense fallback={<Spin />}><RequirementActivityAnalysis data={analysisData!} onBack={() => setAnalysis(false)} openRequirement={openRequirement} /></Suspense> : <div className="yx-activity-chart">
        <div className="yx-activity-chart-heading"><h3>{listMode === 'absence' ? `${view === 'month' ? t("engineering.thisMonth") : t("engineering.lastV0Days", undefined, { v0: queryDays })}未记录产出人员` : listMode === 'review' ? t("engineering.activityRequiringReview") : listMode === 'unmapped' ? t("engineering.gitActivityFromUnlinkedAccounts") : t("engineering.v0MemberHeatmap", undefined, { v0: roleInfo.metrics.find(m => m.value === metric)?.label ?? '' })}</h3><Space size={6} wrap>{listMode !== 'absence' && <><Select size="small" aria-label={t("engineering.sortMembers")} value={sort} onChange={value => { setSort(value); updateLocation({ activitySort: value }) }} options={sortOptions} style={{ width: 155 }} /><Button size="small" aria-label={t("engineering.toggleSortDirection")} onClick={() => { const next = order === 'desc' ? 'asc' : 'desc'; setOrder(next); updateLocation({ activityOrder: next }) }}>{order === 'desc' ? t("engineering.descending") : t("engineering.ascending")}</Button>{hiddenHistory > 0 && <Button size="small" onClick={() => setShowHistory(v => !v)}>{showHistory ? t("engineering.hideFormerMembers") : t("engineering.formerMembersV0", undefined, { v0: hiddenHistory })}</Button>}<Button size="small" type={expanded ? 'primary' : 'default'} onClick={() => setExpanded(v => !v)}>{expanded ? t("engineering.hideCellMetrics") : t("engineering.showCellMetrics")}</Button>{role === 'code' && metric === 'commit' && <Select size="small" aria-label={t("engineering.heatmapColorScale")} value={colorByLines ? 'lines' : 'commits'}
          onChange={value => updateLocation({ activityColor: value === 'lines' ? 'lines' : '' })}
          options={[{ value:'commits',label:t("engineering.colorByCommits") },{ value:'lines',label:t("engineering.colorByChangedLines") }]} />}
          <ActivityScreenshot through={data?.through} disabled={!members.length || loading} /><Segmented size="small" aria-label={t("engineering.heatmapTimeRange")} value={view} onChange={value => { setView(String(value)); updateLocation({ activity: String(value) }) }} options={[{ value: 'week', label: t("engineering.lastV0Days", undefined, { v0: Math.min(7, days) }) }, { value: 'month', label: t("engineering.monthlyOverview") }]} /></>}</Space></div>
        {listMode === 'review' && <p className="yx-muted">按岗位活动及已保存规则筛查；点击标记成员查看活跃天数、提交量、行数和原始证据。</p>}
        {listMode === 'absence' && data && !data.coverage.rosterConfigured && <Alert type="info" showIcon title="尚未配置完整统计名单，无法判断组织全员是否无记录" action={<Button size="small" onClick={() => setManage('roster')}>{t("engineering.manageRoster")}</Button>} style={{ marginBottom: 12 }} />}
        {listMode === 'absence' && data && !data.coverage.complete && <Alert type="info" showIcon title="所选区间采集尚未覆盖完整，无记录者保留待核验" style={{ marginBottom: 12 }} />}
        {!members.length ? <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={loading ? t("engineering.loadingCollectedData") : listMode === 'absence' ? '没有可确认的未记录产出人员' : listMode === 'review' ? '所选范围没有触发核验规则的记录' : t("engineering.noSyncedMembersOrActivityInThisRange")} /> : listMode === 'absence' ? <div className="yx-activity-table-scroll"><table className="yx-activity-absence"><thead><tr><th>{t("engineering.member")}</th><th>{t("engineering.group")}</th><th>{t("engineering.mostRecentActivity")}</th><th>{t("engineering.activityThisPeriod")}</th><th>{t("engineering.reportingStatus")}</th><th /></tr></thead><tbody>{members.map(p => <tr key={p.name}><td>{memberLabel(p)}</td><td>{p.group}</td><td>{p.lastActivityAt ? dateTime(p.lastActivityAt) : t("engineering.noRecordsInTheCollectedRange")}</td><td>0 条</td><td>{p.status === 'noRecords' ? `${view === 'month' ? t("engineering.thisMonth") : t("engineering.lastV0Days", undefined, { v0: queryDays })}未记录产出` : t("engineering.pendingReview")}</td><td><Button type="link" size="small" onClick={() => setSelection({ person: p.name, history: true })}>{t("engineering.mostRecentActivity")}</Button></td></tr>)}</tbody></table></div> : data && <ActivityMatrix data={data} members={members} metric={metric} view={view} expanded={expanded} colorByLines={colorByLines} reviews={reviews} select={setSelection} associate={() => setManage('aliases')} />}
        {data && <div className="yx-activity-chart-footer"><span><i />{colorByLines ? '深浅按已知新增与删除行数之和' : role === 'code' ? '深浅按提交次数' : '深浅按当前指标次数'} · 时间为当天最后活动{role === 'code' && ' · S 为 Sonar 门禁通过率，回为回归用例通过率'} <em>{t("engineering.saturdaySunday")}</em></span><span>{t("engineering.current")} {members.length} 项{!showHistory && hiddenHistory > 0 && listMode === 'all' ? ` · 历史 ${hiddenHistory} 项已收起` : ' · 全部展示'}</span></div>}
      </div>}
    </Spin>
    <AuthorIdentities sourceId={sourceId} commits={(data?.events || []).filter(e => e.kind === 'commit' && e.actor).map(e => ({ author: e.actor, authorRaw: e.authorRaw }))} open={manage === 'aliases'} onOpenChange={open => setManage(open ? 'aliases' : null)} onSaved={refreshData} />
    <ActivityRoster sourceId={sourceId} candidates={(data?.members || []).filter(p => p.name)} open={manage === 'roster'} onClose={() => setManage(null)} onSaved={refreshData} />
    <ActivityReviewSettings sourceId={sourceId} open={manage === 'review'} onClose={() => setManage(null)} onSaved={refreshData} />
    {data && <ActivityDetails data={data} selection={selection} onClose={() => setSelection(null)} metric={metric} scope={scope} openRequirement={openRequirement} review={selection?.person !== undefined ? reviews.get(selection.person) : undefined} />}
  </section>
}
