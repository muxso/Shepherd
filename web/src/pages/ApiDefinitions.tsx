import { useEffect, useMemo, useState } from 'react'
import {
  Button,
  Card,
  Descriptions,
  Empty,
  Input,
  List,
  Modal,
  Form,
  Select,
  Space,
  Tabs,
  Tag,
  Typography,
  message,
} from 'antd'
import { PlusOutlined, ImportOutlined, ReloadOutlined } from '@ant-design/icons'
import { api, ApiError, type ApiDefinition } from '../api'
import { useApp } from '../context'
import { methodColor, statusColor } from '../components/tags'
import CasesPanel from './CasesPanel'
import MocksPanel from './MocksPanel'

const METHODS = ['GET', 'POST', 'PUT', 'DELETE', 'PATCH']
const PROTOCOLS = ['HTTP', 'GRPC', 'SQL', 'REDIS', 'WEBSOCKET']

export default function ApiDefinitions() {
  const { projectId } = useApp()
  const [defs, setDefs] = useState<ApiDefinition[]>([])
  const [loading, setLoading] = useState(false)
  const [selectedId, setSelectedId] = useState<string>('')
  const [search, setSearch] = useState('')
  const [createOpen, setCreateOpen] = useState(false)
  const [importOpen, setImportOpen] = useState(false)

  const load = async () => {
    if (!projectId) {
      setDefs([])
      return
    }
    setLoading(true)
    try {
      const list = await api.definitions(projectId)
      setDefs(list)
      if (list.length && !list.some((d) => d.id === selectedId)) setSelectedId(list[0].id)
      if (!list.length) setSelectedId('')
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : '加载接口失败')
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId])

  const filtered = useMemo(
    () =>
      defs.filter(
        (d) =>
          d.name.toLowerCase().includes(search.toLowerCase()) ||
          d.path.toLowerCase().includes(search.toLowerCase()),
      ),
    [defs, search],
  )
  const selected = defs.find((d) => d.id === selectedId)

  if (!projectId)
    return (
      <div style={{ padding: 48 }}>
        <Empty description="请先在右上角选择项目" />
      </div>
    )

  return (
    <div style={{ display: 'flex', height: '100%' }}>
      {/* 左:接口列表 */}
      <div
        style={{
          width: 340,
          background: '#fff',
          borderRight: '1px solid #f0f0f0',
          display: 'flex',
          flexDirection: 'column',
        }}
      >
        <div style={{ padding: 12, borderBottom: '1px solid #f5f5f5' }}>
          <Space.Compact style={{ width: '100%' }}>
            <Input.Search
              placeholder="搜索名称 / 路径"
              allowClear
              onChange={(e) => setSearch(e.target.value)}
            />
          </Space.Compact>
          <Space style={{ marginTop: 8 }}>
            <Button type="primary" icon={<PlusOutlined />} size="small" onClick={() => setCreateOpen(true)}>
              新建接口
            </Button>
            <Button icon={<ImportOutlined />} size="small" onClick={() => setImportOpen(true)}>
              导入
            </Button>
            <Button icon={<ReloadOutlined />} size="small" onClick={load} />
          </Space>
        </div>
        <div style={{ flex: 1, overflow: 'auto' }}>
          <List
            loading={loading}
            dataSource={filtered}
            locale={{ emptyText: <Empty description="暂无接口,点击「新建接口」" /> }}
            renderItem={(d) => (
              <List.Item
                onClick={() => setSelectedId(d.id)}
                style={{
                  cursor: 'pointer',
                  padding: '10px 14px',
                  background: d.id === selectedId ? '#e8f0ff' : undefined,
                  borderLeft: d.id === selectedId ? '3px solid #1664ff' : '3px solid transparent',
                }}
              >
                <div style={{ width: '100%' }}>
                  <Space>
                    <Tag color={methodColor(d.method)} style={{ margin: 0, fontWeight: 600 }}>
                      {d.method || d.protocol}
                    </Tag>
                    <Typography.Text strong ellipsis style={{ maxWidth: 220 }}>
                      {d.name}
                    </Typography.Text>
                  </Space>
                  <div className="ms-mono" style={{ color: '#8a9099', fontSize: 12, marginTop: 4 }}>
                    {d.path || '—'}
                  </div>
                </div>
              </List.Item>
            )}
          />
        </div>
      </div>

      {/* 右:详情 + 用例 + Mock */}
      <div style={{ flex: 1, padding: 16, overflow: 'auto' }}>
        {!selected ? (
          <Card>
            <Empty description="选择左侧接口查看详情" />
          </Card>
        ) : (
          <Card
            title={
              <Space>
                <Tag color={methodColor(selected.method)} style={{ fontWeight: 600 }}>
                  {selected.method || selected.protocol}
                </Tag>
                {selected.name}
                <Tag color={statusColor(selected.status)}>{selected.status}</Tag>
              </Space>
            }
          >
            <Tabs
              items={[
                {
                  key: 'info',
                  label: '基本信息',
                  children: (
                    <Descriptions column={2} bordered size="small">
                      <Descriptions.Item label="名称">{selected.name}</Descriptions.Item>
                      <Descriptions.Item label="协议">{selected.protocol}</Descriptions.Item>
                      <Descriptions.Item label="方法">{selected.method || '—'}</Descriptions.Item>
                      <Descriptions.Item label="状态">{selected.status}</Descriptions.Item>
                      <Descriptions.Item label="路径" span={2}>
                        <span className="ms-mono">{selected.path || '—'}</span>
                      </Descriptions.Item>
                      <Descriptions.Item label="ID" span={2}>
                        <span className="ms-mono">{selected.id}</span>
                      </Descriptions.Item>
                    </Descriptions>
                  ),
                },
                {
                  key: 'cases',
                  label: '接口用例',
                  children: <CasesPanel definition={selected} />,
                },
                {
                  key: 'mock',
                  label: 'Mock',
                  children: <MocksPanel definition={selected} />,
                },
              ]}
            />
          </Card>
        )}
      </div>

      <CreateDefinitionModal
        open={createOpen}
        onClose={() => setCreateOpen(false)}
        projectId={projectId}
        onCreated={(d) => {
          setCreateOpen(false)
          load().then(() => setSelectedId(d.id))
        }}
      />
      <ImportModal
        open={importOpen}
        onClose={() => setImportOpen(false)}
        projectId={projectId}
        onDone={() => {
          setImportOpen(false)
          load()
        }}
      />
    </div>
  )
}

