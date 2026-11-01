import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { Alert, Button, Card, Empty, Segmented, Select, Table, Popover, Tag, Space } from 'antd'
import { ReloadOutlined } from '@ant-design/icons'
import StatisticsNotes from './StatisticsNotes'
import DayRecords, { type DayCache } from './DayRecords'
import { devopsApi, type DevopsDashboard, type DevopsSource, type OutputMetric } from './api'
import './productivity.css'

type Role = 'product' | 'testing' | 'development'
const roles: Record<Role, { label: string; metrics: [string, string][] }> = {
  product: { label: '产品', metrics: [['requirements', '新增需求'], ['delivered', '已交付需求']] },
  testing: { label: '测试', metrics: [['testcases', '新增用例'], ['bugs', '发现缺陷']] },
  development: { label: '研发', metrics: [['linkedCommits', '需求关联提交'], ['commits', '组织实际提交'], ['additions', '关联提交新增行'], ['deletions', '关联提交删除行']] },
}
interface ProductivityProps { projectId: string }
const number = (v: number) => v.toLocaleString('zh-CN')
function Comparison({ metric }: { metric: OutputMetric }) {
  const diff = metric.current - metric.previous
  return <span className="yx-comparison">{metric.previous === 0
    ? metric.current ? `较上期新增 ${number(metric.current)}` : '与上期持平'
    : `${diff > 0 ? '+' : ''}${((diff / metric.previous) * 100).toFixed(1)}% 较上期`}</span>
}

// The host only supplies an opaque project reference. Facts stay in the Yunxiao plugin.
export default function Productivity({ projectId }: ProductivityProps) {
  const [sources, setSources] = useState<DevopsSource[]>([])
  const [source, setSource] = useState('')
  const [days, setDays] = useState(30)
  const [data, setData] = useState<DevopsDashboard | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [revision, setRevision] = useState(0)
  useEffect(() => {
    let live = true
    setError(''); setLoading(true)
    devopsApi.sources().then(result => {
      if (!live) return
      const matching = result.items.filter(item => item.project_id === projectId)
      setSources(matching)
      setSource(current => matching.some(item => item.id === current) ? current : matching[0]?.id || '')
      if (!matching.length) setLoading(false)
    }).catch(e => { if (live) { setError(String(e.message || e)); setLoading(false) } })
    return () => { live = false }
  }, [projectId, revision])
  useEffect(() => {
    if (!source) return
    let live = true
    setLoading(true); setData(null); setError('')
    devopsApi.dashboard(source, days).then(result => { if (live) setData(result) })
      .catch(e => { if (live) setError(String(e.message || e)) })
      .finally(() => { if (live) setLoading(false) })
    return () => { live = false }
  }, [source, days, revision])
  if (!loading && !error && !sources.length) return null
  if (error) return <Alert type="error" showIcon message="岗位产出读取失败" description={error} />
  if (!data?.productivity) return null
  const info = sources.find(item => item.id === source)!
  return <ProductivityContent key={source} data={data} info={info} controls={<Space size={8} wrap>
    {sources.length > 1 && <Select aria-label="产出数据源" value={source} onChange={setSource} options={sources.map(item => ({ value: item.id, label: item.name }))} />}
    <Select aria-label="产出统计区间" value={days} onChange={setDays} options={[7,30,90,365].map(value => ({ value, label: `最近 ${value} 天` }))} />
    <Button aria-label="刷新产出统计" icon={<ReloadOutlined />} onClick={() => setRevision(value => value + 1)} />
  </Space>} />
}

