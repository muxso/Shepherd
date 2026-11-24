import { ReadError } from '../../components/Page'
import { useI18n } from '../../i18n'
import { useEffect, useState } from 'react'
import { Alert, Button, Card, Drawer, Empty, Input, InputNumber, Popover, Select, Space, Spin, Table, Tag, Typography, Tooltip, Pagination, Switch, DatePicker, Segmented } from 'antd'
import { QuestionCircleOutlined } from '@ant-design/icons'
import { useLocation, useNavigate } from 'react-router-dom'
import dayjs from 'dayjs'
import 'dayjs/locale/zh-cn'
import monthLocale from 'antd/es/date-picker/locale/zh_CN'
import { shanghaiMonth, validMonth } from './memberMonth'
import { MemberRadar, GroupRadar, onTimeRate, radarColor, radarValues } from './MemberRadar'
import MonthlyComparison from './MonthlyComparison'
import MemberManagementOverview from './MemberManagementOverview'
import BugQuality, { BugRows } from './BugQuality'
import { devopsApi, type MemberMetric, type MemberMetricsData, type MemberRequirementMetric } from './api'
import './member-metrics.css'
import { MemberSonar } from './MemberSonar'
import { sonarRates } from './SonarNewCodeMetrics'

const percent = (value: number | null) => value == null ? '待评价' : `${value.toFixed(1)}%`
const sortFields = [{key:'delivered',label:'requirementsDelivered',scope:'requirementParticipation'},{key:'bugs',label:'linkedDefectsCount',scope:'requirementParticipation'},{key:'bugRate',label:'deliveredRequirementDefectRate',scope:'requirementParticipation'},{key:'processedBugs',label:'assignedDefectsHandledCount',scope:'defectOwnership'},{key:'reopenedBugs',label:'reopenedDefects',scope:'defectOwnership'},{key:'reopenCount',label:'reopenEvents',scope:'defectOwnership'},{key:'reopenRate',label:'defectReopenRate',scope:'defectOwnership'},{key:'name',label:'name',scope:'general'}]
const qualityCount = (p:MemberMetric) => p.qualityBugs ?? 0
const unknownCount = (p:MemberMetric) => p.reopenUnknownBugs ?? Math.max(0,qualityCount(p)-(p.reopenEvaluatedBugs ?? 0))
const historyCount = (p:MemberMetric) => unknownCount(p) > 0 && !p.reopenEvaluatedBugs ? '待同步' : `${unknownCount(p) > 0 ? '≥' : ''}${p.reopenCount ?? 0}`
function ReopenValue({ member:p,rate=false }: { member:MemberMetric; rate?:boolean }) {
  const { t } = useI18n()
  return <Tooltip title={`按缺陷本月状态流转统计，含遗留缺陷，归源缺陷责任人。比例：重开Bug ${p.reopenedBugs ?? 0} / 本月修复、验证或重开Bug ${p.processedBugs ?? 0}；已核验历史 ${p.reopenEvaluatedBugs ?? 0} / ${qualityCount(p)}。人工沟通返修次数不能从状态流转推算。`}>
    <div className="yx-member-number"><strong>{rate ? percent(p.reopenRate ?? null) : `${historyCount(p)}${historyCount(p) === t("engineering.awaitingSync") ? '' : ' 次'}`}</strong><small>{unknownCount(p) > 0 ? `已核验 ${p.reopenEvaluatedBugs ?? 0} / ${qualityCount(p)} 个Bug` : rate ? `${p.reopenedBugs ?? 0} / ${p.processedBugs ?? 0} 个Bug` : '按状态流转累加'}</small></div>
  </Tooltip>
}
const classes: Record<string,string> = { notDue:'不在本月到期范围',pendingRelease:'待发布，不计延期',onTime:'已交付，不计延期',deliveredLate:'已交付，不计延期',delivered:'已交付，不计延期',overdueAtEnd:'未交付且已超期',unknown:'状态待核验' }
function MemberName({ name, onClick }: { name:string; onClick:() => void }) {
  return <Tooltip title={name}><Button type="link" size="small" className="yx-member-name" aria-label={name} onClick={onClick}><span>{name}</span></Button></Tooltip>
}
function RateValue({ value, numerator, denominator, label }: { value:number | null; numerator:number; denominator:number; label:string }) {
  return <Tooltip title={`${label}：${numerator} / ${denominator}；${value == null ? '缺少有效分母或关联数据，暂不评价' : '按需求去重'}`}>
    <div className="yx-member-number"><strong>{percent(value)}</strong><small>{numerator} / {denominator} 项需求</small></div>
  </Tooltip>
}
function MemberSonarRates({ member }: { member: MemberMetric }) {
  if (!member.sonar?.recent.length) return null
  const { coverage, duplication } = sonarRates(member.sonar.recent[0])
  return <small>最近一次 Sonar · 覆盖率 {coverage} · 重复率 {duplication}</small>
}

