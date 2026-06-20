import { useEffect, useState } from 'react'
import {
  Button,
  Card,
  Empty,
  Form,
  Input,
  List,
  Modal,
  Select,
  Space,
  Table,
  Tag,
  Typography,
  message,
} from 'antd'
import { PlusOutlined, PlayCircleOutlined, ReloadOutlined } from '@ant-design/icons'
import { api, ApiError, type Scenario, type ScenarioStep } from '../api'
import { useApp } from '../context'
import { methodColor } from '../components/tags'

const STEP_KINDS = ['CASE', 'REQUEST', 'SCENARIO']

export default function Scenarios() {
  const { projectId } = useApp()
  const [list, setList] = useState<Scenario[]>([])
  const [loading, setLoading] = useState(false)
  const [selId, setSelId] = useState('')
  const [steps, setSteps] = useState<ScenarioStep[]>([])
  const [createOpen, setCreateOpen] = useState(false)
  const [stepOpen, setStepOpen] = useState(false)
  const [running, setRunning] = useState(false)

  const load = async () => {
    if (!projectId) {
      setList([])
      return
    }
    setLoading(true)
    try {
      const ls = await api.scenarios(projectId)
      setList(ls)
      if (ls.length && !ls.some((s) => s.id === selId)) setSelId(ls[0].id)
      if (!ls.length) {
        setSelId('')
        setSteps([])
      }
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : '加载场景失败')
    } finally {
      setLoading(false)
    }
  }

  const loadSteps = async (id: string) => {
    if (!id) return
    try {
      const s = await api.getScenario(id)
      setSteps(s.steps || [])
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : '加载步骤失败')
    }
  }

  useEffect(() => {
    load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId])
  useEffect(() => {
    loadSteps(selId)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selId])

  const selected = list.find((s) => s.id === selId)

  const run = async () => {
    if (!selected) return
    setRunning(true)
    try {
      await api.runScenario(selected.id, selected.projectId)
      message.success('场景已触发执行')
    } catch (e) {
      message.error(e instanceof ApiError ? `执行失败:${e.status}` : '执行失败')
    } finally {
      setRunning(false)
    }
  }

  if (!projectId)
    return (
      <div style={{ padding: 48 }}>
        <Empty description="请先在右上角选择项目" />
      </div>
    )

  return (
    <div style={{ display: 'flex', height: '100%' }}>
      <div style={{ width: 300, background: '#fff', borderRight: '1px solid #f0f0f0' }}>
        <div style={{ padding: 12, borderBottom: '1px solid #f5f5f5' }}>
          <Space>
            <Button type="primary" icon={<PlusOutlined />} size="small" onClick={() => setCreateOpen(true)}>
              新建场景
            </Button>
            <Button icon={<ReloadOutlined />} size="small" onClick={load} />
          </Space>
        </div>
        <List
          loading={loading}
          dataSource={list}
          locale={{ emptyText: <Empty description="暂无场景" /> }}
          renderItem={(s) => (
            <List.Item
              onClick={() => setSelId(s.id)}
              style={{
                cursor: 'pointer',
                padding: '10px 14px',
                background: s.id === selId ? '#e8f0ff' : undefined,
                borderLeft: s.id === selId ? '3px solid #1664ff' : '3px solid transparent',
              }}
            >
              <Space>
                <Typography.Text strong>{s.name}</Typography.Text>
                <Tag>{s.status}</Tag>
              </Space>
            </List.Item>
          )}
        />
      </div>

      <div style={{ flex: 1, padding: 16, overflow: 'auto' }}>
        {!selected ? (
          <Card>
            <Empty description="选择或新建一个场景" />
          </Card>
        ) : (
          <Card
            title={
              <Space>
                <Typography.Text strong>{selected.name}</Typography.Text>
                <Tag>{selected.status}</Tag>
              </Space>
            }
            extra={
              <Space>
                <Button icon={<PlusOutlined />} size="small" onClick={() => setStepOpen(true)}>
                  添加步骤
                </Button>
                <Button
                  type="primary"
                  icon={<PlayCircleOutlined />}
                  size="small"
                  loading={running}
                  onClick={run}
                >
                  运行场景
                </Button>
              </Space>
            }
          >
            <Table<ScenarioStep>
              rowKey="id"
              size="small"
              dataSource={[...steps].sort((a, b) => a.order - b.order)}
              pagination={false}
              locale={{ emptyText: <Empty description="暂无步骤,点击「添加步骤」" /> }}
              columns={[
                { title: '#', dataIndex: 'order', width: 50 },
                {
                  title: '类型',
                  dataIndex: 'kind',
                  width: 110,
                  render: (k: string) => <Tag color="blue">{k}</Tag>,
                },
                {
                  title: '内容',
                  render: (_: unknown, s: ScenarioStep) => {
                    if (s.request)
                      return (
                        <Space>
                          <Tag color={methodColor(s.request.method)}>{s.request.method}</Tag>
                          <span className="ms-mono">{s.request.url}</span>
                        </Space>
                      )
                    if (s.caseId) return <span className="ms-mono">用例 {s.caseId}</span>
                    if (s.scenarioId) return <span className="ms-mono">子场景 {s.scenarioId}</span>
                    if (s.control) return <Typography.Text type="secondary">控制器</Typography.Text>
                    return '—'
                  },
                },
              ]}
            />
          </Card>
        )}
      </div>

      <Modal
        title="新建场景"
        open={createOpen}
        onCancel={() => setCreateOpen(false)}
        footer={null}
        destroyOnHidden
      >
        <CreateScenarioForm
          projectId={projectId}
          onCreated={(s) => {
            setCreateOpen(false)
            load().then(() => setSelId(s.id))
          }}
        />
      </Modal>

      <Modal
        title="添加步骤"
        open={stepOpen}
        onCancel={() => setStepOpen(false)}
        footer={null}
        destroyOnHidden
        width={600}
      >
        {selected && (
          <AddStepForm
            scenarioId={selected.id}
            nextOrder={steps.length ? Math.max(...steps.map((s) => s.order)) + 1 : 1}
            onAdded={() => {
              setStepOpen(false)
              loadSteps(selected.id)
            }}
          />
        )}
      </Modal>
    </div>
  )
}

