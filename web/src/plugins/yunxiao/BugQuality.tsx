import { useI18n } from '../../i18n'
import { useState } from 'react'
import { Alert, Button, Card, Drawer, Input, Space, Table, Tag, Tooltip, Typography } from 'antd'
import type { BugQualityData, MemberBugMetric, MemberMetric } from './api'
const time=(value:string) => value && Number.isFinite(Date.parse(value)) ? new Intl.DateTimeFormat('zh-CN',{timeZone:'Asia/Shanghai',year:'numeric',month:'2-digit',day:'2-digit',hour:'2-digit',minute:'2-digit',hour12:false}).format(new Date(value)) : '—'
const names=(value:string[]) => value?.length ? value.join('、') : '未记录'
export function BugRows({ rows }: { rows:MemberBugMetric[] }) {
  const { t } = useI18n()
  const [selected,setSelected]=useState<MemberBugMetric | null>(null)
  return <>
    <Table<MemberBugMetric> aria-label={t("engineering.defectReopenAudit")} size="small" tableLayout="fixed" rowKey="externalId" dataSource={rows} scroll={{x:1220}} pagination={{pageSize:8,hideOnSinglePage:true,showSizeChanger:false,showTotal:n=>t("engineering.v0Defects", undefined, { v0: n })}} columns={[
      {title:t("engineering.defectId"),width:140,fixed:'left',ellipsis:true,render:(_:unknown,b)=><Tooltip title={b.title}><Button type="link" className="yx-member-name" size="small" onClick={()=>setSelected(b)}>{b.serial || b.externalId}</Button></Tooltip>},
      {title:t("engineering.defectType"),dataIndex:'bugType',width:110,ellipsis:true,render:(v:string)=>v || t("engineering.notRecorded")},
      {title:t("engineering.reopenEvents"),width:130,align:'right',defaultSortOrder:'descend',sorter:(a,b,order)=>a.reopenCount==null || b.reopenCount==null ? ((a.reopenCount==null?1:0)-(b.reopenCount==null?1:0))*(order==='descend'?-1:1) : a.reopenCount-b.reopenCount,render:(_:unknown,b)=>b.reopenCount==null?t("engineering.awaitingSync"):b.reopenCount},
      {title:t("engineering.defectOwnerInSource"),width:140,ellipsis:true,render:(_:unknown,b)=><Tooltip title={names(b.responsibles)}>{names(b.responsibles)}</Tooltip>},
      {title:t("engineering.developerInSource"),width:150,ellipsis:true,render:(_:unknown,b)=><Tooltip title={names(b.developers)}>{names(b.developers)}</Tooltip>},
      {title:t("engineering.testerReporterInSource"),width:180,ellipsis:true,render:(_:unknown,b)=><Tooltip title={t("engineering.testerV0ReporterV1", undefined, { v0: names(b.testers), v1: b.reporter || '未记录' })}><div className="yx-member-number"><span>{names(b.testers)}</span><small>发现人：{b.reporter || t("engineering.notRecorded")}</small></div></Tooltip>},
      {title:t("engineering.latestReopenTransition"),width:180,render:(_:unknown,b)=>time(b.reopenTimes.at(-1) || b.history?.at(-1)?.at || b.createdAt)},
      {title:t("engineering.currentStatus"),dataIndex:'status',width:110,ellipsis:true},
    ]}/>
    <Drawer open={!!selected} width="min(950px,95vw)" onClose={()=>setSelected(null)} title={t("engineering.v0YunxiaoStatusHistory", undefined, { v0: selected?.serial || selected?.externalId || '' })}>
      {selected && <>
        <Typography.Paragraph>{selected.title}</Typography.Paragraph>
        <Space wrap><Tag>创建：{time(selected.createdAt)}</Tag><Tag>责任人：{names(selected.responsibles)}</Tag><Tag>开发者：{names(selected.developers)}</Tag><Tag>测试人员：{names(selected.testers)}</Tag></Space>
        <Alert style={{marginBlock:16}} type="info" showIcon message={t("engineering.v0ReopenEventsInTheSelectedMonthTimesUseTheShanghaiTimeZoneCountsComeFromYunxiaoStatusTransitions", undefined, { v0: selected.reopenCount ?? '待同步' })} />
        <Table size="small" rowKey="id" dataSource={selected.history || []} pagination={{pageSize:12,hideOnSinglePage:true}} scroll={{x:600}} columns={[
          {title:t("engineering.changedAt"),dataIndex:'at',render:time,width:220},
          {title:t("engineering.previousStatus"),dataIndex:'from',width:150},
          {title:t("engineering.newStatus"),dataIndex:'to',width:150},
          {title:t("engineering.reopenedThisMonth"),render:(_:unknown,e)=>selected.reopenTimes.some(at=>Date.parse(at)===Date.parse(e.at))?<Tag color="orange">{t("engineering.reopenEvent")}</Tag>:'—'},
        ]}/>
      </>}
    </Drawer>
  </>
}
export default function BugQuality({ data,group,members }: { data?:BugQualityData; group:string; members:MemberMetric[] }) {
  const { t } = useI18n()
  const [search,setSearch]=useState('')
  if (!data) return null
  const groupNames=new Set(members.filter(p=>p.group===group).map(p=>p.name))
  const scope=data.rows.filter(b=>!group || b.responsibles.some(name=>groupNames.has(name)))
  const rows=scope.filter(b=>[b.serial,b.title,...b.responsibles,...b.developers,...b.testers,b.reporter].join(' ').toLocaleLowerCase().includes(search.toLocaleLowerCase()))
  const processed=scope.filter(b=>b.processed).length,reopened=scope.filter(b=>(b.reopenCount??0)>0).length,events=scope.reduce((n,b)=>n+(b.reopenCount??0),0),unknown=scope.some(b=>!b.historyAvailable)
  return <Card id="yx-bug-quality" size="small" title={t("engineering.assignedDefectsReopenAudit")} className="yx-members-table" style={{marginTop:16}}>
    <Typography.Paragraph type="secondary">{t("engineering.theAuditUsesDefectOwnershipAndStatusTransitionsInTheSourceAndIncludesPendingAcceptanceAndOlderDefectsTheRadarCountsDefectsLinkedToRequirementsDeliveredThisMonth")}</Typography.Paragraph>
    <Space wrap style={{marginBottom:12}}><Tag>{group || t("engineering.allGroups")}</Tag><Tag>本月完成处理 / 重开 {processed} 个Bug</Tag><Tag>重开 {reopened} 个Bug · {unknown?'≥':''}{events} 次</Tag><Tag>{t("engineering.reopenRate")} {unknown || !processed?t("engineering.notRated"):`${(reopened/processed*100).toFixed(1)}%`}</Tag></Space>
    {!!data.unknownSourceBugs && <Alert type="warning" showIcon message={t("engineering.historyIsUnverifiedForV0DefectsCountsAreLowerBoundsAndTheCorrespondingRatesAreUnavailable", undefined, { v0: data.unknownSourceBugs })} style={{marginBottom:12}}/>}
    <Input.Search aria-label={t("engineering.searchDefectReopenHistory")} placeholder={t("engineering.defectIdOwnerDeveloperOrTester")} allowClear value={search} onChange={e=>setSearch(e.target.value)} style={{maxWidth:420,marginBottom:12}}/>
    <BugRows rows={rows}/>
    <Typography.Paragraph type="secondary" style={{marginTop:12,marginBottom:0}}>{t("engineering.reopenRateIsTheShareOfDistinctDefectsThatReopenedAmongThoseThatReopenedOrReachedACompletedStatusThisMonthSelectADefectIdToReviewItsHistoryTheRateIsUnavailableIfTheDataOrHistoryIsIncomplete")}</Typography.Paragraph>
  </Card>
}