function MemberSonarSummary({ member, onDetails }: { member: MemberMetric; onDetails: () => void }) {
  const { t } = useI18n()
  const analysis = member.sonar?.recent[0]
  if (!analysis) return <div className="yx-member-sonar-empty">{t("engineering.noAttributedSonarAnalysesThisMonth")}</div>
  const { coverage, duplication } = sonarRates(analysis)
  const issues = analysis.analysisQuality?.newIssues ?? analysis.newIssues
  const repository = analysis.repositoryName || analysis.projectKey || analysis.repositoryId
  const branch = analysis.branch || analysis.push?.branch
  const revision = [repository, branch].filter(Boolean).join(' / ')
  const gate = analysis.gate === 'OK' ? '门禁通过' : analysis.gate === 'ERROR' ? '门禁未通过' : '门禁待评价'
  return <section className="yx-member-sonar" aria-label="最近一次 Sonar 扫描">
    <div className="yx-member-sonar-heading">
      <strong>Sonar</strong>
      <span data-gate={analysis.gate}>{gate}</span>
      <Button type="link" size="small" onClick={onDetails} aria-label={`查看 ${member.name} 的扫描明细`}>明细</Button>
    </div>
    <Tooltip title={`${revision} · ${analysis.revision.slice(0, 12)}`}><div className="yx-member-sonar-source">{revision}</div></Tooltip>
    <dl className="yx-member-sonar-metrics">
      <div><dt>{t('sonar.newIssues', '新增问题')}</dt><dd>{issues?.toLocaleString() ?? '—'}</dd></div>
      <div><dt>{t('sonar.coverage', '覆盖率')}</dt><dd>{coverage}</dd></div>
      <div><dt>{t('sonar.duplication', '重复率')}</dt><dd>{duplication}</dd></div>
    </dl>
  </section>
}

function MemberBugMetrics({ member }: { member: MemberMetric }) {
  return <div className="yx-member-bug-metrics">
    <div>已交付需求 · 含关联缺陷 {member.bugAffectedRequirements} / {member.deliveredRequirements} 项（{percent(member.bugRate)}）</div>
    <div>责任缺陷 · 本月处理或重开 {member.processedBugs ?? 0} 个 · 重开 {member.reopenedBugs ?? 0} 个（{percent(member.reopenRate ?? null)}）</div>
  </div>
}

