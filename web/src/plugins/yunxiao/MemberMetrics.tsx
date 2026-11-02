import { useEffect, useState } from 'react'
import { Alert, Button, Card, Drawer, Empty, Input, InputNumber, Popover, Select, Space, Spin, Table, Tag, Typography, Tooltip, Pagination, Switch } from 'antd'
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
function MemberName({ name, onClick }: { name:string; onClick:() => void }) {
  return <Tooltip title={name}><Button type="link" size="small" className="yx-member-name" aria-label={name} onClick={onClick}><span>{name}</span></Button></Tooltip>
}
function RateValue({ value, numerator, denominator, label }: { value:number | null; numerator:number; denominator:number; label:string }) {
  return <Tooltip title={`${label}：${numerator} / ${denominator}；${value == null ? '缺少有效分母或关联数据，暂不评价' : '按需求去重'}`}>
    <div className="yx-member-number"><strong>{percent(value)}</strong><small>{numerator} / {denominator} 项需求</small></div>
  </Tooltip>
}
export default function MemberMetrics({ sourceId,revision }: { sourceId:string; revision:number }) {
  const navigate = useNavigate()
  const [month,setMonth] = useState(() => new URLSearchParams(window.location.search).get('month') || previousMonth())
  const [target,setTarget] = useState(10),[draftTarget,setDraftTarget] = useState<number | null>(10)
  const [group,setGroup] = useState(''),[search,setSearch] = useState(''),[sort,setSort] = useState('delivered')
  const [includeInactive,setIncludeInactive] = useState(false)
  const [data,setData] = useState<MemberMetricsData | null>(null),[loading,setLoading] = useState(false),[error,setError] = useState('')
  const [selected,setSelected] = useState<MemberMetric | null>(null),[memberPage,setMemberPage] = useState(1),[pageSize,setPageSize] = useState(6)
  useEffect(() => {
    if (!/^\d{4}-\d{2}$/.test(month)) return
    let live = true
    setData(null);setError('');setLoading(true);setSelected(null);setMemberPage(1)
    devopsApi.memberMetrics(sourceId,month,target).then(value => { if (live) setData(value) })
      .catch(e => { if (live) setError(e instanceof ApiError && e.status === 403 ? '需要项目、需求和缺陷的读取权限。' : String(e.message || e)) })
      .finally(() => { if (live) setLoading(false) })
    return () => { live = false }
  },[sourceId,month,target,revision])
  const metrics = data?.metrics
  const groups = Array.from(new Set(metrics?.members.map(p => p.group) || [])).sort()
  const activeGroup = groups.includes(group) ? group : ''
  const rows = (metrics?.members || []).filter(p => (!activeGroup || p.group === activeGroup) && p.name.toLocaleLowerCase().includes(search.toLocaleLowerCase()) && (includeInactive || p.newRequirements+p.deliveredRequirements+p.dueRequirements > 0))
    .sort((a,b) => (sort === 'delivered' ? b.deliveredRequirements-a.deliveredRequirements || b.newRequirements-a.newRequirements : 0) || a.name.localeCompare(b.name,'zh-CN'))
  const currentPage = Math.min(memberPage,Math.max(1,Math.ceil(rows.length/pageSize)))
  const pageRows = rows.slice((currentPage-1)*pageSize,currentPage*pageSize)
  const targetValid = draftTarget != null && Number.isFinite(draftTarget) && draftTarget > 0 && draftTarget <= 10000
  const pagination = (label:string) => rows.length > pageSize ? <nav className="yx-member-pagination" aria-label={label}><Pagination size="small" current={currentPage} pageSize={pageSize} total={rows.length} showSizeChanger locale={{ items_per_page:'人 / 页' }} pageSizeOptions={[6,12]} showTotal={total => `共 ${total} 人，表格与雷达图同步翻页`} onChange={(page,size) => { setMemberPage(size === pageSize ? page : 1);setPageSize(size) }} /></nav> : null
  return <section aria-label="个人月度雷达指标" className="yx-member-page">
    <div className="yx-member-toolbar"><Space wrap size={12}>
      <label>统计月份 <Input aria-label="统计月份" type="month" value={month} style={{ width:160 }} onChange={e => { if (e.target.value) setMonth(e.target.value) }} /></label>
      <label>个人交付参考目标 <InputNumber aria-label="个人交付参考目标" value={draftTarget} min={0.1} max={10000} onChange={setDraftTarget} /> 项</label>
      <Button disabled={!targetValid || draftTarget === target} onClick={() => { if (targetValid) setTarget(draftTarget!) }}>应用参考目标</Button>
    </Space><Popover trigger="click" title="个人指标统计口径" content={<div className="yx-member-notes">
      <p>需求量按本月已发布需求，归当前负责人及明确登记的开发、测试、协作等参与人；仅识别带用户标识的人员类型字段，自由文本不作为成员。多人分别计一次，不能通过成员数求组总量。</p>
      <p>关联Bug率=本月已交付需求中，本月存在有效关联Bug的需求数/本月交付需求数。按同一源的明确父需求或关联记录识别；取消、重复及误报等状态排除。参与需求的缺陷情况不代表个人责任认定。</p>
      <p>延期率=(延期交付+月底未交付)/(本月应交付-状态不明)。包含全部优先级，取最早排期日当天结束；无有效分母时待评价。本月未结束时以采集时点为截点，仅纳入已经到期的需求。</p>
      <p>雷达三轴：交付量=min(已交付/参考目标,1)×100；关联质量=100-Bug率；交付时效=100-延期率。缺失维度只显示已知点；参考目标仅调整展示尺度，不产生综合OKR分。</p>
      <p>历史月份由当前快照推算，无法还原排期、负责人修改或重开；截点后变更的非发布状态与缺少状态时间的到期需求列为不明。分组冲突或未知时列为待归属；可开启“包含无活动成员”查看本月无记录的已同步参与人。</p>
      {metrics && <p>有效Bug {metrics.newValidBugs} 条，其中关联本月交付需求 {metrics.linkedValidBugs} 条；未记录排期 {metrics.requirementsMissingDue} 项，缺少创建/交付时间 {metrics.requirementsMissingTime} 项。</p>}
    </div>}><Button type="text" icon={<QuestionCircleOutlined />}>统计口径</Button></Popover></div>
    {error && <Alert type="error" message="个人指标读取失败" description={error} showIcon />}
    <Spin spinning={loading}>
      <div className="yx-member-results">
      {metrics && <>
        <div className="yx-member-scope"><Tag>{metrics.month}</Tag><span>上海时区 · 截止 {metrics.endExclusive.replace('T',' ').slice(0,16)}（不含）</span><span>当前快照推算，关联Bug率供复盘参考</span></div>
        <div className="yx-member-filters">
          <Select aria-label="雷达组织分组" style={{ width:170 }} value={activeGroup} onChange={v => { setGroup(v);setMemberPage(1) }} options={[{ value:'',label:'全部分组' },...groups.map(value => ({ value,label:value }))]} />
          <Input.Search aria-label="筛选雷达成员" placeholder="搜索成员" allowClear style={{ width:210 }} onChange={e => { setSearch(e.target.value);setMemberPage(1) }} />
          <Select aria-label="成员排序" style={{ width:160 }} value={sort} onChange={v => { setSort(v);setMemberPage(1) }} options={[{ value:'delivered',label:'按交付量排序' },{ value:'name',label:'按姓名排序' }]} />
          <label className="yx-member-inactive"><Switch size="small" aria-label="包含无活动成员" checked={includeInactive} onChange={v => { setIncludeInactive(v);setMemberPage(1) }} />包含无活动成员</label>
          <Typography.Text type="secondary">筛选 {rows.length} / 已同步 {metrics.members.length} 人</Typography.Text>
        </div>
        <Card size="small" title="个人指标表" className="yx-members-table">
          <Table<MemberMetric> size="small" tableLayout="fixed" rowKey="name" dataSource={pageRows} scroll={{ x:960 }} pagination={false} columns={[
            { title:'成员',dataIndex:'name',fixed:'left',width:140,ellipsis:true,render:(name:string,p) => <MemberName name={name} onClick={() => setSelected(p)} /> },
            { title:'分组',dataIndex:'group',width:150,ellipsis:true },
            { title:'交付需求量',dataIndex:'deliveredRequirements',width:115,align:'right',render:(v:number,p) => <div className="yx-member-number"><strong>{v.toLocaleString('zh-CN')}</strong><small>新增 {p.newRequirements.toLocaleString('zh-CN')}</small></div> },
            { title:'关联Bug数',dataIndex:'linkedBugs',width:105,align:'right' },
            { title:'关联Bug率 ↓',width:170,align:'right',render:(_:unknown,p) => <RateValue label="关联Bug率" value={p.bugRate} numerator={p.bugAffectedRequirements} denominator={p.deliveredRequirements} /> },
            { title:'延期率 ↓',width:170,align:'right',render:(_:unknown,p) => <RateValue label="延期率" value={p.delayRate} numerator={p.deliveredLate+p.overdueAtEnd} denominator={p.dueRequirements-p.unknownDelivery} /> },
            { title:'状态不明',dataIndex:'unknownDelivery',width:110,align:'right' },
          ]} />
        </Card>
        {pagination('成员统一分页')}
        <Card size="small" title="个人雷达图" className="yx-members-charts" extra={<span className="yx-member-chart-guide">统一0–100，越向外越好</span>}>
          {!pageRows.length ? <Empty description="没有符合条件的成员，可调整筛选条件" /> : <div className="yx-member-radars">{pageRows.map(p => <article className="yx-member-radar-card" key={p.name} aria-label={`${p.name}指标卡`}>
            <div className="yx-member-card-heading"><MemberName name={p.name} onClick={() => setSelected(p)} /><Tooltip title={p.group}><span className="yx-member-card-group">{p.group}</span></Tooltip></div>
            <Radar member={p} />
            <dl className="yx-member-card-metrics"><div><dt>交付需求</dt><dd>{p.deliveredRequirements.toLocaleString('zh-CN')}<small> 项</small></dd></div><div><dt>关联Bug率</dt><dd>{percent(p.bugRate)}</dd></div><div><dt>延期率</dt><dd>{percent(p.delayRate)}</dd></div></dl>
            <div className="yx-member-card-footer">{p.radar.some(value => value == null) ? '部分维度待评价' : '点击成员查看需求明细'}</div>
          </article>)}</div>}
        </Card>
        {pagination('雷达图分页')}
      </>}
      {!metrics && !loading && !error && <Empty description="请选择统计月份" />}
      </div>
    </Spin>
    <Drawer open={!!selected} title={<Tooltip title={selected?.name}><span className="yx-member-drawer-title">{selected?.name || ''} · 月度需求明细</span></Tooltip>} width="min(1100px, 95vw)" onClose={() => setSelected(null)}>
      <Table<MemberRequirementMetric> size="small" tableLayout="fixed" rowKey="externalId" dataSource={selected?.requirements || []} scroll={{ x:860 }} pagination={{ pageSize:15 }} columns={[
        { title:'需求',width:330,ellipsis:true,render:(_:unknown,r) => <MemberName name={`${r.serial || r.externalId} ${r.title}`} onClick={() => navigate(`/devops?${new URLSearchParams({ source:sourceId,reference:r.externalId })}`)} /> },
        { title:'当前状态',dataIndex:'status',width:100,ellipsis:true },{ title:'截止日期',dataIndex:'due',width:110,render:(v:string) => v || '未记录' },{ title:'本月交付',dataIndex:'delivered',width:90,render:(v:boolean) => v ? '是' : '否' },
        { title:'交付分类',dataIndex:'deliveryClass',width:120,render:(v:string) => classes[v] || v },{ title:'本月关联Bug',dataIndex:'bugIds',width:110,align:'right',render:(v:string[]) => v.length },
      ]} />
    </Drawer>
  </section>
}