function CreateScenarioForm({
  projectId,
  onCreated,
}: {
  projectId: string
  onCreated: (s: Scenario) => void
}) {
  const [saving, setSaving] = useState(false)
  return (
    <Form
      layout="vertical"
      onFinish={async (v: { name: string }) => {
        setSaving(true)
        try {
          const s = await api.createScenario(projectId, v.name)
          message.success('场景已创建')
          onCreated(s)
        } catch (e) {
          message.error(e instanceof ApiError ? e.message : '创建失败')
        } finally {
          setSaving(false)
        }
      }}
    >
      <Form.Item name="name" label="场景名" rules={[{ required: true }]}>
        <Input placeholder="如:下单主流程" autoFocus />
      </Form.Item>
      <Button type="primary" htmlType="submit" loading={saving} block>
        创建
      </Button>
    </Form>
  )
}

function AddStepForm({
  scenarioId,
  nextOrder,
  onAdded,
}: {
  scenarioId: string
  nextOrder: number
  onAdded: () => void
}) {
  const [form] = Form.useForm()
  const [kind, setKind] = useState('REQUEST')
  const [saving, setSaving] = useState(false)
  return (
    <Form
      form={form}
      layout="vertical"
      initialValues={{
        kind: 'REQUEST',
        order: nextOrder,
        method: 'GET',
        assertions: '[{"type":"StatusIs","args":200}]',
      }}
      onValuesChange={(c) => c.kind && setKind(c.kind)}
      onFinish={async (v) => {
        setSaving(true)
        try {
          if (v.kind === 'REQUEST') {
            let assertions: unknown = []
            if (v.assertions?.trim()) assertions = JSON.parse(v.assertions)
            await api.addStep(scenarioId, {
              kind: 'REQUEST',
              order: v.order,
              request: { method: v.method, url: v.url, body: v.body || null, assertions },
            })
          } else {
            await api.addStep(scenarioId, { kind: v.kind, order: v.order, refId: v.refId })
          }
          message.success('步骤已添加')
          onAdded()
        } catch (e) {
          message.error(e instanceof ApiError ? e.message : '添加失败(检查断言 JSON)')
        } finally {
          setSaving(false)
        }
      }}
    >
      <Space>
        <Form.Item name="kind" label="类型">
          <Select style={{ width: 140 }} options={STEP_KINDS.map((k) => ({ value: k, label: k }))} />
        </Form.Item>
        <Form.Item name="order" label="顺序">
          <Input type="number" style={{ width: 100 }} />
        </Form.Item>
      </Space>

      {kind === 'REQUEST' ? (
        <>
          <Space.Compact style={{ width: '100%' }}>
            <Form.Item name="method" label="方法" style={{ width: 120 }}>
              <Select options={['GET', 'POST', 'PUT', 'DELETE', 'PATCH'].map((m) => ({ value: m, label: m }))} />
            </Form.Item>
            <Form.Item name="url" label="URL" style={{ flex: 1 }} rules={[{ required: true }]}>
              <Input className="ms-mono" placeholder="http://127.0.0.1:9180/healthz" />
            </Form.Item>
          </Space.Compact>
          <Form.Item name="body" label="请求体(可选)">
            <Input.TextArea rows={2} className="ms-mono" />
          </Form.Item>
          <Form.Item name="assertions" label="断言(JSON 数组)">
            <Input.TextArea rows={2} className="ms-mono" />
          </Form.Item>
        </>
      ) : (
        <Form.Item
          name="refId"
          label={kind === 'CASE' ? '用例 ID' : '子场景 ID'}
          rules={[{ required: true }]}
        >
          <Input className="ms-mono" placeholder="引用的资源 id" />
        </Form.Item>
      )}

      <Button type="primary" htmlType="submit" loading={saving} block>
        添加
      </Button>
    </Form>
  )
}
