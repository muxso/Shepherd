import { useEffect, useMemo, useState } from 'react'
import {
  Button,
  Descriptions,
  Dropdown,
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
import {
  PlusOutlined,
  ImportOutlined,
  ReloadOutlined,
  ApiOutlined,
  FolderOutlined,
  FolderAddOutlined,
  MoreOutlined,
  InboxOutlined,
} from '@ant-design/icons'
import type { ColumnsType } from 'antd/es/table'
import { api, ApiError, type ApiDefinition, type ApiModule } from '../api'
import { useApp } from '../context'
import { methodColor, statusColor } from '../components/tags'
import CasesPanel from './CasesPanel'
import MocksPanel from './MocksPanel'
import RequestEditor from '../components/RequestEditor'
import { useOpenParam } from '../components/Workspace'

const METHODS = ['GET', 'POST', 'PUT', 'DELETE', 'PATCH']
const PROTOCOLS = ['HTTP', 'GRPC', 'SQL', 'REDIS', 'WEBSOCKET']
const LIST_KEY = '__list__'

export default function ApiDefinitions() {
  const { projectId } = useApp()
  const [defs, setDefs] = useState<ApiDefinition[]>([])
  const [modules, setModules] = useState<ApiModule[]>([])
  const [loading, setLoading] = useState(false)
  const [search, setSearch] = useState('')
  const [moduleKey, setModuleKey] = useState('ALL') // ALL | UNFILED | <moduleId>
  const [createOpen, setCreateOpen] = useState(false)
  const [importOpen, setImportOpen] = useState(false)
  const [moduleForm, setModuleForm] = useState<{ mode: 'create' | 'rename'; id?: string; parentId?: string | null; name?: string } | null>(null)
  const [openIds, setOpenIds] = useState<string[]>([])
  const [activeKey, setActiveKey] = useState(LIST_KEY)

  const load = async () => {
    if (!projectId) {
      setDefs([])
      setModules([])
      return
    }
    setLoading(true)
    try {
      const [ds, ms] = await Promise.all([api.definitions(projectId), api.modules(projectId)])
      setDefs(Array.isArray(ds) ? ds : [])
      setModules(Array.isArray(ms) ? ms : [])
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : '加载失败')
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

  // 模块树:全部接口 > [未归类] + 模块(按 parentId 嵌套)。
  const treeData = useMemo(() => {
    const countIn = (mid: string) => defs.filter((d) => d.moduleId === mid).length
    const unfiled = defs.filter((d) => !d.moduleId).length
    const moduleNode = (m: ApiModule): any => {
      const children = modules.filter((x) => (x.parentId || null) === m.id).map(moduleNode)
      return {
        key: m.id,
        title: <ModuleTitle name={`${m.name} (${countIn(m.id)})`} onAction={(a) => onModuleAction(a, m)} />,
        icon: <FolderOutlined />,
        children: children.length ? children : undefined,
      }
    }
    const roots = modules.filter((m) => !m.parentId).map(moduleNode)
    return [
      {
        key: 'ALL',
        title: `全部接口 (${defs.length})`,
        icon: <ApiOutlined />,
        children: [
          { key: 'UNFILED', title: `未归类 (${unfiled})`, icon: <InboxOutlined /> },
          ...roots,
        ],
      },
    ]
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [defs, modules])

  const onModuleAction = (action: string, m: ApiModule) => {
    if (action === 'rename') setModuleForm({ mode: 'rename', id: m.id, name: m.name })
    else if (action === 'sub') setModuleForm({ mode: 'create', parentId: m.id })
    else if (action === 'delete')
      Modal.confirm({
        title: `删除模块「${m.name}」?`,
        content: '其下接口将变为未归类(不会删除接口)。',
        okButtonProps: { danger: true },
        onOk: async () => {
          try {
            await api.deleteModule(m.id)
            message.success('已删除')
            if (moduleKey === m.id) setModuleKey('ALL')
            load()
          } catch (e) {
            message.error(e instanceof ApiError ? e.message : '删除失败')
          }
        },
      })
  }

  const filtered = useMemo(
    () =>
      defs.filter((d) => {
        const mod =
          moduleKey === 'ALL' ? true : moduleKey === 'UNFILED' ? !d.moduleId : d.moduleId === moduleKey
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
  useOpenParam(openDef) // 支持 ?open=<definitionId> 深链打开
  const closeTab = (id: string) => {
    setOpenIds((ids) => {
      const next = ids.filter((x) => x !== id)
      setActiveKey((cur) => (cur === id ? next[next.length - 1] || LIST_KEY : cur))
      return next
    })
  }

  const move = async (d: ApiDefinition, moduleId: string | null) => {
    try {
      await api.moveDefinition(d.id, moduleId)
      message.success('已移动')
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : '移动失败')
    }
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
    {
      title: '模块',
      dataIndex: 'moduleId',
      width: 120,
      render: (mid?: string | null) => {
        const m = modules.find((x) => x.id === mid)
        return m ? <Tag color="geekblue">{m.name}</Tag> : <span style={{ color: '#bbb' }}>未归类</span>
      },
    },
    { title: '状态', dataIndex: 'status', width: 100, render: (s: string) => <Tag color={statusColor(s)}>{s}</Tag> },
    {
      title: '操作',
      width: 120,
      render: (_, d) => (
        <Space size={0}>
          <Button type="link" size="small" onClick={() => openDef(d.id)}>打开</Button>
          <Dropdown
            trigger={['click']}
            menu={{
              items: [
                { key: 'UNFILED', label: '移到「未归类」' },
                { type: 'divider' as const },
                ...modules.map((m) => ({ key: m.id, label: `移到「${m.name}」` })),
              ],
              onClick: ({ key }) => move(d, key === 'UNFILED' ? null : key),
            }}
          >
            <Button type="link" size="small">移动</Button>
          </Dropdown>
        </Space>
      ),
    },
  ]

  const listTab = (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '10px 12px', borderBottom: '1px solid #f0f0f0' }}>
        <Button type="primary" icon={<PlusOutlined />} onClick={() => setCreateOpen(true)}>添加接口</Button>
        <Button icon={<ImportOutlined />} onClick={() => setImportOpen(true)}>导入</Button>
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
          locale={{ emptyText: <Empty description="暂无接口" /> }}
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
            <Tag color={methodColor(d.method)} style={{ margin: 0 }}>{d.method || d.protocol}</Tag>
            <span style={{ maxWidth: 120, display: 'inline-block', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', verticalAlign: 'middle' }}>{d.name}</span>
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
      <div style={{ width: 240, background: '#fff', borderRight: '1px solid #f0f0f0', display: 'flex', flexDirection: 'column' }}>
        <div style={{ display: 'flex', alignItems: 'center', padding: '10px 12px', borderBottom: '1px solid #f5f5f5' }}>
          <span style={{ fontWeight: 600 }}>模块</span>
          <div style={{ flex: 1 }} />
          <Button size="small" type="text" icon={<FolderAddOutlined />} title="新建顶层模块" onClick={() => setModuleForm({ mode: 'create', parentId: null })} />
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
        moduleId={['ALL', 'UNFILED'].includes(moduleKey) ? null : moduleKey}
        onCreated={(d) => {
          setCreateOpen(false)
          load().then(() => openDef(d.id))
        }}
      />
      <ImportModal open={importOpen} onClose={() => setImportOpen(false)} projectId={projectId} onDone={() => { setImportOpen(false); load() }} />
      <ModuleFormModal
        state={moduleForm}
        projectId={projectId}
        onClose={() => setModuleForm(null)}
        onDone={() => { setModuleForm(null); load() }}
      />
    </div>
  )
}

function ModuleTitle({ name, onAction }: { name: string; onAction: (a: string) => void }) {
  return (
    <span style={{ display: 'inline-flex', alignItems: 'center', width: '100%' }}>
      <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{name}</span>
      <Dropdown
        trigger={['click']}
        menu={{
          items: [
            { key: 'sub', label: '新建子模块' },
            { key: 'rename', label: '重命名' },
            { type: 'divider' as const },
            { key: 'delete', label: '删除', danger: true },
          ],
          onClick: ({ key, domEvent }) => {
            domEvent.stopPropagation()
            onAction(key)
          },
        }}
      >
        <MoreOutlined onClick={(e) => e.stopPropagation()} style={{ padding: '0 4px', color: '#999' }} />
      </Dropdown>
    </span>
  )
}

function ModuleFormModal({
  state,
  projectId,
  onClose,
  onDone,
}: {
  state: { mode: 'create' | 'rename'; id?: string; parentId?: string | null; name?: string } | null
  projectId: string
  onClose: () => void
  onDone: () => void
}) {
  const [name, setName] = useState('')
  useEffect(() => setName(state?.name || ''), [state])
  if (!state) return null
  const submit = async () => {
    if (!name.trim()) return
    try {
      if (state.mode === 'create') await api.createModule({ projectId, parentId: state.parentId ?? null, name: name.trim() })
      else await api.renameModule(state.id!, name.trim())
      message.success('已保存')
      onDone()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : '保存失败')
    }
  }
  return (
    <Modal title={state.mode === 'create' ? '新建模块' : '重命名模块'} open onCancel={onClose} onOk={submit} destroyOnHidden>
      <Input placeholder="模块名" value={name} onChange={(e) => setName(e.target.value)} onPressEnter={submit} autoFocus />
    </Modal>
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
        <Tag color={statusColor(definition.status)} style={{ marginLeft: 8, alignSelf: 'center' }}>{definition.status}</Tag>
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
                <Descriptions.Item label="路径" span={2}><span className="ms-mono">{definition.path || '—'}</span></Descriptions.Item>
                <Descriptions.Item label="ID" span={2}><span className="ms-mono" style={{ fontSize: 12 }}>{definition.id}</span></Descriptions.Item>
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
  moduleId,
  onCreated,
}: {
  open: boolean
  onClose: () => void
  projectId: string
  moduleId: string | null
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
            // 若当前选中某模块,创建后顺手归入
            if (moduleId) await api.moveDefinition(d.id, moduleId).catch(() => undefined)
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
