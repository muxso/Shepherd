import { useEffect, useState } from 'react'
import { Alert, Button, Card, Col, Empty, Form, Input, InputNumber, List, Radio, Row, Segmented, Select, Space, Statistic, Table, Tag, Typography } from 'antd'
import { message } from '../feedback'
import { ThunderboltOutlined, ReloadOutlined } from '@ant-design/icons'
import { api, ApiError, type Environment, type PerfLatency, type PerfReport, type Scenario } from '../api'
import { useApp } from '../context'
import { methodColor } from '../components/tags'
import { regAdd, regList, type RegItem } from '../registry'
import { useI18n } from '../i18n'

const METHODS = ['GET', 'POST', 'PUT', 'DELETE']
type TargetType = 'url' | 'scenario'

// SCENARIO 是场景压测的伪 method,展示为「场景」;真 HTTP 方法(GET/POST…)是专有名词,原样。
const kindLabel = (t: (k: string, d?: string) => string, m: string) =>
  m === 'SCENARIO' ? t('perf.kindScenario', '场景') : m

// 报告状态码 → 展示文案;未登记的原样兜底。
const statusLabel = (t: (k: string, d?: string) => string, s: string) => {
  const zh: Record<string, string> = { COMPLETED: '已完成', RUNNING: '压测中', ERROR: '失败' }
  return zh[s] ? t(`perf.st.${s}`, zh[s]) : s
}
type LoadMode = 'iterations' | 'duration'

export default function Perf() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const [reports, setReports] = useState<RegItem[]>([])
  const [selId, setSelId] = useState('')

  useEffect(() => {
    const list = regList('perf', projectId)
    setReports(list)
    setSelId((cur) => (list.some((r) => r.id === cur) ? cur : list[0]?.id || ''))
  }, [projectId])

  if (!projectId)
    return (
      <div style={{ padding: 48 }}>
        <Empty description={t('common.selectProject', '请先在顶部选择项目')} />
      </div>
    )

  return (
    <div style={{ display: 'flex', height: '100%' }}>
      <div style={{ width: 300, background: 'var(--panel)', borderRight: '1px solid var(--border-soft)', display: 'flex', flexDirection: 'column' }}>
        <div style={{ padding: 12, borderBottom: '1px solid var(--border-soft)', fontWeight: 600 }}>{t('perf.reports', '压测报告')}</div>
        <List
          dataSource={reports}
          locale={{ emptyText: <Empty description={t('perf.emptyReports', '暂无报告,右侧发起压测')} /> }}
          renderItem={(r) => (
            <List.Item
              onClick={() => setSelId(r.id)}
              style={{
                cursor: 'pointer',
                padding: '10px 14px',
                background: r.id === selId ? 'var(--brand-soft)' : undefined,
                borderLeft: r.id === selId ? '3px solid var(--brand)' : '3px solid transparent',
              }}
            >
              <Space direction="vertical" size={2} style={{ width: '100%' }}>
                <Space size={4}>
                  <Tag color={methodColor(r.meta?.method || 'GET')} style={{ margin: 0 }}>
                    {kindLabel(t, r.meta?.method || 'GET')}
                  </Tag>
                  <Typography.Text ellipsis style={{ maxWidth: 200 }} className="ms-mono">
                    {r.label}
                  </Typography.Text>
                </Space>
                <Typography.Text type="secondary" style={{ fontSize: 11 }}>
                  {new Date(r.createdAt).toLocaleString()}
                </Typography.Text>
              </Space>
            </List.Item>
          )}
        />
      </div>

      <div style={{ flex: 1, overflow: 'auto', padding: 16 }}>
        <RunForm
          projectId={projectId}
          onStarted={(id, label, method) => {
            setReports(regAdd('perf', projectId, { id, label, createdAt: Date.now(), meta: { method } }))
            setSelId(id)
          }}
        />
        {selId ? <ReportView key={selId} reportId={selId} /> : null}
      </div>
    </div>
  )
}

