import { useI18n } from '../../i18n'
import { useEffect, useMemo, useState } from 'react'
import { Alert, Button, Collapse, Drawer, Empty, Input, Space, Spin, Tag } from 'antd'
import { activityApi, type ActivityEvent, type ActivityKind, type TeamActivityData } from './api'
import ActivitySourceLink from './ActivitySourceLink'
import { SonarNewCodeMetrics } from './SonarNewCodeMetrics'
import type { ActivityReview } from './activityReview'
import { dateTime } from './codeActivity'
import { lineCount, passRateText, repositoryKey, uniqueRequirements } from './activityModel'
export type ActivitySelection = { date?: string; person?: string; history?: boolean }
export default function ActivityDetails({ data, selection, onClose, metric, scope, openRequirement, review }: { data: TeamActivityData; selection: ActivitySelection | null; onClose: () => void; metric: ActivityKind; scope: string[]; openRequirement: (id: string) => void; review?: ActivityReview }) {
  const { t } = useI18n()
  const [search, setSearch] = useState(''), [history, setHistory] = useState<TeamActivityData | null>(null), [loading, setLoading] = useState(false), [error, setError] = useState('')
  useEffect(() => { setSearch('') }, [selection])
  useEffect(() => {
    setHistory(null); setError('')
    if (!selection?.history) return
    let live = true; setLoading(true)
    activityApi.activity(data.source.id, data.role, 365).then(v => { if (live) setHistory(v) }).catch(e => { if (live) setError(String(e.message || e)) }).finally(() => { if (live) setLoading(false) })
    return () => { live = false }
  }, [selection, data.source.id, data.role])
  const events = useMemo(() => (selection?.history ? history?.events || [] : data.events).filter(e => selection?.history ? e.actor === selection.person : e.date === selection?.date && e.kind === metric && (selection.person === undefined || e.actor === selection.person) && scope.includes(e.actor)), [selection, history, data.events, metric, scope])
  const bugs = data.bugs.filter(b => b.date === selection?.date && (selection?.person !== undefined ? b.responsibles.includes(selection.person) : b.responsibles.some(p => scope.includes(p))))
  const scans = (data.sonarDaily ?? []).filter(day => day.date === selection?.date &&
    (selection.person !== undefined ? day.member === selection.person : scope.includes(day.member)))
  const regressions = (data.regressionDaily ?? []).filter(day => day.date === selection?.date &&
    (selection.person !== undefined ? day.member === selection.person : scope.includes(day.member)))
  const names = [...new Set([...events.map(e => e.actor), ...scans.map(day => day.member), ...regressions.map(day => day.member), ...bugs.flatMap(b => b.responsibles.filter(p => scope.includes(p)))])].filter(name => selection?.person === undefined || name === selection.person)
  if (selection?.person !== undefined && !names.includes(selection.person)) names.push(selection.person)
  const labelFor = (name: string) => data.members.find(p => p.name === name)?.displayName || name || '未识别作者'
  const visible = names.filter(name => labelFor(name).toLowerCase().includes(search.toLowerCase())).sort((a, b) => events.filter(e => e.actor === b).length - events.filter(e => e.actor === a).length || a.localeCompare(b, 'zh-CN'))
  const requirementLink = (r: { externalId: string; serial: string | null; title: string | null; url?: string | null }) => <div className="yx-activity-requirement"><ActivitySourceLink url={r.url}>{r.serial || r.externalId} {r.title || '（名称未同步）'}</ActivitySourceLink><Button type="link" size="small" onClick={() => openRequirement(r.externalId)}>{t("engineering.activityDetails")}</Button></div>
  const title = selection?.history ? t('engineering.v0LatestActivity', undefined, { v0: labelFor(selection.person || '') }) : `${selection?.date || ''} · ${selection?.person !== undefined ? labelFor(selection.person) : t('engineering.allMembersOnThisDay')}`
  const eventCard = (e: ActivityEvent) => <div className="yx-activity-evidence" key={e.id}><div className="yx-activity-evidence-heading"><strong><ActivitySourceLink url={e.url}>{e.serial || e.sha?.slice(0, 10) || e.externalId} {e.title || ''}</ActivitySourceLink></strong>{e.kind === 'commit' && <ActivitySourceLink url={e.url}>{t("engineering.viewCodeChanges")}</ActivitySourceLink>}</div><small>{dateTime(e.at)} · {e.status || e.kind}{e.repo ? ` · ${e.repo}` : ''}</small>{e.kind === 'commit' && <small>本次新增 +{lineCount([e], 'additions')} / 删除 −{lineCount([e], 'deletions')} 行{!e.linked && ' · 未关联需求'}</small>}{e.kind === 'commit' && <small>分支：{e.branches?.length ? e.branches.join('、') : e.branches ? t("engineering.noScanReturnedForThisBranch") : t("engineering.branchInformationPending")}</small>}</div>
  return <Drawer title={title} open={!!selection} onClose={onClose} size={720} styles={{ wrapper: { maxWidth: '96vw' } }} rootClassName="yx-activity-drawer" destroyOnHidden>
    {error && <Alert type="error" showIcon title={error} />}
    {review && <Alert type="warning" showIcon title={t("engineering.activityNeedsReview")} description={<><p>{review.summary}</p>{review.reasons.map(reason => <div key={reason}>{reason}</div>)}</>} style={{ marginBottom: 16 }} />}
    <Spin spinning={loading}>
      {selection?.history ? <><p className="yx-muted">{t("engineering.collectedActivityFromThePastYearMainChartFiltersStayInEffect")}{data.members.find(p => p.name === selection.person)?.lastActivityAt && t("engineering.latestActivityAtV0", undefined, { v0: dateTime(data.members.find(p => p.name === selection.person)!.lastActivityAt!) })}</p>{events.slice().sort((a, b) => Date.parse(b.at) - Date.parse(a.at)).slice(0, 30).map(eventCard)}{!loading && !events.length && <Empty description={t("engineering.noActivityCollectedForThisMemberInThePastYear")} />}</> : <>
        <div className="yx-activity-day-summary"><span>{t("engineering.memberAccount")} <b>{names.filter(Boolean).length}</b></span>{names.includes('') && <span>另有未识别作者提交</span>}<span>{t("engineering.selectedMetric")} <b>{events.length}</b> 条</span>{data.role === 'code' && <span>{t("engineering.linkedRequirements")} <b>{uniqueRequirements(events).length}</b> 项</span>}</div>
        {selection?.person === undefined && <Input.Search allowClear aria-label={t("engineering.searchMembersForThisDay")} placeholder={t("engineering.searchMembersForThisDay")} value={search} onChange={e => setSearch(e.target.value)} style={{ marginBottom: 16 }} />}
        <Collapse key={`${selection?.date}:${selection?.person ?? ''}`} accordion defaultActiveKey={selection?.person !== undefined ? [selection.person] : []} items={visible.map(name => {
          const own = events.filter(e => e.actor === name), ownBugs = bugs.filter(b => b.responsibles.includes(name)), requirements = uniqueRequirements(own)
          const sonar = scans.find(day => day.member === name)
          const regression = regressions.find(day => day.member === name)
          const repositories = [...new Set(own.map(repositoryKey))]
          const last = own.map(e => e.at).sort((a, b) => Date.parse(a) - Date.parse(b)).at(-1)
          return { key: name, label: <div className="yx-activity-member-summary"><span><strong>{labelFor(name)}</strong><small>{data.members.find(p => p.name === name)?.group || t("engineering.defectOwnerInSource")}</small></span><span>{own.length} 条{ownBugs.length ? ` · Bug ${ownBugs.length}` : ''}<small>最后 {last ? dateTime(last).slice(11) : t("engineering.noRecordsForThisMetric")}</small></span></div>, children: data.role === 'code' ? <>
            <p className="yx-muted">新增 +{lineCount(own, 'additions')} / 删除 −{lineCount(own, 'deletions')} 行 · 原始账号 {[...new Set(own.map(e => e.authorRaw))].join('、') || t("engineering.noCommitsOnThisDay")}</p>
            {sonar && <div className="yx-activity-evidence">
              <strong>Sonar 新增代码扫描 · {sonar.pushes} 次</strong>
              <small>{t("engineering.gatePassRate")} {passRateText(sonar.passed, sonar.passed + sonar.failed)} · 通过 {sonar.passed} · 失败 {sonar.failed} · 未评价 {sonar.pushes - sonar.passed - sonar.failed}</small>
              {(sonar.recent ?? []).map(analysis => <SonarNewCodeMetrics key={analysis.taskId} analysis={analysis} />)}
              {!!sonar.unverifiedBaseline && <small>{sonar.unverifiedBaseline} 次扫描的 Push 新代码基线待核验</small>}
            </div>}
            {regression && <div className="yx-activity-evidence">
              <strong>回归用例通过率 · {passRateText(regression.passed, regression.evaluated)}</strong>
              <small>{t("engineering.passed")} {regression.passed} / 已执行 {regression.evaluated}</small>
            </div>}
            <h4>当天关联需求 · {requirements.length} 项</h4>
            {requirements.map(r => { const linked = own.filter(e => e.requirements.some(link => link.externalId === r.externalId)); return <div className="yx-activity-evidence" key={r.externalId}>{requirementLink(r)}<small>{linked.length} 次关联提交 · {[...new Set(linked.map(e => e.repo || String(e.repoId)))].join('、')}</small><small>+{lineCount(linked, 'additions')} / −{lineCount(linked, 'deletions')} 行</small></div> })}
            {!requirements.length && <span className="yx-muted">{t("engineering.noExplicitlyLinkedRequirements")}</span>}
            {own.some(e => !e.linked) && <p><Tag>另有 {own.filter(e => !e.linked).length} 次提交未关联需求</Tag></p>}
            <h4>变动仓库 · {own.length ? repositories.length : 0}</h4>
            {repositories.map(repo => { const rows = own.filter(e => repositoryKey(e) === repo); return <div className="yx-activity-evidence" key={repo}><strong><ActivitySourceLink url={rows.find(e => e.repositoryUrl)?.repositoryUrl}>{rows[0].repo || repo}</ActivitySourceLink></strong><small>{rows.length} 次 · +{lineCount(rows, 'additions')} / −{lineCount(rows, 'deletions')} 行</small></div> })}
            {!!ownBugs.length && <><h4>当天新增责任 Bug · {ownBugs.length}</h4>{ownBugs.map(b => <div key={b.id} className="yx-activity-evidence"><strong><ActivitySourceLink url={b.url}>{b.serial || b.id} {b.title}</ActivitySourceLink></strong><small>{b.status} · 创建 {dateTime(b.at)}</small></div>)}</>}
            <details className="yx-activity-commit-log" open={selection?.person !== undefined && own.length <= 10}><summary>{t("engineering.viewCommitsForThisDay")}</summary>{own.map(eventCard)}</details>
          </> : <Space direction="vertical" style={{ width: '100%' }}>{own.map(eventCard)}</Space> }
        })} />
        {!visible.length && <Empty description={t("engineering.noMatchingCollectedActivity")} />}
      </>}
    </Spin>
  </Drawer>
}
