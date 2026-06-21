import { useEffect, useState } from 'react'
import {
  Button,
  Card,
  Col,
  Descriptions,
  Drawer,
  Empty,
  Form,
  Input,
  Modal,
  Row,
  Space,
  Statistic,
  Table,
  Tabs,
  Tag,
  Typography,
  message,
} from 'antd'
import { BranchesOutlined, FlagOutlined, PartitionOutlined, PlayCircleOutlined, SendOutlined } from '@ant-design/icons'
import {
  api,
  ApiError,
  type DeliveryEvent,
  type Requirement,
  type Task,
  type VerificationReport,
} from '../api'
import { useApp } from '../context'
import { Workspace, WorkList, useWorkTabs } from '../components/Workspace'
import { regAdd, regList, type RegItem } from '../registry'

const toLines = (s: string) => s.split('\n').map((x) => x.trim()).filter(Boolean)
const taskColor = (s: string) => (s === 'VERIFIED' ? 'green' : s === 'FAILED' ? 'red' : s === 'PENDING' ? 'default' : 'blue')

// 需求与编排合一:需求列表 → 详情 Tab(需求信息/版本/基线/拆分 → 拆分图任务+运行+交付+验证)。
export default function Requirements() {
  const { projectId } = useApp()
  const [items, setItems] = useState<RegItem[]>([])
  const [createOpen, setCreateOpen] = useState(false)
  const tabs = useWorkTabs()

  useEffect(() => {
    setItems(regList('requirement', projectId))
    tabs.reset()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId])

  if (!projectId) return <div style={{ padding: 48 }}><Empty description="请先在顶部选择项目" /></div>

  const detailTabs = items
    .filter((r) => tabs.openIds.includes(r.id))
    .map((r) => ({
      key: r.id,
      label: r.label,
      children: <RequirementDetail key={r.id} reqId={r.id} projectId={projectId} onChanged={() => setItems(regList('requirement', projectId))} />,
    }))

  return (
    <>
      <Workspace
        listLabel="全部需求"
        activeKey={tabs.activeKey}
        onChange={tabs.setActiveKey}
        onClose={tabs.close}
        tabs={detailTabs}
        listContent={
          <WorkList<RegItem>
            onNew={() => setCreateOpen(true)}
            newLabel="新建需求"
            data={items}
            onRowClick={(r) => tabs.open(r.id)}
            emptyText="暂无需求"
            columns={[
              { title: '标题', dataIndex: 'label' },
              { title: '已拆分', dataIndex: 'meta', width: 100, render: (m?: Record<string, string>) => (m?.decompositionId ? <Tag color="geekblue">是</Tag> : '—') },
            ]}
          />
        }
      />
      <Modal title="新建需求" open={createOpen} onCancel={() => setCreateOpen(false)} footer={null} destroyOnHidden>
        <Form
          layout="vertical"
          onFinish={async (v: { title: string; criteria: string }) => {
            try {
              const r = await api.createRequirement({ projectId, title: v.title, acceptanceCriteria: toLines(v.criteria || '') })
              message.success('需求已创建')
              setItems(regAdd('requirement', projectId, { id: r.id, label: v.title, createdAt: Date.now() }))
              setCreateOpen(false)
              tabs.open(r.id)
            } catch (e) {
              message.error(e instanceof ApiError ? e.message : '创建失败')
            }
          }}
        >
          <Form.Item name="title" label="标题" rules={[{ required: true }]}><Input placeholder="如:用户登录" autoFocus /></Form.Item>
          <Form.Item name="criteria" label="验收标准(每行一条)"><Input.TextArea rows={4} placeholder={'登录成功\n错误密码拒绝'} /></Form.Item>
          <Button type="primary" htmlType="submit" block>创建</Button>
        </Form>
      </Modal>
    </>
  )
}

