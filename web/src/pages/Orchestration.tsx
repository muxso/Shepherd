import { useEffect, useState } from 'react'
import {
  Button,
  Card,
  Col,
  Drawer,
  Empty,
  Input,
  List,
  Row,
  Space,
  Statistic,
  Table,
  Tag,
  Typography,
  message,
} from 'antd'
import { PlayCircleOutlined, ReloadOutlined, SendOutlined, SafetyCertificateOutlined } from '@ant-design/icons'
import { api, ApiError, type DeliveryEvent, type Task, type VerificationReport } from '../api'
import { useApp } from '../context'
import { regList, regAdd, type RegItem } from '../registry'

const taskColor = (s: string) => {
  const v = s.toUpperCase()
  if (v === 'VERIFIED') return 'green'
  if (v === 'FAILED') return 'red'
  if (v === 'PENDING') return 'default'
  return 'blue'
}

export default function Orchestration() {
  const { projectId } = useApp()
  const [items, setItems] = useState<RegItem[]>([])
  const [selId, setSelId] = useState('')
  const [manualId, setManualId] = useState('')

  useEffect(() => {
    const list = regList('decomposition', projectId)
    setItems(list)
    setSelId((cur) => (list.some((p) => p.id === cur) ? cur : list[0]?.id || ''))
  }, [projectId])

  if (!projectId)
    return (
      <div style={{ padding: 48 }}>
        <Empty description="请先在顶部选择项目" />
      </div>
    )

  const selected = items.find((i) => i.id === selId)

  return (
    <div style={{ display: 'flex', height: '100%' }}>
      <div style={{ width: 300, background: '#fff', borderRight: '1px solid #f0f0f0', display: 'flex', flexDirection: 'column' }}>
        <div style={{ padding: 12, borderBottom: '1px solid #f5f5f5' }}>
          <Typography.Text type="secondary" style={{ fontSize: 12 }}>
            拆分图来自「需求 → 自动拆分」。也可手动输入 ID 加载:
          </Typography.Text>
          <Space.Compact style={{ width: '100%', marginTop: 8 }}>
            <Input
              size="small"
              placeholder="decomposition id"
              value={manualId}
              onChange={(e) => setManualId(e.target.value)}
              className="ms-mono"
            />
            <Button
              size="small"
              onClick={() => {
                if (!manualId.trim()) return
                setItems(regAdd('decomposition', projectId, { id: manualId.trim(), label: manualId.trim(), createdAt: Date.now() }))
                setSelId(manualId.trim())
                setManualId('')
              }}
            >
              加载
            </Button>
          </Space.Compact>
        </div>
        <List
          dataSource={items}
          locale={{ emptyText: <Empty description="暂无拆分图" /> }}
          renderItem={(p) => (
            <List.Item
              onClick={() => setSelId(p.id)}
              style={{
                cursor: 'pointer',
                padding: '10px 14px',
                background: p.id === selId ? '#e8f0ff' : undefined,
                borderLeft: p.id === selId ? '3px solid #1664ff' : '3px solid transparent',
              }}
            >
              <Space direction="vertical" size={0}>
                <Typography.Text strong ellipsis style={{ maxWidth: 230 }}>
                  {p.label}
                </Typography.Text>
                <span className="ms-mono" style={{ fontSize: 11, color: '#8a9099' }}>
                  {p.id.slice(0, 18)}…
                </span>
              </Space>
            </List.Item>
          )}
        />
      </div>

      <div style={{ flex: 1, overflow: 'auto', padding: 16 }}>
        {!selected ? (
          <Card>
            <Empty description="选择一个拆分图" />
          </Card>
        ) : (
          <DecompositionView key={selected.id} decompId={selected.id} verificationId={selected.meta?.verificationId} />
        )}
      </div>
    </div>
  )
}

