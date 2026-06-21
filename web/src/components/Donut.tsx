// 甜甜圈环形图(纯 SVG,无依赖)。segments 为各分段 {value,color,label};中心显示总数。
export interface DonutSeg {
  label: string
  value: number
  color: string
}

export default function Donut({
  segments,
  size = 120,
  thickness = 16,
  centerLabel = '总数',
}: {
  segments: DonutSeg[]
  size?: number
  thickness?: number
  centerLabel?: string
}) {
  const total = segments.reduce((s, x) => s + x.value, 0)
  const r = (size - thickness) / 2
  const circ = 2 * Math.PI * r
  const cx = size / 2
  let offset = 0

  return (
    <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`}>
      <g transform={`rotate(-90 ${cx} ${cx})`}>
        <circle cx={cx} cy={cx} r={r} fill="none" stroke="#f0f2f5" strokeWidth={thickness} />
        {total > 0 &&
          segments
            .filter((s) => s.value > 0)
            .map((s, i) => {
              const len = (circ * s.value) / total
              const seg = (
                <circle
                  key={i}
                  cx={cx}
                  cy={cx}
                  r={r}
                  fill="none"
                  stroke={s.color}
                  strokeWidth={thickness}
                  strokeDasharray={`${len} ${circ - len}`}
                  strokeDashoffset={-offset}
                />
              )
              offset += len
              return seg
            })}
      </g>
      <text x={cx} y={cx - 2} textAnchor="middle" fontSize={20} fontWeight={700} fill="#1f2329">
        {total}
      </text>
      <text x={cx} y={cx + 16} textAnchor="middle" fontSize={11} fill="#8a9099">
        {centerLabel}
      </text>
    </svg>
  )
}