function RequirementDetail({ reqId, projectId, onChanged }: { reqId: string; projectId: string; onChanged: () => void }) {
  const [req, setReq] = useState<Requirement | null>(null)
  const [verOpen, setVerOpen] = useState(false)
  const reg = regList('requirement', projectId).find((r) => r.id === reqId)
  const [decompId, setDecompId] = useState<string | undefined>(reg?.meta?.decompositionId)
  const [verId, setVerId] = useState<string | undefined>(reg?.meta?.verificationId)

  const load = async () => {
    try {
      setReq(await api.getRequirement(reqId))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : '加载需求失败')
    }
  }
  useEffect(() => {
    load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reqId])

  const setBaseline = () => {
    let v = String(req?.baselineVersion ?? 1)
    Modal.confirm({
      title: '定基线到版本',
      content: <Input defaultValue={v} onChange={(e) => (v = e.target.value)} style={{ marginTop: 8 }} />,
      onOk: async () => {
        try {
          await api.setBaseline(reqId, Number(v))
          message.success('基线已更新')
          load()
        } catch (e) {
          message.error(e instanceof ApiError ? e.message : '定基线失败')
        }
      },
    })
  }

  const doBreakdown = async () => {
    try {
      const r = await api.breakdown(reqId)
      message.success(`已拆分:${r.tasks.length} 个任务`)
      regAdd('requirement', projectId, { id: reqId, label: req?.title || reqId, createdAt: reg?.createdAt || Date.now(), meta: { decompositionId: r.id, verificationId: r.verificationId } })
      setDecompId(r.id)
      setVerId(r.verificationId)
      onChanged()
    } catch (e) {
      message.error(e instanceof ApiError ? `拆分失败:${e.status}` : '拆分失败')
    }
  }

  return (
    <div style={{ padding: '12px 16px', height: '100%', overflow: 'auto' }}>
      <Tabs
        items={[
          {
            key: 'info',
            label: '需求信息',
            children: (
              <>
                <Space style={{ marginBottom: 12 }} wrap>
                  <Button icon={<BranchesOutlined />} size="small" onClick={() => setVerOpen(true)}>新增版本</Button>
                  <Button icon={<FlagOutlined />} size="small" onClick={setBaseline}>定基线</Button>
                  <Button type="primary" icon={<PartitionOutlined />} size="small" onClick={doBreakdown}>自动拆分</Button>
                </Space>
                <Descriptions column={1} size="small" bordered>
                  <Descriptions.Item label="标题">{req?.title}</Descriptions.Item>
                  <Descriptions.Item label="基线版本">v{req?.baselineVersion}</Descriptions.Item>
                  <Descriptions.Item label="状态">{req?.status}</Descriptions.Item>
                  <Descriptions.Item label="验收标准">
                    {req?.acceptanceCriteria?.length ? (
                      <ul style={{ margin: 0, paddingLeft: 18 }}>{req.acceptanceCriteria.map((c, i) => <li key={i}>{c}</li>)}</ul>
                    ) : '—'}
                  </Descriptions.Item>
                </Descriptions>
              </>
            ),
          },
          {
            key: 'orch',
            label: '拆分 / 交付 / 验证',
            children: decompId ? (
              <DecompositionView decompId={decompId} verificationId={verId} />
            ) : (
              <Empty description="尚未拆分,去「需求信息」点「自动拆分」生成任务图" />
            ),
          },
        ]}
      />
      <Modal title="新增需求版本" open={verOpen} onCancel={() => setVerOpen(false)} footer={null} destroyOnHidden>
        <Form
          layout="vertical"
          onFinish={async (v: { description: string; criteria: string }) => {
            try {
              const r = await api.addRequirementVersion(reqId, { description: v.description, acceptanceCriteria: toLines(v.criteria || '') })
              message.success(`已创建版本 v${r.version}`)
              setVerOpen(false)
              load()
            } catch (e) {
              message.error(e instanceof ApiError ? e.message : '创建版本失败')
            }
          }}
        >
          <Form.Item name="description" label="版本说明" rules={[{ required: true }]}><Input placeholder="如:支持飞书登录" autoFocus /></Form.Item>
          <Form.Item name="criteria" label="验收标准(每行一条)"><Input.TextArea rows={4} /></Form.Item>
          <Button type="primary" htmlType="submit" block>创建版本</Button>
        </Form>
      </Modal>
    </div>
  )
}

