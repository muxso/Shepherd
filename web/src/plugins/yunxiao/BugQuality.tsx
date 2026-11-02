import { useState } from 'react'
import { Alert, Button, Card, Drawer, Input, Space, Table, Tag, Tooltip, Typography } from 'antd'
import type { BugQualityData, MemberBugMetric, MemberMetric } from './api'
const time=(value:string) => value && Number.isFinite(Date.parse(value)) ? new Intl.DateTimeFormat('zh-CN',{timeZone:'Asia/Shanghai',year:'numeric',month:'2-digit',day:'2-digit',hour:'2-digit',minute:'2-digit',hour12:false}).format(new Date(value)) : '—'
const names=(value:string[]) => value?.length ? value.join('、') : '未记录'
export function BugRows({ rows }: { rows:MemberBugMetric[] }) {
  const [selected,setSelected]=useState<MemberBugMetric | null>(null)
  return <>
    <Table<MemberBugMetric> aria-label="缺陷状态重开核验表" size="small" tableLayout="fixed" rowKey="externalId" dataSource={rows} scroll={{x:1220}} pagination={{pageSize:8,hideOnSinglePage:true,showSizeChanger:false,showTotal:n=>`共 ${n} 个缺陷`}} columns={[
      {title:'缺陷ID',width:140,fixed:'left',ellipsis:true,render:(_:unknown,b)=><Tooltip title={b.title}><Button type="link" className="yx-member-name" size="small" onClick={()=>setSelected(b)}>{b.serial || b.externalId}</Button></Tooltip>},
      {title:'缺陷类型',dataIndex:'bugType',width:110,ellipsis:true,render:(v:string)=>v || '未记录'},
      {title:'状态重开次数',width:130,align:'right',defaultSortOrder:'descend',sorter:(a,b,order)=>a.reopenCount==null || b.reopenCount==null ? ((a.reopenCount==null?1:0)-(b.reopenCount==null?1:0))*(order==='descend'?-1:1) : a.reopenCount-b.reopenCount,render:(_:unknown,b)=>b.reopenCount==null?'待同步':b.reopenCount},
      {title:'源缺陷责任人',width:140,ellipsis:true,render:(_:unknown,b)=><Tooltip title={names(b.responsibles)}>{names(b.responsibles)}</Tooltip>},
      {title:'源开发者',width:150,ellipsis:true,render:(_:unknown,b)=><Tooltip title={names(b.developers)}>{names(b.developers)}</Tooltip>},
      {title:'源测试人员 / 发现人',width:180,ellipsis:true,render:(_:unknown,b)=><Tooltip title={`测试人员：${names(b.testers)}；发现人：${b.reporter || '未记录'}`}><div className="yx-member-number"><span>{names(b.testers)}</span><small>发现人：{b.reporter || '未记录'}</small></div></Tooltip>},
      {title:'最近重开 / 流转',width:180,render:(_:unknown,b)=>time(b.reopenTimes.at(-1) || b.history?.at(-1)?.at || b.createdAt)},
      {title:'当前状态',dataIndex:'status',width:110,ellipsis:true},
    ]}/>
    <Drawer open={!!selected} width="min(950px,95vw)" onClose={()=>setSelected(null)} title={`${selected?.serial || selected?.externalId || ''} · 云效状态流水`}>
      {selected && <>
        <Typography.Paragraph>{selected.title}</Typography.Paragraph>
        <Space wrap><Tag>创建：{time(selected.createdAt)}</Tag><Tag>责任人：{names(selected.responsibles)}</Tag><Tag>开发者：{names(selected.developers)}</Tag><Tag>测试人员：{names(selected.testers)}</Tag></Space>
        <Alert style={{marginBlock:16}} type="info" showIcon message={`所选月状态重开 ${selected.reopenCount ?? '待同步'} 次；时间为上海时区。仅依据云效状态流转，不推算沟通返修次数。`} />
        <Table size="small" rowKey="id" dataSource={selected.history || []} pagination={{pageSize:12,hideOnSinglePage:true}} scroll={{x:600}} columns={[
          {title:'变更时间',dataIndex:'at',render:time,width:220},
          {title:'原状态',dataIndex:'from',width:150},
          {title:'新状态',dataIndex:'to',width:150},
          {title:'所选月重开',render:(_:unknown,e)=>selected.reopenTimes.some(at=>Date.parse(at)===Date.parse(e.at))?<Tag color="orange">重开事件</Tag>:'—'},
        ]}/>
      </>}
    </Drawer>
  </>
}
export default function BugQuality({ data,group,members }: { data?:BugQualityData; group:string; members:MemberMetric[] }) {
  const [search,setSearch]=useState('')
  if (!data) return null
  const groupNames=new Set(members.filter(p=>p.group===group).map(p=>p.name))
  const scope=data.rows.filter(b=>!group || b.responsibles.some(name=>groupNames.has(name)))
  const rows=scope.filter(b=>[b.serial,b.title,...b.responsibles,...b.developers,...b.testers,b.reporter].join(' ').toLocaleLowerCase().includes(search.toLocaleLowerCase()))
  const processed=scope.filter(b=>b.processed).length,reopened=scope.filter(b=>(b.reopenCount??0)>0).length,events=scope.reduce((n,b)=>n+(b.reopenCount??0),0),unknown=scope.some(b=>!b.historyAvailable)
  return <Card id="yx-bug-quality" size="small" title="缺陷重开核验" className="yx-members-table" style={{marginTop:16}}>
    <Typography.Paragraph type="secondary">按缺陷本身的状态时间统计，包含历史遗留缺陷，不要求需求当月交付。归属仅取源“缺陷责任人”；源开发者、测试人员和发现人分别展示。沟通返修次数与状态重开次数不同。</Typography.Paragraph>
    <Space wrap style={{marginBottom:12}}><Tag>{group || '全部分组'}</Tag><Tag>本月完成处理 / 重开 {processed} 个Bug</Tag><Tag>重开 {reopened} 个Bug · {unknown?'≥':''}{events} 次</Tag><Tag>状态重开率 {unknown || !processed?'待评价':`${(reopened/processed*100).toFixed(1)}%`}</Tag></Space>
    {!!data.unknownSourceBugs && <Alert type="warning" showIcon message={`有 ${data.unknownSourceBugs} 个源缺陷历史未核验，次数为已知下限，相关比例待评价。`} style={{marginBottom:12}}/>}
    <Input.Search aria-label="搜索缺陷重开流水" placeholder="缺陷编号、责任人、开发者或测试人员" allowClear value={search} onChange={e=>setSearch(e.target.value)} style={{maxWidth:420,marginBottom:12}}/>
    <BugRows rows={rows}/>
    <Typography.Paragraph type="secondary" style={{marginTop:12,marginBottom:0}}>比例＝本月重开过的Bug去重数 / 本月进入已解决、已验证、已发布、已关闭等完成态或发生重开的Bug去重数。点击缺陷ID核对逐条状态流水；无数据或历史不完整时待评价。</Typography.Paragraph>
  </Card>
}