export default function MemberMetrics({ sourceId,revision }: { sourceId:string; revision:number }) {
  const { t } = useI18n()
  const navigate = useNavigate(), location = useLocation()
  const query = new URLSearchParams(location.search)
  const rawMonth = query.get('month')
  const [currentMonth,setCurrentMonth] = useState(shanghaiMonth)
  useEffect(() => { const timer = window.setInterval(() => setCurrentMonth(shanghaiMonth()),60_000); return () => window.clearInterval(timer) },[])
  const month = validMonth(rawMonth) ? rawMonth : currentMonth
  const group = query.get('group') ?? (query.get('perspective') === 'group' ? query.get('activityGroup') || '' : '')
  const rawTarget = Number(query.get('requirementTarget') || 10)
  const target = Number.isFinite(rawTarget) && rawTarget > 0 && rawTarget <= 10000 ? rawTarget : 10
  const chartView = query.get('chart') === 'members' || !group ? 'members' : 'group'
  const [draftTarget,setDraftTarget] = useState<number | null>(target)
  const [invalidMonth,setInvalidMonth] = useState(!!rawMonth && !validMonth(rawMonth))
  const explicitDetails = query.get('details')
  const showDetails = explicitDetails ? explicitDetails === '1' : query.has('chart') || localStorage.getItem('shepherd.devops.memberDetails') === 'true'
  const [search,setSearch] = useState('')
  const sort = sortFields.some(field => field.key === query.get('sort')) ? query.get('sort')! : 'delivered'
  const order = query.get('order') === 'asc' ? 'asc' : 'desc'
  const [includeInactive,setIncludeInactive] = useState(false)
  const [knownGroups,setKnownGroups] = useState<string[]>([])
  const [result,setResult] = useState<{ key:string; data?:MemberMetricsData; error?:unknown; loading:boolean } | null>(null)
  const [selected,setSelected] = useState<MemberMetric | null>(null),[memberPage,setMemberPage] = useState(1),[pageSize,setPageSize] = useState(6)
  const requestKey = JSON.stringify([sourceId,month,target,revision])
  const data = result?.key === requestKey ? result.data : undefined
  const error = result?.key === requestKey ? result.error : undefined
  const loading = result?.key !== requestKey || result.loading
  const updateQuery = (patch:Record<string,string>,replace=false) => {
    const next = new URLSearchParams(location.search); next.set('month',month)
    for (const [key,value] of Object.entries(patch)) { if (value) next.set(key,value); else next.delete(key) }
    for (const key of ['reference','kind','metric','date']) next.delete(key)
    next.set('view','radar'); next.set('source',sourceId)
    navigate({ pathname:location.pathname,search:next.toString() },{ replace })
  }
  const setShowDetails = (value: boolean) => {
    localStorage.setItem('shepherd.devops.memberDetails', String(value))
    updateQuery({ details: value ? '1' : '0' })
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
      .catch(e => { if (live) setResult({ key:requestKey,loading:false,error:e }) })
    return () => { live = false }
  },[requestKey,sourceId,month,target])
  const metrics = data?.metrics
  const groups = knownGroups
  const groupOptions = group && !groups.includes(group) ? [...groups,group] : groups
  const activeGroup = group
  const rows = (metrics?.members || []).filter(p => (!activeGroup || p.group === activeGroup) && p.name.toLocaleLowerCase().includes(search.toLocaleLowerCase()) && (includeInactive || p.newRequirements+p.deliveredRequirements+p.dueRequirements+(p.qualityBugs ?? 0)+(p.sonar?.pushes ?? 0) > 0))
    .sort((a,b) => {
      const value = (p:MemberMetric) => sort === 'delivered' ? p.deliveredRequirements : sort === 'bugs' ? p.linkedBugs : sort === 'bugRate' ? p.bugRate : sort === 'processedBugs' ? (unknownCount(p) ? null : p.processedBugs ?? 0) : sort === 'reopenedBugs' ? (unknownCount(p) ? null : p.reopenedBugs ?? 0) : sort === 'reopenCount' ? (unknownCount(p) === 0 ? p.reopenCount ?? 0 : null) : p.reopenRate ?? null
      const x=value(a),y=value(b)
      if (sort !== 'name' && (x == null || y == null)) return (x == null ? 1 : 0)-(y == null ? 1 : 0) || a.name.localeCompare(b.name,'zh-CN')
      const difference = sort === 'name' ? a.name.localeCompare(b.name,'zh-CN') : x!-y!
      return difference*(order === 'asc' ? 1 : -1) || a.name.localeCompare(b.name,'zh-CN')
    })
  const changeSort = (field:string,direction:string) => { setMemberPage(1);updateQuery({ sort:field,order:direction }) }
  let selectedNames:string[] | null = null
  try {
    const names:unknown = JSON.parse(query.get('compare') || 'null')
    if (Array.isArray(names) && names.every(name => typeof name === 'string' && name.length <= 300)) selectedNames = [...new Set(names)]
  } catch { /* A malformed selection falls back to the visible team defaults. */ }
  const comparisonNames = selectedNames ?? rows.slice(0,6).map(p => p.name)
  const comparisonSet = new Set(comparisonNames)
  const compared = rows.filter(p => comparisonSet.has(p.name))
  const currentPage = Math.min(memberPage,Math.max(1,Math.ceil(rows.length/pageSize)))
  const pageRows = rows.slice((currentPage-1)*pageSize,currentPage*pageSize)
  const targetValid = draftTarget != null && Number.isFinite(draftTarget) && draftTarget > 0 && draftTarget <= 10000
  const pagination = (label:string) => rows.length > pageSize ? <nav className="yx-member-pagination" aria-label={label}><Pagination size="small" current={currentPage} pageSize={pageSize} total={rows.length} showSizeChanger locale={{ items_per_page:'人 / 页' }} pageSizeOptions={[6,12]} showTotal={total => `共 ${total} 人，${chartView === 'group' ? '明细表分页' : '表格与雷达图同步翻页'}`} onChange={(page,size) => { setMemberPage(size === pageSize ? page : 1);setPageSize(size) }} /></nav> : null
  return <section aria-label={t("engineering.monthlyIndividualMetrics")} className="yx-member-page">
    <div className="yx-member-toolbar"><Space wrap size={12}>
      <label>{t("engineering.reportingMonth")} <DatePicker aria-label={t("engineering.reportingMonth")} picker="month" locale={monthLocale} value={dayjs(`${month}-01`).locale('zh-cn')} format="YYYY年MM月" allowClear={false} inputReadOnly style={{ width:160 }} disabledDate={date => date.format('YYYY-MM') > shanghaiMonth()} onChange={date => { if (date) { setInvalidMonth(false);updateQuery({ month:date.format('YYYY-MM') }) } }} /></label>
      <label>{t("engineering.individualDeliveryTarget")} <InputNumber aria-label={t("engineering.individualDeliveryTarget")} value={draftTarget} min={0.1} max={10000} onChange={setDraftTarget} /> 项</label>
      <Button onClick={() => {setShowDetails(true);window.setTimeout(() => document.getElementById('yx-bug-quality')?.scrollIntoView({behavior:'smooth'}),0)}}>{t("engineering.reviewDefectHistory")}</Button>
      <Button disabled={!targetValid || draftTarget === target} onClick={() => { if (targetValid) updateQuery({ requirementTarget:String(draftTarget) }) }}>{t("engineering.applyTarget")}</Button>
    </Space><Popover trigger="click" title={t("engineering.howIndividualMetricsAreCalculated")} content={<div className="yx-member-notes">
      <p>需求量按本月已发布需求，归当前负责人及明确登记的开发、测试、协作等参与人；仅识别带用户标识的人员类型字段，自由文本不作为成员。多人分别计一次，不能通过成员数求组总量。</p>
      <p>含Bug交付需求占比=本月已交付需求中，本月存在有效关联Bug的需求数/本月交付需求数。按同一源的明确父需求或关联记录识别；取消、重复及误报等状态排除。参与需求的缺陷情况不代表个人责任认定。</p>
      <p>延期率=未交付且已超期需求数/(本月应交付-状态不明)。当前待发布、已发布、已交付及其他完成终态不计延期，超期后完成、统计月之后完成或缺少完成时间也不计延期；历史月份同样按当前快照排除已交付项。包含全部优先级，取最早排期日当天结束；本月进行中仅评价已经到期的需求，无有效分母时待评价。</p>
      <p>状态重开独立于需求交付：纳入本月发生修复完成、验证完成或重开状态流转的全部源Bug，包含过去月份创建的遗留缺陷，不要求已关联需求或需求已发布。次数按本月重开事件累加；比例=本月重开过的Bug去重数/本月修复、验证或重开Bug去重数。归属仅取源“缺陷责任人”，不以当前处理人、开发者或需求参与人代替；缺失归待归属。历史未核验时比例待评价、次数显示已知下限。状态流转不能推算聊天中重复返修次数；可在下方核验表逐条查看源状态流水。</p>
      <p>雷达四轴：交付量达成、已交付需求无关联缺陷率、按期交付率和最近一次 Sonar 新代码覆盖率。按期率的分母为按期交付、超期后交付和仍逾期的需求；Sonar 轴取本月最新归属扫描的新代码覆盖率，成员卡片同时显示这次扫描的新代码指标和门禁结论。</p>
      <p>历史月份由当前快照推算，无法还原需求排期、负责人修改或需求重开；截点后变更的非发布状态与缺少状态时间的到期需求列为不明。分组冲突或未知时列为待归属；可开启“包含无活动成员”查看本月无记录的已同步参与人。</p>
      {metrics && <p>有效Bug {metrics.newValidBugs} 条，其中关联本月交付需求 {metrics.linkedValidBugs} 条；未记录排期 {metrics.requirementsMissingDue} 项，缺少创建/交付时间 {metrics.requirementsMissingTime} 项。</p>}
    </div>}><Button type="text" icon={<QuestionCircleOutlined />}>{t("engineering.metricDefinitions")}</Button></Popover></div>
        <div className="yx-member-filters">
          <Select aria-label={t("engineering.radarGroup")} style={{ width:170 }} value={activeGroup} onChange={v => { setMemberPage(1);setPageSize(v ? 12 : 6);updateQuery({ group:v,activityGroup:'',chart:v ? 'group' : 'members',compare:'' }) }} options={[{ value:'',label:t("engineering.allGroups") },...groupOptions.map(value => ({ value,label:value }))]} />
          <Input.Search aria-label={t("engineering.filterRadarMembers")} placeholder={t("engineering.searchMembers")} allowClear style={{ width:210 }} onChange={e => { setSearch(e.target.value);setMemberPage(1) }} />
          <Select aria-label={t("engineering.sortMembers")} style={{ width:200 }} value={`${sort}:${order}`} onChange={v => { const [field,direction] = v.split(':');changeSort(field,direction) }} options={['requirementParticipation', 'defectOwnership', 'general'].map(scope => ({label:t('engineering.' + scope),options:sortFields.filter(field => field.scope === scope).flatMap(field => ['desc','asc'].map(direction => ({value:`${field.key}:${direction}`,label:`${t('engineering.' + field.label)} ${direction === 'asc' ? t("engineering.ascending") : t("engineering.descending")}`})))}))} />
          <label className="yx-member-inactive"><Switch size="small" aria-label={t("engineering.includeInactiveMembers")} checked={includeInactive} onChange={v => { setIncludeInactive(v);setMemberPage(1) }} />{t("engineering.includeInactiveMembers")}</label>
          <Typography.Text type="secondary">{loading ? t("engineering.updating") : t("engineering.v0ShownV1Synced", undefined, { v0: rows.length, v1: metrics?.members.length || 0 })}</Typography.Text>
        </div>
    <Button style={{ marginBottom:12 }} onClick={() => setShowDetails(!showDetails)}>{showDetails ? t("engineering.backToMetrics") : t("engineering.viewRequirementAndDefectDetails")}</Button>
    {invalidMonth && <Alert type="info" showIcon closable onClose={() => setInvalidMonth(false)} message="月份参数不可用，已显示当前月份，请按需重新选择。" style={{ marginBottom:12 }} />}
    {error != null && <ReadError error={error} title={t("engineering.couldNotLoadIndividualMetrics")} />}
    <Spin spinning={loading}>
      <div className="yx-member-results">
      {metrics && <>
        <div className="yx-member-scope"><Tag>{metrics.month}</Tag><span>上海时间 · 截止 {metrics.endExclusive.replace('T',' ').slice(0,16)}（不含）</span><span>当前快照推算，含Bug交付需求占比供复盘参考</span></div>
        <Card size="small" title={activeGroup ? t("engineering.v0RequirementDeliveryRadar", undefined, { v0: activeGroup }) : t("engineering.individualRequirementDeliveryRadar")} className="yx-members-charts" extra={<Segmented aria-label={t("engineering.radarView")} value={chartView} onChange={value => updateQuery({ chart:String(value) })} options={[{ value:'members',label:t("engineering.individualRadar") },{ value:'group',label:t("engineering.groupComparison"),disabled:!activeGroup }]} />}>
          <div className="yx-member-chart-caption">{activeGroup ? `${activeGroup} · 匹配 ${rows.length} 位成员 · ${month}` : t("engineering.allMembers")} · 统一 0–100，越向外越好</div>
          {chartView === 'group' ? <section aria-label={t("engineering.teamMemberComparison")}>
            <div className="yx-group-radar-toolbar"><span>{t("engineering.compareMembers")}</span><Select aria-label={t("engineering.selectMembersToCompare")} mode="multiple" maxTagCount="responsive" popupMatchSelectWidth={false} classNames={{ popup: { root: 'yx-group-radar-options' } }} style={{ flex:1,minWidth:180 }} value={compared.map(p => p.name)} options={rows.map(p => ({ value:p.name,label:p.name }))} onChange={names => updateQuery({ compare:JSON.stringify(names) })} placeholder={t("engineering.selectGroupMembers")} /><Typography.Text type="secondary">已选 {compared.length} / {rows.length} 人</Typography.Text></div>
            {!compared.length ? <Empty description={t("engineering.selectGroupMembersToCompare")} /> : <div className="yx-group-radar-layout">
              <GroupRadar group={activeGroup} members={compared} />
              <div className="yx-group-radar-legend">{compared.map((p,index) => <article key={p.name} className="yx-group-radar-person"><span className="yx-group-radar-swatch" style={{ background:radarColor(index) }} /><div><MemberName name={p.name} onClick={() => setSelected(p)} /><span>已交付 {p.deliveredRequirements} 项 · 按期率 {percent(onTimeRate(p))}</span><MemberBugMetrics member={p} /><MemberSonarRates member={p} />{radarValues(p).some(value => value == null) && <small>{t("engineering.onlyAvailableMetricsArePlotted")}</small>}</div></article>)}</div>
            </div>}
          </section> : <>
          {!pageRows.length ? <Empty description={t("engineering.noMatchingMembersAdjustTheFilters")} /> : <div className="yx-member-radars">{pageRows.map(p => <article className="yx-member-radar-card" key={p.name} aria-label={`${p.name}指标卡`}>
            <div className="yx-member-card-heading"><MemberName name={p.name} onClick={() => setSelected(p)} /><Tooltip title={p.group}><span className="yx-member-card-group">{p.group}</span></Tooltip></div>
            <MemberRadar member={p} />
            <dl className="yx-member-card-metrics"><div><dt>{t("engineering.deliveredRequirements")}</dt><dd>{p.deliveredRequirements.toLocaleString('zh-CN')}<small> 项</small></dd></div><div><dt>{t("engineering.deliveredRequirementsWithLinkedDefects")}</dt><dd>{percent(p.bugRate)}</dd></div><div><dt>{t("engineering.overdueRate")}</dt><dd>{percent(p.delayRate)}</dd></div></dl>
            <MemberSonarSummary member={p} onDetails={() => setSelected(p)} />
          </article>)}</div>}
          </>}
        </Card>
        {pagination(t("engineering.radarChartPagination"))}
        {!showDetails ? <MemberManagementOverview members={rows} select={setSelected} /> : <>
        <Card size="small" title={t("engineering.requirementDeliveryByParticipant")} className="yx-members-table yx-member-delivery-table">
          <Typography.Paragraph type="secondary">交付需求按参与人统计；关联Bug数为这些交付需求在本月新增的有效关联Bug去重数。含Bug交付需求占比＝其中有Bug的需求数 / 交付需求数。</Typography.Paragraph>
          <Table<MemberMetric> size="small" tableLayout="fixed" rowKey="name" dataSource={pageRows} scroll={{ x:1140 }} pagination={false} onChange={(_,__,sorter,extra) => { if (extra.action !== 'sort') return; const selected = Array.isArray(sorter) ? sorter[0] : sorter; if (selected?.order) changeSort(String(selected.columnKey),selected.order === 'ascend' ? 'asc' : 'desc') }} columns={[
            { title:t("engineering.member"),dataIndex:'name',fixed:'left',width:140,ellipsis:true,render:(name:string,p) => <MemberName name={name} onClick={() => setSelected(p)} /> },
            { title:t("engineering.group"),dataIndex:'group',width:150,ellipsis:true },
            { title:t("engineering.requirementsDelivered"),key:'delivered',sorter:true,sortDirections:['descend','ascend','descend'],sortOrder:sort === 'delivered' ? (order === 'asc' ? 'ascend' : 'descend') : null,dataIndex:'deliveredRequirements',width:115,align:'right',render:(v:number,p) => <div className="yx-member-number"><strong>{v.toLocaleString('zh-CN')}</strong><small>{t("engineering.added")} {p.newRequirements.toLocaleString('zh-CN')}</small></div> },
            { title:t("engineering.linkedDefects"),key:'bugs',sorter:true,sortDirections:['descend','ascend','descend'],sortOrder:sort === 'bugs' ? (order === 'asc' ? 'ascend' : 'descend') : null,dataIndex:'linkedBugs',width:105,align:'right' },
            { title:t("engineering.deliveredRequirementsWithDefects"),dataIndex:'bugAffectedRequirements',width:140,align:'right' },
            { title:'含Bug交付需求占比 ↓',key:'bugRate',sorter:true,sortDirections:['descend','ascend','descend'],sortOrder:sort === 'bugRate' ? (order === 'asc' ? 'ascend' : 'descend') : null,width:210,align:'right',render:(_:unknown,p) => <RateValue label={t("engineering.deliveredRequirementsWithDefects")} value={p.bugRate} numerator={p.bugAffectedRequirements} denominator={p.deliveredRequirements} /> },
            { title:'延期率 ↓',width:170,align:'right',render:(_:unknown,p) => <RateValue label={t("engineering.overdueRate")} value={p.delayRate} numerator={p.overdueAtEnd} denominator={p.dueRequirements-p.unknownDelivery} /> },
            { title:t("engineering.unknownStatus"),dataIndex:'unknownDelivery',width:110,align:'right' },
          ]} />
        </Card>
        {metrics.comparisons && <MonthlyComparison metrics={metrics} members={pageRows} />}
        <Card size="small" title={t("engineering.defectQualityBySourceOwner")} className="yx-member-quality-table">
          <Typography.Paragraph type="secondary">责任处理Bug为本月进入已解决、已验证、已发布、已关闭等完成态或发生重开的责任Bug，包含遗留缺陷，按Bug去重。重开Bug占比＝重开Bug数 / 责任处理Bug数；状态重开次数按事件累加。</Typography.Paragraph>
          <Table<MemberMetric> size="small" tableLayout="fixed" rowKey="name" dataSource={pageRows} scroll={{x:1050}} pagination={false} onChange={(_,__,sorter,extra) => { if (extra.action !== 'sort') return; const selected = Array.isArray(sorter) ? sorter[0] : sorter; if (selected?.order) changeSort(String(selected.columnKey),selected.order === 'ascend' ? 'asc' : 'desc') }} columns={[
            { title:t("engineering.member"),dataIndex:'name',fixed:'left',width:140,ellipsis:true,render:(name:string,p) => <MemberName name={name} onClick={() => setSelected(p)} /> },
            { title:t("engineering.group"),dataIndex:'group',width:150,ellipsis:true },
            { title:t("engineering.assignedDefectsHandled"),key:'processedBugs',sorter:true,sortDirections:['descend','ascend','descend'],sortOrder:sort === 'processedBugs' ? (order === 'asc' ? 'ascend' : 'descend') : null,width:160,align:'right',render:(_:unknown,p) => <div className="yx-member-number"><strong>{unknownCount(p)>0 ? '≥' : ''}{p.processedBugs ?? 0} 个</strong><small>按Bug去重</small></div> },
            { title:t("engineering.reopenedDefects"),key:'reopenedBugs',sorter:true,sortDirections:['descend','ascend','descend'],sortOrder:sort === 'reopenedBugs' ? (order === 'asc' ? 'ascend' : 'descend') : null,width:140,align:'right',render:(_:unknown,p) => <div className="yx-member-number"><strong>{unknownCount(p)>0 ? '≥' : ''}{p.reopenedBugs ?? 0} 个</strong><small>本月发生过重开</small></div> },
            { title:t("engineering.reopenEvents"),key:'reopenCount',sorter:true,sortDirections:['descend','ascend','descend'],sortOrder:sort === 'reopenCount' ? (order === 'asc' ? 'ascend' : 'descend') : null,width:125,align:'right',render:(_:unknown,p) => <ReopenValue member={p} /> },
            { title:'重开Bug占比 ↓',key:'reopenRate',sorter:true,sortDirections:['descend','ascend','descend'],sortOrder:sort === 'reopenRate' ? (order === 'asc' ? 'ascend' : 'descend') : null,width:170,align:'right',render:(_:unknown,p) => <ReopenValue member={p} rate /> },
            { title:t("engineering.historyUnverified"),dataIndex:'reopenUnknownBugs',width:120,align:'right',render:(v:number) => v ?? 0 },
          ]}/>
        </Card>
        {pagination(t("engineering.memberPagination"))}
        <BugQuality data={metrics.bugQuality} group={activeGroup} members={metrics.members}/>
        </>}
      </>}
      {!metrics && !loading && !error && <Empty description={t("engineering.selectAReportingMonth")} />}
      </div>
    </Spin>
    <Drawer open={!!selected} title={<Tooltip title={selected?.name}><span className="yx-member-drawer-title">{selected?.name || ''} · 月度需求明细</span></Tooltip>} width="min(1100px, 95vw)" onClose={() => setSelected(null)}>
      <Typography.Title level={5}>{t("engineering.requirementDetails")}</Typography.Title>
      <Table<MemberRequirementMetric> size="small" tableLayout="fixed" rowKey="externalId" dataSource={selected?.requirements || []} scroll={{ x:860 }} pagination={{ pageSize:15 }} columns={[
        { title:t("engineering.requirements"),width:330,ellipsis:true,render:(_:unknown,r) => <MemberName name={`${r.serial || r.externalId} ${r.title}`} onClick={() => navigate(`/devops?${new URLSearchParams({ source:sourceId,reference:r.externalId })}`)} /> },
        { title:t("engineering.currentStatus"),dataIndex:'status',width:100,ellipsis:true },{ title:t("engineering.dueDate"),dataIndex:'due',width:110,render:(v:string) => v || t("engineering.notRecorded") },{ title:t("engineering.deliveredThisMonth"),dataIndex:'delivered',width:90,render:(v:boolean) => v ? t("engineering.yes") : t("engineering.no") },
        { title:t("engineering.deliveryCategory"),dataIndex:'deliveryClass',width:120,render:(v:string,r) => r.currentDelivered ? '已交付，不计延期' : classes[v] || v },{ title:t("engineering.defectsLinkedThisMonth"),dataIndex:'bugIds',width:110,align:'right',render:(v:string[]) => v.length },
      ]} />
      {selected && <MemberSonar member={selected} />}
      <Typography.Title level={5}>{t("engineering.assignedDefectsHandled")}</Typography.Title>
      <Typography.Paragraph type="secondary">按源“缺陷责任人”归属，仅列本月进入完成态或重开的处理Bug，以及历史待核验记录。此处对应责任处理Bug分母，点击缺陷编号核对状态流水。</Typography.Paragraph>
      <BugRows rows={(selected?.bugs || []).filter(b => b.processed || !b.historyAvailable)}/>
    </Drawer>
  </section>
}
