import { useEffect, useState } from 'react'
import { Button, Card, Descriptions, Empty, Form, Input, List, Modal, Space, Tag, Typography, message } from 'antd'
import { PlusOutlined, BranchesOutlined, FlagOutlined, PartitionOutlined } from '@ant-design/icons'
import { api, ApiError, type Requirement } from '../api'
import { useApp } from '../context'
import { regAdd, regList, type RegItem } from '../registry'

const toLines = (s: string) =>
  s
    .split('\n')
    .map((x) => x.trim())
    .filter(Boolean)

export default function Requirements() {
  const { projectId } = useApp()
  const [items, setItems] = useState<RegItem[]>([])
  const [selId, setSelId] = useState('')
  const [createOpen, setCreateOpen] = useState(false)

  useEffect(() => {
    const list = regList('requirement', projectId)
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
      <div style={{ width: 280, background: '#fff', borderRight: '1px solid #f0f0f0' }}>
        <div style={{ padding: 12, borderBottom: '1px solid #f5f5f5' }}>
          <Button type="primary" icon={<PlusOutlined />} size="small" block onClick={() => setCreateOpen(true)}>
            新建需求
          </Button>
        </div>
        <List
          dataSource={items}
          locale={{ emptyText: <Empty description="暂无需求" /> }}
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
            <Empty description="选择或新建一个需求" />
          </Card>
        ) : (
          <RequirementDetail key={selected.id} reqId={selected.id} projectId={projectId} />
        )}
      </div>

      <Modal title="新建需求" open={createOpen} onCancel={() => setCreateOpen(false)} footer={null} destroyOnHidden>
        <Form
          layout="vertical"
          onFinish={async (v: { title: string; criteria: string }) => {
            try {
              const r = await api.createRequirement({
                projectId,
                title: v.title,
                acceptanceCriteria: toLines(v.criteria || ''),
              })
              message.success('需求已创建')
              setItems(regAdd('requirement', projectId, { id: r.id, label: v.title, createdAt: Date.now() }))
              setSelId(r.id)
              setCreateOpen(false)
            } catch (e) {
              message.error(e instanceof ApiError ? e.message : '创建失败')
            }
          }}
        >
          <Form.Item name="title" label="标题" rules={[{ required: true }]}>
            <Input placeholder="如:用户登录" autoFocus />
          </Form.Item>
          <Form.Item name="criteria" label="验收标准(每行一条)">
            <Input.TextArea rows={4} placeholder={'登录成功\n错误密码拒绝'} />
          </Form.Item>
          <Button type="primary" htmlType="submit" block>
            创建
          </Button>
        </Form>
      </Modal>
    </div>
  )
}

function RequirementDetail({ reqId, projectId }: { reqId: string; projectId: string }) {
  const [req, setReq] = useState<Requirement | null>(null)
  const [loading, setLoading] = useState(false)
  const [verOpen, setVerOpen] = useState(false)

  const load = async () => {
    setLoading(true)
    try {
      setReq(await api.getRequirement(reqId))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : '加载需求失败')
    } finally {
      setLoading(false)
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
      message.success(`已拆分:${r.tasks.length} 个任务,已开验证账本`)
      // 登记到「拆分/交付/验证」页,便于后续编排
      regAdd('decomposition', projectId, {
        id: r.id,
        label: req?.title || reqId,
        createdAt: Date.now(),
        meta: { verificationId: r.verificationId },
      })
    } catch (e) {
      message.error(e instanceof ApiError ? `拆分失败:${e.status}` : '拆分失败')
    }
  }

  return (
    <Card
      loading={loading}
      title={
        <Space>
          <Typography.Text strong>{req?.title}</Typography.Text>
          {req && <Tag color="blue">基线 v{req.baselineVersion}</Tag>}
          {req && <Tag color={req.status === 'BASELINED' ? 'green' : 'default'}>{req.status}</Tag>}
        </Space>
      }
      extra={
        <Space wrap>
          <Button icon={<BranchesOutlined />} size="small" onClick={() => setVerOpen(true)}>
            新增版本
          </Button>
          <Button icon={<FlagOutlined />} size="small" onClick={setBaseline}>
            定基线
          </Button>
          <Button type="primary" icon={<PartitionOutlined />} size="small" onClick={doBreakdown}>
            自动拆分
          </Button>
        </Space>
      }
    >
      <Descriptions column={1} size="small" bordered>
        <Descriptions.Item label="标题">{req?.title}</Descriptions.Item>
        <Descriptions.Item label="基线版本">v{req?.baselineVersion}</Descriptions.Item>
        <Descriptions.Item label="状态">{req?.status}</Descriptions.Item>
        <Descriptions.Item label="验收标准">
          {req?.acceptanceCriteria?.length ? (
            <ul style={{ margin: 0, paddingLeft: 18 }}>
              {req.acceptanceCriteria.map((c, i) => (
                <li key={i}>{c}</li>
              ))}
            </ul>
          ) : (
            '—'
          )}
        </Descriptions.Item>
        <Descriptions.Item label="ID">
          <span className="ms-mono" style={{ fontSize: 12 }}>
            {reqId}
          </span>
        </Descriptions.Item>
      </Descriptions>
      <Typography.Paragraph type="secondary" style={{ marginTop: 12, fontSize: 12 }}>
        「自动拆分」会生成任务拆分图并开启验证账本,随后到「拆分/交付/验证」页编排执行。
      </Typography.Paragraph>

      <Modal title="新增需求版本" open={verOpen} onCancel={() => setVerOpen(false)} footer={null} destroyOnHidden>
        <Form
          layout="vertical"
          onFinish={async (v: { description: string; criteria: string }) => {
            try {
              const r = await api.addRequirementVersion(reqId, {
                description: v.description,
                acceptanceCriteria: toLines(v.criteria || ''),
              })
              message.success(`已创建版本 v${r.version}`)
              setVerOpen(false)
              load()
            } catch (e) {
              message.error(e instanceof ApiError ? e.message : '创建版本失败')
            }
          }}
        >
          <Form.Item name="description" label="版本说明" rules={[{ required: true }]}>
            <Input placeholder="如:支持飞书登录" autoFocus />
          </Form.Item>
          <Form.Item name="criteria" label="验收标准(每行一条)">
            <Input.TextArea rows={4} />
          </Form.Item>
          <Button type="primary" htmlType="submit" block>
            创建版本
          </Button>
        </Form>
      </Modal>
    </Card>
  )
}
