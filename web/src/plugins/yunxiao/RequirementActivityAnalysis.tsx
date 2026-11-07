import { useEffect, useMemo, useRef, useState } from 'react'
import { Alert, Button, Drawer, Empty, Segmented, Select, Space, Table } from 'antd'
import { Chart } from '@antv/g2'
import { useThemeMode } from '../../themeMode'
import type { TeamActivityData } from './api'
import ActivitySourceLink from './ActivitySourceLink'
import { lineCount, requirementFocus, type RequirementFocus } from './activityModel'
const measures = [{ value: 'people', label: '参与人数', unit: '人' }, { value: 'lines', label: '已采集代码变动', unit: '行' }, { value: 'repositories', label: '涉及仓库', unit: '个' }, { value: 'bugs', label: '新增有效 Bug', unit: '个' }] as const
type Measure = typeof measures[number]['value']
const amount = (r: RequirementFocus, metric: Measure) => metric === 'bugs' ? r.bugs.length : r[metric]
function Distribution({ rows, canBugs, select }: { rows: RequirementFocus[]; canBugs: boolean; select: (id: string) => void }) {
  const ref = useRef<HTMLDivElement>(null), { mode } = useThemeMode()
  const [error, setError] = useState(''), [rendering, setRendering] = useState(true)
  useEffect(() => {
    if (!ref.current || !rows.length) return
    const chart = new Chart({ container: ref.current, autoFit: true, height: 340 })
    let live = true
    setError(''); setRendering(true)
    chart.options({
      type: 'point', theme: mode,
      data: rows.map(r => ({ ...r, title: `${r.serial || r.id} ${r.title || ''}`, logLines: Math.log10(r.lines + 1), bugCount: canBugs ? r.bugs.length : null, bugBand: !canBugs ? '无缺陷读取权限' : r.bugs.length >= 3 ? '3+ Bug' : r.bugs.length ? '1–2 Bug' : '0 Bug' })),
      encode: { x: 'people', y: 'logLines', size: 'repositories', color: 'bugBand' },
      scale: { x: { nice: true }, size: { range: [5, 24] }, color: { domain: ['0 Bug', '1–2 Bug', '3+ Bug', '无缺陷读取权限'], range: mode === 'dark' ? ['#78a2df', '#e1b36c', '#e18e90', '#7f8fa2'] : ['#7b9fd9', '#d8a05d', '#ce7b7d', '#9ca8b7'] } },
      axis: { x: { title: '提交参与人数 / 人', labelFormatter: (v: number) => Number.isInteger(Number(v)) ? String(v) : '' }, y: { title: '代码变动 / 行（对数刻度）', labelFormatter: (v: number) => Math.round(10 ** Number(v) - 1).toLocaleString() } },
      legend: { size: false, color: { position: 'bottom' } },
      style: { fillOpacity: 0.65, lineWidth: 1 },
      tooltip: { title: 'title', items: [{ field: 'people', name: '参与人数' }, { field: 'lines', name: '已采集变动行' }, { field: 'repositories', name: '涉及仓库' }, { field: 'bugCount', name: '新增有效 Bug' }, { field: 'unknownLines', name: '行数未采集的提交' }] },
    })
    chart.on('element:click', (event: { data?: { data?: { id?: string } } }) => { if (event.data?.data?.id) select(event.data.data.id) })
    chart.render().then(() => { if (live) setRendering(false) }).catch(e => { if (live) { setError(String(e.message || e)); setRendering(false) } })
    return () => { live = false; chart.destroy() }
  }, [rows, mode, canBugs, select])
  return <>{error && <Alert type="error" showIcon title={error} />}<div ref={ref} aria-busy={rendering} className="yx-activity-distribution" aria-label="需求参与人数、代码变动、仓库与缺陷气泡图" /><p className="yx-muted">气泡面积表示涉及仓库数，颜色表示本期新增有效 Bug。代码变动仅为已采集行数，缺少行数时为下限。</p></>
}
export default function RequirementActivityAnalysis({ data, onBack, openRequirement }: { data: TeamActivityData; onBack: () => void; openRequirement: (id: string) => void }) {
  const rows = useMemo(() => requirementFocus(data), [data])
  const [metric, setMetric] = useState<Measure>('people'), [view, setView] = useState('bubble'), [selected, setSelected] = useState<string | null>(null)
  const sorted = useMemo(() => [...rows].sort((a, b) => amount(b, metric) - amount(a, metric) || a.id.localeCompare(b.id)), [rows, metric])
  const detail = rows.find(r => r.id === selected)
  if (!data.permissions.requirements) return <Empty description="需要需求读取权限才能查看需求投入分析"><Button onClick={onBack}>返回人员产出</Button></Empty>
  return <div className="yx-activity-analysis">
    <div className="yx-activity-analysis-head"><div><h3>需求投入与质量</h3><span className="yx-muted">{data.from} — {data.through} · {rows.length} 项明确关联代码的需求</span></div><Button onClick={onBack}>返回人员产出</Button></div>
    {!rows.length ? <Empty description="所选范围尚无明确关联需求的提交" /> : <>
      <div className="yx-activity-winners">{measures.map(m => {
        const available = m.value !== 'bugs' || data.permissions.bugs
        const max = Math.max(0, ...rows.map(r => amount(r, m.value))), winners = rows.filter(r => amount(r, m.value) === max), winner = winners[0]
        return <button key={m.value} type="button" disabled={!available} onClick={() => setSelected(winner.id)}><span>{m.label}最多</span><strong>{available ? max.toLocaleString() : '—'}<small>{m.unit}</small></strong><span>{available ? winner.serial || winner.id : '无缺陷读取权限'}</span>{available && <small title={winner.title || ''}>{winner.title || '需求名称未同步'}{winners.length > 1 ? ` · ${winners.length} 项并列` : ''}</small>}</button>
      })}</div>
      <div className="yx-activity-analysis-tools"><Segmented value={view} onChange={v => setView(String(v))} options={[{ value: 'bubble', label: '气泡分布' }, { value: 'table', label: '四维对比' }]} /><Select aria-label="需求分析排序" value={metric} onChange={setMetric} options={measures.filter(m => m.value !== 'bugs' || data.permissions.bugs).map(m => ({ value: m.value, label: `按${m.label}` }))} /></div>
      {view === 'bubble' && <Distribution rows={rows} canBugs={data.permissions.bugs} select={setSelected} />}
      <Table rowKey="id" size="small" pagination={false} dataSource={sorted} scroll={{ x: 780 }} columns={[
        { title: '需求', width: 330, render: (_, r) => <Button type="link" className="yx-activity-requirement" onClick={() => setSelected(r.id)}>{r.serial || r.id} {r.title || '（名称未同步）'}</Button> },
        { title: '提交参与', width: 100, render: (_, r) => `${r.people} 人` },
        { title: '代码变动', width: 130, render: (_, r) => `${r.unknownLines ? '≥' : ''}${r.lines.toLocaleString()} 行` },
        { title: '涉及仓库', width: 100, dataIndex: 'repositories' },
        ...(data.permissions.bugs ? [{ title: '新增有效 Bug', width: 120, render: (_: unknown, r: RequirementFocus) => r.bugs.length || '—' }] : []),
      ]} />
      <p className="yx-muted">参与人数是提交覆盖，不能推算实际人天。同一提交可关联多项需求，不能相加当团队总量；没有明确需求关联的提交单独保留。Bug 按创建时间和显式需求关联去重，不推定个人责任。</p>
    </>}
    <Drawer title={detail ? `${detail.serial || detail.id} · 需求证据` : '需求证据'} open={!!detail} onClose={() => setSelected(null)} size={680} styles={{ wrapper: { maxWidth: '96vw' } }}>
      {detail && <><h3><ActivitySourceLink url={detail.url}>{detail.title}</ActivitySourceLink></h3><p className="yx-muted">参与 {[...new Set(detail.events.map(e => e.actor))].join('、')}</p><p>新增 +{lineCount(detail.events, 'additions')} / 删除 −{lineCount(detail.events, 'deletions')} 行</p><h4>关联仓库</h4>{[...new Set(detail.events.map(e => e.repo || String(e.repoId)))].map(repo => <div className="yx-activity-evidence" key={repo}><ActivitySourceLink url={detail.events.find(e => (e.repo || String(e.repoId)) === repo && e.repositoryUrl)?.repositoryUrl}>{repo}</ActivitySourceLink></div>)}{!!detail.bugs.length && <><h4>本期新增有效 Bug</h4>{detail.bugs.map(b => <div className="yx-activity-evidence" key={b.id}><ActivitySourceLink url={b.url}>{b.serial || b.id} {b.title}</ActivitySourceLink><small>{b.status} · {b.date}</small></div>)}</>}<Space><Button onClick={() => openRequirement(detail.id)}>查看需求产出</Button></Space></>}
    </Drawer>
  </div>
}
