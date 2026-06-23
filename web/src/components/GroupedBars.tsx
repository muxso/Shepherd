// 分组柱状图(纯 SVG,无依赖)。每个 row 一组,组内每个 series 一根柱。
// 用 viewBox + width:100% 自适应容器宽度;无数据时由调用方处理。
export interface BarSeries {
  key: string
  label: string
  color: string
}
export interface BarRow {
  name: string
  values: Record<string, number>
}

export default function GroupedBars({ series, rows, height = 260 }: { series: BarSeries[]; rows: BarRow[]; height?: number }) {
  const W = 860
  const H = height
  const padL = 44
  const padR = 12
  const padT = 12
  const padB = 40
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
      {/* 图例 */}
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 16, marginBottom: 8 }}>
        {series.map((s) => (
          <span key={s.key} style={{ display: 'inline-flex', alignItems: 'center', fontSize: 12, color: '#5b6470' }}>
            <span style={{ width: 10, height: 10, borderRadius: 2, background: s.color, marginRight: 6 }} />
            {s.label}
          </span>
        ))}
      </div>
      <svg width="100%" viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="xMidYMid meet">
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
                return (
                  <g key={s.key}>
                    <rect x={x} y={y} width={barW} height={Math.max(0, h)} fill={s.color} rx={2}>
                      <title>{`${row.name} · ${s.label}: ${v}`}</title>
                    </rect>
                    {v > 0 && <text x={x + barW / 2} y={y - 3} textAnchor="middle" fontSize="10" fill="#8a9099">{v}</text>}
                  </g>
                )
              })}
              <text x={gx + groupW / 2} y={H - padB + 18} textAnchor="middle" fontSize="12" fill="#5b6470">{truncate(row.name, 10)}</text>
            </g>
          )
        })}
      </svg>
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
