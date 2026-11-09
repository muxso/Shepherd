import { Button, Tooltip } from 'antd'
import { useState } from 'react'

type DailyCell = { count: number; lastTime?: string | null; additions?: number | null; deletions?: number | null; lineStatsIncomplete?: boolean; requirements: number; bugs: number; created: number; published: number; testcases: number }
type HeatmapRowSpec = { label: string; group: string; values: number[]; cells?: DailyCell[]; totals?: DailyCell }
type HeatmapSpec = {
  kind: 'activity_heatmap'
  title: string
  metric: string
  dates: string[]
  rows: HeatmapRowSpec[]
  detailMode?: string
  expanded?: boolean
  detailNote?: string
  from: string
  through: string
  observedAt?: string
  coverageComplete?: boolean
  granularity?: 'daily' | 'weekly'
}

type BarSpec = { kind: 'bar'; title: string; metric: string; labels: string[]; values: number[]; from: string; through: string; coverageComplete?: boolean }
export type GroundedChartSpec = HeatmapSpec | BarSpec

export function GroundedChart({ chart }: { chart: GroundedChartSpec }) {
  const [showDetails, setShowDetails] = useState(true)
  if (chart.kind === 'bar') return <MetricBars chart={chart} />
  if (chart.kind !== 'activity_heatmap' || !Array.isArray(chart.rows) || !Array.isArray(chart.dates) || !chart.dates.length) return null
  const dates = chart.dates.slice(0, 366)
  const rows = chart.rows.slice(0, 500)
  const maximum = rows.reduce((maximum, row) => row.values.reduce((current, value) => Number.isFinite(value) && value >= 0 ? Math.max(current, value) : current, maximum), 1)
  const detailed = rows.some(row => row.cells?.length)
  const expanded = detailed && showDetails
  const mode = chart.detailMode || 'code'
  const columns = `minmax(86px, 120px) repeat(${dates.length}, minmax(${expanded ? 76 : 27}px, 1fr))${detailed ? ' repeat(3, 32px)' : ''}`
  return <section aria-label={chart.title} style={{ marginTop: 12, border: '1px solid var(--border-soft)', borderRadius: 10, padding: 12, background: 'var(--panel)' }}>
    <div style={{ display: 'flex', justifyContent: 'space-between', gap: 8, alignItems: 'center', marginBottom: 4 }}><b style={{ color: 'var(--text)' }}>{chart.title}</b>{detailed && <Button size="small" onClick={() => setShowDetails(v => !v)}>{expanded ? '收起格子指标' : '展开格子指标'}</Button>}</div>
    <div style={{ color: 'var(--text-3)', fontSize: 12, marginBottom: 12 }}>{chart.from && chart.through ? `${chart.from} ～ ${chart.through} · 上海时间` : '当前查询快照'}{chart.metric ? ` · ${chart.metric}` : ''}{chart.granularity === 'weekly' ? ' · 按周汇总' : ''}</div>
    <div style={{ overflowX: 'auto' }}>
      <div style={{ display: 'grid', gridTemplateColumns: columns, gap: 3, alignItems: 'center', fontSize: 11 }}>
        <span style={{ color: 'var(--text-3)' }}>成员 / 分组</span>
        {dates.map((date) => {
          const weekday = new Date(`${date}T00:00:00Z`).getUTCDay()
          const weekend = weekday === 0 || weekday === 6
          return <span key={date} style={{ textAlign: 'center', borderRadius: 3, padding: '5px 0', background: weekend ? 'rgba(178,126,73,.12)' : undefined, color: weekend ? 'var(--warning, #a46b36)' : 'var(--text-3)' }}>{date.slice(5)}<small style={{ display: 'block', marginTop: 2 }}>周{'日一二三四五六'[weekday]}</small></span>
        })}
        {detailed && (mode === 'code' ? ['提交', '需求', 'Bug'] : mode === 'product' ? ['活动', '新建', '发布'] : ['活动', '用例', 'Bug']).map(label => <span key={label} style={{ textAlign: 'center', color: 'var(--text-3)' }}>{label}</span>)}
        {rows.map((row, index) => <HeatmapRow key={`${row.group}:${row.label}:${index}`} row={row} dates={dates} maximum={maximum} expanded={expanded} detailed={detailed} mode={mode} />)}
      </div>
    </div>
    {chart.detailNote && <div style={{ color: 'var(--text-3)', fontSize: 12, marginTop: 8 }}>{chart.detailNote}</div>}
    {!rows.length && <div style={{ padding: 16, color: 'var(--text-3)' }}>所选范围暂未观测到活动记录。</div>}
    <div style={{ fontSize: 11, color: 'var(--text-3)', marginTop: 10 }}>颜色越深，已采集活动越多；空白表示未观测到记录。* 表示行数不全；期间需求与 Bug 合计分别去重。{chart.coverageComplete === false && '采集覆盖尚不完整，不能据此判断没有工作。'}</div>
  </section>
}