export function ProductivityContent({ data, info, controls, embedded = false }: { data: DevopsDashboard; info: DevopsSource; controls?: ReactNode; embedded?: boolean }) {
  const [role, setRole] = useState<Role>('product')
  const [metricKey, setMetricKey] = useState('requirements')
  const [activeDay, setActiveDay] = useState<string | null>(null)
  const cache = useMemo<DayCache>(() => new Map(), [data, info.id])
  const productivity = data.productivity
  const config = roles[role]
  const metric = productivity?.metrics[metricKey]
  const max = Math.max(1, ...(metric?.trend.map(point => point.value) || []))
  const total = metric?.contributors.reduce((sum, item) => sum + item.count, 0) || 0
  const delivered = productivity?.metrics.delivered?.current || 0
  const requirements = data.totals.requirements || 0
  if (!productivity) return null
  return <section className="yx-output" aria-label="岗位产出统计">
    <Card size="small" title={<Space size={8}><span role="heading" aria-level={3}>岗位产出</span>{!embedded && <Tag>云效插件</Tag>}</Space>}
      extra={<Space size={8} wrap>
        <Segmented aria-label="产出岗位" value={role} options={Object.entries(roles).map(([value,item]) => ({ value, label:item.label }))}
          onChange={value => { const next = value as Role; setRole(next); setMetricKey(roles[next].metrics[0][0]); setActiveDay(null) }} />
        {controls}
        {!embedded && <StatisticsNotes info={info} data={data} />}
      </Space>}>
        <div className="yx-output-kpis">
          {config.metrics.map(([key, label]) => { const item = productivity.metrics[key]; return <button type="button" key={key}
            className={`yx-output-kpi ${key === metricKey ? 'selected' : ''}`} aria-pressed={key === metricKey} onClick={() => setMetricKey(key)}>
            <span>{label}</span><strong>{item ? number(item.current) : '—'}</strong>{item && <Comparison metric={item} />}
          </button> })}
        </div>
        <div className="yx-output-charts">
          <Card className="yx-output-trend" size="small" title={`${config.metrics.find(([key]) => key === metricKey)?.[1]}趋势`} extra={<span className="yx-muted">每日 · 上海时间</span>}>
            <div className="yx-block-chart" role="img" aria-label={`每日产出趋势，总计 ${metric?.current || 0}`}>
              {metric?.trend.map(point => { const count = Math.ceil(point.value / max * 12); return <Popover key={point.date} trigger={['hover','focus']} placement="top" mouseEnterDelay={0.2}
                open={activeDay === point.date} onOpenChange={open => setActiveDay(current => open ? point.date : current === point.date ? null : current)}
                title={`${point.date} · ${number(point.value)}`} content={point.value > 0 && activeDay === point.date
                  ? <DayRecords key={`${info.id}:${metricKey}:${point.date}`} source={info.id} metric={metricKey} date={point.date} cache={cache} />
                  : <span className="yx-muted">当日无记录</span>}>
                <div className="yx-block-column" tabIndex={0} aria-label={`${point.date}: ${point.value}`}>
                  {Array.from({ length: 12 }, (_, index) => <i key={index} className={index >= 12 - count ? 'filled' : ''} />)}
                </div>
              </Popover> })}
            </div>
            <div className="yx-output-axis"><span>{productivity.from}</span><span>悬浮查看当日记录</span><span>{productivity.through}</span></div>
          </Card>
          <Card size="small" title={metricKey === 'delivered' ? '交付归属（当前负责人）' : '贡献分布'}>
            {metric?.contributors.length ? <div className="yx-contributors">{metric.contributors.slice(0, 6).map(person => <div key={person.name}>
              <div className="yx-person"><span>{person.name}</span><span>{number(person.count)} <small>{total ? Math.round(person.count / total * 100) : 0}%</small></span></div>
              <div className="yx-person-track"><i style={{ width: `${total ? person.count / total * 100 : 0}%` }} /></div>
            </div>)}</div> : <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="本期暂无记录" />}
          </Card>
        </div>
        {role === 'product' && <div className="yx-delivery">
          <div><strong>{number(delivered)}</strong> 本期交付 / {number(requirements)} 当前需求 <span className="yx-muted">· 当前存量的交付占比</span></div>
          <div className="yx-segments" role="meter" aria-label="本期交付占当前需求比例" aria-valuemin={0} aria-valuemax={100} aria-valuenow={requirements ? Math.min(100, delivered / requirements * 100) : 0}>
            {Array.from({ length: 50 }, (_, i) => <i key={i} className={requirements && i < delivered / requirements * 50 ? 'filled' : ''} />)}
          </div><div className="yx-output-axis"><span>0%</span><span>50%</span><span>100%</span></div>
          <div className="yx-muted">延期需求 {number(data.totals.delayed)}</div>
        </div>}
        <Table size="small" rowKey="name" pagination={{ pageSize: 5, hideOnSinglePage: true }} dataSource={metric?.contributors || []} columns={[
          { title: metricKey === 'delivered' ? '当前负责人' : '产出人', dataIndex: 'name' },
          { title: '本期数量', dataIndex: 'count', render: number },
          { title: '占比', render: (_: unknown, person: { count: number }) => `${total ? (person.count / total * 100).toFixed(1) : 0}%` },
        ]} />
        {!embedded && <div className="yx-output-status">{info.last_success_at ? `最近同步 ${new Date(info.last_success_at).toLocaleString('zh-CN')}` : '等待首次同步'} · {info.name}</div>}
    </Card>
  </section>
}
