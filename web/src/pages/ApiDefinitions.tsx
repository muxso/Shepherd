import { useEffect, useMemo, useState } from 'react'
import {
  Button,
  Descriptions,
  Empty,
  Input,
  Table,
  Modal,
  Form,
  Select,
  Space,
  Tabs,
  Tag,
  Tree,
  Typography,
  message,
} from 'antd'
import {
  PlusOutlined,
  ImportOutlined,
  ReloadOutlined,
  ApiOutlined,
  FolderOutlined,
  CloseOutlined,
} from '@ant-design/icons'
import type { ColumnsType } from 'antd/es/table'
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
  const [moduleKey, setModuleKey] = useState<string>('ALL') // ALL | proto:HTTP ...
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
      setDefs(Array.isArray(list) ? list : [])
    } catch (e) {
      setDefs([])
      message.error(e instanceof ApiError ? e.message : '加载接口失败')
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId])

  // 模块树:按协议分组(后端暂无模块概念,以协议作为可点击的“模块”)。
  const treeData = useMemo(() => {
    const byProto = new Map<string, number>()
    defs.forEach((d) => byProto.set(d.protocol, (byProto.get(d.protocol) || 0) + 1))
    return [
      {
        title: `全部接口 (${defs.length})`,
        key: 'ALL',
        icon: <FolderOutlined />,
        children: [...byProto.entries()].map(([p, n]) => ({
          title: `${p} (${n})`,
          key: `proto:${p}`,
          icon: <ApiOutlined />,
        })),
      },
    ]
  }, [defs])

  const filtered = useMemo(
    () =>
      defs.filter((d) => {
        const mod = moduleKey === 'ALL' || d.protocol === moduleKey.replace('proto:', '')
        const q =
          d.name.toLowerCase().includes(search.toLowerCase()) ||
          d.path.toLowerCase().includes(search.toLowerCase())
        return mod && q
      }),
    [defs, search, moduleKey],
  )
  const selected = defs.find((d) => d.id === selectedId)

  const columns: ColumnsType<ApiDefinition> = [
    {
      title: '名称',
      dataIndex: 'name',
      ellipsis: true,
      render: (name: string, d) => (
        <Space size={4}>
          <Tag color={methodColor(d.method)} style={{ margin: 0, fontWeight: 600, minWidth: 48, textAlign: 'center' }}>
            {d.method || d.protocol}
          </Tag>
          <span style={{ fontWeight: 500 }}>{name}</span>
        </Space>
      ),
    },
    {
      title: '路径',
      dataIndex: 'path',
      ellipsis: true,
      render: (p: string) => <span className="ms-mono" style={{ color: '#5b6470' }}>{p || '—'}</span>,
    },
    {
      title: '状态',
      dataIndex: 'status',
      width: 110,
      render: (s: string) => <Tag color={statusColor(s)}>{s}</Tag>,
    },
    {
      title: '操作',
      width: 90,
      render: (_, d) => (
        <Button type="link" size="small" onClick={() => setSelectedId(d.id)}>
          详情
        </Button>
      ),
    },
  ]

  if (!projectId)
    return (
      <div style={{ padding: 48 }}>
        <Empty description="请先在右上角选择项目" />
      </div>
    )

  return (
    <div style={{ display: 'flex', height: '100%' }}>
      {/* 一栏:模块树 */}
      <div style={{ width: 220, background: '#fff', borderRight: '1px solid #f0f0f0', display: 'flex', flexDirection: 'column' }}>
        <div style={{ padding: '12px 14px', fontWeight: 600, color: '#1f2329', borderBottom: '1px solid #f5f5f5' }}>
          模块
        </div>
        <div style={{ flex: 1, overflow: 'auto', padding: 8 }}>
          <Tree
            showIcon
            blockNode
            defaultExpandAll
            selectedKeys={[moduleKey]}
            treeData={treeData}
            onSelect={(keys) => keys.length && setModuleKey(String(keys[0]))}
          />
        </div>
      </div>

      {/* 二栏:接口列表 */}
      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', minWidth: 0 }}>
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 8,
            padding: '12px 16px',
            background: '#fff',
            borderBottom: '1px solid #f0f0f0',
          }}
        >
          <Button type="primary" icon={<PlusOutlined />} onClick={() => setCreateOpen(true)}>
            添加接口
          </Button>
          <Button icon={<ImportOutlined />} onClick={() => setImportOpen(true)}>
            导入
          </Button>
          <div style={{ flex: 1 }} />
          <Input.Search
            placeholder="搜索名称 / 路径"
            allowClear
            style={{ width: 240 }}
            onChange={(e) => setSearch(e.target.value)}
          />
          <Button icon={<ReloadOutlined />} onClick={load} />
        </div>
        <div style={{ flex: 1, overflow: 'auto', padding: 16 }}>
          <Table<ApiDefinition>
            rowKey="id"
            size="middle"
            loading={loading}
            dataSource={filtered}
            columns={columns}
            onRow={(d) => ({ onClick: () => setSelectedId(d.id), style: { cursor: 'pointer' } })}
            rowClassName={(d) => (d.id === selectedId ? 'ms-row-active' : '')}
            pagination={{ pageSize: 15, size: 'small', showTotal: (t) => `共 ${t} 个接口` }}
            locale={{ emptyText: <Empty description="暂无接口,点击「添加接口」" /> }}
          />
        </div>
      </div>

      {/* 三栏:详情(选中后出现) */}
      {selected && (
        <div style={{ width: 460, background: '#fff', borderLeft: '1px solid #f0f0f0', display: 'flex', flexDirection: 'column' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '10px 14px', borderBottom: '1px solid #f0f0f0' }}>
            <Tag color={methodColor(selected.method)} style={{ fontWeight: 600 }}>
              {selected.method || selected.protocol}
            </Tag>
            <Typography.Text strong ellipsis style={{ flex: 1 }}>
              {selected.name}
            </Typography.Text>
            <Tag color={statusColor(selected.status)}>{selected.status}</Tag>
            <Button type="text" size="small" icon={<CloseOutlined />} onClick={() => setSelectedId('')} />
          </div>
          {/* 请求行(MeterSphere 风:方法 + 路径) */}
          <div style={{ display: 'flex', gap: 0, padding: '10px 14px', borderBottom: '1px solid #f5f5f5' }}>
            <span
              style={{
                background: '#f5f7fa',
                border: '1px solid #e5e8ec',
                borderRight: 'none',
                borderRadius: '6px 0 0 6px',
                padding: '4px 10px',
                fontWeight: 600,
                color: methodColorHex(selected.method),
              }}
            >
              {selected.method || selected.protocol}
            </span>
            <Input readOnly value={selected.path || '—'} className="ms-mono" style={{ borderRadius: '0 6px 6px 0' }} />
          </div>
          <div style={{ flex: 1, overflow: 'auto', padding: '0 14px' }}>
            <Tabs
              items={[
                {
                  key: 'info',
                  label: '基本信息',
                  children: (
                    <Descriptions column={1} size="small" bordered>
                      <Descriptions.Item label="名称">{selected.name}</Descriptions.Item>
                      <Descriptions.Item label="协议">{selected.protocol}</Descriptions.Item>
                      <Descriptions.Item label="方法">{selected.method || '—'}</Descriptions.Item>
                      <Descriptions.Item label="路径">
                        <span className="ms-mono">{selected.path || '—'}</span>
                      </Descriptions.Item>
                      <Descriptions.Item label="状态">{selected.status}</Descriptions.Item>
                      <Descriptions.Item label="ID">
                        <span className="ms-mono" style={{ fontSize: 12 }}>{selected.id}</span>
                      </Descriptions.Item>
                    </Descriptions>
                  ),
                },
                { key: 'cases', label: '接口用例', children: <CasesPanel definition={selected} /> },
                { key: 'mock', label: 'Mock', children: <MocksPanel definition={selected} /> },
              ]}
            />
          </div>
        </div>
      )}

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

function methodColorHex(m: string): string {
  switch (m.toUpperCase()) {
    case 'GET':
      return '#2e7d32'
    case 'POST':
      return '#1664ff'
    case 'PUT':
      return '#ef6c00'
    case 'DELETE':
      return '#c62828'
    default:
      return '#5b6470'
  }
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
      title="添加接口"
      open={open}
      onCancel={onClose}
      onOk={() => form.submit()}
      confirmLoading={saving}
      destroyOnHidden
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
      destroyOnHidden
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