function RunForm({
  projectId,
  onStarted,
}: {
  projectId: string
  onStarted: (id: string, label: string, method: string) => void
}) {
  const { t } = useI18n()
  const [form] = Form.useForm()
  const [running, setRunning] = useState(false)
  const [target, setTarget] = useState<TargetType>('url')
  const [mode, setMode] = useState<LoadMode>('iterations')
  const [scenarios, setScenarios] = useState<Scenario[]>([])
  const [envs, setEnvs] = useState<Environment[]>([])

  // 场景/环境下拉:进入页面即拉,切换目标时无需等待。
  useEffect(() => {
    if (!projectId) return
    api.scenarios(projectId).then(setScenarios).catch(() => setScenarios([]))
    api.environments(projectId).then(setEnvs).catch(() => setEnvs([]))
  }, [projectId])

  const submit = async (v: Record<string, unknown>) => {
    setRunning(true)
    try {
      // 施压量:按次数 → iterations;按时长 → durationMs(秒 × 1000)。
      const load =
        mode === 'duration'
          ? { durationMs: Math.max(1, Number(v.durationSec) || 0) * 1000 }
          : { iterations: Number(v.iterations) || 100 }
      const concurrency = Number(v.concurrency) || 10

      if (target === 'scenario') {
        const sc = scenarios.find((s) => s.id === v.scenarioId)
        const r = await api.runScenarioPerf({
          projectId,
          scenarioId: String(v.scenarioId),
          environmentId: v.environmentId ? String(v.environmentId) : undefined,
          concurrency,
          ...load,
        })
        message.success(`${t('perf.scenarioStarted', '场景压测已发起,每轮')} ${r.stepCount} ${t('perf.steps', '步')}`)
        onStarted(r.reportId, sc?.name || String(v.scenarioId), 'SCENARIO')
      } else {
        const r = await api.runPerf({
          projectId,
          method: String(v.method),
          url: String(v.url),
          concurrency,
          ...load,
        })
        message.success(`${t('perf.started', '已发起')}:${r.status}`)
        onStarted(r.reportId, `${v.method} ${v.url}`, String(v.method))
      }
    } catch (e) {
      message.error(e instanceof ApiError ? `${t('perf.startFail', '发起失败')}:${e.status}` : t('perf.startFail', '发起失败'))
    } finally {
      setRunning(false)
    }
  }

  return (
    <Card size="small" style={{ marginBottom: 16 }} title={<Space><ThunderboltOutlined />{t('perf.start', '发起压测')}</Space>}>
      <Form form={form} layout="vertical" initialValues={{ method: 'GET', concurrency: 10, iterations: 100, durationSec: 30 }} onFinish={submit}>
        <Form.Item label={t('perf.target', '压测目标')} style={{ marginBottom: 12 }}>
          <Segmented
            value={target}
            onChange={(val) => setTarget(val as TargetType)}
            options={[
              { label: t('perf.targetUrl', '单接口'), value: 'url' },
              { label: t('perf.targetScenario', '场景'), value: 'scenario' },
            ]}
          />
        </Form.Item>

        {target === 'url' ? (
          <Row gutter={12}>
            <Col flex="100px">
              <Form.Item name="method" label="Method">
                <Select options={METHODS.map((m) => ({ value: m, label: m }))} />
              </Form.Item>
            </Col>
            <Col flex="auto">
              <Form.Item name="url" label="URL" rules={[{ required: true, message: t('perf.urlRequired', '请输入 URL') }]}>
                <Input placeholder="http://127.0.0.1:9180/healthz" className="ms-mono" />
              </Form.Item>
            </Col>
          </Row>
        ) : (
          <>
            <Row gutter={12}>
              <Col span={14}>
                <Form.Item name="scenarioId" label={t('perf.scenario', '场景')} rules={[{ required: true, message: t('perf.scenarioRequired', '请选择场景') }]}>
                  <Select
                    showSearch
                    optionFilterProp="label"
                    placeholder={t('perf.selectScenario', '选择场景')}
                    options={scenarios.map((s) => ({ value: s.id, label: s.name }))}
                  />
                </Form.Item>
              </Col>
              <Col span={10}>
                <Form.Item name="environmentId" label={t('perf.environment', '环境')}>
                  <Select
                    allowClear
                    placeholder={t('perf.noEnv', '不注入环境')}
                    options={envs.map((e) => ({ value: e.id, label: e.name }))}
                  />
                </Form.Item>
              </Col>
            </Row>
            <Alert type="info" showIcon style={{ marginBottom: 12 }} message={t('perf.scenarioHint', '一次施压 = 跑完整条场景链(登录→提取→鉴权调用→…),延迟为端到端耗时')} />
          </>
        )}

        <Row gutter={12} align="bottom">
          <Col flex="120px">
            <Form.Item name="concurrency" label={t('perf.concurrency', '并发')}>
              <InputNumber min={1} max={1000} style={{ width: '100%' }} />
            </Form.Item>
          </Col>
          {/* flex=none:随文案自适应宽度,英文较长时不折行堆叠。 */}
          <Col flex="none">
            <Form.Item label={t('perf.loadMode', '施压方式')}>
              <Radio.Group
                value={mode}
                onChange={(e) => setMode(e.target.value)}
                optionType="button"
                buttonStyle="solid"
                style={{ whiteSpace: 'nowrap' }}
              >
                <Radio.Button value="iterations">{t('perf.byIterations', '按次数')}</Radio.Button>
                <Radio.Button value="duration">{t('perf.byDuration', '按时长')}</Radio.Button>
              </Radio.Group>
            </Form.Item>
          </Col>
          <Col flex="160px">
            {mode === 'iterations' ? (
              <Form.Item name="iterations" label={t('perf.iterations', '迭代')}>
                <InputNumber min={1} max={100000} style={{ width: '100%' }} />
              </Form.Item>
            ) : (
              <Form.Item name="durationSec" label={t('perf.durationSec', '时长 (秒)')}>
                <InputNumber min={1} max={3600} style={{ width: '100%' }} />
              </Form.Item>
            )}
          </Col>
          <Col flex="auto" style={{ textAlign: 'right' }}>
            <Form.Item label=" ">
              <Button type="primary" htmlType="submit" loading={running} icon={<ThunderboltOutlined />}>
                {t('perf.runBtn', '压测')}
              </Button>
            </Form.Item>
          </Col>
        </Row>
      </Form>
    </Card>
  )
}

