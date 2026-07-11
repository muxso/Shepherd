// 甜甜圈环形图(纯 SVG,无依赖)。segments 为各分段 {value,color,label};中心显示总数。
// 悬浮分段:该段加粗、其余变淡,中心切换为该段「数值 + 名称 + 占比」。
import { useState } from 'react'
import { useI18n } from '../i18n'

export interface DonutSeg {
  label: string
  value: number
  color: string
}

export default function Donut({
  segments,
  size = 120,
  thickness = 16,
  centerLabel,
}: {
  segments: DonutSeg[]
  size?: number
  thickness?: number
  centerLabel?: string
}) {
  const { t } = useI18n()
  const [hover, setHover] = useState<number | null>(null)
  const label = centerLabel ?? t('donut.total', '总数')
  const total = segments.reduce((s, x) => s + x.value, 0)
  const r = (size - thickness) / 2
  const circ = 2 * Math.PI * r
  const cx = size / 2
  let offset = 0

  const visible = segments.map((s, i) => ({ ...s, i })).filter((s) => s.value > 0)
  const hovered = hover != null ? visible.find((s) => s.i === hover) : undefined
  const pct = hovered && total > 0 ? `${((hovered.value * 100) / total).toFixed(0)}%` : ''

  return (
    <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`}>
      <g transform={`rotate(-90 ${cx} ${cx})`}>
        <circle cx={cx} cy={cx} r={r} fill="none" stroke="#f0f2f5" strokeWidth={thickness} />
        {total > 0 &&
          visible.map((s) => {
            const len = (circ * s.value) / total
            const active = hover === s.i
            const seg = (
              <circle
                key={s.i}
                cx={cx}
                cy={cx}
                r={r}
                fill="none"
                stroke={s.color}
                strokeWidth={active ? thickness + 4 : thickness}
                strokeDasharray={`${len} ${circ - len}`}
                strokeDashoffset={-offset}
                opacity={hover == null || active ? 1 : 0.3}
                style={{ transition: 'stroke-width 0.12s, opacity 0.12s', cursor: 'default' }}
                onMouseEnter={() => setHover(s.i)}
                onMouseLeave={() => setHover(null)}
              >
                <title>{`${s.label}: ${s.value}${total > 0 ? ` (${((s.value * 100) / total).toFixed(0)}%)` : ''}`}</title>
              </circle>
            )
            offset += len
            return seg
          })}
      </g>
      <text x={cx} y={cx - 2} textAnchor="middle" fontSize={20} fontWeight={700} fill="currentColor" style={{ color: hovered ? hovered.color : 'var(--text)', pointerEvents: 'none' }}>
        {hovered ? hovered.value : total}
      </text>
      <text x={cx} y={cx + 16} textAnchor="middle" fontSize={11} fill="#8a9099" style={{ pointerEvents: 'none' }}>
        {hovered ? `${hovered.label} ${pct}` : label}
      </text>
    </svg>
  )
}