function DecompositionView({ decompId, verificationId }: { decompId: string; verificationId?: string }) {
  const [tasks, setTasks] = useState<Task[]>([])
  const [loading, setLoading] = useState(false)
  const [running, setRunning] = useState(false)
  const [summary, setSummary] = useState<{ total: number; verified: number; failed: number; blocked: number; rounds: number } | null>(null)
  const [report, setReport] = useState<VerificationReport | null>(null)
  const [eventsFor, setEventsFor] = useState<Task | null>(null)

  const load = async () => {
    setLoading(true)
    try {
      const d = await api.decomposition(decompId)
      setTasks(d.tasks || [])
      if (verificationId) api.verificationReport(verificationId).then(setReport).catch(() => undefined)
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : '加载拆分图失败')
    } finally {
      setLoading(false)
    }
  }
  useEffect(() => {
    load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [decompId])

  const run = async () => {
    setRunning(true)
    try {
      const s = await api.runDecomposition(decompId)
      setSummary(s)
      message.success(`运行完成:验证 ${s.verified}/${s.total}`)
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? `运行失败:${e.status}` : '运行失败')
    } finally {
      setRunning(false)
    }
  }

  const dispatch = async (t: Task) => {
    try {
      await api.createDelivery({ decompositionId: decompId, taskId: t.id, title: t.title, executor: 'CLAUDE_CODE' })
      message.success(`已派发任务 ${t.id}`)
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? `派发失败:${e.status}` : '派发失败')
    }
  }

  return (
    <Space direction="vertical" style={{ width: '100%' }} size={16}>
      <Card
        loading={loading}
        title={<Space><Typography.Text strong>任务拆分图</Typography.Text><span className="ms-mono" style={{ fontSize: 12, color: '#8a9099' }}>{decompId.slice(0, 24)}…</span></Space>}
        extra={
          <Space>
            <Button type="primary" icon={<PlayCircleOutlined />} size="small" loading={running} onClick={run}>
              并行运行
            </Button>
            <Button icon={<ReloadOutlined />} size="small" onClick={load} />
          </Space>
        }
      >
        {summary && (
          <Row gutter={16} style={{ marginBottom: 16 }}>
            <Col span={5}><Card size="small"><Statistic title="总任务" value={summary.total} /></Card></Col>
            <Col span={5}><Card size="small"><Statistic title="已验证" value={summary.verified} valueStyle={{ color: '#2e7d32' }} /></Card></Col>
            <Col span={5}><Card size="small"><Statistic title="失败" value={summary.failed} valueStyle={{ color: summary.failed ? '#c62828' : undefined }} /></Card></Col>
            <Col span={5}><Card size="small"><Statistic title="阻塞" value={summary.blocked} /></Card></Col>
            <Col span={4}><Card size="small"><Statistic title="轮次" value={summary.rounds} /></Card></Col>
          </Row>
        )}
        <Table<Task>
          rowKey="id"
          size="small"
          dataSource={tasks}
          pagination={false}
          locale={{ emptyText: <Empty description="暂无任务" /> }}
          columns={[
            { title: '任务', dataIndex: 'title', ellipsis: true },
            { title: 'ID', dataIndex: 'id', width: 80, render: (v: string) => <span className="ms-mono">{v}</span> },
            { title: '状态', dataIndex: 'status', width: 110, render: (s: string) => <Tag color={taskColor(s)}>{s}</Tag> },
            {
              title: '依赖',
              dataIndex: 'dependencies',
              render: (d?: string[]) => (d?.length ? d.join(', ') : '—'),
            },
            {
              title: '操作',
              width: 150,
              render: (_, t) => (
                <Space>
                  <Button type="link" size="small" icon={<SendOutlined />} onClick={() => dispatch(t)}>
                    派发
                  </Button>
                  <Button type="link" size="small" onClick={() => setEventsFor(t)}>
                    事件
                  </Button>
                </Space>
              ),
            },
          ]}
        />
      </Card>

      {verificationId && (
        <Card size="small" title={<Space><SafetyCertificateOutlined />验证报告(覆盖链)</Space>}>
          {report ? (
            <Space size={32} align="center">
              <Statistic title="已满足标准" value={report.satisfied ?? 0} />
              <Space direction="vertical" size={2}>
                <Typography.Text type="secondary" style={{ fontSize: 14 }}>
                  完整性
                </Typography.Text>
                <Tag color={report.complete ? 'green' : 'orange'}>{report.complete ? '已完整' : '有缺口'}</Tag>
              </Space>
              <span className="ms-mono" style={{ fontSize: 12, color: '#8a9099' }}>vid {verificationId.slice(0, 12)}…</span>
            </Space>
          ) : (
            <Typography.Text type="secondary">暂无报告</Typography.Text>
          )}
        </Card>
      )}

      <EventsDrawer decompId={decompId} task={eventsFor} onClose={() => setEventsFor(null)} />
    </Space>
  )
}

function EventsDrawer({ decompId, task, onClose }: { decompId: string; task: Task | null; onClose: () => void }) {
  const [events, setEvents] = useState<DeliveryEvent[]>([])
  const [loading, setLoading] = useState(false)

  useEffect(() => {
    if (!task) return
    setLoading(true)
    setEvents([])
    api
      .deliveries(decompId, task.id)
      .then(async (atts) => {
        const all: DeliveryEvent[] = []
        for (const a of atts) {
          const id = a.id || a.attemptId
          if (id) all.push(...(await api.deliveryEvents(id).catch(() => [])))
        }
        setEvents(all)
      })
      .catch((e) => message.error(e instanceof ApiError ? e.message : '加载事件失败'))
      .finally(() => setLoading(false))
  }, [task, decompId])

  return (
    <Drawer title={task ? `交付事件 · ${task.title}` : ''} open={!!task} onClose={onClose} width={520}>
      <Table<DeliveryEvent>
        rowKey={(_, i) => String(i)}
        size="small"
        loading={loading}
        dataSource={events}
        pagination={false}
        locale={{ emptyText: <Empty description="暂无事件(先派发任务)" /> }}
        columns={[
          { title: '#', dataIndex: 'seq', width: 50, render: (v?: number) => v ?? '—' },
          { title: '类型', dataIndex: 'kind', width: 100, render: (k: string) => <Tag>{k}</Tag> },
          { title: '消息', dataIndex: 'message', render: (m?: string) => m || '—' },
        ]}
      />
    </Drawer>
  )
}
