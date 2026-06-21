import { useEffect, useMemo, useState } from 'react'
import { Button, Empty, Form, Input, Modal, Select, Space, Table, Tag, Tree, Typography, message } from 'antd'
import { PlayCircleOutlined, PlusOutlined } from '@ant-design/icons'
import { api, ApiError, type Scenario, type ScenarioStep } from '../api'
import { useApp } from '../context'
import { methodColor } from '../components/tags'
import { Workspace, WorkList, PaneHeader, useWorkTabs } from '../components/Workspace'
import AssertionEditor from '../components/AssertionEditor'

const STEP_KINDS = ['CASE', 'REQUEST', 'SCENARIO']

export default function Scenarios() {
  const { projectId } = useApp()
  const [list, setList] = useState<Scenario[]>([])
  const [loading, setLoading] = useState(false)
  const [search, setSearch] = useState('')
  const [statusKey, setStatusKey] = useState('ALL')
  const [createOpen, setCreateOpen] = useState(false)
  const tabs = useWorkTabs()

  const load = async () => {
    if (!projectId) return setList([])
    setLoading(true)
    try {
      setList(await api.scenarios(projectId))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : '加载场景失败')
    } finally {
      setLoading(false)
    }
  }
  useEffect(() => {
    load()
    tabs.reset()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId])

  const treeData = useMemo(() => {
    const byStatus = new Map<string, number>()
    list.forEach((s) => byStatus.set(s.status, (byStatus.get(s.status) || 0) + 1))
    return [
      {
        key: 'ALL',
        title: `全部场景 (${list.length})`,
        children: [...byStatus.entries()].map(([s, n]) => ({ key: `st:${s}`, title: `${s} (${n})` })),
      },
    ]
  }, [list])

  const filtered = useMemo(
    () =>
      list.filter((s) => {
        const st = statusKey === 'ALL' || s.status === statusKey.replace('st:', '')
        return st && s.name.toLowerCase().includes(search.toLowerCase())
      }),
    [list, search, statusKey],
  )

  if (!projectId) return <div style={{ padding: 48 }}><Empty description="请先在顶部选择项目" /></div>

  const left = (
    <>
      <PaneHeader title="状态" />
      <div style={{ flex: 1, overflow: 'auto', padding: 8 }}>
        <Tree blockNode defaultExpandAll selectedKeys={[statusKey]} treeData={treeData} onSelect={(k) => k.length && setStatusKey(String(k[0]))} />
      </div>
    </>
  )

  const detailTabs = tabs.openIds
    .map((id) => list.find((s) => s.id === id))
    .filter((s): s is Scenario => !!s)
    .map((s) => ({ key: s.id, label: s.name, children: <ScenarioDetail scenario={s} /> }))

  return (
    <>
      <Workspace
        left={left}
        listLabel="全部场景"
        activeKey={tabs.activeKey}
        onChange={tabs.setActiveKey}
        onClose={tabs.close}
        tabs={detailTabs}
        listContent={
          <WorkList<Scenario>
            onNew={() => setCreateOpen(true)}
            newLabel="新建场景"
            onSearch={setSearch}
            searchPlaceholder="搜索场景名"
            onRefresh={load}
            data={filtered}
            loading={loading}
            onRowClick={(s) => tabs.open(s.id)}
            emptyText="暂无场景"
            columns={[
              { title: '名称', dataIndex: 'name', ellipsis: true },
              { title: '状态', dataIndex: 'status', width: 120, render: (s: string) => <Tag>{s}</Tag> },
            ]}
          />
        }
      />
      <Modal title="新建场景" open={createOpen} onCancel={() => setCreateOpen(false)} footer={null} destroyOnHidden>
        <CreateScenarioForm
          projectId={projectId}
          onCreated={(s) => {
            setCreateOpen(false)
            load().then(() => tabs.open(s.id))
          }}
        />
      </Modal>
    </>
  )
}

