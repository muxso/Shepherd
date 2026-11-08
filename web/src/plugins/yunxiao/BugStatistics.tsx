import { useEffect, useState } from 'react'
import { useLocation,useNavigate } from 'react-router-dom'
import { Alert, Card, DatePicker, Empty, Select, Spin, Tag } from 'antd'
import dayjs from 'dayjs'
import locale from 'antd/es/date-picker/locale/zh_CN'
import { devopsApi, type FilteredBugStatistics } from './api'
import { ManagementBars, ManagementTrend } from './ManagementCharts'
import './management.css'
const roleLabels = { responsible: '缺陷责任人', reporter: '实际发现人', assignee: '当前处理人' }
type Role = keyof typeof roleLabels
export default function BugStatistics({ sourceId, revision }: { sourceId: string; revision: number }) {
  const location=useLocation(),navigate=useNavigate(),query=new URLSearchParams(location.search)
  const initialRole=query.get('bugRole') as Role
  const [range, setRange] = useState<[string, string] | null>(()=>query.get('bugFrom') && query.get('bugThrough') ? [query.get('bugFrom')!,query.get('bugThrough')!] : null), [days, setDays] = useState(Math.max(1,Math.min(366,Number(query.get('days')) || 7))), [role, setRole] = useState<Role>(initialRole in roleLabels ? initialRole : 'responsible'), [person, setPerson] = useState(query.get('bugPerson') || ''), [group, setGroup] = useState(query.get('bugGroup') || '')
  const remember=(values:Record<string,string>)=>{const q=new URLSearchParams(location.search);q.set('view','bugs');q.set('source',sourceId);for(const [key,value] of Object.entries(values)){if(value)q.set(key,value);else q.delete(key)}navigate({pathname:location.pathname,search:q.toString()},{replace:true})}
  const choosePerson=(value:string)=>{setPerson(value);remember({bugPerson:value})}
  useEffect(() => {
    const q = new URLSearchParams(location.search), nextRole = q.get('bugRole') as Role
    const nextRange: [string, string] | null = q.get('bugFrom') && q.get('bugThrough') ? [q.get('bugFrom')!, q.get('bugThrough')!] : null
    setRange(value => JSON.stringify(value) === JSON.stringify(nextRange) ? value : nextRange)
    setDays(Math.max(1, Math.min(366, Number(q.get('days')) || 7)))
    setRole(nextRole in roleLabels ? nextRole : 'responsible'); setPerson(q.get('bugPerson') || ''); setGroup(q.get('bugGroup') || '')
  }, [location.search])

  const [result, setResult] = useState<{ key: string; data: FilteredBugStatistics } | null>(null), [error, setError] = useState(''), [loading, setLoading] = useState(false)
  const key = JSON.stringify([sourceId, range, days, role, person, group, revision]), data = result?.key === key ? result.data : undefined
  const options = result?.data
  useEffect(() => {
    let live = true; setLoading(true); setError('')
    const timer = window.setTimeout(() => {
      devopsApi.bugStatistics(sourceId, { ...(range ? { from: range[0], through: range[1] } : { days: String(days) }), personRole: role, ...(person ? { person } : {}), ...(group ? { group } : {}) }).then(data => { if (live) setResult({ key, data }) }).catch(e => { if (live) setError(String(e.message || e)) }).finally(() => { if (live) setLoading(false) })
    }, 120)
    return () => { live = false; window.clearTimeout(timer) }
  }, [key, sourceId, range, days, role, person, group])
  const count = (field: 'total' | 'unassigned' | 'linked' | 'unlinked') => data?.[field]?.toLocaleString() ?? '—'
  return <section aria-label="云效缺陷统计">
    <div className="yx-management-filters">
      <Select aria-label="缺陷时间范围" value={range ? 'custom' : String(days)} style={{ width: 150 }} onChange={value => { if (value !== 'custom') { setDays(Number(value)); setRange(null);remember({days:value,bugFrom:'',bugThrough:''}) } }} options={[7, 30, 90, 365].map(value => ({ value: String(value), label: `最近 ${value} 天` })).concat(range ? [{ value: 'custom', label: '自定义区间' }] : [])} />
      <DatePicker.RangePicker aria-label="缺陷创建时间" locale={locale} value={range ? [dayjs(range[0]), dayjs(range[1])] : null} disabledDate={d => d.isAfter(dayjs(), 'day')} onChange={value => { const dates:[string,string] | null=value?.[0] && value[1] ? [value[0].format('YYYY-MM-DD'),value[1].format('YYYY-MM-DD')] : null;setRange(dates);remember({bugFrom:dates?.[0] || '',bugThrough:dates?.[1] || ''}) }} />
      <Select aria-label="缺陷人员口径" value={role} onChange={value => { setRole(value); setPerson(''); setGroup('');remember({bugRole:value,bugPerson:'',bugGroup:''}) }} style={{ width: 150 }} options={Object.entries(roleLabels).map(([value, label]) => ({ value, label }))} />
      <Select aria-label="缺陷人员" placeholder="全部人员" allowClear showSearch value={person || undefined} onChange={value => choosePerson(value || '')} style={{ width: 160 }} options={[...new Set([...(options?.personOptions || []), ...(person ? [person] : [])])].map(value => ({ value, label: value }))} />
      <Select aria-label="缺陷分组" placeholder="全部分组" allowClear value={group || undefined} onChange={value => {setGroup(value || '');remember({bugGroup:value || ''})}} style={{ width: 170 }} options={[...new Set([...(options?.groups || []), ...(group ? [group] : [])])].map(value => ({ value, label: value }))} />
    </div>
    {error && <Alert type="error" showIcon title="缺陷统计读取失败" description={error} style={{ marginBottom: 16 }} />}
    <Spin spinning={loading}>
      <div className="yx-management-scope"><span>{data ? `${data.from} — ${data.through} · 按创建时间 · 上海` : '按缺陷创建时间统计'}</span><span>人员口径：{roleLabels[role]}</span></div>
      <div className="yx-management-summary">{[['所选缺陷', count('total'), '按缺陷去重'], ['责任待归属', count('unassigned'), '源缺陷责任人尚未登记'], ['已关联需求', count('linked'), '同数据源内明确关联'], ['未关联需求', count('unlinked'), '优先核对关联完整性']].map(([label, value, note]) => <article key={label}><span>{label}</span><strong>{value}</strong><small>{note}</small></article>)}</div>
      {data && <>
        <div className="yx-management-grid-layout">
          <Card size="small" title="缺陷流入趋势"><ManagementTrend title="所选范围新增缺陷趋势" series={[{ key: 'bugs', label: '新增缺陷', color: '#d69660', points: data.trend || [] }]} /></Card>
          <Card size="small" title="当前状态分布" extra={<Tag>当前快照</Tag>}><ManagementBars items={data.statuses.map(s => ({ name: s.status, value: s.count }))} unit=" 个" /></Card>
          <Card size="small" title={`${roleLabels[role]} · 所选缺陷分布`}><ManagementBars items={data.people.map(p => ({ name: p.name, value: p.count }))} unit=" 个" onSelect={choosePerson} /></Card>
          <Card size="small" title="优先级分布"><ManagementBars items={(data.priorities || []).map(p => ({ name: p.name, value: p.count }))} unit=" 个" /></Card>
        </div>
        {!data.total && <Empty description="筛选范围内暂无缺陷，可调整时间、人员或分组" />}
        <p className="yx-management-notes">按创建时间筛选后展示当前状态，不推算本期解决耗时。多人责任缺陷在总数中去重，各人员关联数不能相加为团队总数。分组取对应人员的配置或唯一已知分组。{data.missingTime > 0 && `另有 ${data.missingTime} 条源缺陷缺少创建时间，未进入时间统计。`}</p>
      </>}
    </Spin>
  </section>
}
