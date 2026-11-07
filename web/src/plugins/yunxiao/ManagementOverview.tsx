import { useState } from 'react'
import { Card, Empty, Segmented, Tag } from 'antd'
import { useNavigate } from 'react-router-dom'
import type { DevopsDashboard } from './api'
import { ManagementBars, ManagementTrend } from './ManagementCharts'
import { dateKey } from './codeActivity'
import './management.css'
export default function ManagementOverview({ data, sourceId, days }: { data: DevopsDashboard | null; sourceId: string; days: number }) {
  const [statusScope,setStatusScope] = useState('active')
  const navigate = useNavigate(), metrics = data?.productivity?.metrics
  const records = (kind: string, extra: Record<string, string> = {}) => navigate(`/devops?${new URLSearchParams({ view: 'records', source: sourceId, kind, days: String(days), ...extra })}`)
  const metricRoute = (metric: string, date: string) => records(metric === 'bugs' ? 'bug' : 'requirement', { metric, date })
  const groups = new Map<string, number>(); for (const author of data?.authors || []) groups.set(author.group || '待归属', (groups.get(author.group || '待归属') || 0) + author.commits)
  const status = (kind: string) => (data?.statuses || []).filter(s => s.kind === kind && (statusScope === 'all' || !['已发布','已交付','已完成','已关闭','已取消','已拒绝','已验收','完成'].includes(s.status))).sort((a, b) => b.count - a.count).map(s => ({ name: s.status || '未记录', value: s.count }))
  const capture = data?.source?.last_success_at, cutoff = capture && Number.isFinite(Date.parse(capture)) ? dateKey(capture) : undefined
  const series = (keys: string[], colors: string[]) => keys.map((key, i) => ({ key, label: ({ requirements: '新增需求', delivered: '已发布需求', bugs: '新增缺陷' } as Record<string, string>)[key], color: colors[i], points: (metrics?.[key]?.trend || []).filter(p => !cutoff || p.date <= cutoff) }))
  if (!data) return <Empty description="正在读取研发管理概览" />
  return <section aria-label="研发管理概览">
    <div className="yx-management-scope"><span>{data.productivity ? `${data.productivity.from} — ${data.productivity.through} · 上海` : `最近 ${days} 天`}</span><span>先看交付变化，再定位当前风险</span></div>
    <div className="yx-management-summary">{[
      { label: '本期已发布', value: metrics?.delivered?.current ?? data.totals.delivered, note: '按已发布状态时间', action: () => records('requirement', { metric: 'delivered', period: 'true' }) },
      { label: '本期新增需求', value: metrics?.requirements?.current, note: '按真实创建时间', action: () => records('requirement', { metric: 'requirements', period: 'true' }) },
      { label: '本期新增缺陷', value: metrics?.bugs?.current, note: '进入缺陷视图继续筛查', action: () => navigate(`/devops?${new URLSearchParams({ view: 'bugs', source: sourceId, days:String(days) })}`) },
      { label: '当前延期', value: data.totals.delayed, note: '紧急 / 高 / 中，排除待发布', action: () => records('requirement', { delayedOnly: 'true' }) },
    ].map(item => <button type="button" key={item.label} onClick={item.action}><span>{item.label}</span><strong>{item.value?.toLocaleString() ?? '待采集'}</strong><small>{item.note}</small></button>)}</div>
    <div className="yx-management-grid-layout">
      <Card size="small" title="需求交付节奏" extra={<Tag>上海自然日</Tag>}><ManagementTrend title="新增需求与已发布需求趋势" series={series(['requirements', 'delivered'], ['var(--brand)', '#65a89c'])} partialDate={cutoff} onSelect={metricRoute} /></Card>
      <Card size="small" title={statusScope === 'active' ? '当前在途需求' : '全部需求分布'} extra={<Segmented size="small" value={statusScope} onChange={v=>setStatusScope(String(v))} options={[{value:'active',label:'在途'},{value:'all',label:'全部'}]} />}><ManagementBars items={status('requirement')} unit=" 项" onSelect={value => records('requirement', { status: value })} limit={7} /></Card>
      <Card size="small" title="缺陷流入趋势"><ManagementTrend title="新增缺陷趋势" series={series(['bugs'], ['#d69660'])} partialDate={cutoff} onSelect={metricRoute} /></Card>
      <Card size="small" title="代码活动 · 按分组" extra={<Tag>滚动 {days} 天</Tag>}><ManagementBars items={[...groups].map(([name, value]) => ({ name, value })).sort((a, b) => b.value - a.value)} unit=" 次" onSelect={group => records('commit', { group })} limit={7} /></Card>
    </div>
    <p className="yx-management-notes">新增与发布用于观察交付节奏，不是同一批需求的转化率。当前延期和状态分布来自当前快照；末日按同步截点展示，尚未采集的日期不补零；图表可点击进入对应明细。代码次数表示已采集活动，不能折算工时。</p>
  </section>
}
