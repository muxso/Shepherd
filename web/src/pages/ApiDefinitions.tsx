import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { Button, Checkbox, Dropdown, Empty, Input, Table, Modal, Segmented, Select, Space, Tabs, Tag, Tree, Upload } from 'antd'
import { message, modal } from '../feedback'
import {
  PlusOutlined,
  ImportOutlined,
  ReloadOutlined,
  ApiOutlined,
  FolderOutlined,
  FolderAddOutlined,
  MoreOutlined,
  InboxOutlined,
  SaveOutlined,
} from '@ant-design/icons'
import type { ColumnsType } from 'antd/es/table'
import { api, ApiError, type ApiDefinition, type ApiModule, type ApiSpec } from '../api'
import { useApp } from '../context'
import { methodColor, statusColor } from '../components/tags'
import CasesPanel from './CasesPanel'
import MocksPanel from './MocksPanel'
import RequestEditor from '../components/RequestEditor'
import ApiSpecPanel, { emptySpec } from '../components/ApiSpecPanel'
import ReferencesPanel from '../components/ReferencesPanel'
import ChangeHistoryPanel from '../components/ChangeHistoryPanel'
import { useOpenParam } from '../components/Workspace'
import { parseOperations, buildCaseUrl } from '../openapi'
import { useI18n } from '../i18n'

const METHODS = ['GET', 'POST', 'PUT', 'DELETE', 'PATCH']
const PROTOCOLS = ['HTTP', 'GRPC', 'SQL', 'REDIS', 'WEBSOCKET']
const LIST_KEY = '__list__'
const NEW_KEY = '__new__'

/** 把服务端时间文本("2026-06-21 12:34:56.78+00")渲染为 "2026-06-21 12:34:56";空/解析失败回退 "—"。 */
function fmtTs(ts?: string): string {
  if (!ts) return '—'
  const m = ts.match(/^(\d{4}-\d{2}-\d{2})[ T](\d{2}:\d{2}:\d{2})/)
  return m ? `${m[1]} ${m[2]}` : '—'
}

