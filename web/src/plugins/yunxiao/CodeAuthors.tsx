import { useEffect, useMemo, useState } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { Alert, Button, Card, DatePicker, Empty, Popover, Segmented, Select, Space, Spin, Table, Tag, Tooltip } from 'antd'
import dayjs from 'dayjs'
import { devopsApi, type CodeAuthorsData, type CodeCommit } from './api'

const zone = 'Asia/Shanghai'
const formatter = new Intl.DateTimeFormat('en-GB', { timeZone: zone, year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', second: '2-digit', hourCycle: 'h23' })
const parts = (date: Date) => Object.fromEntries(formatter.formatToParts(date).map(p => [p.type, p.value]))
function dateTime(value: string) { const p = parts(new Date(value)); return `${p.year}-${p.month}-${p.day} ${p.hour}:${p.minute}:${p.second}` }
function dateKey(value: string) { return dateTime(value).slice(0, 10) }
const currentMonth = () => dateTime(new Date().toISOString()).slice(0, 7)
function initialMonth(search: string) { const v = new URLSearchParams(search).get('month') || ''; return /^\d{4}-(0[1-9]|1[0-2])$/.test(v) && Number(v.slice(0, 4)) > 0 ? v : currentMonth() }
const latest = (items: CodeCommit[]) => items.reduce((a, c) => (!a || Date.parse(c.authoredAt) > Date.parse(a)) ? c.authoredAt : a, '')
const knownSum = (items: CodeCommit[], field: 'additions' | 'deletions') => items.reduce((n, c) => n + (c[field] ?? 0), 0)
const unknown = (items: CodeCommit[]) => items.filter(c => c.additions == null || c.deletions == null).length
const languages = (items: CodeCommit[]) => Array.from(new Set(items.flatMap(c => c.languages || []))).sort()
function lineTotal(items: CodeCommit[], field: 'additions' | 'deletions') {
  const missing = items.filter(c => c[field] == null).length
  return <Tooltip title={missing ? `另有 ${missing} 次提交的${field === 'additions' ? '新增' : '删除'}行数未采集` : undefined}>{missing ? `≥${knownSum(items, field).toLocaleString()}` : knownSum(items, field).toLocaleString()}</Tooltip>
}
function LanguageTags({ items }: { items: CodeCommit[] }) {
  const values = languages(items)
  return values.length ? <Tooltip title={`仓库默认分支源码，按扩展名与字节数识别；语言组成：${values.join('、')}`}><Space size={[0, 4]} wrap>{values.slice(0, 3).map(v => <Tag key={v}>{v}</Tag>)}{values.length > 3 && <Tag>+{values.length - 3}</Tag>}</Space></Tooltip> : <Tag>未识别</Tag>
}
function groupBy(items: CodeCommit[], key: (c: CodeCommit) => string) {
  const values = new Map<string, CodeCommit[]>()
  for (const c of items) { const k = key(c); const row = values.get(k); if (row) row.push(c); else values.set(k, [c]) }
  return Array.from(values, ([key, commits]) => ({ key, commits }))
}

function Trend({ commits, metric, canRequirements, openRequirement }: { commits: CodeCommit[]; metric: string; canRequirements: boolean; openRequirement: (id: string) => void }) {
  const [active, setActive] = useState<string | null>(null)
  const points = useMemo(() => groupBy(commits, c => dateKey(c.authoredAt)).map(row => ({ ...row, at: latest(row.commits), value: metric === 'commits' ? row.commits.length : unknown(row.commits) ? null : knownSum(row.commits, 'additions') + knownSum(row.commits, 'deletions') })).sort((a, b) => a.key.localeCompare(b.key)), [commits, metric])
  if (!points.length) return <Empty description="所选范围暂无代码提交" />
  const min = Date.parse(points[0].at), max = Date.parse(points[points.length - 1].at)
  const x = (at: string) => max === min ? 510 : 64 + (Date.parse(at) - min) / (max - min) * 900
  const ceiling = Math.max(4, Math.ceil(Math.max(...points.map(p => p.value ?? 0)) / 4) * 4)
  const y = (value: number) => 258 - value / ceiling * 206
  const segments: string[] = []; let segment = ''
  for (const point of points) { if (point.value == null) { if (segment) segments.push(segment); segment = '' } else segment += `${x(point.at)},${y(point.value)} ` }
  if (segment) segments.push(segment)
  const stride = Math.max(1, Math.ceil(points.length / 6))
  return <>
    <div style={{ overflowX: 'auto', maxWidth: '100%' }}>
      <svg role="group" aria-label="每日代码提交折线趋势，横轴为每日最后提交时间" viewBox="0 0 1020 340" style={{ width: '100%', minWidth: 680, display: 'block' }}>
        {[0, 1, 2, 3, 4].map(i => <g key={i}><line x1={64} x2={964} y1={52 + i * 51.5} y2={52 + i * 51.5} stroke="var(--border-soft)" /><text x={55} y={56 + i * 51.5} textAnchor="end" fontSize={12} fill="var(--text-3)">{Math.round(ceiling * (4 - i) / 4).toLocaleString()}</text></g>)}
        <text x={64} y={26} fontSize={13} fill="var(--text-2)">{metric === 'commits' ? '提交次数（次）' : '代码变更行数（新增 + 删除）'}</text>
        {segments.map((s, i) => <polyline key={i} points={s} fill="none" stroke="var(--brand)" strokeWidth={2.5} />)}
        {points.map((p, index) => {
          const repos = groupBy(p.commits, c => c.repoId)
          const requirements = Array.from(new Map(p.commits.flatMap(c => c.requirements).map(r => [r.externalId, r])).values())
          const label = `${p.key} 最后提交 ${dateTime(p.at).slice(11)}，${p.commits.length} 次提交`
          return <g key={p.key}>
            <Popover open={active === p.key} onOpenChange={open => setActive(open ? p.key : null)} trigger={['hover', 'click']} placement="top" content={<div data-testid="code-author-point-detail" style={{ width: 370, maxWidth: 'calc(100vw - 56px)', maxHeight: 320, overflowY: 'auto', overflowWrap: 'anywhere' }}>
              <strong>{p.key} · {p.commits.length} 次提交</strong><div>最后提交：{dateTime(p.at)}（上海）</div>
              <div>新增 {lineTotal(p.commits, 'additions')} / 删除 {lineTotal(p.commits, 'deletions')} 行{unknown(p.commits) > 0 && ` · ${unknown(p.commits)} 次行数未知`}</div>
              <div style={{ marginTop: 10, fontWeight: 600 }}>代码仓库</div>{repos.map(r => <div key={r.key}>{r.commits[0].repo || r.key} · {r.commits.length} 次 <LanguageTags items={r.commits} /></div>)}
              <div style={{ marginTop: 10, fontWeight: 600 }}>关联需求</div>{!canRequirements ? <span>无需求读取权限</span> : requirements.length ? requirements.map(r => r.title ? <div key={r.externalId}><Button type="link" size="small" style={{ whiteSpace: 'normal', height: 'auto', textAlign: 'left', paddingInline: 0 }} onClick={() => openRequirement(r.externalId)}>{r.serial || r.externalId} {r.title}</Button></div> : <div key={r.externalId}>{r.externalId}（名称未同步）</div>) : <span>未关联需求</span>}
            </div>}>
              <g role="button" tabIndex={0} onFocus={e => { if (e.currentTarget.matches(':focus-visible')) setActive(p.key) }} onKeyDown={e => { if (e.key === 'Escape') setActive(null); if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); setActive(p.key) } }} aria-label={label} style={{ cursor: 'pointer', outlineOffset: 3 }}><circle cx={x(p.at)} cy={p.value == null ? 258 : y(p.value)} r={12} fill="transparent" /><circle cx={x(p.at)} cy={p.value == null ? 258 : y(p.value)} r={4.5} fill={p.value == null ? 'var(--panel)' : 'var(--brand)'} stroke="var(--brand)" strokeWidth={2} /></g>
            </Popover>
            {(index % stride === 0 || index === points.length - 1) && <text x={x(p.at)} y={283} textAnchor="middle" fontSize={12} fill="var(--text-3)"><tspan x={x(p.at)}>{p.key.slice(5)}</tspan><tspan x={x(p.at)} dy={17}>{dateTime(p.at).slice(11, 16)}</tspan></text>}
          </g>
        })}
        <text x={514} y={331} textAnchor="middle" fontSize={12} fill="var(--text-3)">每日最后提交时间（Git 作者时间 · 上海时区）</text>
      </svg>
    </div>
    {metric === 'lines' && points.some(p => p.value == null) && <div className="yx-muted">空心点表示当天存在未采集的行数，该日期不绘制代码量数值；可悬停查看已知数据。</div>}
    <div className="yx-muted">每个有提交的自然日一个点，以当天最后一次提交的实际作者时间定位；无提交日期不生成时间点。</div>
  </>
}

