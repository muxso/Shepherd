import { useEffect, useState } from 'react'
import { Alert, Button, Card, Drawer, Empty, Input, InputNumber, Popover, Select, Space, Spin, Table, Tag, Typography } from 'antd'
import { QuestionCircleOutlined } from '@ant-design/icons'
import { useNavigate } from 'react-router-dom'
import { ApiError } from '../../api'
import { devopsApi, type MemberMetric, type MemberMetricsData, type MemberRequirementMetric } from './api'
import './member-metrics.css'

const percent = (value: number | null) => value == null ? '待评价' : `${value.toFixed(1)}%`
function previousMonth() {
  const parts = new Intl.DateTimeFormat('en-CA', { timeZone: 'Asia/Shanghai', year: 'numeric', month: '2-digit' }).formatToParts(new Date())
  const year = Number(parts.find(p => p.type === 'year')?.value), month = Number(parts.find(p => p.type === 'month')?.value)
  return month === 1 ? `${year-1}-12` : `${year}-${String(month-1).padStart(2,'0')}`
}
const points = [[0,-85],[-73.6,42.5],[73.6,42.5]]
const labels = ['交付量达成','关联质量','交付时效']
function Radar({ member }: { member: MemberMetric }) {
  const complete = member.radar.every(value => value != null)
  const polygon = points.map(([x,y],i) => `${x*(member.radar[i] ?? 0)/100},${y*(member.radar[i] ?? 0)/100}`).join(' ')
  return <svg viewBox="-140 -135 280 240" role="img" aria-label={`${member.name}个人雷达图`} className="yx-member-radar">
    <title>{member.name}：交付 {member.deliveredRequirements} 项，关联Bug率 {percent(member.bugRate)}，延期率 {percent(member.delayRate)}</title>
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
const classes: Record<string,string> = { notDue:'不在本月到期范围',onTime:'按期交付',deliveredLate:'延期交付',overdueAtEnd:'月底未交付',unknown:'状态不明' }
export default function MemberMetrics({ sourceId,revision }: { sourceId:string; revision:number }) {
  const navigate = useNavigate()
  const [month,setMonth] = useState(() => new URLSearchParams(window.location.search).get('month') || previousMonth())
  const [target,setTarget] = useState(10),[draftTarget,setDraftTarget] = useState<number | null>(10)
  const [group,setGroup] = useState(''),[search,setSearch] = useState('')
  const [data,setData] = useState<MemberMetricsData | null>(null),[loading,setLoading] = useState(false),[error,setError] = useState('')
  const [selected,setSelected] = useState<MemberMetric | null>(null),[radarPage,setRadarPage] = useState(1)
  useEffect(() => {
    if (!/^\d{4}-\d{2}$/.test(month)) return
    let live = true
    setData(null);setError('');setLoading(true);setSelected(null);setRadarPage(1)
    devopsApi.memberMetrics(sourceId,month,target).then(value => { if (live) setData(value) })
      .catch(e => { if (live) setError(e instanceof ApiError && e.status === 403 ? '需要项目、需求和缺陷的读取权限。' : String(e.message || e)) })
      .finally(() => { if (live) setLoading(false) })
    return () => { live = false }
  },[sourceId,month,target,revision])
  const metrics = data?.metrics
  const groups = Array.from(new Set(metrics?.members.map(p => p.group) || [])).sort()
  const activeGroup = groups.includes(group) ? group : ''
  const rows = (metrics?.members || []).filter(p => (!activeGroup || p.group === activeGroup) && p.name.toLocaleLowerCase().includes(search.toLocaleLowerCase()))
  const targetValid = draftTarget != null && Number.isFinite(draftTarget) && draftTarget > 0 && draftTarget <= 10000
  return <section aria-label="个人月度雷达指标">
    <div className="yx-member-toolbar"><Space wrap size={12}>
      <label>统计月份 <Input aria-label="统计月份" type="month" value={month} style={{ width:160 }} onChange={e => { if (e.target.value) setMonth(e.target.value) }} /></label>
      <label>个人交付参考目标 <InputNumber aria-label="个人交付参考目标" value={draftTarget} min={0.1} max={10000} onChange={setDraftTarget} /> 项</label>
      <Button disabled={!targetValid || draftTarget === target} onClick={() => { if (targetValid) setTarget(draftTarget!) }}>应用参考目标</Button>
    </Space><Popover trigger="click" title="个人指标统计口径" content={<div className="yx-member-notes">
      <p>需求量按本月已发布需求，归当前负责人及明确登记的开发、测试、协作等参与人；多人分别计一次，不能通过成员数求组总量。未确认角色的旁观者不计入。</p>
      <p>关联Bug率=本月已交付需求中，本月存在有效关联Bug的需求数/本月交付需求数。按同一源的明确父需求或关联记录识别；取消、重复及误报等状态排除。关联表示参与需求的缺陷情况，不代表个人责任认定。</p>
      <p>延期率=(延期交付+月底未交付)/(本月应交付-状态不明)。包含全部优先级，取最早排期日当天结束；无有效分母时待评价。本月未结束时以采集时点为截点，仅纳入已经到期的需求。</p>
      <p>雷达三轴：交付量=min(已交付/参考目标,1)×100；关联质量=100-Bug率；交付时效=100-延期率。缺失维度只显示已知点，不填满多边形；参考目标仅调整展示尺度，不产生综合OKR分。</p>
      <p>历史月份由当前快照推算，无法还原排期、负责人修改或重开；截点后变更的非发布状态与缺少状态时间的到期需求列为不明。分组使用已同步的明确映射，有冲突或未知时列为待归属。</p>
    </div>}><Button type="text" icon={<QuestionCircleOutlined />}>统计口径</Button></Popover></div>
    {error && <Alert type="error" message="个人指标读取失败" description={error} showIcon />}
    <Spin spinning={loading}>
      {metrics && <>
        <div className="yx-member-scope"><Tag>{metrics.month}</Tag><span>上海时区 · 截止 {metrics.endExclusive.replace('T',' ').slice(0,16)}（不含）</span><Tag>当前快照推算</Tag><span>有效Bug {metrics.newValidBugs} 条，关联本月交付需求 {metrics.linkedValidBugs} 条</span></div>
        <Alert type="info" showIcon message="个人指标供复盘参考；关联Bug率不等于个人责任归因。" description={`未记录排期 ${metrics.requirementsMissingDue} 项，缺少创建/交付时间 ${metrics.requirementsMissingTime} 项；无分母或缺少关联数据的指标显示待评价。`} style={{ marginBottom:16 }} />
        <Space wrap style={{ marginBottom:16 }}>
          <Select aria-label="雷达组织分组" style={{ minWidth:180 }} value={activeGroup} onChange={v => { setGroup(v);setRadarPage(1) }} options={[{ value:'',label:'全部分组' },...groups.map(value => ({ value,label:value }))]} />
          <Input.Search aria-label="筛选雷达成员" placeholder="搜索成员" allowClear style={{ width:220 }} onChange={e => { setSearch(e.target.value);setRadarPage(1) }} />
          <Typography.Text type="secondary">{rows.length} 位已同步参与人</Typography.Text>
        </Space>
        <Card size="small" title="个人指标表" style={{ marginBottom:16 }}><Table<MemberMetric> size="small" rowKey="name" dataSource={rows} scroll={{ x:1150 }} pagination={{ pageSize:10,hideOnSinglePage:true }} columns={[
          { title:'成员',dataIndex:'name',fixed:'left',width:110,render:(name:string,p) => <Button type="link" size="small" onClick={() => setSelected(p)}>{name}</Button> },
          { title:'分组',dataIndex:'group',width:140 },{ title:'新增需求',dataIndex:'newRequirements' },
          { title:'交付需求量',dataIndex:'deliveredRequirements',sorter:(a,b) => a.deliveredRequirements-b.deliveredRequirements },{ title:'关联Bug数',dataIndex:'linkedBugs' },
          { title:'缺陷需求 / 交付',render:(_:unknown,p) => `${p.bugAffectedRequirements} / ${p.deliveredRequirements}` },{ title:'关联Bug率 ↓',dataIndex:'bugRate',render:percent },
          { title:'延期 / 可评价到期',render:(_:unknown,p) => `${p.deliveredLate+p.overdueAtEnd} / ${p.dueRequirements-p.unknownDelivery}` },
          { title:'延期率 ↓',dataIndex:'delayRate',render:percent },{ title:'状态不明',dataIndex:'unknownDelivery' },
        ]} /></Card>
        <Card size="small" title="个人雷达图" extra={<Typography.Text type="secondary">统一0–100，越向外越好 · 参考目标 {metrics.requirementTarget} 项</Typography.Text>}>
          {!rows.length ? <Empty description="没有符合条件的已同步参与人" /> : <>
            <div className="yx-member-radars">{rows.slice((radarPage-1)*12,radarPage*12).map(p => <article className="yx-member-radar-card" key={p.name} aria-label={`${p.name}指标卡`}>
              <Button type="link" onClick={() => setSelected(p)}>{p.name}</Button><Tag>{p.group}</Tag><Radar member={p} />
              <p>交付 {p.deliveredRequirements} 项 · Bug率 {percent(p.bugRate)} · 延期率 {percent(p.delayRate)}</p>
              {p.radar.some(value => value == null) && <Typography.Text type="secondary">部分维度待评价</Typography.Text>}
            </article>)}</div>
            {rows.length > 12 && <Space className="yx-member-pagination"><Button disabled={radarPage === 1} onClick={() => setRadarPage(p => p-1)}>上一页</Button><span>{radarPage} / {Math.ceil(rows.length/12)}</span><Button disabled={radarPage*12 >= rows.length} onClick={() => setRadarPage(p => p+1)}>下一页</Button></Space>}
          </>}
        </Card>
      </>}
      {!metrics && !loading && !error && <Empty description="请选择统计月份" />}
    </Spin>
    <Drawer open={!!selected} title={`${selected?.name || ''} · 月度需求明细`} width="min(1100px, 95vw)" onClose={() => setSelected(null)}>
      <Table<MemberRequirementMetric> size="small" rowKey="externalId" dataSource={selected?.requirements || []} scroll={{ x:800 }} pagination={{ pageSize:15 }} columns={[
        { title:'需求',render:(_:unknown,r) => <Button type="link" size="small" onClick={() => navigate(`/devops?${new URLSearchParams({ source:sourceId,reference:r.externalId })}`)}>{r.serial || r.externalId} {r.title}</Button> },
        { title:'当前状态',dataIndex:'status' },{ title:'截止日期',dataIndex:'due',render:(v:string) => v || '未记录' },{ title:'本月交付',dataIndex:'delivered',render:(v:boolean) => v ? '是' : '否' },
        { title:'交付分类',dataIndex:'deliveryClass',render:(v:string) => classes[v] || v },{ title:'本月有效关联Bug',dataIndex:'bugIds',render:(v:string[]) => v.length },
      ]} />
    </Drawer>
  </section>
}