function ScenarioDetail({ scenario }: { scenario: Scenario }) {
  const [steps, setSteps] = useState<ScenarioStep[]>([])
  const [stepOpen, setStepOpen] = useState(false)
  const [running, setRunning] = useState(false)

  const loadSteps = async () => {
    try {
      const s = await api.getScenario(scenario.id)
      setSteps(s.steps || [])
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : '加载步骤失败')
    }
  }
  useEffect(() => {
    loadSteps()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scenario.id])

  const run = async () => {
    setRunning(true)
    try {
      await api.runScenario(scenario.id, scenario.projectId)
      message.success('场景已触发执行')
    } catch (e) {
      message.error(e instanceof ApiError ? `执行失败:${e.status}` : '执行失败')
    } finally {
      setRunning(false)
    }
  }

  return (
    <div style={{ padding: '12px 16px', height: '100%', overflow: 'auto' }}>
      <Space style={{ marginBottom: 12 }}>
        <Button icon={<PlusOutlined />} size="small" onClick={() => setStepOpen(true)}>添加步骤</Button>
        <Button type="primary" icon={<PlayCircleOutlined />} size="small" loading={running} onClick={run}>运行场景</Button>
      </Space>
      <Table<ScenarioStep>
        rowKey="id"
        size="small"
        dataSource={[...steps].sort((a, b) => a.order - b.order)}
        pagination={false}
        locale={{ emptyText: <Empty description="暂无步骤,点「添加步骤」" /> }}
        columns={[
          { title: '#', dataIndex: 'order', width: 50 },
          { title: '类型', dataIndex: 'kind', width: 110, render: (k: string) => <Tag color="blue">{k}</Tag> },
          {
            title: '内容',
            render: (_, s) => {
              if (s.request)
                return <Space><Tag color={methodColor(s.request.method)}>{s.request.method}</Tag><span className="ms-mono">{s.request.url}</span></Space>
              if (s.caseId) return <span className="ms-mono">用例 {s.caseId}</span>
              if (s.scenarioId) return <span className="ms-mono">子场景 {s.scenarioId}</span>
              if (s.control) return <Typography.Text type="secondary">控制器</Typography.Text>
              return '—'
            },
          },
        ]}
      />
      <Modal title="添加步骤" open={stepOpen} onCancel={() => setStepOpen(false)} footer={null} destroyOnHidden width={600}>
        <AddStepForm
          scenarioId={scenario.id}
          nextOrder={steps.length ? Math.max(...steps.map((s) => s.order)) + 1 : 1}
          onAdded={() => {
            setStepOpen(false)
            loadSteps()
          }}
        />
      </Modal>
    </div>
  )
}

function CreateScenarioForm({ projectId, onCreated }: { projectId: string; onCreated: (s: Scenario) => void }) {
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
      <Button type="primary" htmlType="submit" loading={saving} block>创建</Button>
    </Form>
  )
}

function AddStepForm({ scenarioId, nextOrder, onAdded }: { scenarioId: string; nextOrder: number; onAdded: () => void }) {
  const [form] = Form.useForm()
  const [kind, setKind] = useState('REQUEST')
  const [saving, setSaving] = useState(false)
  return (
    <Form
      form={form}
      layout="vertical"
      initialValues={{ kind: 'REQUEST', order: nextOrder, method: 'GET', assertions: [{ type: 'StatusIs', args: 200 }] }}
      onValuesChange={(c) => c.kind && setKind(c.kind)}
      onFinish={async (v) => {
        setSaving(true)
        try {
          if (v.kind === 'REQUEST') {
            await api.addStep(scenarioId, {
              kind: 'REQUEST',
              order: v.order,
              request: { method: v.method, url: v.url, body: v.body || null, assertions: v.assertions || [] },
            })
          } else {
            await api.addStep(scenarioId, { kind: v.kind, order: v.order, refId: v.refId })
          }
          message.success('步骤已添加')
          onAdded()
        } catch (e) {
          message.error(e instanceof ApiError ? e.message : '添加失败')
        } finally {
          setSaving(false)
        }
      }}
    >
      <Space>
        <Form.Item name="kind" label="类型"><Select style={{ width: 140 }} options={STEP_KINDS.map((k) => ({ value: k, label: k }))} /></Form.Item>
        <Form.Item name="order" label="顺序"><Input type="number" style={{ width: 100 }} /></Form.Item>
      </Space>
      {kind === 'REQUEST' ? (
        <>
          <Space.Compact style={{ width: '100%' }}>
            <Form.Item name="method" label="方法" style={{ width: 120 }}><Select options={['GET', 'POST', 'PUT', 'DELETE', 'PATCH'].map((m) => ({ value: m, label: m }))} /></Form.Item>
            <Form.Item name="url" label="URL" style={{ flex: 1 }} rules={[{ required: true }]}><Input className="ms-mono" placeholder="http://127.0.0.1:9180/healthz" /></Form.Item>
          </Space.Compact>
          <Form.Item name="body" label="请求体(可选)"><Input.TextArea rows={2} className="ms-mono" /></Form.Item>
          <Form.Item name="assertions" label="断言"><AssertionEditor /></Form.Item>
        </>
      ) : (
        <Form.Item name="refId" label={kind === 'CASE' ? '用例 ID' : '子场景 ID'} rules={[{ required: true }]}>
          <Input className="ms-mono" placeholder="引用的资源 id" />
        </Form.Item>
      )}
      <Button type="primary" htmlType="submit" loading={saving} block>添加</Button>
    </Form>
  )
}
