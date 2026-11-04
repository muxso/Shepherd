import { useState } from 'react'
import { Card, Select, Space, Table, Typography } from 'antd'
import type { ComparisonMember, ComparisonPeriod, MemberMetric, MemberMetricsData } from './api'
const fields = [
  { key:'deliveredRequirements',label:'交付需求量',rate:false },
  { key:'linkedBugs',label:'关联Bug数',rate:false },
  { key:'bugRate',label:'含Bug交付需求占比',rate:true },
  { key:'delayRate',label:'延期率',rate:true },
  { key:'reopenCount',label:'状态重开次数',rate:false },
  { key:'reopenRate',label:'重开Bug占比',rate:true },
] as const
export default function MonthlyComparison({ metrics,members }: { metrics:MemberMetricsData['metrics']; members:MemberMetric[] }) {
  const [key,setKey] = useState<string>('deliveredRequirements')
  const field = fields.find(f => f.key === key) || fields[0]
  const comparison = metrics.comparisons!
  const value = (m?:ComparisonMember) => !m || (field.key === 'reopenCount' && m.reopenUnknownBugs > 0) ? null : m[field.key]
  const format = (v:number | null) => v == null ? '待评价' : field.rate ? `${v.toFixed(1)}%` : v.toLocaleString('zh-CN')
  const prior = (period:ComparisonPeriod | null,name:string) => value(period?.members.find(p => p.name === name))
  const delta = (current:number | null,base:number | null) => {
    if (current == null || base == null) return '待评价'
    const d = field.rate ? current-base : base === 0 ? null : (current-base)/base*100
    return d == null ? '基期为0' : `${d > 0 ? '+' : ''}${d.toFixed(1)}${field.rate ? ' 个百分点' : '%'}`
  }
  const range = (p:ComparisonPeriod | null) => p ? `${p.start.slice(0,16).replace('T',' ')} 至 ${p.endExclusive.slice(0,16).replace('T',' ')}（不含）` : '无可用基期'
  return <Card className="yx-member-comparison" size="small" title="月度同比 / 环比" extra={<Select aria-label="同比环比指标" value={field.key} options={fields.map(f => ({ value:f.key,label:f.label }))} onChange={setKey} style={{ width:190 }} />}>
    <Typography.Paragraph type="secondary">{comparison.sameElapsedTime ? '本月进行中：按相同日、时刻对齐基期，较短月份截止月末。' : '历史完整月份：按整月对比。'}数量比较增减百分比，比率比较百分点；基期为0或缺少数据时不计算增幅。由同一当前快照推算。</Typography.Paragraph>
    <Space wrap style={{ marginBottom:12 }}><Typography.Text type="secondary">环比：{range(comparison.previousMonth)}</Typography.Text><Typography.Text type="secondary">同比：{range(comparison.previousYear)}</Typography.Text></Space>
    <Table<MemberMetric> size="small" rowKey="name" tableLayout="fixed" dataSource={members} scroll={{ x:1020 }} pagination={false} columns={[
      { title:'成员',dataIndex:'name',width:140,ellipsis:true,fixed:'left' },
      { title:`本期 ${metrics.month}`,width:145,align:'right',render:(_,p) => format(value(p)) },
      { title:`上月 ${comparison.previousMonth?.month || '—'}`,width:145,align:'right',render:(_,p) => format(prior(comparison.previousMonth,p.name)) },
      { title:field.rate ? '环比（百分点）' : '环比',width:150,align:'right',render:(_,p) => delta(value(p),prior(comparison.previousMonth,p.name)) },
      { title:`去年同期 ${comparison.previousYear?.month || '—'}`,width:170,align:'right',render:(_,p) => format(prior(comparison.previousYear,p.name)) },
      { title:field.rate ? '同比（百分点）' : '同比',width:150,align:'right',render:(_,p) => delta(value(p),prior(comparison.previousYear,p.name)) },
    ]} />
  </Card>
}
