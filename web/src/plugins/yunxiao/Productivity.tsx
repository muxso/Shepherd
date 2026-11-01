import { useEffect, useState, type CSSProperties } from 'react'
import { Alert, Button, Card, Empty, Segmented, Select, Spin, Table, Tooltip } from 'antd'
import { ReloadOutlined } from '@ant-design/icons'
import { usePlugins } from '..'
import { devopsApi, type DevopsDashboard, type DevopsSource, type OutputMetric } from './api'
import './productivity.css'

type Role = 'product' | 'testing' | 'development'
const roles: Record<Role, { label: string; color: string; metrics: [string, string][] }> = {
  product: { label: '产品', color: '#8b5cf6', metrics: [['requirements', '新增需求'], ['delivered', '已交付需求']] },
  testing: { label: '测试', color: '#f59e0b', metrics: [['testcases', '新增用例'], ['bugs', '发现缺陷']] },
  development: { label: '研发', color: '#1685ff', metrics: [['linkedCommits', '需求关联提交'], ['commits', '组织实际提交'], ['additions', '关联提交新增行'], ['deletions', '关联提交删除行']] },
}
interface ProductivityProps { projectId: string; sourceId?: string; days?: number; revision?: number }
const number = (v: number) => v.toLocaleString('zh-CN')
function Comparison({ metric }: { metric: OutputMetric }) {
  const diff = metric.current - metric.previous
  return <span className="yx-comparison">{metric.previous === 0
    ? metric.current ? `较上期新增 ${number(metric.current)}` : '与上期持平'
    : `${diff > 0 ? '+' : ''}${((diff / metric.previous) * 100).toFixed(1)}% 较上期`}</span>
}

