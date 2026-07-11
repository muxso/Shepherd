// 分组柱状图(纯 SVG,无依赖)。每个 row 一组,组内每个 series 一根柱。
// 宽度策略:测量容器宽度,内容自然宽 = 行数 × 每组最小宽。两者取大 →
//   组少时撑满容器,组多时按自然宽渲染并由外层横向滚动(不再挤压)。
// 悬浮:柱子即时高亮(其余变淡)+ 自绘 tooltip 跟随,不依赖原生 title 的延迟提示。
import { useEffect, useRef, useState } from 'react'

export interface BarSeries {
  key: string
  label: string
  color: string
}
export interface BarRow {
  name: string
  values: Record<string, number>
}

const MIN_GROUP_W = 72 // 每组最小宽度(px);组多时据此撑出滚动。

export default function GroupedBars({ series, rows, height = 260 }: { series: BarSeries[]; rows: BarRow[]; height?: number }) {
  const wrapRef = useRef<HTMLDivElement>(null)
  const [cw, setCw] = useState(800)
  // 悬浮中的柱:行/系列下标 + 柱顶坐标(svg 坐标系,滚动容器内定位 tooltip 用)。
  const [hover, setHover] = useState<{ ri: number; si: number; x: number; y: number } | null>(null)
  useEffect(() => {
    const el = wrapRef.current
    if (!el) return
    const ro = new ResizeObserver((entries) => {
      const w = entries[0]?.contentRect.width
      if (w && w > 0) setCw(w)
    })
    ro.observe(el)
    return () => ro.disconnect()
  }, [])

  const H = height
  const padL = 44
  const padR = 12
  const padT = 12
  const padB = 40
  // 自然宽 = 左右留白 + 行数 × 最小组宽;与容器宽取大。
  const naturalW = padL + padR + rows.length * MIN_GROUP_W
  const W = Math.max(cw, naturalW)
  const plotW = W - padL - padR
  const plotH = H - padT - padB

  // y 轴上界取「4 等分的 nice 值」。
  const rawMax = Math.max(1, ...rows.flatMap((r) => series.map((s) => r.values[s.key] ?? 0)))
  const niceMax = niceCeil(rawMax)
  const yOf = (v: number) => padT + plotH - (v / niceMax) * plotH
  const ticks = [0, 0.25, 0.5, 0.75, 1].map((f) => Math.round(niceMax * f))

  const groupW = plotW / Math.max(1, rows.length)
  const barGap = 4
  const barW = Math.min(26, (groupW - barGap * (series.length + 1)) / series.length)

  return (
    <div>
      {/* 图例(固定,不随横向滚动) */}
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 16, marginBottom: 8 }}>
        {series.map((s) => (
          <span key={s.key} style={{ display: 'inline-flex', alignItems: 'center', fontSize: 12, color: 'var(--text-2)' }}>
            <span style={{ width: 10, height: 10, borderRadius: 2, background: s.color, marginRight: 6 }} />
            {s.label}
          </span>
        ))}
      </div>
      <div ref={wrapRef} style={{ overflowX: 'auto', overflowY: 'hidden', position: 'relative' }}>
        <svg width={W} height={H} viewBox={`0 0 ${W} ${H}`} style={{ display: 'block' }}>
          {/* 网格 + y 轴刻度 */}
          {ticks.map((tk, i) => (
            <g key={i}>
              <line x1={padL} y1={yOf(tk)} x2={W - padR} y2={yOf(tk)} stroke="#f0f2f5" />
              <text x={padL - 8} y={yOf(tk) + 4} textAnchor="end" fontSize="11" fill="#a8adb5">{tk}</text>
            </g>
          ))}
          {/* 分组柱 */}
          {rows.map((row, ri) => {
            const gx = padL + ri * groupW
            const inner = barW * series.length + barGap * (series.length - 1)
            const startX = gx + (groupW - inner) / 2
            return (
              <g key={ri}>
                {series.map((s, si) => {
                  const v = row.values[s.key] ?? 0
                  const x = startX + si * (barW + barGap)
                  const y = yOf(v)
                  const h = padT + plotH - y
                  const active = hover?.ri === ri && hover?.si === si
                  return (
                    <g key={s.key}>
                      <rect
                        x={x}
                        y={y}
                        width={barW}
                        height={Math.max(0, h)}
                        fill={s.color}
                        rx={2}
                        opacity={hover == null || active ? 1 : 0.35}
                        style={{ transition: 'opacity 0.12s', cursor: 'default' }}
                        onMouseEnter={() => setHover({ ri, si, x: x + barW / 2, y })}
                        onMouseLeave={() => setHover(null)}
                      />
                      {v > 0 && <text x={x + barW / 2} y={y - 3} textAnchor="middle" fontSize="10" fill="#8a9099" style={{ pointerEvents: 'none' }}>{v}</text>}
                    </g>
                  )
                })}
                <text x={gx + groupW / 2} y={H - padB + 18} textAnchor="middle" fontSize="12" fill="#5b6470">{truncate(row.name, 10)}</text>
              </g>
            )
          })}
        </svg>
        {/* 自绘 tooltip:定位在柱顶上方,随滚动容器一起滚。 */}
        {hover && (
          <div
            style={{
              position: 'absolute',
              left: hover.x,
              top: Math.max(0, hover.y - 8),
              transform: 'translate(-50%, -100%)',
              background: 'var(--panel)',
              border: '1px solid var(--border)',
              borderRadius: 6,
              boxShadow: '0 4px 12px rgba(0, 0, 0, 0.10)',
              padding: '6px 10px',
              fontSize: 12,
              whiteSpace: 'nowrap',
              pointerEvents: 'none',
              zIndex: 1,
            }}
          >
            <div style={{ color: 'var(--text-2)', marginBottom: 2 }}>{rows[hover.ri]?.name}</div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
              <span style={{ width: 8, height: 8, borderRadius: 2, background: series[hover.si]?.color }} />
              <span style={{ color: 'var(--text)' }}>{series[hover.si]?.label}</span>
              <b style={{ color: 'var(--text)' }}>{rows[hover.ri]?.values[series[hover.si]?.key ?? ''] ?? 0}</b>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

// 取 ≥ v 的「好看上界」:1/2/5 × 10^n。
function niceCeil(v: number): number {
  if (v <= 5) return 5
  const pow = Math.pow(10, Math.floor(Math.log10(v)))
  const n = v / pow
  const mult = n <= 1 ? 1 : n <= 2 ? 2 : n <= 5 ? 5 : 10
  return mult * pow
}

function truncate(s: string, n: number): string {
  return s.length <= n ? s : s.slice(0, n - 1) + '…'
}