export default function ApiDefinitions() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const [defs, setDefs] = useState<ApiDefinition[]>([])
  const [modules, setModules] = useState<ApiModule[]>([])
  const [loading, setLoading] = useState(false)
  const [search, setSearch] = useState('')
  const [moduleKey, setModuleKey] = useState('ALL') // ALL | UNFILED | <moduleId>
  const [creating, setCreating] = useState(false)
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
      message.error(e instanceof ApiError ? e.message : t('apidef.loadFailed', '加载失败'))
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
        // 文件夹图标放进 title 内,不用 Tree 的 icon 槽:否则「icon + 100%宽 title」会溢出换行成两行。
        title: <ModuleTitle name={`${m.name} (${countIn(m.id)})`} onAction={(a) => onModuleAction(a, m)} />,
        children: children.length ? children : undefined,
      }
    }
    const roots = modules.filter((m) => !m.parentId).map(moduleNode)
    return [
      {
        key: 'ALL',
        title: `${t('apidef.allApis', '全部接口')} (${defs.length})`,
        icon: <ApiOutlined />,
        children: [
          { key: 'UNFILED', title: `${t('apidef.unfiled', '未归类')} (${unfiled})`, icon: <InboxOutlined /> },
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
      modal.confirm({
        title: `${t('apidef.deleteModuleTitle', '删除模块')}「${m.name}」?`,
        content: t('apidef.deleteModuleContent', '其下接口将变为未归类(不会删除接口)。'),
        okButtonProps: { danger: true },
        onOk: async () => {
          try {
            await api.deleteModule(m.id)
            message.success(t('apidef.deleted', '已删除'))
            if (moduleKey === m.id) setModuleKey('ALL')
            load()
          } catch (e) {
            message.error(e instanceof ApiError ? e.message : t('apidef.deleteFailed', '删除失败'))
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
    if (id === NEW_KEY) {
      setCreating(false)
      setActiveKey((cur) => (cur === NEW_KEY ? LIST_KEY : cur))
      return
    }
    setOpenIds((ids) => {
      const next = ids.filter((x) => x !== id)
      setActiveKey((cur) => (cur === id ? next[next.length - 1] || LIST_KEY : cur))
      return next
    })
  }

  const move = async (d: ApiDefinition, moduleId: string | null) => {
    try {
      await api.moveDefinition(d.id, moduleId)
      message.success(t('apidef.moved', '已移动'))
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('apidef.moveFailed', '移动失败'))
    }
  }

  // 复制接口:同协议/方法/路径新建一条「<名> copy」,归入同模块。
  const duplicate = async (d: ApiDefinition) => {
    try {
      const c = await api.createDefinition({ projectId, name: `${d.name} copy`, protocol: d.protocol, method: d.method, path: d.path })
      if (d.moduleId) await api.moveDefinition(c.id, d.moduleId).catch(() => undefined)
      message.success(t('apidef.duplicated', '已复制'))
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('apidef.createFailed', '创建失败'))
    }
  }

  const columns: ColumnsType<ApiDefinition> = [
    { title: 'ID', dataIndex: 'num', width: 90, render: (num: number | undefined, d) => <span className="ms-mono" style={{ color: '#8a9099', fontSize: 12 }} title={d.id}>{num ?? '—'}</span> },
    { title: t('apidef.colName', '名称'), dataIndex: 'name', ellipsis: true, render: (name: string) => <span style={{ fontWeight: 500 }}>{name}</span> },
    { title: t('apidef.protocol', '协议'), dataIndex: 'protocol', width: 90, render: (p: string) => <Tag>{p}</Tag> },
    { title: t('apidef.reqType', '请求类型'), dataIndex: 'method', width: 100, render: (m: string) => <Tag color={methodColor(m)} style={{ fontWeight: 600 }}>{m || '—'}</Tag> },
    { title: t('apidef.colPath', '路径'), dataIndex: 'path', ellipsis: true, render: (p: string) => <span className="ms-mono" style={{ color: '#5b6470' }}>{p || '—'}</span> },
    { title: t('apidef.colStatus', '状态'), dataIndex: 'status', width: 100, render: (s: string) => <Tag color={statusColor(s)}>{s}</Tag> },
    {
      title: t('apidef.colModule', '模块'),
      dataIndex: 'moduleId',
      width: 120,
      render: (mid?: string | null) => {
        const m = modules.find((x) => x.id === mid)
        return m ? <Tag color="geekblue">{m.name}</Tag> : <span style={{ color: '#bbb' }}>{t('apidef.unfiled', '未归类')}</span>
      },
    },
    {
      title: t('apidef.tags', '标签'),
      dataIndex: 'spec',
      width: 140,
      render: (spec?: ApiDefinition['spec']) => {
        const tags = spec?.tags || []
        return tags.length ? <Space size={[2, 2]} wrap>{tags.map((tg) => <Tag key={tg} style={{ margin: 0 }}>{tg}</Tag>)}</Space> : <span style={{ color: '#bbb' }}>—</span>
      },
    },
    {
      title: t('apidef.colCreatedBy', '创建人'),
      dataIndex: 'createdBy',
      width: 110,
      ellipsis: true,
      render: (u?: string) => u ? <span style={{ color: '#5b6470' }}>{u}</span> : <span style={{ color: '#bbb' }}>—</span>,
    },
    {
      title: t('apidef.colCreatedAt', '创建时间'),
      dataIndex: 'createdAt',
      width: 160,
      render: (ts?: string) => <span style={{ color: '#8a9099', fontSize: 12 }}>{fmtTs(ts)}</span>,
    },
    {
      title: t('apidef.colAction', '操作'),
      width: 150,
      render: (_, d) => (
        <Space size={0}>
          <Button type="link" size="small" onClick={() => openDef(d.id)}>{t('a.edit', '编辑')}</Button>
          <Button type="link" size="small" onClick={() => openDef(d.id)}>{t('apidef.run', '执行')}</Button>
          <Dropdown
            trigger={['click']}
            menu={{
              items: [
                { key: 'dup', label: t('apidef.duplicate', '复制') },
                { type: 'divider' as const },
                { key: 'UNFILED', label: t('apidef.moveToUnfiled', '移到「未归类」') },
                ...modules.map((m) => ({ key: m.id, label: `${t('apidef.moveToPrefix', '移到')}「${m.name}」` })),
              ],
              onClick: ({ key }) => (key === 'dup' ? duplicate(d) : move(d, key === 'UNFILED' ? null : key)),
            }}
          >
            <Button type="link" size="small">{t('a.more', '更多')}</Button>
          </Dropdown>
        </Space>
      ),
    },
  ]

  const listTab = (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '10px 12px', borderBottom: '1px solid #f0f0f0' }}>
        <Button type="primary" icon={<PlusOutlined />} onClick={() => { setCreating(true); setActiveKey(NEW_KEY) }}>{t('apidef.addApi', '添加接口')}</Button>
        <Button icon={<ImportOutlined />} onClick={() => setImportOpen(true)}>{t('a.import', '导入')}</Button>
        <div style={{ flex: 1 }} />
        <Input.Search placeholder={t('apidef.searchPlaceholder', '搜索名称 / 路径')} allowClear style={{ width: 240 }} onChange={(e) => setSearch(e.target.value)} />
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
          pagination={{ pageSize: 15, size: 'small', showTotal: (total) => `${t('apidef.totalPrefix', '共')} ${total} ${t('apidef.totalSuffix', '个接口')}` }}
          locale={{ emptyText: <Empty description={t('apidef.emptyApis', '暂无接口')} /> }}
        />
      </div>
    </div>
  )

  const tabItems = [
    { key: LIST_KEY, label: t('apidef.allApis', '全部接口'), closable: false, children: listTab },
    ...(creating
      ? [{
          key: NEW_KEY,
          label: t('apidef.newApi', '新建接口'),
          children: (
            <NewDefinitionTab
              projectId={projectId}
              moduleId={['ALL', 'UNFILED'].includes(moduleKey) ? null : moduleKey}
              onCancel={() => closeTab(NEW_KEY)}
              onCreated={(d) => {
                setCreating(false)
                load().then(() => openDef(d.id))
              }}
            />
          ),
        }]
      : []),
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
        <Empty description={t('apidef.selectProjectTopRight', '请先在右上角选择项目')} />
      </div>
    )

  return (
    <div style={{ display: 'flex', height: '100%' }}>
      <div style={{ width: 240, background: '#fff', borderRight: '1px solid #f0f0f0', display: 'flex', flexDirection: 'column' }}>
        <div style={{ display: 'flex', alignItems: 'center', padding: '10px 12px', borderBottom: '1px solid #f5f5f5' }}>
          <span style={{ fontWeight: 600 }}>{t('apidef.modules', '模块')}</span>
          <div style={{ flex: 1 }} />
          <Button size="small" type="text" icon={<FolderAddOutlined />} title={t('apidef.newTopModule', '新建顶层模块')} onClick={() => setModuleForm({ mode: 'create', parentId: null })} />
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
  const { t } = useI18n()
  return (
    <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6, width: '100%', minWidth: 0 }}>
      <FolderOutlined style={{ color: '#8a9099', flexShrink: 0 }} />
      <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{name}</span>
      <Dropdown
        trigger={['click']}
        menu={{
          items: [
            { key: 'sub', label: t('apidef.newSubModule', '新建子模块') },
            { key: 'rename', label: t('apidef.rename', '重命名') },
            { type: 'divider' as const },
            { key: 'delete', label: t('a.delete', '删除'), danger: true },
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
  const { t } = useI18n()
  const [name, setName] = useState('')
  useEffect(() => setName(state?.name || ''), [state])
  if (!state) return null
  const submit = async () => {
    if (!name.trim()) return
    try {
      if (state.mode === 'create') await api.createModule({ projectId, parentId: state.parentId ?? null, name: name.trim() })
      else await api.renameModule(state.id!, name.trim())
      message.success(t('apidef.saved', '已保存'))
      onDone()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('apidef.saveFailed', '保存失败'))
    }
  }
  return (
    <Modal title={state.mode === 'create' ? t('apidef.newModule', '新建模块') : t('apidef.renameModule', '重命名模块')} open onCancel={onClose} onOk={submit} destroyOnHidden>
      <Input placeholder={t('apidef.moduleName', '模块名')} value={name} onChange={(e) => setName(e.target.value)} onPressEnter={submit} autoFocus />
    </Modal>
  )
}

function ApiDetail({ definition }: { definition: ApiDefinition }) {
  const { t } = useI18n()
  // 顶层标签 + 定义页内的 定义/调试 切换(对标 MeterSphere:调试是定义内的模式)。
  const [tab, setTab] = useState('preview')
  const [defMode, setDefMode] = useState<'define' | 'debug'>('define')
  const [modules, setModules] = useState<ApiModule[]>([])
  const [meta, setMeta] = useState<{ tags: string[]; description?: string }>({ tags: [] })

  useEffect(() => {
    let alive = true
    api.modules(definition.projectId).then((m) => alive && setModules(Array.isArray(m) ? m : [])).catch(() => undefined)
    api
      .getDefinition(definition.id)
      .then((d) => alive && setMeta({ tags: d.spec?.tags || [], description: d.spec?.description }))
      .catch(() => undefined)
    return () => {
      alive = false
    }
  }, [definition.id, definition.projectId])

  const moduleName = modules.find((m) => m.id === definition.moduleId)?.name || t('apidef.unfiled', '未归类')

  // 预览头:状态/方法/[id]/名称 + 元信息行(对齐参考图 #1)。
  const previewHeader = (
    <div style={{ marginBottom: 16 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 10, flexWrap: 'wrap' }}>
        <Tag color={statusColor(definition.status)} style={{ margin: 0 }}>{definition.status}</Tag>
        <Tag color={methodColor(definition.method)} style={{ margin: 0, fontWeight: 600 }}>{definition.method || definition.protocol}</Tag>
        <span className="ms-mono" style={{ color: '#8a9099', fontSize: 12 }}>[{definition.num ?? '—'}]</span>
        <span style={{ fontWeight: 600, fontSize: 15, color: '#1f2329' }}>{definition.name}</span>
      </div>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: '6px 28px', fontSize: 12 }}>
        <Meta label={t('apidef.colPath', '路径')} value={<span className="ms-mono">{definition.path || '—'}</span>} />
        <Meta label={t('apidef.tags', '标签')} value={meta.tags.length ? meta.tags.join(', ') : '—'} />
        <Meta label={t('apidef.descLabel', '描述')} value={meta.description || '—'} />
        <Meta label={t('apidef.ownerModule', '所属模块')} value={moduleName} />
        <Meta label={t('apidef.colCreatedBy', '创建人')} value={definition.createdBy || '—'} />
        <Meta label={t('apidef.colCreatedAt', '创建时间')} value={fmtTs(definition.createdAt)} />
        <Meta label={t('apidef.updatedAt', '更新时间')} value={fmtTs(definition.updatedAt)} />
      </div>
    </div>
  )

  // 预览:详情 / 引用关系 / 变更历史(对标参考图 #1,这三者是预览的子标签)。
  const previewTab = (
    <>
      {previewHeader}
      <Tabs
        size="small"
        items={[
          { key: 'detail', label: t('apidef.detail', '详情'), children: <ApiSpecPanel definition={definition} mode="preview" /> },
          { key: 'refs', label: t('apidef.references', '引用关系'), children: <ReferencesPanel definition={definition} /> },
          { key: 'history', label: t('apidef.changeHistory', '变更历史'), children: <ChangeHistoryPanel definition={definition} /> },
        ]}
      />
    </>
  )

  // 定义:定义/调试 模式切换 + 对应面板。
  const defineTab = (
    <div>
      <Segmented
        value={defMode}
        onChange={(v) => setDefMode(v as 'define' | 'debug')}
        options={[
          { label: t('apidef.define', '定义'), value: 'define' },
          { label: t('apidef.debug', '调试'), value: 'debug' },
        ]}
        style={{ marginBottom: 12 }}
      />
      {defMode === 'define' ? (
        <ApiSpecPanel definition={definition} mode="define" />
      ) : (
        <RequestEditor initialMethod={definition.method || 'GET'} initialUrl={definition.path || ''} lockedProtocol={definition.protocol} />
      )}
    </div>
  )

  return (
    <div style={{ padding: '12px 16px', height: '100%', overflow: 'auto' }}>
      <Tabs
        activeKey={tab}
        onChange={setTab}
        tabBarExtraContent={
          tab === 'preview' ? (
            <Space size={4}>
              <Button size="small" onClick={() => setTab('define')}>{t('a.edit', '编辑')}</Button>
              <Button type="primary" size="small" onClick={() => { setTab('define'); setDefMode('debug') }}>{t('apidef.serverRun', '服务端执行')}</Button>
            </Space>
          ) : undefined
        }
        items={[
          { key: 'preview', label: t('apidef.preview', '预览'), children: previewTab },
          { key: 'define', label: t('apidef.define', '定义'), children: defineTab },
          { key: 'cases', label: t('apidef.casesTab', '用例'), children: <CasesPanel definition={definition} /> },
          { key: 'mock', label: 'MOCK', children: <MocksPanel definition={definition} /> },
        ]}
      />
    </div>
  )
}

/** 元信息条目:标签 + 值(对齐参考图预览头)。 */
function Meta({ label, value }: { label: string; value: ReactNode }) {
  return (
    <span style={{ color: '#8a9099' }}>
      {label} <span style={{ color: '#5b6470' }}>{value}</span>
    </span>
  )
}

/** 新建接口工作 Tab(对齐参考图:协议/方法/路径请求行 + 名称 + 描述 + 保存/取消)。 */
function NewDefinitionTab({
  projectId,
  moduleId,
  onCancel,
  onCreated,
}: {
  projectId: string
  moduleId: string | null
  onCancel: () => void
  onCreated: (d: ApiDefinition) => void
}) {
  const { t } = useI18n()
  const [name, setName] = useState('')
  const [protocol, setProtocol] = useState('HTTP')
  const [method, setMethod] = useState('GET')
  const [path, setPath] = useState('')
  const [spec, setSpec] = useState<ApiSpec>(emptySpec())
  const [defMode, setDefMode] = useState<'define' | 'debug'>('define')
  const [saving, setSaving] = useState(false)
  const isHttp = protocol === 'HTTP'

  const save = async () => {
    if (!name.trim()) return message.warning(t('apidef.nameRequired', '请填接口名称'))
    setSaving(true)
    try {
      const d = await api.createDefinition({ projectId, name: name.trim(), protocol, method: isHttp ? method : '', path })
      if (moduleId) await api.moveDefinition(d.id, moduleId).catch(() => undefined)
      // 把「定义」里编辑的请求/响应规格一并落库。
      await api.updateDefinitionSpec(d.id, spec).catch(() => undefined)
      message.success(t('apidef.apiCreated', '接口已创建'))
      onCreated(d)
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('apidef.createFailed', '创建失败'))
    } finally {
      setSaving(false)
    }
  }

  // 给 ApiSpecPanel(create)用的合成定义:无 id,仅带 projectId/protocol 供其渲染。
  const draftDef = { id: '', num: 0, projectId, name, protocol, method, path, status: 'DRAFT', moduleId, spec } as unknown as ApiDefinition

  return (
    <div style={{ padding: '12px 16px', height: '100%', overflow: 'auto' }}>
      {/* 请求行:协议 + 方法 + 路径 + 定义/调试 + 保存(对齐参考图 #6/#7) */}
      <div style={{ display: 'flex', gap: 8, marginBottom: 12, alignItems: 'center' }}>
        <Select value={protocol} onChange={setProtocol} style={{ width: 120 }} options={PROTOCOLS.map((p) => ({ value: p, label: p }))} />
        {isHttp && <Select value={method} onChange={setMethod} style={{ width: 100 }} options={METHODS.map((m) => ({ value: m, label: m }))} />}
        <Input value={path} onChange={(e) => setPath(e.target.value)} placeholder={isHttp ? '/api/login' : t('apidef.pathOrTarget', '路径 / 目标')} className="ms-mono" style={{ flex: 1 }} />
        {isHttp && (
          <Segmented
            value={defMode}
            onChange={(v) => setDefMode(v as 'define' | 'debug')}
            options={[
              { label: t('apidef.define', '定义'), value: 'define' },
              { label: t('apidef.debug', '调试'), value: 'debug' },
            ]}
          />
        )}
        <Button onClick={onCancel}>{t('a.cancel', '取消')}</Button>
        <Button type="primary" loading={saving} icon={<SaveOutlined />} onClick={save}>{t('a.save', '保存')}</Button>
      </div>
      <Input value={name} onChange={(e) => setName(e.target.value)} placeholder={t('apidef.nameInputPlaceholder', '请输入接口名称')} style={{ marginBottom: 12 }} autoFocus />
      {!isHttp ? (
        <span style={{ color: '#8a9099', fontSize: 12 }}>{t('apidef.nonHttpHint', '该协议当前仅登记/存储,执行能力待接入;保存后可在列表查看。')}</span>
      ) : defMode === 'define' ? (
        <ApiSpecPanel definition={draftDef} mode="create" value={spec} onChange={setSpec} />
      ) : (
        <RequestEditor initialMethod={method} initialUrl={path} lockedProtocol={protocol} />
      )}
    </div>
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
  const { t } = useI18n()
  const [text, setText] = useState('')
  const [fileName, setFileName] = useState('')
  const [genCases, setGenCases] = useState(true)
  const [saving, setSaving] = useState(false)

  const readFile = (file: File) => {
    const reader = new FileReader()
    reader.onload = () => {
      setText(String(reader.result || ''))
      setFileName(file.name)
    }
    reader.readAsText(file)
    return false // 阻止 antd 自动上传
  }

  const doImport = async () => {
    let parsed: any
    try {
      parsed = JSON.parse(text)
    } catch {
      message.error(t('apidef.invalidJson', '不是合法 JSON(上传文件或粘贴 OpenAPI/Swagger 文档)'))
      return
    }
    setSaving(true)
    try {
      const r = await api.importDefinitions(projectId, parsed)
      let caseCount = 0
      if (genCases && r.created.length) {
        // 按 schema 自动为每个导入的接口生成一条用例(必填 query + 示例 body)。
        const ops = parseOperations(parsed)
        await Promise.all(
          r.created.map(async (d) => {
            const op = ops[`${d.method} ${d.path}`]
            try {
              await api.createCase(d.id, {
                name: op?.summary || d.name,
                method: d.method,
                url: buildCaseUrl(d.path, op),
                body: op?.bodyExample,
                assertions: [{ type: 'StatusIs', args: 200 }],
              })
              caseCount++
            } catch {
              /* 单条失败不阻断 */
            }
          }),
        )
      }
      message.success(
        `${t('apidef.importSuccessNew', '导入成功:新增接口')} ${r.created.length}${t('apidef.importSkipped', ',跳过')} ${r.skipped}${genCases ? `${t('apidef.importGenCases', ',生成用例')} ${caseCount}` : ''}`,
      )
      setText('')
      setFileName('')
      onDone()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('apidef.importFailed', '导入失败'))
    } finally {
      setSaving(false)
    }
  }

  return (
    <Modal
      title={t('apidef.importTitle', '导入接口(OpenAPI 3.x / Swagger 2.0)')}
      open={open}
      onCancel={onClose}
      confirmLoading={saving}
      destroyOnHidden
      width={680}
      okText={t('a.import', '导入')}
      okButtonProps={{ disabled: !text.trim() }}
      onOk={doImport}
    >
      <Upload.Dragger accept=".json,.yaml,.yml" beforeUpload={readFile} showUploadList={false} style={{ marginBottom: 12 }}>
        <p style={{ margin: 0 }}>
          <InboxOutlined style={{ fontSize: 28, color: '#7c3aed' }} />
        </p>
        <p style={{ margin: '6px 0 0' }}>{t('apidef.uploadHint', '点击或拖拽 OpenAPI/Swagger JSON 文件到此')}</p>
        {fileName && <p style={{ color: '#7c3aed', margin: '4px 0 0' }}>{t('apidef.selectedFile', '已选:')}{fileName}</p>}
      </Upload.Dragger>
      <Checkbox checked={genCases} onChange={(e) => setGenCases(e.target.checked)} style={{ marginBottom: 8 }}>
        {t('apidef.genCasesHint', '按 schema 自动生成用例(填充必填参数 + 示例请求体,免手敲)')}
      </Checkbox>
      <Input.TextArea
        rows={8}
        value={text}
        onChange={(e) => setText(e.target.value)}
        placeholder={t('apidef.pasteHint', '也可直接粘贴文档内容')}
        className="ms-mono"
      />
    </Modal>
  )
}
