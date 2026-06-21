import { useEffect, useState } from 'react'
import { Button, Card, Col, Empty, Form, Input, InputNumber, List, Row, Select, Space, Statistic, Table, Tag, Typography } from 'antd'
import { message } from '../feedback'
import { ThunderboltOutlined, ReloadOutlined } from '@ant-design/icons'
import { api, ApiError, type PerfReport } from '../api'
import { useApp } from '../context'
import { methodColor } from '../components/tags'
import { regAdd, regList, type RegItem } from '../registry'
import { useI18n } from '../i18n'

const METHODS = ['GET', 'POST', 'PUT', 'DELETE']

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
      <div style={{ width: 300, background: '#fff', borderRight: '1px solid #f0f0f0', display: 'flex', flexDirection: 'column' }}>
        <div style={{ padding: 12, borderBottom: '1px solid #f5f5f5', fontWeight: 600 }}>{t('perf.reports', '压测报告')}</div>
        <List
          dataSource={reports}
          locale={{ emptyText: <Empty description={t('perf.emptyReports', '暂无报告,右侧发起压测')} /> }}
          renderItem={(r) => (
            <List.Item
              onClick={() => setSelId(r.id)}
              style={{
                cursor: 'pointer',
                padding: '10px 14px',
                background: r.id === selId ? '#f3eefe' : undefined,
                borderLeft: r.id === selId ? '3px solid #7c3aed' : '3px solid transparent',
              }}
            >
              <Space direction="vertical" size={2} style={{ width: '100%' }}>
                <Space size={4}>
                  <Tag color={methodColor(r.meta?.method || 'GET')} style={{ margin: 0 }}>
                    {r.meta?.method || 'GET'}
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
  return (
    <Card size="small" style={{ marginBottom: 16 }} title={<Space><ThunderboltOutlined />{t('perf.start', '发起压测')}</Space>}>
      <Form
        form={form}
        layout="inline"
        initialValues={{ method: 'GET', concurrency: 10, iterations: 100 }}
        onFinish={async (v) => {
          setRunning(true)
          try {
            const r = await api.runPerf({
              projectId,
              method: v.method,
              url: v.url,
              concurrency: v.concurrency,
              iterations: v.iterations,
            })
            message.success(`${t('perf.started', '已发起')}:${r.status}`)
            onStarted(r.reportId, `${v.method} ${v.url}`, v.method)
          } catch (e) {
            message.error(e instanceof ApiError ? `${t('perf.startFail', '发起失败')}:${e.status}` : t('perf.startFail', '发起失败'))
          } finally {
            setRunning(false)
          }
        }}
      >
        <Form.Item name="method">
          <Select style={{ width: 100 }} options={METHODS.map((m) => ({ value: m, label: m }))} />
        </Form.Item>
        <Form.Item name="url" rules={[{ required: true, message: t('perf.urlRequired', '请输入 URL') }]} style={{ flex: 1, minWidth: 280 }}>
          <Input placeholder="http://127.0.0.1:9180/healthz" className="ms-mono" />
        </Form.Item>
        <Form.Item name="concurrency" label={t('perf.concurrency', '并发')}>
          <InputNumber min={1} max={1000} />
        </Form.Item>
        <Form.Item name="iterations" label={t('perf.iterations', '迭代')}>
          <InputNumber min={1} max={100000} />
        </Form.Item>
        <Form.Item>
          <Button type="primary" htmlType="submit" loading={running}>
            {t('perf.runBtn', '压测')}
          </Button>
        </Form.Item>
      </Form>
    </Card>
  )
}

function ReportView({ reportId }: { reportId: string }) {
  const { t } = useI18n()
  const [rep, setRep] = useState<PerfReport | null>(null)
  const [loading, setLoading] = useState(false)

  const load = async () => {
    setLoading(true)
    try {
      setRep(await api.perfReport(reportId))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('perf.loadFail', '加载报告失败'))
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reportId])

  if (!rep) return <Card loading={loading} />

  const lat = rep.latency || { min: 0, max: 0, mean: 0, p50: 0, p90: 0, p95: 0, p99: 0 }
  return (
    <Card
      loading={loading}
      title={
        <Space>
          <Tag color={methodColor(rep.method)}>{rep.method}</Tag>
          <span className="ms-mono">{rep.url}</span>
          <Tag color={rep.status === 'COMPLETED' ? 'green' : 'blue'}>{rep.status}</Tag>
        </Space>
      }
      extra={<Button icon={<ReloadOutlined />} size="small" onClick={load} />}
    >
      <Row gutter={16} style={{ marginBottom: 16 }}>
        <Col span={6}><Card size="small"><Statistic title={t('perf.totalReq', '总请求')} value={rep.total} /></Card></Col>
        <Col span={6}><Card size="small"><Statistic title={t('perf.throughput', '吞吐 (req/s)')} value={rep.throughputRps} precision={1} valueStyle={{ color: '#7c3aed' }} /></Card></Col>
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
      <Space style={{ marginTop: 12, color: '#8a9099', fontSize: 12 }} wrap>
        <span>{t('perf.concurrency', '并发')} {rep.concurrency}</span>
        <span>{t('perf.iterations', '迭代')} {rep.iterations || '—'}</span>
        <span>{t('perf.elapsed', '总耗时')} {rep.elapsedMs} ms</span>
      </Space>
    </Card>
  )
}