const number = (value?: number | null) => value == null ? '—' : value.toLocaleString()
function HeatmapRow({ row, dates, maximum, expanded, detailed, mode }: { row: HeatmapRowSpec; dates: string[]; maximum: number; expanded: boolean; detailed: boolean; mode: string }) {
  const totals = row.totals
  const summary = mode === 'code' ? [totals?.count, totals?.requirements, totals?.bugs] : mode === 'product' ? [totals?.count, totals?.created, totals?.published] : [totals?.count, totals?.testcases, totals?.bugs]
  return <>
    <Tooltip title={`${row.label} · ${row.group}`}><span style={{ color: 'var(--text)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', padding: '3px 0' }}>{row.label}</span></Tooltip>
    {dates.map((date, index) => {
      const value = Number.isFinite(row.values[index]) && row.values[index] > 0 ? row.values[index] : 0
      const cell = row.cells?.[index]
      const intensity = value ? 15 + Math.round(65 * Math.sqrt(value / maximum)) : 0
      const content = expanded && cell && (cell.count > 0 || cell.bugs > 0) ? <>
        <span style={{ fontWeight: 600 }}>{cell.lastTime || '—'}{cell.bugs > 0 && <small style={{ marginLeft: 3, color: 'var(--warning, #a46b36)', background: 'rgba(178,126,73,.14)', padding: '0 2px', borderRadius: 2 }}>Bug {cell.bugs}</small>}</span>
        {mode === 'code' ? <>{cell.count > 0 && <small>{cell.additions == null && cell.deletions == null ? '行数未知' : `+${number(cell.additions)} / −${number(cell.deletions)}`}{cell.lineStatsIncomplete ? '*' : ''}</small>}{cell.requirements > 0 && <small>关联需求 {cell.requirements}</small>}</> : <><small>{cell.count} 次活动</small><small>{mode === 'product' ? `新建 ${cell.created} · 发布 ${cell.published}` : `用例 ${cell.testcases}`}</small></>}
      </> : value || '·'
      return <Tooltip key={date} title={<span>{row.label} · {date}：{cell?.count ?? value} 条已采集活动{cell?.lineStatsIncomplete && <><br />部分行数缺失，当前仅为已知下限</>}</span>}>
        <span aria-label={`${row.label} ${date} ${value}`} style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', borderRadius: 3, minHeight: expanded ? 60 : 24, gap: 2, background: value ? `color-mix(in srgb, var(--brand) ${intensity}%, var(--panel))` : 'var(--panel-2)', color: 'var(--text)', fontVariantNumeric: 'tabular-nums', textAlign: 'center' }}>{content}</span>
      </Tooltip>
    })}
    {detailed && summary.map((value, index) => <span key={index} style={{ textAlign: 'center', color: index === 2 && mode !== 'product' && value ? 'var(--warning, #a46b36)' : 'var(--text-3)', fontVariantNumeric: 'tabular-nums' }}>{value || '—'}</span>)}
  </>
}

function MetricBars({ chart }: { chart: BarSpec }) {
  if (!Array.isArray(chart.labels) || !Array.isArray(chart.values)) return null
  const maximum = chart.values.reduce((maximum, value) => Number.isFinite(value) ? Math.max(maximum, value) : maximum, 1)
  return <section aria-label={chart.title} style={{ marginTop: 12, border: '1px solid var(--border-soft)', borderRadius: 10, padding: 12, background: 'var(--panel)' }}>
    <div style={{ fontWeight: 600 }}>{chart.title}</div>
    <div style={{ color: 'var(--text-3)', fontSize: 12, margin: '4px 0 12px' }}>{chart.from && chart.through ? `${chart.from} ～ ${chart.through} · 上海时间` : '当前查询快照'}{chart.metric ? ` · ${chart.metric}` : ''}</div>
    {chart.labels.slice(0, 30).map((label, index) => {
      const value = Number.isFinite(chart.values[index]) && chart.values[index] > 0 ? chart.values[index] : 0
      return <div key={`${label}:${index}`} style={{ display: 'grid', gridTemplateColumns: 'minmax(80px, 125px) 1fr 68px', gap: 8, alignItems: 'center', margin: '8px 0', fontSize: 12 }}>
        <Tooltip title={label}><span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{label}</span></Tooltip>
        <span style={{ background: 'var(--panel-2)', height: 14, borderRadius: 4, overflow: 'hidden' }}><span style={{ display: 'block', width: `${value / maximum * 100}%`, height: '100%', background: 'var(--brand)', borderRadius: 4 }} /></span>
        <span style={{ textAlign: 'right', fontVariantNumeric: 'tabular-nums' }}>{value.toLocaleString()}</span>
      </div>
    })}
    <div style={{ color: 'var(--text-3)', fontSize: 11, marginTop: 10 }}>展示 {Math.min(30, chart.labels.length)} / {chart.labels.length} 行已采集结果。{chart.coverageComplete === false && '采集覆盖尚不完整，不能据此判断没有工作。'}</div>
  </section>
}