// The host only supplies an opaque project reference. Facts stay in the Yunxiao plugin.
export default function Productivity(props: ProductivityProps) {
  const { enabled } = usePlugins()
  return enabled('yunxiao') ? <ProjectProductivity key={`${props.projectId}:${props.sourceId || ''}`} {...props} /> : null
}
function ProjectProductivity({ projectId, sourceId, days: selectedDays, revision: outerRevision = 0 }: ProductivityProps) {
  const [sources, setSources] = useState<DevopsSource[]>([])
  const [source, setSource] = useState('')
  const [localDays, setDays] = useState(30)
  const days = selectedDays ?? localDays
  const [role, setRole] = useState<Role>('product')
  const [metricKey, setMetricKey] = useState('requirements')
  const [data, setData] = useState<DevopsDashboard | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [revision, setRevision] = useState(0)
  useEffect(() => {
    let live = true
    setError(''); setLoading(true)
    devopsApi.sources().then(result => {
      if (!live) return
      const matching = result.items.filter(item => item.project_id === projectId && (!sourceId || item.id === sourceId))
      setSources(matching)
      setSource(current => matching.some(item => item.id === current) ? current : matching[0]?.id || '')
      if (!matching.length) setLoading(false)
    }).catch(e => { if (live) { setError(String(e.message || e)); setLoading(false) } })
    return () => { live = false }
  }, [projectId, sourceId, revision, outerRevision])
  useEffect(() => {
    if (!source) return
    let live = true
    setLoading(true); setData(null); setError('')
    devopsApi.dashboard(source, days).then(result => { if (live) setData(result) })
      .catch(e => { if (live) setError(String(e.message || e)) })
      .finally(() => { if (live) setLoading(false) })
    return () => { live = false }
  }, [source, days, revision, outerRevision])
  const config = roles[role]
  const metric = data?.productivity?.metrics[metricKey]
  const max = Math.max(1, ...(metric?.trend.map(point => point.value) || []))
  const total = metric?.contributors.reduce((sum, item) => sum + item.count, 0) || 0
  const info = sources.find(item => item.id === source)
  const missing = metric?.missingTime || 0
  const unknown = data?.productivity?.metrics.unknownStats?.current || 0
  const delivered = data?.productivity?.metrics.delivered?.current || 0
  const requirements = data?.totals.requirements || 0
  return <section className="yx-output" aria-label="岗位产出统计" style={{ '--yx-accent': config.color } as CSSProperties}>
    <div className="yx-output-heading">
      <div><h2>岗位产出</h2><p>云效需求与交付 · 当前项目</p></div>
      <div className="yx-output-controls">
        <Segmented aria-label="产出岗位" value={role} options={Object.entries(roles).map(([value, item]) => ({ value, label: item.label }))}
          onChange={value => { const next = value as Role; setRole(next); setMetricKey(roles[next].metrics[0][0]) }} />
        {sources.length > 1 && <Select aria-label="产出数据源" value={source} onChange={setSource} options={sources.map(item => ({ value: item.id, label: item.name }))} />}
        {selectedDays === undefined && <Select aria-label="产出统计区间" value={days} onChange={setDays} options={[7, 30, 90, 365].map(value => ({ value, label: `最近 ${value} 天` }))} />}
        <Button aria-label="刷新产出统计" icon={<ReloadOutlined />} onClick={() => setRevision(value => value + 1)} />
      </div>
    </div>
    {error && <Alert type="error" showIcon message="岗位产出读取失败" description={error} />}
    <Spin spinning={loading}>
      {!loading && !error && !sources.length ? <Empty description="当前项目尚未关联云效数据源" /> : data?.productivity ? <>
        <div className="yx-output-kpis">
          {config.metrics.map(([key, label]) => { const item = data.productivity!.metrics[key]; return <button type="button" key={key}
            className={`yx-output-kpi ${key === metricKey ? 'selected' : ''}`} aria-pressed={key === metricKey} onClick={() => setMetricKey(key)}>
            <span>{label}</span><strong>{item ? number(item.current) : '—'}</strong>{item && <Comparison metric={item} />}
          </button> })}
        </div>
        <div className="yx-output-charts">
          <Card className="yx-output-trend" size="small" title={`${config.metrics.find(([key]) => key === metricKey)?.[1]}趋势`} extra={<span className="yx-muted">每日 · 上海时间</span>}>
            <div className="yx-block-chart" role="img" aria-label={`每日产出趋势，总计 ${metric?.current || 0}`}>
              {metric?.trend.map(point => { const count = Math.ceil(point.value / max * 12); return <Tooltip key={point.date} title={`${point.date} · ${number(point.value)}`}>
                <div className="yx-block-column" tabIndex={0} aria-label={`${point.date}: ${point.value}`}>
                  {Array.from({ length: 12 }, (_, index) => <i key={index} className={index >= 12 - count ? 'filled' : ''} />)}
                </div>
              </Tooltip> })}
            </div>
            <div className="yx-output-axis"><span>{data.productivity.from}</span><span>悬停查看每日精确值</span><span>{data.productivity.through}</span></div>
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
          <div className="yx-muted">延期需求 {number(data.totals.delayed)} · 只统计紧急、高、中优先级；新增与交付使用各自的事件日期。</div>
        </div>}
        <Table size="small" rowKey="name" pagination={{ pageSize: 5, hideOnSinglePage: true }} dataSource={metric?.contributors || []} columns={[
          { title: metricKey === 'delivered' ? '当前负责人' : '产出人', dataIndex: 'name' },
          { title: '本期数量', dataIndex: 'count', render: number },
          { title: '占比', render: (_: unknown, person: { count: number }) => `${total ? (person.count / total * 100).toFixed(1) : 0}%` },
        ]} />
        <p className="yx-output-footnote">{info?.last_success_at ? `最近同步 ${new Date(info.last_success_at).toLocaleString('zh-CN')}` : '尚无成功同步'} · 同步每小时更新。环比为前一个等长自然日区间。</p>
        <p className="yx-output-footnote">{role === 'development' ? `组织提交排除合并提交；需求关联提交按当前源需求关联统计。增删行仅统计关联提交，${unknown} 次提交行数未知。代码采集：${info?.code_since?.slice(0, 10) || '未知'} 至 ${info?.code_until?.slice(0, 10) || '未知'}。` : `新增按创建时间和创建人；交付按当前已发布状态与状态变更时间、当前负责人统计，不推算历史经手人。${missing} 条缺少时间，未计入趋势；${metric?.unattributed || 0} 条产出人待归属。`} 岗位用于选择指标，不据此推断成员职级或绩效。</p>
      </> : !loading && !error && sources.length > 0 ? <Empty description="统计数据暂不可用，请刷新或确认服务版本" /> : null}
    </Spin>
  </section>
}
