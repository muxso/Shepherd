import { useEffect, useState } from 'react'
import {
  Button,
  Card,
  Col,
  Empty,
  Form,
  Input,
  List,
  Modal,
  Row,
  Select,
  Space,
  Statistic,
  Table,
  Tag,
  Typography,
  message,
} from 'antd'
import {
  PlusOutlined,
  PlayCircleOutlined,
  ReloadOutlined,
  FileMarkdownOutlined,
  LinkOutlined,
  ClockCircleOutlined,
} from '@ant-design/icons'
import { api, ApiError, type ApiCase, type PlanCase, type PlanStats } from '../api'
import { useApp } from '../context'
import { outcomeColor } from '../components/tags'
import { regAdd, regList, type RegItem } from '../registry'

export default function TestPlans() {
  const { projectId } = useApp()
  const [plans, setPlans] = useState<RegItem[]>([])
  const [selId, setSelId] = useState('')
  const [createOpen, setCreateOpen] = useState(false)

  useEffect(() => {
    const list = regList('plan', projectId)
    setPlans(list)
    setSelId((cur) => (list.some((p) => p.id === cur) ? cur : list[0]?.id || ''))
  }, [projectId])

  const selected = plans.find((p) => p.id === selId)

  if (!projectId)
    return (
      <div style={{ padding: 48 }}>
        <Empty description="请先在顶部选择项目" />
      </div>
    )

  return (
    <div style={{ display: 'flex', height: '100%' }}>
      <div style={{ width: 280, background: '#fff', borderRight: '1px solid #f0f0f0', display: 'flex', flexDirection: 'column' }}>
        <div style={{ padding: 12, borderBottom: '1px solid #f5f5f5' }}>
          <Button type="primary" icon={<PlusOutlined />} size="small" block onClick={() => setCreateOpen(true)}>
            新建测试计划
          </Button>
        </div>
        <List
          dataSource={plans}
          locale={{ emptyText: <Empty description="暂无计划" /> }}
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
              <Typography.Text strong ellipsis>
                {p.label}
              </Typography.Text>
            </List.Item>
          )}
        />
      </div>

      <div style={{ flex: 1, overflow: 'auto', padding: 16 }}>
        {!selected ? (
          <Card>
            <Empty description="选择或新建一个测试计划" />
          </Card>
        ) : (
          <PlanDetail key={selected.id} planId={selected.id} name={selected.label} projectId={projectId} />
        )}
      </div>

      <Modal title="新建测试计划" open={createOpen} onCancel={() => setCreateOpen(false)} footer={null} destroyOnHidden>
        <CreatePlanForm
          projectId={projectId}
          onCreated={(id, name) => {
            setCreateOpen(false)
            setPlans(regAdd('plan', projectId, { id, label: name, createdAt: Date.now() }))
            setSelId(id)
          }}
        />
      </Modal>
    </div>
  )
}

function CreatePlanForm({ projectId, onCreated }: { projectId: string; onCreated: (id: string, name: string) => void }) {
  const [saving, setSaving] = useState(false)
  return (
    <Form
      layout="vertical"
      onFinish={async (v: { name: string }) => {
        setSaving(true)
        try {
          const p = await api.createPlan({ projectId, name: v.name })
          message.success('计划已创建')
          onCreated(p.id, v.name)
        } catch (e) {
          message.error(e instanceof ApiError ? e.message : '创建失败')
        } finally {
          setSaving(false)
        }
      }}
    >
      <Form.Item name="name" label="计划名" rules={[{ required: true }]}>
        <Input placeholder="如:回归冒烟" autoFocus />
      </Form.Item>
      <Button type="primary" htmlType="submit" loading={saving} block>
        创建
      </Button>
    </Form>
  )
}

