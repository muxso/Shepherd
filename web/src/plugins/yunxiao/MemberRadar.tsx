import { useI18n } from '../../i18n'
import type { MemberMetric } from './api'

const axes = ['deliveryTargetAttainment', 'defectFreeDeliveryRate', 'onTimeDeliveryRate', 'sonarNewCodeCoverage']
const radius = 72
const directions = [[0, -1], [-1, 0], [0, 1], [1, 0]]
const labels = [[0, -104], [-128, -9], [0, 92], [128, -9]]
const levels = [20, 40, 60, 80, 100]
const chartViewBox = '-196 -117 392 232'

function plotPoint(value: number, index: number) {
  const [dx, dy] = directions[index]
  const distance = radius * Math.max(0, Math.min(100, value)) / 100
  return { x: dx * distance, y: dy * distance, tx: dy * distance * .42, ty: -dx * distance * .42 }
}

type Point = ReturnType<typeof plotPoint>
const curve = (a: Point, b: Point) => `C ${a.x + a.tx},${a.y + a.ty} ${b.x - b.tx},${b.y - b.ty} ${b.x},${b.y}`

export function onTimeRate(member: MemberMetric) {
  if (member.onTimeRate !== undefined) return member.onTimeRate
  const due = member.onTime + member.deliveredLate + member.overdueAtEnd
  return due > 0 ? 100 * member.onTime / due : null
}

export function radarValues(member: MemberMetric) {
  const latest = member.sonar?.recent[0]
  return [member.radar[0], member.radar[1], onTimeRate(member), latest?.analysisQuality?.coverage ?? latest?.newCoverage ?? null]
}

function useRadarLabels() {
  const { lang, t } = useI18n()
  const numbers = new Intl.NumberFormat(lang, { minimumFractionDigits: 1, maximumFractionDigits: 1 })
  return {
    axisLabel: (index: number) => t('engineering.' + axes[index]),
    format: (value: number | null, index: number) => value == null
      ? t(index === 3 ? 'engineering.noData' : 'engineering.notRated')
      : numbers.format(value),
  }
}
const radarColors = ['#4786d4', '#299a9c', '#cf8958', '#8c69c1', '#bf6588', '#557db7']
export const radarColor = (index: number) => radarColors[index] ?? `hsl(${(index * 137.508) % 360} 55% 48%)`

function RadarGrid({ values }: { values?: ReturnType<typeof radarValues> }) {
  const { axisLabel, format } = useRadarLabels()
  return <>
    {levels.map(level => <g key={level} className="yx-radar-grid">
      <circle r={radius * level / 100} className={level === 100 ? 'yx-radar-outline' : undefined} />
      <text x={4} y={-radius * level / 100 + 9}>{level}</text>
    </g>)}
    {directions.map(([dx, dy], index) => {
      const x = dx * radius
      const y = dy * radius
      const [labelX, labelY] = labels[index]
      return <g key={axes[index]}>
        <line className="yx-radar-axis" x1={0} y1={0} x2={x} y2={y} />
        <text className="yx-radar-label" x={labelX} y={labelY} textAnchor="middle">
          {axisLabel(index)}
          {values && <tspan x={labelX} dy={15}>{format(values[index], index)}</tspan>}
        </text>
      </g>
    })}
  </>
}

function RadarSeries({ member, index }: { member: MemberMetric; index?: number }) {
  const { axisLabel, format } = useRadarLabels()
  const values = radarValues(member)
  const color = index === undefined ? undefined : radarColor(index)
  const coords = values.map((value, i) => value == null ? null : plotPoint(value, i))
  const complete = coords.every((point): point is Point => point !== null)
  const segments = coords.map((point, i) => {
    const next = coords[(i + 1) % coords.length]
    if (!point || !next) return ''
    return `${complete && i > 0 ? '' : `M ${point.x},${point.y} `}${curve(point, next)}`
  }).join(' ')
  return <g data-member={member.name} style={{ color }}>
    <title>{member.name}: {axes.map((_, i) => `${axisLabel(i)} ${format(values[i], i)}`).join(', ')}</title>
    <path className="yx-radar-value" d={complete ? `${segments} Z` : segments} style={complete ? undefined : { fill: 'none' }} />
    {coords.map((point, i) => point && <circle key={axes[i]} className="yx-radar-point" cx={point.x} cy={point.y} r={2.7} />)}
  </g>
}

export function MemberRadar({ member }: { member: MemberMetric }) {
  const { t } = useI18n()
  return <svg viewBox={chartViewBox} role="img" aria-label={t("engineering.individualRadarForV0", undefined, { v0: member.name })} className="yx-member-radar">
    <RadarGrid values={radarValues(member)} />
    <RadarSeries member={member} />
  </svg>
}
export function GroupRadar({ group, members }: { group: string; members: MemberMetric[] }) {
  const { t } = useI18n()
  return <svg viewBox={chartViewBox} role="img" aria-label={t("engineering.teamRadarComparisonForV0", undefined, { v0: group })} className="yx-group-radar">
    <desc>{t("engineering.allMembersShareA0100ScaleMissingValuesAreLeftBlank")}</desc>
    <RadarGrid />
    {members.map((member, index) => <RadarSeries member={member} index={index} key={member.name} />)}
  </svg>
}
