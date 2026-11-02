import type { MemberMetric } from './api'
const percent = (value:number | null) => value == null ? '待评价' : `${value.toFixed(1)}%`
const points = [[0,-85],[-73.6,42.5],[73.6,42.5]]
const labels = ['交付量达成','交付需求质量','交付时效']
export function MemberRadar({ member }: { member: MemberMetric }) {
  const complete = member.radar.every(value => value != null)
  const polygon = points.map(([x,y],i) => `${x*(member.radar[i] ?? 0)/100},${y*(member.radar[i] ?? 0)/100}`).join(' ')
  return <svg viewBox="-140 -135 280 240" role="img" aria-label={`${member.name}个人雷达图`} className="yx-member-radar">
    <title>{member.name}：交付 {member.deliveredRequirements} 项，含Bug交付需求占比 {percent(member.bugRate)}，延期率 {percent(member.delayRate)}</title>
    <desc>统一0至100，越向外越好。质量与时效分别为100减去对应比率。缺失维度不补零或满分。</desc>
    {[20,40,60,80,100].map(level => <g key={level} className="yx-radar-grid"><polygon points={points.map(([x,y]) => `${x*level/100},${y*level/100}`).join(' ')} /><text x={4} y={-85*level/100-2}>{level}</text></g>)}
    {points.map(([x,y],i) => <line className="yx-radar-axis" key={i} x1={0} y1={0} x2={x} y2={y} />)}
    {complete && <polygon className="yx-radar-value" points={polygon} />}
    {points.map(([x,y],i) => <g key={i}>
      {member.radar[i] != null && <circle className="yx-radar-point" cx={x*member.radar[i]!/100} cy={y*member.radar[i]!/100} r={2.7} />}
      <text className="yx-radar-label" x={x*1.3} y={y*1.3} textAnchor="middle"><tspan x={x*1.3}>{labels[i]}</tspan><tspan x={x*1.3} dy={15}>{member.radar[i] == null ? '待评价' : member.radar[i]!.toFixed(1)}</tspan></text>
    </g>)}
  </svg>
}
export const radarColors = ['#3370FF','#D97706','#0F8A6B','#9254DE','#C83E75','#137C9B']
export function GroupRadar({ group,members }: { group:string; members:MemberMetric[] }) {
  return <svg viewBox="-140 -135 280 240" role="img" aria-label={`${group}组内成员雷达对比图`} className="yx-group-radar">
    <title>{group}：{members.map(p => p.name).join('、')}的交付量、关联质量与交付时效</title>
    <desc>所有成员共用0至100刻度，颜色与右侧成员对应；缺失数据仅绘制已知点。</desc>
    {[20,40,60,80,100].map(level => <g key={level} className="yx-radar-grid"><polygon points={points.map(([x,y]) => `${x*level/100},${y*level/100}`).join(' ')} /><text x={4} y={-85*level/100-2}>{level}</text></g>)}
    {points.map(([x,y],i) => <g key={i}><line className="yx-radar-axis" x1={0} y1={0} x2={x} y2={y} /><text className="yx-radar-label" x={x*1.3} y={y*1.3} textAnchor="middle">{labels[i]}</text></g>)}
    {members.map((member,index) => {
      const color=radarColors[index]
      const polygon=points.map(([x,y],i) => `${x*(member.radar[i] ?? 0)/100},${y*(member.radar[i] ?? 0)/100}`).join(' ')
      return <g key={member.name} data-member={member.name}><title>{member.name}：交付 {member.deliveredRequirements} 项，含Bug交付需求占比 {percent(member.bugRate)}，延期率 {percent(member.delayRate)}</title>
        {member.radar.every(value => value != null) && <polygon className="yx-radar-value" points={polygon} style={{ stroke:color,fill:color,fillOpacity:.04,strokeDasharray:index%2 ? '4 2' : undefined }} />}
        {points.map(([x,y],i) => member.radar[i] == null ? null : <circle key={i} cx={x*member.radar[i]!/100} cy={y*member.radar[i]!/100} r={2.7} style={{ fill:color }} />)}
      </g>
    })}
  </svg>
}
