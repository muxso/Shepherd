import { useEffect, useState } from 'react'
import { Alert, Button, Card, Drawer, Empty, Input, InputNumber, Popover, Select, Space, Spin, Table, Tag, Typography, Tooltip, Pagination, Switch, DatePicker, Segmented } from 'antd'
import { QuestionCircleOutlined } from '@ant-design/icons'
import { useLocation, useNavigate } from 'react-router-dom'
import dayjs from 'dayjs'
import 'dayjs/locale/zh-cn'
import monthLocale from 'antd/es/date-picker/locale/zh_CN'
import { previousMonth, shanghaiMonth, validMonth } from './memberMonth'
import { MemberRadar, GroupRadar, radarColors } from './MemberRadar'
import { ApiError } from '../../api'
import { devopsApi, type MemberMetric, type MemberMetricsData, type MemberRequirementMetric } from './api'
import './member-metrics.css'

const percent = (value: number | null) => value == null ? '待评价' : `${value.toFixed(1)}%`
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
  const navigate = useNavigate(), location = useLocation()
  const query = new URLSearchParams(location.search)
  const rawMonth = query.get('month')
  const month = validMonth(rawMonth) ? rawMonth : previousMonth()
  const group = query.get('group') || ''
  const rawTarget = Number(query.get('requirementTarget') || 10)
  const target = Number.isFinite(rawTarget) && rawTarget > 0 && rawTarget <= 10000 ? rawTarget : 10
  const chartView = query.get('chart') === 'members' || !group ? 'members' : 'group'
  const [draftTarget,setDraftTarget] = useState<number | null>(target)
  const [invalidMonth,setInvalidMonth] = useState(!!rawMonth && !validMonth(rawMonth))
  const [search,setSearch] = useState(''),[sort,setSort] = useState('delivered')
  const [includeInactive,setIncludeInactive] = useState(false)
  const [knownGroups,setKnownGroups] = useState<string[]>([])
  const [result,setResult] = useState<{ key:string; data?:MemberMetricsData; error?:string; loading:boolean } | null>(null)
  const [selected,setSelected] = useState<MemberMetric | null>(null),[memberPage,setMemberPage] = useState(1),[pageSize,setPageSize] = useState(6)
  const requestKey = JSON.stringify([sourceId,month,target,revision])
  const data = result?.key === requestKey ? result.data : undefined
  const error = result?.key === requestKey ? result.error : ''
  const loading = result?.key !== requestKey || result.loading
  const updateQuery = (patch:Record<string,string>,replace=false) => {
    const next = new URLSearchParams(location.search); next.set('month',month)
    for (const [key,value] of Object.entries(patch)) { if (value) next.set(key,value); else next.delete(key) }
    for (const key of ['reference','kind','metric','date']) next.delete(key)
    next.set('view','radar'); next.set('source',sourceId)
    navigate({ pathname:location.pathname,search:next.toString() },{ replace })
  }
  useEffect(() => { setDraftTarget(target) },[target])
  useEffect(() => {
    if (!rawMonth || validMonth(rawMonth)) return
    setInvalidMonth(true)
    const next = new URLSearchParams(location.search); next.set('month',month)
    navigate({ pathname:location.pathname,search:next.toString() },{ replace:true })
  },[rawMonth,month,location.pathname,location.search,navigate])
  useEffect(() => {
    let live = true
    setResult({ key:requestKey,loading:true });setSelected(null);setMemberPage(1)
    devopsApi.memberMetrics(sourceId,month,target).then(value => { if (live) { setKnownGroups(Array.from(new Set(value.metrics.members.map(p => p.group))).sort()); setResult({ key:requestKey,data:value,loading:false }) } })
      .catch(e => { if (live) setResult({ key:requestKey,loading:false,error:e instanceof ApiError && e.status === 403 ? '需要项目、需求和缺陷的读取权限。' : String(e.message || e) }) })
    return () => { live = false }
  },[requestKey,sourceId,month,target])
  const metrics = data?.metrics
  const groups = knownGroups
  const groupOptions = group && !groups.includes(group) ? [...groups,group] : groups
  const activeGroup = group
  const rows = (metrics?.members || []).filter(p => (!activeGroup || p.group === activeGroup) && p.name.toLocaleLowerCase().includes(search.toLocaleLowerCase()) && (includeInactive || p.newRequirements+p.deliveredRequirements+p.dueRequirements > 0))
    .sort((a,b) => (sort === 'delivered' ? b.deliveredRequirements-a.deliveredRequirements || b.newRequirements-a.newRequirements : 0) || a.name.localeCompare(b.name,'zh-CN'))
  let selectedNames:string[] | null = null
  try {
    const names:unknown = JSON.parse(query.get('compare') || 'null')
    if (Array.isArray(names) && names.length <= 6 && names.every(name => typeof name === 'string' && name.length <= 300)) selectedNames = [...new Set(names)]
  } catch { /* A malformed selection falls back to the visible team defaults. */ }
  const comparisonNames = selectedNames ?? rows.slice(0,6).map(p => p.name)
  const compared = comparisonNames.flatMap(name => rows.find(p => p.name === name) || []).slice(0,6)
  const currentPage = Math.min(memberPage,Math.max(1,Math.ceil(rows.length/pageSize)))
  const pageRows = rows.slice((currentPage-1)*pageSize,currentPage*pageSize)
  const targetValid = draftTarget != null && Number.isFinite(draftTarget) && draftTarget > 0 && draftTarget <= 10000
  const pagination = (label:string) => rows.length > pageSize ? <nav className="yx-member-pagination" aria-label={label}><Pagination size="small" current={currentPage} pageSize={pageSize} total={rows.length} showSizeChanger locale={{ items_per_page:'人 / 页' }} pageSizeOptions={[6,12]} showTotal={total => `共 ${total} 人，${chartView === 'group' ? '明细表分页' : '表格与雷达图同步翻页'}`} onChange={(page,size) => { setMemberPage(size === pageSize ? page : 1);setPageSize(size) }} /></nav> : null
  return <section aria-label="个人月度雷达指标" className="yx-member-page">
    <div className="yx-member-toolbar"><Space wrap size={12}>
      <label>统计月份 <DatePicker aria-label="统计月份" picker="month" locale={monthLocale} value={dayjs(`${month}-01`).locale('zh-cn')} format="YYYY年MM月" allowClear={false} inputReadOnly style={{ width:160 }} disabledDate={date => date.format('YYYY-MM') > shanghaiMonth()} onChange={date => { if (date) { setInvalidMonth(false);updateQuery({ month:date.format('YYYY-MM') }) } }} /></label>
      <label>个人交付参考目标 <InputNumber aria-label="个人交付参考目标" value={draftTarget} min={0.1} max={10000} onChange={setDraftTarget} /> 项</label>
      <Button disabled={!targetValid || draftTarget === target} onClick={() => { if (targetValid) updateQuery({ requirementTarget:String(draftTarget) }) }}>应用参考目标</Button>
    </Space><Popover trigger="click" title="个人指标统计口径" content={<div className="yx-member-notes">
      <p>需求量按本月已发布需求，归当前负责人及明确登记的开发、测试、协作等参与人；仅识别带用户标识的人员类型字段，自由文本不作为成员。多人分别计一次，不能通过成员数求组总量。</p>
      <p>关联Bug率=本月已交付需求中，本月存在有效关联Bug的需求数/本月交付需求数。按同一源的明确父需求或关联记录识别；取消、重复及误报等状态排除。参与需求的缺陷情况不代表个人责任认定。</p>
      <p>延期率=(延期交付+月底未交付)/(本月应交付-状态不明)。包含全部优先级，取最早排期日当天结束；无有效分母时待评价。本月未结束时以采集时点为截点，仅纳入已经到期的需求。</p>
      <p>雷达三轴：交付量=min(已交付/参考目标,1)×100；关联质量=100-Bug率；交付时效=100-延期率。缺失维度只显示已知点；参考目标仅调整展示尺度，不产生综合OKR分。</p>
      <p>历史月份由当前快照推算，无法还原排期、负责人修改或重开；截点后变更的非发布状态与缺少状态时间的到期需求列为不明。分组冲突或未知时列为待归属；可开启“包含无活动成员”查看本月无记录的已同步参与人。</p>
      {metrics && <p>有效Bug {metrics.newValidBugs} 条，其中关联本月交付需求 {metrics.linkedValidBugs} 条；未记录排期 {metrics.requirementsMissingDue} 项，缺少创建/交付时间 {metrics.requirementsMissingTime} 项。</p>}
    </div>}><Button type="text" icon={<QuestionCircleOutlined />}>统计口径</Button></Popover></div>
        <div className="yx-member-filters">
          <Select aria-label="雷达组织分组" style={{ width:170 }} value={activeGroup} onChange={v => { setMemberPage(1);setPageSize(v ? 12 : 6);updateQuery({ group:v,chart:v ? 'group' : 'members',compare:'' }) }} options={[{ value:'',label:'全部分组' },...groupOptions.map(value => ({ value,label:value }))]} />
          <Input.Search aria-label="筛选雷达成员" placeholder="搜索成员" allowClear style={{ width:210 }} onChange={e => { setSearch(e.target.value);setMemberPage(1) }} />
          <Select aria-label="成员排序" style={{ width:160 }} value={sort} onChange={v => { setSort(v);setMemberPage(1) }} options={[{ value:'delivered',label:'按交付量排序' },{ value:'name',label:'按姓名排序' }]} />
          <label className="yx-member-inactive"><Switch size="small" aria-label="包含无活动成员" checked={includeInactive} onChange={v => { setIncludeInactive(v);setMemberPage(1) }} />包含无活动成员</label>
          <Typography.Text type="secondary">{loading ? '正在更新…' : `筛选 ${rows.length} / 已同步 ${metrics?.members.length || 0} 人`}</Typography.Text>
        </div>
    {invalidMonth && <Alert type="info" showIcon closable onClose={() => setInvalidMonth(false)} message="月份参数不可用，已显示上一个完整月份，请按需重新选择。" style={{ marginBottom:12 }} />}
    {error && <Alert type="error" message="个人指标读取失败" description={error} showIcon />}
    <Spin spinning={loading}>
      <div className="yx-member-results">
      {metrics && <>
        <div className="yx-member-scope"><Tag>{metrics.month}</Tag><span>上海时区 · 截止 {metrics.endExclusive.replace('T',' ').slice(0,16)}（不含）</span><span>当前快照推算，关联Bug率供复盘参考</span></div>
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
        <Card size="small" title={activeGroup ? `${activeGroup} · 雷达图` : '个人雷达图'} className="yx-members-charts" extra={<Segmented aria-label="雷达展示方式" value={chartView} onChange={value => updateQuery({ chart:String(value) })} options={[{ value:'members',label:'个人雷达' },{ value:'group',label:'组内对比',disabled:!activeGroup }]} />}>
          <div className="yx-member-chart-caption">{activeGroup ? `${activeGroup} · 匹配 ${rows.length} 位成员 · ${month}` : '全部成员'} · 统一0–100，越向外越好</div>
          {chartView === 'group' ? <section aria-label="小组成员雷达对比">
            <div className="yx-group-radar-toolbar"><span>对比成员</span><Select aria-label="选择组内对比成员" mode="multiple" maxCount={6} maxTagCount="responsive" style={{ flex:1,minWidth:180 }} value={compared.map(p => p.name)} options={rows.map(p => ({ value:p.name,label:p.name }))} onChange={names => updateQuery({ compare:JSON.stringify(names) })} placeholder="选择本组成员，最多6人" /><Typography.Text type="secondary">已选 {compared.length} / {rows.length} 人</Typography.Text></div>
            {!compared.length ? <Empty description="请选择本组成员进行对比" /> : <div className="yx-group-radar-layout">
              <GroupRadar group={activeGroup} members={compared} />
              <div className="yx-group-radar-legend">{compared.map((p,index) => <article key={p.name} className="yx-group-radar-person"><span className="yx-group-radar-swatch" style={{ background:radarColors[index] }} /><div><MemberName name={p.name} onClick={() => setSelected(p)} /><span>交付 {p.deliveredRequirements} 项 · 关联Bug率 {percent(p.bugRate)} · 延期率 {percent(p.delayRate)}</span>{p.radar.some(value => value == null) && <small>部分维度待评价，仅绘制已知点</small>}</div></article>)}</div>
            </div>}
          </section> : <>
          {!pageRows.length ? <Empty description="没有符合条件的成员，可调整筛选条件" /> : <div className="yx-member-radars">{pageRows.map(p => <article className="yx-member-radar-card" key={p.name} aria-label={`${p.name}指标卡`}>
            <div className="yx-member-card-heading"><MemberName name={p.name} onClick={() => setSelected(p)} /><Tooltip title={p.group}><span className="yx-member-card-group">{p.group}</span></Tooltip></div>
            <MemberRadar member={p} />
            <dl className="yx-member-card-metrics"><div><dt>交付需求</dt><dd>{p.deliveredRequirements.toLocaleString('zh-CN')}<small> 项</small></dd></div><div><dt>关联Bug率</dt><dd>{percent(p.bugRate)}</dd></div><div><dt>延期率</dt><dd>{percent(p.delayRate)}</dd></div></dl>
            <div className="yx-member-card-footer">{p.radar.some(value => value == null) ? '部分维度待评价' : '点击成员查看需求明细'}</div>
          </article>)}</div>}
          </>}
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