function ReportView({ reportId }: { reportId: string }) {
  const { t } = useI18n()
  const [rep, setRep] = useState<PerfReport | null>(null)
  const [loading, setLoading] = useState(false)

  // silent=true:轮询用,不弹 loading / 不弹错误 toast。
  const load = async (silent = false) => {
    if (!silent) setLoading(true)
    try {
      setRep(await api.perfReport(reportId))
    } catch (e) {
      if (!silent) message.error(e instanceof ApiError ? e.message : t('perf.loadFail', '加载报告失败'))
    } finally {
      if (!silent) setLoading(false)
    }
  }

  // 首次加载(切报告时先清空,避免显示上一份)
  useEffect(() => {
    setRep(null)
    load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reportId])

  // RUNNING 期间静默轮询(场景压测整条链耗时可达数十秒),跑完自动停。
  // 依赖 rep 对象身份:每次 setRep 换新引用 → 重新计时下一拍;状态转 COMPLETED/ERROR 即不再续约。
  useEffect(() => {
    if (rep?.status !== 'RUNNING') return
    const id = window.setTimeout(() => load(true), 1500)
    return () => window.clearTimeout(id)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rep])

  if (!rep) return <Card loading={loading} />

  const running = rep.status === 'RUNNING'
  // RUNNING 时后端 latency 仍是空 jsonb({}),各分位缺失 → 用 0 兜底,避免分位表出现空白行。
  const l = (rep.latency || {}) as Partial<PerfLatency>
  const lat = {
    min: l.min ?? 0, max: l.max ?? 0, mean: l.mean ?? 0,
    p50: l.p50 ?? 0, p90: l.p90 ?? 0, p95: l.p95 ?? 0, p99: l.p99 ?? 0,
  }
  return (
    <Card
      loading={loading}
      title={
        <Space>
          <Tag color={methodColor(rep.method)}>{kindLabel(t, rep.method)}</Tag>
          <span className="ms-mono">{rep.url}</span>
          <Tag color={rep.status === 'COMPLETED' ? 'green' : running ? 'processing' : 'blue'}>
            {running ? `${statusLabel(t, rep.status)}…` : statusLabel(t, rep.status)}
          </Tag>
        </Space>
      }
      extra={<Button icon={<ReloadOutlined spin={running} />} size="small" onClick={() => load()} />}
    >
      <Row gutter={16} style={{ marginBottom: 16 }}>
        <Col span={6}><Card size="small"><Statistic title={t('perf.totalReq', '总请求')} value={rep.total} /></Card></Col>
        <Col span={6}><Card size="small"><Statistic title={t('perf.throughput', '吞吐 (req/s)')} value={rep.throughputRps} precision={1} valueStyle={{ color: 'var(--brand)' }} /></Card></Col>
        <Col span={6}><Card size="small"><Statistic title={t('perf.success', '成功')} value={rep.success} valueStyle={{ color: '#2e7d32' }} /></Card></Col>
        <Col span={6}><Card size="small"><Statistic title={t('perf.errorRate', '失败率')} value={(rep.errorRate * 100).toFixed(1)} suffix="%" valueStyle={{ color: rep.failed ? '#c62828' : undefined }} /></Card></Col>
      </Row>
      <Typography.Text strong style={{ fontSize: 13 }}>
        {t('perf.latencyDist', '延迟分布(ms)')}
      </Typography.Text>
      <Table
        style={{ marginTop: 8 }}
        size="small"
        pagination={false}
        rowKey="k"
        dataSource={[
          { k: 'min', v: lat.min },
          { k: 'mean', v: lat.mean },
          { k: 'p50', v: lat.p50 },
          { k: 'p90', v: lat.p90 },
          { k: 'p95', v: lat.p95 },
          { k: 'p99', v: lat.p99 },
          { k: 'max', v: lat.max },
        ]}
        columns={[
          { title: t('perf.percentile', '分位'), dataIndex: 'k' },
          { title: t('perf.latency', '延迟 (ms)'), dataIndex: 'v' },
        ]}
      />
      <Space style={{ marginTop: 12, color: 'var(--text-3)', fontSize: 12 }} wrap>
        <span>{t('perf.concurrency', '并发')} {rep.concurrency}</span>
        <span>{t('perf.iterations', '迭代')} {rep.iterations || '—'}</span>
        <span>{t('perf.elapsed', '总耗时')} {rep.elapsedMs} ms</span>
      </Space>
    </Card>
  )
}