function CreateDefinitionModal({
  open,
  onClose,
  projectId,
  onCreated,
}: {
  open: boolean
  onClose: () => void
  projectId: string
  onCreated: (d: ApiDefinition) => void
}) {
  const [form] = Form.useForm()
  const [saving, setSaving] = useState(false)
  return (
    <Modal
      title="新建接口"
      open={open}
      onCancel={onClose}
      onOk={() => form.submit()}
      confirmLoading={saving}
      destroyOnClose
    >
      <Form
        form={form}
        layout="vertical"
        preserve={false}
        initialValues={{ protocol: 'HTTP', method: 'GET' }}
        onFinish={async (v) => {
          setSaving(true)
          try {
            const d = await api.createDefinition({ projectId, ...v })
            message.success('接口已创建')
            onCreated(d)
          } catch (e) {
            message.error(e instanceof ApiError ? e.message : '创建失败')
          } finally {
            setSaving(false)
          }
        }}
      >
        <Form.Item name="name" label="名称" rules={[{ required: true }]}>
          <Input placeholder="如:用户登录" />
        </Form.Item>
        <Space>
          <Form.Item name="protocol" label="协议">
            <Select style={{ width: 130 }} options={PROTOCOLS.map((p) => ({ value: p, label: p }))} />
          </Form.Item>
          <Form.Item name="method" label="方法">
            <Select style={{ width: 110 }} options={METHODS.map((m) => ({ value: m, label: m }))} />
          </Form.Item>
        </Space>
        <Form.Item name="path" label="路径">
          <Input placeholder="/api/login" className="ms-mono" />
        </Form.Item>
      </Form>
    </Modal>
  )
}

function ImportModal({
  open,
  onClose,
  projectId,
  onDone,
}: {
  open: boolean
  onClose: () => void
  projectId: string
  onDone: () => void
}) {
  const [text, setText] = useState('')
  const [saving, setSaving] = useState(false)
  return (
    <Modal
      title="导入接口(OpenAPI 3.x / Swagger 2.0 JSON)"
      open={open}
      onCancel={onClose}
      confirmLoading={saving}
      onOk={async () => {
        let parsed: unknown
        try {
          parsed = JSON.parse(text)
        } catch {
          message.error('不是合法 JSON')
          return
        }
        setSaving(true)
        try {
          const r = await api.importDefinitions(projectId, parsed)
          message.success(`导入成功:新增 ${r.created.length},跳过 ${r.skipped}`)
          setText('')
          onDone()
        } catch (e) {
          message.error(e instanceof ApiError ? e.message : '导入失败')
        } finally {
          setSaving(false)
        }
      }}
      width={640}
    >
      <Input.TextArea
        rows={14}
        value={text}
        onChange={(e) => setText(e.target.value)}
        placeholder='{"openapi":"3.0.0","paths":{...}}'
        className="ms-mono"
      />
    </Modal>
  )
}