export default function CodeAuthors({ sourceId, revision }: { sourceId: string; revision: number }) {
  const location = useLocation(), navigate = useNavigate()
  const [month, setMonth] = useState(() => initialMonth(location.search))
  const [data, setData] = useState<CodeAuthorsData | null>(null), [loading, setLoading] = useState(false), [error, setError] = useState('')
  const [author, setAuthor] = useState(''), [group, setGroup] = useState(''), [repo, setRepo] = useState(''), [language, setLanguage] = useState(''), [metric, setMetric] = useState('commits')
  useEffect(() => { setMonth(initialMonth(location.search)) }, [location.search])
  useEffect(() => { let live = true; setLoading(true); setError(''); setData(null); devopsApi.codeAuthors(sourceId, month).then(v => { if (live) setData(v) }).catch(e => { if (live) setError(String(e.message || e)) }).finally(() => { if (live) setLoading(false) }); return () => { live = false } }, [sourceId, month, revision])
  const commits = data?.commits || []
  const selected = commits.filter(c => (!author || c.author === author) && (!group || c.group === group) && (!repo || c.repoId === repo) && (!language || (language === '?' ? !c.languages.length : c.languages.includes(language))))
  const authors = groupBy(selected, c => JSON.stringify([c.author, c.group])).map(r => ({ ...r, name: r.commits[0].author || '待归属', group: r.commits[0].group || '待归属', last: latest(r.commits) })).sort((a, b) => b.commits.length - a.commits.length || a.name.localeCompare(b.name))
  const options = (values: string[], all: string) => [{ value: '', label: all }, ...Array.from(new Set(values)).filter(Boolean).sort().map(value => ({ value, label: value }))]
  const openRequirement = (id: string) => navigate(`/devops?${new URLSearchParams({ source: sourceId, reference: id, month, view: 'delivery' })}`)
  const changeMonth = (value: string) => { const next = new URLSearchParams(location.search); next.set('month', value); next.set('view', 'authors'); next.set('source', sourceId); navigate({ pathname: location.pathname, search: next.toString() }) }
  return <div style={{ minWidth: 0 }}>
    <Space wrap size={[10, 12]} style={{ marginBottom: 16, maxWidth: '100%' }}>
      <label>统计月份 <DatePicker aria-label="代码统计月份" picker="month" allowClear={false} value={dayjs(`${month}-01`)} format="YYYY年MM月" onChange={v => { if (v) changeMonth(v.format('YYYY-MM')) }} /></label>
      <Select aria-label="代码作者筛选" showSearch style={{ width: 175, maxWidth: '100%' }} value={author} onChange={setAuthor} options={options(commits.map(c => c.author), '全部作者')} />
      <Select aria-label="代码分组筛选" style={{ width: 175, maxWidth: '100%' }} value={group} onChange={setGroup} options={options(commits.map(c => c.group), '全部分组')} />
      <Select aria-label="代码仓库筛选" showSearch optionFilterProp="label" style={{ width: 240, maxWidth: '100%' }} value={repo} onChange={setRepo} options={[{ value: '', label: '全部仓库' }, ...Array.from(new Map(commits.map(c => [c.repoId, c.repo || c.repoId])).entries()).sort((a, b) => a[1].localeCompare(b[1])).map(([value, label]) => ({ value, label }))]} />
      <Select aria-label="仓库语言筛选" style={{ width: 165, maxWidth: '100%' }} value={language} onChange={setLanguage} options={[...options(languages(commits), '全部仓库语言'), ...(commits.some(c => !c.languages.length) ? [{ value: '?', label: '未识别语言' }] : [])]} />
    </Space>
    {data?.source.last_success_at && <div className="yx-muted" style={{ marginBottom: 12 }}>最近同步 {dateTime(data.source.last_success_at)}（上海）；仅统计已采集提交{data.source.code_since && data.source.code_until ? `，最近采集范围 ${dateTime(data.source.code_since)} 至 ${dateTime(data.source.code_until)}（不含）` : ''}。</div>}
    {error && <Alert type="error" showIcon message={error} style={{ marginBottom: 12 }} />}
    <Spin spinning={loading}>
      <Card title={`${author || '全部作者'} · 每日代码提交趋势`} size="small" style={{ marginBottom: 16 }} extra={<Segmented aria-label="代码趋势指标" value={metric} onChange={v => setMetric(String(v))} options={[{ value: 'commits', label: '提交次数' }, { value: 'lines', label: '变更行数' }]} />}>
        <Trend commits={selected} metric={metric} canRequirements={!!data?.permissions.requirements} openRequirement={openRequirement} />
      </Card>
      <Card title="代码作者与仓库明细" size="small">
        <div className="yx-muted" style={{ marginBottom: 12 }}>本期 {selected.length.toLocaleString()} 次提交 · {new Set(selected.map(c => c.repoId)).size} 个仓库。每仓库 SHA 去重，排除合并提交；仓库语言按默认分支源码扩展名识别，最近提交为本期最后一次 Git 作者时间。</div>
        <Table size="small" rowKey="key" dataSource={authors} pagination={{ pageSize: 10, showSizeChanger: false }} scroll={{ x: 1150 }} expandable={{ expandedRowRender: row => <Table size="small" rowKey="key" pagination={false} scroll={{ x: 920 }} dataSource={groupBy(row.commits, c => c.repoId)} columns={[
          { title: '代码仓库', render: (_, r) => <Tooltip title={r.commits[0].repo}><span>{r.commits[0].repo || r.key}</span></Tooltip>, width: 250, ellipsis: true },
          { title: '仓库语言', render: (_, r) => <LanguageTags items={r.commits} />, width: 210 },
          { title: '提交次数', render: (_, r) => r.commits.length, width: 90 },
          { title: '新增 / 删除行', render: (_, r) => <>{lineTotal(r.commits, 'additions')} / {lineTotal(r.commits, 'deletions')}</>, width: 130 },
          { title: '本期最近提交', render: (_, r) => dateTime(latest(r.commits)), width: 185 },
        ]} /> }} columns={[
          { title: '实际作者', render: (_, r) => <Tooltip title={r.name}><Button type="link" size="small" style={{ maxWidth: '100%', overflow: 'hidden', textOverflow: 'ellipsis' }} onClick={() => setAuthor(r.commits[0].author || '')}>{r.name}</Button></Tooltip>, width: 125 },
          { title: '分组', dataIndex: 'group', width: 140, ellipsis: true },
          { title: '仓库数', render: (_, r) => new Set(r.commits.map(c => c.repoId)).size, width: 85 },
          { title: '仓库语言', render: (_, r) => <LanguageTags items={r.commits} />, width: 190 },
          { title: '提交次数', render: (_, r) => r.commits.length, sorter: (a, b) => a.commits.length - b.commits.length, width: 105 },
          { title: '关联需求提交', render: (_, r) => r.commits.filter(c => c.linked).length, width: 115 },
          { title: '新增行', render: (_, r) => lineTotal(r.commits, 'additions'), width: 90 },
          { title: '删除行', render: (_, r) => lineTotal(r.commits, 'deletions'), width: 90 },
          { title: '行数未知提交', render: (_, r) => unknown(r.commits), width: 110 },
          { title: '本期最近提交', render: (_, r) => dateTime(r.last), sorter: (a, b) => Date.parse(a.last) - Date.parse(b.last), width: 185 },
        ]} />
      </Card>
    </Spin>
  </div>
}
