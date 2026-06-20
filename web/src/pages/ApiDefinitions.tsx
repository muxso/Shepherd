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
  message,
} from 'antd'
import { PlusOutlined, ImportOutlined, ReloadOutlined, ApiOutlined, FolderOutlined } from '@ant-design/icons'
import type { ColumnsType } from 'antd/es/table'
import { api, ApiError, type ApiDefinition } from '../api'
import { useApp } from '../context'
import { methodColor, statusColor } from '../components/tags'
import CasesPanel from './CasesPanel'
import MocksPanel from './MocksPanel'
import RequestEditor from '../components/RequestEditor'

const METHODS = ['GET', 'POST', 'PUT', 'DELETE', 'PATCH']
const PROTOCOLS = ['HTTP', 'GRPC', 'SQL', 'REDIS', 'WEBSOCKET']
const LIST_KEY = '__list__'

export default function ApiDefinitions() {
  const { projectId } = useApp()
  const [defs, setDefs] = useState<ApiDefinition[]>([])
  const [loading, setLoading] = useState(false)
  const [search, setSearch] = useState('')
  const [moduleKey, setModuleKey] = useState('ALL')
  const [createOpen, setCreateOpen] = useState(false)
  const [importOpen, setImportOpen] = useState(false)
  // 多 Tab 工作区:打开的接口 id 列表 + 当前激活 tab
  const [openIds, setOpenIds] = useState<string[]>([])
  const [activeKey, setActiveKey] = useState(LIST_KEY)

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
    setOpenIds([])
    setActiveKey(LIST_KEY)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId])

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

  const openDef = (id: string) => {
    setOpenIds((ids) => (ids.includes(id) ? ids : [...ids, id]))
    setActiveKey(id)
  }
  const closeTab = (id: string) => {
    setOpenIds((ids) => {
      const next = ids.filter((x) => x !== id)
      setActiveKey((cur) => (cur === id ? next[next.length - 1] || LIST_KEY : cur))
      return next
    })
  }

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
    { title: '路径', dataIndex: 'path', ellipsis: true, render: (p: string) => <span className="ms-mono" style={{ color: '#5b6470' }}>{p || '—'}</span> },
    { title: '状态', dataIndex: 'status', width: 110, render: (s: string) => <Tag color={statusColor(s)}>{s}</Tag> },
    { title: '操作', width: 90, render: (_, d) => <Button type="link" size="small" onClick={() => openDef(d.id)}>打开</Button> },
  ]

  const listTab = (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '10px 12px', borderBottom: '1px solid #f0f0f0' }}>
        <Button type="primary" icon={<PlusOutlined />} onClick={() => setCreateOpen(true)}>
          添加接口
        </Button>
        <Button icon={<ImportOutlined />} onClick={() => setImportOpen(true)}>
          导入
        </Button>
        <div style={{ flex: 1 }} />
        <Input.Search placeholder="搜索名称 / 路径" allowClear style={{ width: 240 }} onChange={(e) => setSearch(e.target.value)} />
        <Button icon={<ReloadOutlined />} onClick={load} />
      </div>
      <div style={{ flex: 1, overflow: 'auto', padding: 12 }}>
        <Table<ApiDefinition>
          rowKey="id"
          size="middle"
          loading={loading}
          dataSource={filtered}
          columns={columns}
          onRow={(d) => ({ onClick: () => openDef(d.id), style: { cursor: 'pointer' } })}
          pagination={{ pageSize: 15, size: 'small', showTotal: (t) => `共 ${t} 个接口` }}
          locale={{ emptyText: <Empty description="暂无接口,点击「添加接口」" /> }}
        />
      </div>
    </div>
  )

  const tabItems = [
    { key: LIST_KEY, label: '全部接口', closable: false, children: listTab },
    ...openIds
      .map((id) => defs.find((d) => d.id === id))
      .filter((d): d is ApiDefinition => !!d)
      .map((d) => ({
        key: d.id,
        label: (
          <Space size={4}>
            <Tag color={methodColor(d.method)} style={{ margin: 0 }}>
              {d.method || d.protocol}
            </Tag>
            <span style={{ maxWidth: 120, display: 'inline-block', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', verticalAlign: 'middle' }}>
              {d.name}
            </span>
          </Space>
        ),
        children: <ApiDetail definition={d} />,
      })),
  ]

  if (!projectId)
    return (
      <div style={{ padding: 48 }}>
        <Empty description="请先在右上角选择项目" />
      </div>
    )

  return (
    <div style={{ display: 'flex', height: '100%' }}>
      <div style={{ width: 220, background: '#fff', borderRight: '1px solid #f0f0f0', display: 'flex', flexDirection: 'column' }}>
        <div style={{ padding: '12px 14px', fontWeight: 600, borderBottom: '1px solid #f5f5f5' }}>模块</div>
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

      <div style={{ flex: 1, minWidth: 0, background: '#fff' }}>
        <Tabs
          type="editable-card"
          hideAdd
          activeKey={activeKey}
          onChange={setActiveKey}
          onEdit={(key, action) => action === 'remove' && closeTab(String(key))}
          items={tabItems}
          style={{ height: '100%' }}
          className="ms-worktabs"
        />
      </div>

      <CreateDefinitionModal
        open={createOpen}
        onClose={() => setCreateOpen(false)}
        projectId={projectId}
        onCreated={(d) => {
          setCreateOpen(false)
          load().then(() => openDef(d.id))
        }}
      />
      <ImportModal open={importOpen} onClose={() => setImportOpen(false)} projectId={projectId} onDone={() => { setImportOpen(false); load() }} />
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

// 单个接口详情(作为一个工作 Tab 的内容):请求行 + 子 Tab(基本信息/调试/用例/Mock)。
function ApiDetail({ definition }: { definition: ApiDefinition }) {
  return (
    <div style={{ padding: '12px 16px', height: '100%', overflow: 'auto' }}>
      <div style={{ display: 'flex', gap: 0, marginBottom: 12 }}>
        <span
          style={{
            background: '#f5f7fa',
            border: '1px solid #e5e8ec',
            borderRight: 'none',
            borderRadius: '6px 0 0 6px',
            padding: '5px 12px',
            fontWeight: 600,
            color: methodColorHex(definition.method),
          }}
        >
          {definition.method || definition.protocol}
        </span>
        <Input readOnly value={definition.path || '—'} className="ms-mono" style={{ borderRadius: '0 6px 6px 0' }} />
        <Tag color={statusColor(definition.status)} style={{ marginLeft: 8, alignSelf: 'center' }}>
          {definition.status}
        </Tag>
      </div>
      <Tabs
        items={[
          {
            key: 'info',
            label: '基本信息',
            children: (
              <Descriptions column={2} size="small" bordered>
                <Descriptions.Item label="名称">{definition.name}</Descriptions.Item>
                <Descriptions.Item label="协议">{definition.protocol}</Descriptions.Item>
                <Descriptions.Item label="方法">{definition.method || '—'}</Descriptions.Item>
                <Descriptions.Item label="状态">{definition.status}</Descriptions.Item>
                <Descriptions.Item label="路径" span={2}>
                  <span className="ms-mono">{definition.path || '—'}</span>
                </Descriptions.Item>
                <Descriptions.Item label="ID" span={2}>
                  <span className="ms-mono" style={{ fontSize: 12 }}>{definition.id}</span>
                </Descriptions.Item>
              </Descriptions>
            ),
          },
          { key: 'debug', label: '调试', children: <RequestEditor initialMethod={definition.method || 'GET'} initialUrl={definition.path || ''} /> },
          { key: 'cases', label: '接口用例', children: <CasesPanel definition={definition} /> },
          { key: 'mock', label: 'Mock', children: <MocksPanel definition={definition} /> },
        ]}
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
    <Modal title="添加接口" open={open} onCancel={onClose} onOk={() => form.submit()} confirmLoading={saving} destroyOnHidden>
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
      width={640}
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
    >
      <Input.TextArea rows={14} value={text} onChange={(e) => setText(e.target.value)} placeholder='{"openapi":"3.0.0","paths":{...}}' className="ms-mono" />
    </Modal>
  )
}
