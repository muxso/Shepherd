import { useEffect, useRef, useState } from 'react'
import { Button, Empty, Tooltip } from 'antd'
export interface BarItem { name: string; value: number | null; note?: string }
export function ManagementBars({ items, unit = '', onSelect, limit = 8 }: { items: BarItem[]; unit?: string; onSelect?: (name: string) => void; limit?: number }) {
  const [all, setAll] = useState(false), max = Math.max(1, ...items.map(i => i.value || 0))
  if (!items.length) return <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="所选范围暂无记录" />
  return <div className="yx-management-bars">{(all ? items : items.slice(0, limit)).map(item => <button type="button" key={item.name} disabled={!onSelect} onClick={() => onSelect?.(item.name)} aria-label={`${item.name} ${item.value == null ? '待评价' : `${item.value}${unit}`}`}>
    <Tooltip title={item.note || item.name}><span className="yx-management-bar-name">{item.name}</span></Tooltip><span className="yx-management-bar-track"><i style={{ width: `${item.value == null ? 0 : item.value / max * 100}%` }} /></span><strong>{item.value == null ? '待评价' : `${Number.isInteger(item.value) ? item.value.toLocaleString() : item.value.toFixed(1)}${unit}`}</strong>
  </button>)}{items.length > limit && <Button type="link" size="small" onClick={() => setAll(v => !v)}>{all ? '收起' : `查看全部 ${items.length} 项`}</Button>}</div>
}
export interface TrendSeries { key: string; label: string; color: string; points: { date: string; value: number }[] }
export function ManagementTrend({ title, series, onSelect, partialDate }: { title: string; series: TrendSeries[]; partialDate?: string; onSelect?: (metric: string, date: string) => void }) {
  const ref = useRef<HTMLDivElement>(null), [width, setWidth] = useState(720)
  useEffect(() => { if (!ref.current) return; const observer = new ResizeObserver(([entry]) => setWidth(Math.max(240, entry.contentRect.width))); observer.observe(ref.current); return () => observer.disconnect() }, [series.some(s => s.points.length > 0)])
  const [focus, setFocus] = useState<string | null>(null), [hover, setHover] = useState<number | null>(null)
  const dates = [...new Set(series.flatMap(s => s.points.map(p => p.date)))].sort(), selected = series.find(s => s.key === focus) || series[0]
  const max = Math.max(1, ...series.flatMap(s => s.points.map(p => p.value))), n = dates.length, height = 200, left = 40, right = 14, top = 14, bottom = 28
  const x = (i: number) => left + i / Math.max(1, n - 1) * (width - left - right), y = (v: number) => top + (1 - v / max) * (height - top - bottom)
  const point = (s: TrendSeries, date: string) => s.points.find(p => p.date === date)?.value || 0
  const active = Math.min(n - 1, hover ?? n - 1), date = dates[active]
  if (!n) return <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无可比较的时间记录" />
  return <div ref={ref} className="yx-management-trend">
    <div className="yx-management-trend-legend">{series.map(s => <button type="button" key={s.key} aria-pressed={selected?.key === s.key} onClick={() => setFocus(s.key)}><i style={{ background: s.color }} />{s.label}</button>)}</div>
    <svg role="img" aria-label={title} viewBox={`0 0 ${width} ${height}`} onMouseMove={event => { const rect = event.currentTarget.getBoundingClientRect(); setHover(Math.max(0, Math.min(n - 1, Math.round(((event.clientX - rect.left) / rect.width * width - left) / (width - left - right) * (n - 1))))) }} onMouseLeave={() => setHover(null)}>
      {[0, 0.5, 1].map(f => <g key={f}><line x1={left} x2={width - right} y1={y(max * f)} y2={y(max * f)} className="yx-management-grid" /><text x={left - 8} y={y(max * f) + 4} textAnchor="end">{Math.round(max * f)}</text></g>)}
      {series.map(s => <g key={s.key}><path d={dates.map((d, i) => `${i ? 'L' : 'M'}${x(i)},${y(point(s, d))}`).join(' ')} fill="none" stroke={s.color} strokeWidth={selected?.key === s.key ? 2.5 : 1.5} opacity={selected?.key === s.key ? 1 : 0.55} strokeDasharray={partialDate && dates.at(-1) === partialDate ? '6 3' : undefined} />{hover != null && <circle cx={x(active)} cy={y(point(s, dates[active]))} r={4} fill={s.color} />}</g>)}
      {hover != null && <line x1={x(active)} x2={x(active)} y1={top} y2={height - bottom} className="yx-management-crosshair" />}
      {[...new Set([0, Math.floor((n - 1) / 2), n - 1])].map(i => <text key={i} x={x(i)} y={height - 6} textAnchor={i === 0 ? 'start' : i === n - 1 ? 'end' : 'middle'}>{dates[i].slice(5).replace('-', '/')}</text>)}
    </svg>
    <div className="yx-management-trend-caption"><span>{date}{date === partialDate ? '（截至同步时刻）' : ''} · {series.map(s => `${s.label} ${point(s, date)}`).join(' / ')}</span>{onSelect && selected && <Button type="link" size="small" onClick={() => onSelect(selected.key, date)}>查看{selected.label}记录</Button>}</div>
  </div>
}