function DecompositionView({ decompId, verificationId }: { decompId: string; verificationId?: string }) {
  const [tasks, setTasks] = useState<Task[]>([])
  const [running, setRunning] = useState(false)
  const [summary, setSummary] = useState<{ total: number; verified: number; failed: number; blocked: number; rounds: number } | null>(null)
  const [report, setReport] = useState<VerificationReport | null>(null)
  const [eventsFor, setEventsFor] = useState<Task | null>(null)

  const load = async () => {
    try {
      const d = await api.decomposition(decompId)
      setTasks(d.tasks || [])
      if (verificationId) api.verificationReport(verificationId).then(setReport).catch(() => undefined)
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : '加载拆分图失败')
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
      message.success(`已派发 ${t.id}`)
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? `派发失败:${e.status}` : '派发失败')
    }
  }

  return (
    <Space direction="vertical" style={{ width: '100%' }} size={12}>
      <Space>
        <Button type="primary" icon={<PlayCircleOutlined />} size="small" loading={running} onClick={run}>并行运行</Button>
      </Space>
      {summary && (
        <Row gutter={12}>
          <Col span={5}><Card size="small"><Statistic title="总任务" value={summary.total} /></Card></Col>
          <Col span={5}><Card size="small"><Statistic title="已验证" value={summary.verified} valueStyle={{ color: '#2e7d32' }} /></Card></Col>
          <Col span={5}><Card size="small"><Statistic title="失败" value={summary.failed} /></Card></Col>
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
          { title: 'ID', dataIndex: 'id', width: 70, render: (v: string) => <span className="ms-mono">{v}</span> },
          { title: '状态', dataIndex: 'status', width: 100, render: (s: string) => <Tag color={taskColor(s)}>{s}</Tag> },
          { title: '依赖', dataIndex: 'dependencies', render: (d?: string[]) => (d?.length ? d.join(', ') : '—') },
          {
            title: '操作',
            width: 150,
            render: (_, t) => (
              <Space>
                <Button type="link" size="small" icon={<SendOutlined />} onClick={() => dispatch(t)}>派发</Button>
                <Button type="link" size="small" onClick={() => setEventsFor(t)}>事件</Button>
              </Space>
            ),
          },
        ]}
      />
      {verificationId && (
        <Card size="small" title="验证报告(覆盖链)">
          {report ? (
            <Space size={32} align="center">
              <Statistic title="已满足标准" value={report.satisfied ?? 0} />
              <Space direction="vertical" size={2}>
                <Typography.Text type="secondary" style={{ fontSize: 14 }}>完整性</Typography.Text>
                <Tag color={report.complete ? 'green' : 'orange'}>{report.complete ? '已完整' : '有缺口'}</Tag>
              </Space>
            </Space>
          ) : <Typography.Text type="secondary">暂无报告</Typography.Text>}
        </Card>
      )}
      <EventsDrawer decompId={decompId} task={eventsFor} onClose={() => setEventsFor(null)} />
    </Space>
  )
}

function EventsDrawer({ decompId, task, onClose }: { decompId: string; task: Task | null; onClose: () => void }) {
  const [events, setEvents] = useState<DeliveryEvent[]>([])
  useEffect(() => {
    if (!task) return
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
      .catch(() => undefined)
  }, [task, decompId])
  return (
    <Drawer title={task ? `交付事件 · ${task.title}` : ''} open={!!task} onClose={onClose} width={520}>
      <Table<DeliveryEvent>
        rowKey={(_, i) => String(i)}
        size="small"
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