function PlanDetail({ planId, name, projectId }: { planId: string; name: string; projectId: string }) {
  const [stats, setStats] = useState<PlanStats | null>(null)
  const [cases, setCases] = useState<PlanCase[]>([])
  const [loading, setLoading] = useState(false)
  const [linkOpen, setLinkOpen] = useState(false)
  const [running, setRunning] = useState(false)
  const [mdOpen, setMdOpen] = useState(false)
  const [md, setMd] = useState('')

  const load = async () => {
    setLoading(true)
    try {
      const [s, c] = await Promise.all([api.planStats(planId), api.planCases(planId)])
      setStats(s)
      setCases(Array.isArray(c) ? c : c.items)
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : '加载计划失败')
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [planId])

  const run = async () => {
    setRunning(true)
    try {
      const r = await api.runPlan(planId)
      message.success(`执行完成:${r.executed}/${r.total}`)
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? `执行失败:${e.status}` : '执行失败')
    } finally {
      setRunning(false)
    }
  }

  const openReport = async () => {
    try {
      setMd(await api.planReportMd(planId))
      setMdOpen(true)
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : '获取报告失败')
    }
  }

  const schedule = () => {
    let cron = '0 0 * * * *'
    Modal.confirm({
      title: '配置定时执行(cron)',
      content: (
        <Input defaultValue={cron} onChange={(e) => (cron = e.target.value)} style={{ marginTop: 8 }} className="ms-mono" />
      ),
      onOk: async () => {
        try {
          await api.planSchedule(planId, cron)
          message.success('定时已配置')
        } catch (e) {
          message.error(e instanceof ApiError ? e.message : '配置失败')
        }
      },
    })
  }

  return (
    <Card
      title={
        <Space>
          <Typography.Text strong>{name}</Typography.Text>
          {stats && <Tag color={stats.isPass ? 'green' : 'red'}>{stats.isPass ? '通过' : '未通过'}</Tag>}
        </Space>
      }
      extra={
        <Space wrap>
          <Button icon={<LinkOutlined />} size="small" onClick={() => setLinkOpen(true)}>
            挂用例
          </Button>
          <Button type="primary" icon={<PlayCircleOutlined />} size="small" loading={running} onClick={run}>
            执行计划
          </Button>
          <Button icon={<ClockCircleOutlined />} size="small" onClick={schedule}>
            定时
          </Button>
          <Button icon={<FileMarkdownOutlined />} size="small" onClick={openReport}>
            Markdown 报告
          </Button>
          <Button icon={<ReloadOutlined />} size="small" onClick={load} />
        </Space>
      }
    >
      <Row gutter={16} style={{ marginBottom: 16 }}>
        <Col span={6}><Card size="small"><Statistic title="用例总数" value={stats?.total ?? 0} /></Card></Col>
        <Col span={6}><Card size="small"><Statistic title="通过率" value={((stats?.passRate ?? 0) * 100).toFixed(1)} suffix="%" valueStyle={{ color: '#2e7d32' }} /></Card></Col>
        <Col span={6}><Card size="small"><Statistic title="执行率" value={((stats?.executeRate ?? 0) * 100).toFixed(1)} suffix="%" /></Card></Col>
        <Col span={6}><Card size="small"><Statistic title="状态" value={stats?.status ?? '—'} /></Card></Col>
      </Row>

      <Table<PlanCase>
        rowKey="caseId"
        size="small"
        loading={loading}
        dataSource={cases}
        pagination={false}
        locale={{ emptyText: <Empty description="未挂用例,点击「挂用例」" /> }}
        columns={[
          { title: '用例名', dataIndex: 'name', ellipsis: true },
          { title: '结果', dataIndex: 'status', width: 110, render: (s: string) => <Tag color={outcomeColor(s)}>{s}</Tag> },
          { title: '耗时(ms)', dataIndex: 'latencyMs', width: 100, render: (v?: number | null) => v ?? '—' },
          { title: '状态码', dataIndex: 'statusCode', width: 90, render: (v?: number | null) => v ?? '—' },
          { title: '用例 ID', dataIndex: 'caseId', render: (v: string) => <span className="ms-mono" style={{ fontSize: 12 }}>{v}</span> },
        ]}
      />

      <LinkCaseModal
        open={linkOpen}
        planId={planId}
        projectId={projectId}
        onClose={() => setLinkOpen(false)}
        onLinked={() => {
          setLinkOpen(false)
          load()
        }}
      />

      <Modal title={`Markdown 报告 · ${name}`} open={mdOpen} onCancel={() => setMdOpen(false)} width={760} footer={
        <Space>
          <Button onClick={() => navigator.clipboard?.writeText(md).then(() => message.success('已复制'))}>复制</Button>
          <Button type="primary" onClick={() => setMdOpen(false)}>关闭</Button>
        </Space>
      }>
        <pre style={{ background: '#0f1419', color: '#d6deeb', padding: 12, borderRadius: 6, maxHeight: 520, overflow: 'auto', fontSize: 12 }}>
          {md}
        </pre>
      </Modal>
    </Card>
  )
}

function LinkCaseModal({
  open,
  planId,
  projectId,
  onClose,
  onLinked,
}: {
  open: boolean
  planId: string
  projectId: string
  onClose: () => void
  onLinked: () => void
}) {
  const [cases, setCases] = useState<ApiCase[]>([])
  const [caseId, setCaseId] = useState('')
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    if (open) api.projectCases(projectId).then((p) => setCases(p.items)).catch(() => setCases([]))
  }, [open, projectId])

  const link = async () => {
    const c = cases.find((x) => x.id === caseId)
    if (!c) return
    setSaving(true)
    try {
      await api.linkPlanCase(planId, c.id, c.name)
      message.success('已挂入用例')
      onLinked()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : '挂入失败')
    } finally {
      setSaving(false)
    }
  }

  return (
    <Modal title="挂入接口用例" open={open} onCancel={onClose} onOk={link} confirmLoading={saving} okButtonProps={{ disabled: !caseId }} destroyOnHidden>
      <Select
        style={{ width: '100%' }}
        showSearch
        placeholder="选择项目下的接口用例"
        value={caseId || undefined}
        onChange={setCaseId}
        optionFilterProp="label"
        options={cases.map((c) => ({ value: c.id, label: `${c.method} ${c.name}` }))}
        notFoundContent="项目暂无接口用例(先去「接口定义」建用例)"
      />
    </Modal>
  )
}
