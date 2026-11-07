import { useEffect, useMemo, useState } from 'react'
import { Alert, Button, Collapse, Drawer, Empty, Input, Space, Spin, Tag } from 'antd'
import { activityApi, type ActivityEvent, type ActivityKind, type TeamActivityData } from './api'
import ActivitySourceLink from './ActivitySourceLink'
import { dateTime } from './codeActivity'
import { lineCount, repositoryKey, uniqueRequirements } from './activityModel'
export type ActivitySelection = { date?: string; person?: string; history?: boolean }
export default function ActivityDetails({ data, selection, onClose, metric, scope, openRequirement }: { data: TeamActivityData; selection: ActivitySelection | null; onClose: () => void; metric: ActivityKind; scope: string[]; openRequirement: (id: string) => void }) {
  const [search, setSearch] = useState(''), [history, setHistory] = useState<TeamActivityData | null>(null), [loading, setLoading] = useState(false), [error, setError] = useState('')
  useEffect(() => { setSearch('') }, [selection])
  useEffect(() => {
    setHistory(null); setError('')
    if (!selection?.history) return
    let live = true; setLoading(true)
    activityApi.activity(data.source.id, data.role, 365).then(v => { if (live) setHistory(v) }).catch(e => { if (live) setError(String(e.message || e)) }).finally(() => { if (live) setLoading(false) })
    return () => { live = false }
  }, [selection, data.source.id, data.role])
  const events = useMemo(() => (selection?.history ? history?.events || [] : data.events).filter(e => selection?.history ? e.actor === selection.person : e.date === selection?.date && e.kind === metric && (!selection.person || e.actor === selection.person) && scope.includes(e.actor)), [selection, history, data.events, metric, scope])
  const bugs = data.bugs.filter(b => b.date === selection?.date && (selection?.person ? b.responsibles.includes(selection.person) : b.responsibles.some(p => scope.includes(p))))
  const names = [...new Set([...events.map(e => e.actor), ...bugs.flatMap(b => b.responsibles.filter(p => scope.includes(p)))])].filter(name => !selection?.person || name === selection.person)
  if (selection?.person && !names.includes(selection.person)) names.push(selection.person)
  const visible = names.filter(name => name.toLowerCase().includes(search.toLowerCase())).sort((a, b) => events.filter(e => e.actor === b).length - events.filter(e => e.actor === a).length || a.localeCompare(b, 'zh-CN'))
  const requirementLink = (r: { externalId: string; serial: string | null; title: string | null; url?: string | null }) => <div className="yx-activity-requirement"><ActivitySourceLink url={r.url}>{r.serial || r.externalId} {r.title || '（名称未同步）'}</ActivitySourceLink><Button type="link" size="small" onClick={() => openRequirement(r.externalId)}>产出明细</Button></div>
  const title = selection?.history ? `${selection.person} · 最近记录` : `${selection?.date || ''} · ${selection?.person || '当天全员'}`
  const eventCard = (e: ActivityEvent) => <div className="yx-activity-evidence" key={e.id}><strong><ActivitySourceLink url={e.url}>{e.serial || e.sha?.slice(0, 10) || e.externalId} {e.title || ''}</ActivitySourceLink></strong><small>{dateTime(e.at)} · {e.status || e.kind}{e.repo ? ` · ${e.repo}` : ''}</small></div>
  return <Drawer title={title} open={!!selection} onClose={onClose} size={720} styles={{ wrapper: { maxWidth: '96vw' } }} rootClassName="yx-activity-drawer" destroyOnHidden>
    {error && <Alert type="error" showIcon title={error} />}
    <Spin spinning={loading}>
      {selection?.history ? <><p className="yx-muted">最近一年已采集记录，不改变主图筛选。{data.members.find(p => p.name === selection.person)?.lastActivityAt && `最近一次：${dateTime(data.members.find(p => p.name === selection.person)!.lastActivityAt!)}`}</p>{events.slice().sort((a, b) => Date.parse(b.at) - Date.parse(a.at)).slice(0, 30).map(eventCard)}{!loading && !events.length && <Empty description="最近一年未采集到该成员的活动记录" />}</> : <>
        <div className="yx-activity-day-summary"><span>成员 <b>{names.length}</b></span><span>当前指标 <b>{events.length}</b> 条</span>{data.role === 'code' && <span>关联需求 <b>{uniqueRequirements(events).length}</b> 项</span>}</div>
        {!selection?.person && <Input.Search allowClear aria-label="搜索当天成员" placeholder="搜索当天成员" value={search} onChange={e => setSearch(e.target.value)} style={{ marginBottom: 16 }} />}
        <Collapse key={`${selection?.date}:${selection?.person || ''}`} accordion defaultActiveKey={selection?.person ? [selection.person] : []} items={visible.map(name => {
          const own = events.filter(e => e.actor === name), ownBugs = bugs.filter(b => b.responsibles.includes(name)), requirements = uniqueRequirements(own)
          const repositories = [...new Set(own.map(repositoryKey))]
          const last = own.map(e => e.at).sort((a, b) => Date.parse(a) - Date.parse(b)).at(-1)
          return { key: name, label: <div className="yx-activity-member-summary"><span><strong>{name}</strong><small>{data.members.find(p => p.name === name)?.group || '源缺陷责任人'}</small></span><span>{own.length} 条{ownBugs.length ? ` · Bug ${ownBugs.length}` : ''}<small>最后 {last ? dateTime(last).slice(11) : '无当前指标记录'}</small></span></div>, children: data.role === 'code' ? <>
            <p className="yx-muted">新增 +{lineCount(own, 'additions')} / 删除 −{lineCount(own, 'deletions')} 行 · 原始账号 {[...new Set(own.map(e => e.authorRaw))].join('、') || '当天无提交'}</p>
            <h4>当天关联需求 · {requirements.length} 项</h4>
            {requirements.map(r => { const linked = own.filter(e => e.requirements.some(link => link.externalId === r.externalId)); return <div className="yx-activity-evidence" key={r.externalId}>{requirementLink(r)}<small>{linked.length} 次关联提交 · {[...new Set(linked.map(e => e.repo || String(e.repoId)))].join('、')}</small><small>+{lineCount(linked, 'additions')} / −{lineCount(linked, 'deletions')} 行</small></div> })}
            {!requirements.length && <span className="yx-muted">没有明确关联需求。</span>}
            {own.some(e => !e.linked) && <p><Tag>另有 {own.filter(e => !e.linked).length} 次提交未关联需求</Tag></p>}
            <h4>变动仓库 · {own.length ? repositories.length : 0}</h4>
            {repositories.map(repo => { const rows = own.filter(e => repositoryKey(e) === repo); return <div className="yx-activity-evidence" key={repo}><strong><ActivitySourceLink url={rows.find(e => e.repositoryUrl)?.repositoryUrl}>{rows[0].repo || repo}</ActivitySourceLink></strong><small>{rows.length} 次 · +{lineCount(rows, 'additions')} / −{lineCount(rows, 'deletions')} 行</small></div> })}
            {!!ownBugs.length && <><h4>当天新增责任 Bug · {ownBugs.length}</h4>{ownBugs.map(b => <div key={b.id} className="yx-activity-evidence"><strong><ActivitySourceLink url={b.url}>{b.serial || b.id} {b.title}</ActivitySourceLink></strong><small>{b.status} · 创建 {dateTime(b.at)}</small></div>)}</>}
            <details className="yx-activity-commit-log"><summary>查看当天提交</summary>{own.map(eventCard)}</details>
          </> : <Space direction="vertical" style={{ width: '100%' }}>{own.map(eventCard)}</Space> }
        })} />
        {!visible.length && <Empty description="没有匹配的已采集活动" />}
      </>}
    </Spin>
  </Drawer>
}
