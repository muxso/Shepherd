import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { Button, Checkbox, Divider, Drawer, Dropdown, Empty, Input, Table, Modal, Popover, Radio, Segmented, Select, Space, Switch, Tabs, Tag, Tooltip, Tree, Upload } from 'antd'
import { useSearchParams } from 'react-router-dom'
import { message, modal } from '../feedback'
import {
  PlusOutlined,
  ImportOutlined,
  ReloadOutlined,
  ApiOutlined,
  FolderOutlined,
  MoreOutlined,
  InboxOutlined,
  CopyOutlined,
  QuestionCircleOutlined,
  SaveOutlined,
  FilterOutlined,
  ThunderboltOutlined,
  CodeOutlined,
  DownOutlined,
  EyeOutlined,
  EyeInvisibleOutlined,
  UnorderedListOutlined,
  MinusSquareOutlined,
  SearchOutlined,
  SendOutlined,
} from '@ant-design/icons'
import type { ColumnsType } from 'antd/es/table'
import { api, ApiError, userIdStore, type ApiCase, type ApiDefinition, type ApiModule, type ApiSpec, type DebugResponse, type Environment, type ImportFormat, type ImportSchedule, type ProjectMock } from '../api'
import { columnSearch, useListView, type ListColumn } from '../components/ListView'
import { useApp } from '../context'
import { methodColor, statusColor } from '../components/tags'
import CasesPanel from './CasesPanel'
import MocksPanel from './MocksPanel'
import ApiSpecPanel, { DebugResultPanel, emptySpec, parseCurl, type ApiSpecPanelHandle, type ExecMode, type SentRequest } from '../components/ApiSpecPanel'
import KVEditor, { type KVRow } from '../components/KVEditor'
import AssertionEditor from '../components/AssertionEditor'
import ReferencesPanel from '../components/ReferencesPanel'
import ChangeHistoryPanel from '../components/ChangeHistoryPanel'
import { useOpenParam, ResizableSider } from '../components/Workspace'
import { useI18n } from '../i18n'

const METHODS = ['GET', 'POST', 'PUT', 'DELETE', 'PATCH']
const PROTOCOLS = ['HTTP', 'GRPC', 'SQL', 'REDIS', 'WEBSOCKET']
const API_STATUSES = ['DRAFT', 'DEBUGGING', 'COMPLETED', 'DEPRECATED']
const LIST_KEY = '__list__'
const NEW_KEY = '__new__'

// Case statuses: values are persisted to the backend (stay Chinese); labels are translated via t().
const CASE_STATUSES = ['进行中', '已完成', '已废弃']
const caseStatusKey = (s: string): string =>
  s === '进行中' ? 'apidef.caseStInProgress' : s === '已完成' ? 'apidef.caseStCompleted' : 'apidef.caseStDeprecated'

/** Render server timestamp ("2026-06-21 12:34:56.78+00") as "2026-06-21 12:34:56"; empty/unparsable falls back to "—". */
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
  const [moduleKey, setModuleKey] = useState('ALL') // ALL | UNFILED | <moduleId>
  const [creating, setCreating] = useState(false)
  const [importOpen, setImportOpen] = useState(false)
  // List view mode: API (definitions) / CASE (project cases) / MOCK (project mocks).
  const [viewMode, setViewMode] = useState<'API' | 'CASE' | 'MOCK'>('API')
  const [caseRows, setCaseRows] = useState<ApiCase[]>([])
  const [mockRows, setMockRows] = useState<ProjectMock[]>([])
  const [viewLoading, setViewLoading] = useState(false)
  const [moduleForm, setModuleForm] = useState<{ mode: 'create' | 'rename'; id?: string; parentId?: string | null; name?: string } | null>(null)
  const [openIds, setOpenIds] = useState<string[]>([])
  const [openCases, setOpenCases] = useState<Record<string, ApiCase>>({}) // open case-detail tabs (key = caseId)
  const [activeKey, setActiveKey] = useState(LIST_KEY)
  const [searchParams, setSearchParams] = useSearchParams()
  // Module tree: search / show interfaces in tree / hide empty modules / protocol filter / controlled expand.
  const [moduleSearch, setModuleSearch] = useState('')
  const [showInterfaces, setShowInterfaces] = useState(false)
  const [hideEmpty, setHideEmpty] = useState(false)
  const [protoFilter, setProtoFilter] = useState<string[]>([])
  const [treeExpanded, setTreeExpanded] = useState<string[]>(['ALL'])

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

  // Lazy-load project-level cases / mocks when switching to CASE/MOCK view.
  useEffect(() => {
    if (viewMode === 'API') return
    let alive = true
    setViewLoading(true)
    const p = viewMode === 'CASE' ? api.projectCases(projectId).then((r) => alive && setCaseRows(r.items)) : api.projectMocks(projectId).then((r) => alive && setMockRows(r))
    p.catch(() => undefined).finally(() => alive && setViewLoading(false))
    return () => { alive = false }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [viewMode, projectId])

  // Deep link ?view=<id> is handled by useListView (clears the param after applying); do not parse it here again or it applies twice.

  useEffect(() => {
    load()
    setOpenIds([])
    setActiveKey(LIST_KEY)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId])

  // Protocol filter options come from protocols actually present in this project's definitions.
  const protoOptions = useMemo(() => Array.from(new Set(defs.map((d) => d.protocol).filter(Boolean))).sort(), [defs])

  // Module tree: All APIs > [Unfiled] + modules (nested via parentId); optional interface leaves (method tag),
  // with name search / protocol filter / hide-empty / subtree counts.
  const treeData = useMemo(() => {
    const lc = (s: string) => s.toLowerCase()
    const matchProto = (d: ApiDefinition) => protoFilter.length === 0 || protoFilter.includes(d.protocol)
    const matchText = (d: ApiDefinition) => !moduleSearch || lc(d.name).includes(lc(moduleSearch)) || lc(d.path).includes(lc(moduleSearch))
    const childModulesOf = (mid: string | null) => modules.filter((x) => (x.parentId || null) === mid)
    const subtreeCount = (mid: string): number =>
      defs.filter((d) => d.moduleId === mid && matchProto(d)).length +
      childModulesOf(mid).reduce((n, c) => n + subtreeCount(c.id), 0)
    const ifaceLeaf = (d: ApiDefinition) => ({ key: `api:${d.id}`, isLeaf: true, selectable: true, title: <InterfaceLeaf d={d} /> })
    const moduleNode = (m: ApiModule): any => {
      const subModules = childModulesOf(m.id).map(moduleNode).filter(Boolean)
      const leaves = showInterfaces ? defs.filter((d) => d.moduleId === m.id && matchProto(d) && matchText(d)).map(ifaceLeaf) : []
      const children = [...subModules, ...leaves]
      const total = subtreeCount(m.id)
      const nameMatch = !moduleSearch || lc(m.name).includes(lc(moduleSearch))
      if (moduleSearch && !nameMatch && children.length === 0) return null // hide non-matching nodes while searching
      if (hideEmpty && total === 0) return null
      return {
        key: m.id,
        title: <ModuleTitle name={m.name} count={total} onAction={(a) => onModuleAction(a, m)} />,
        children: children.length ? children : undefined,
      }
    }
    const roots = childModulesOf(null).map(moduleNode).filter(Boolean)
    const unfiledDefs = defs.filter((d) => !d.moduleId && matchProto(d))
    const unfiledLeaves = showInterfaces ? unfiledDefs.filter(matchText).map(ifaceLeaf) : []
    const showUnfiled = !hideEmpty || unfiledDefs.length > 0
    const allCount = defs.filter(matchProto).length
    return [
      {
        key: 'ALL',
        title: `${t('apidef.allApis', '全部接口')} (${allCount})`,
        icon: <ApiOutlined />,
        children: [
          ...(showUnfiled ? [{ key: 'UNFILED', title: `${t('apidef.unfiled', '未归类')} (${unfiledDefs.length})`, icon: <InboxOutlined />, children: unfiledLeaves.length ? unfiledLeaves : undefined }] : []),
          ...roots,
        ],
      },
    ]
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [defs, modules, showInterfaces, moduleSearch, protoFilter, hideEmpty])

  // All expandable keys, for collapse-all / expand-all.
  const allExpandableKeys = useMemo(() => ['ALL', ...modules.map((m) => m.id)], [modules])

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

  const openDef = (id: string) => {
    setOpenIds((ids) => (ids.includes(id) ? ids : [...ids, id]))
    setActiveKey(id)
  }
  // Clicking a CASE row opens its detail tab; the case object is captured at open time so the tab survives view switches.
  const openCase = (c: ApiCase) => {
    setOpenCases((m) => ({ ...m, [c.id]: c }))
    setActiveKey(`case:${c.id}`)
  }
  useOpenParam(openDef) // ?open=<definitionId> deep link
  // ?openCase=<caseId> deep link (case click in the reference graph): fetch project cases, then open the case's detail tab.
  useEffect(() => {
    const cid = searchParams.get('openCase')
    if (!cid || !projectId) return
    api
      .projectCasesAll(projectId)
      .then((cs) => { const c = cs.find((x) => x.id === cid); if (c) openCase(c) })
      .finally(() => { const next = new URLSearchParams(searchParams); next.delete('openCase'); setSearchParams(next, { replace: true }) })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchParams, projectId])
  const closeTab = (id: string) => {
    if (id === NEW_KEY) {
      setCreating(false)
      setActiveKey((cur) => (cur === NEW_KEY ? LIST_KEY : cur))
      return
    }
    if (id.startsWith('case:')) {
      const cid = id.slice(5)
      setOpenCases((m) => { const n = { ...m }; delete n[cid]; return n })
      setActiveKey((cur) => (cur === id ? LIST_KEY : cur))
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

  const removeDef = (d: ApiDefinition) => {
    Modal.confirm({
      title: t('apidef.deleteConfirmTitle', '删除接口定义?'),
      content: t('apidef.deleteConfirmBody', '将删除该接口及其用例 / Mock,且不可恢复。'),
      okType: 'danger',
      okText: t('a.delete', '删除'),
      cancelText: t('a.cancel', '取消'),
      onOk: async () => {
        try {
          await api.deleteDefinition(d.id)
          message.success(t('apidef.deleted', '已删除'))
          load()
        } catch (e) {
          message.error(e instanceof ApiError ? e.message : t('apidef.deleteFailed', '删除失败'))
        }
      },
    })
  }

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

  const allColumns: ListColumn<ApiDefinition>[] = [
    { key: 'num', label: 'ID', title: 'ID', dataIndex: 'num', width: 90, ...columnSearch<ApiDefinition>((d) => `${d.num ?? ''} ${d.id}`, t), render: (num: number | undefined, d) => <span className="ms-mono" style={{ color: 'var(--text-3)', fontSize: 12 }} title={d.id}>{num ?? '—'}</span> },
    { key: 'name', label: t('apidef.colName', '名称'), title: t('apidef.colName', '名称'), dataIndex: 'name', ellipsis: true, ...columnSearch<ApiDefinition>((d) => d.name, t), render: (name: string) => <span style={{ fontWeight: 500 }}>{name}</span> },
    {
      key: 'protocol', label: t('apidef.protocol', '协议'), title: t('apidef.protocol', '协议'), dataIndex: 'protocol', width: 100,
      filters: PROTOCOLS.map((p) => ({ text: p, value: p })),
      onFilter: (v, d) => d.protocol === v,
      render: (p: string) => <Tag>{p}</Tag>,
    },
    {
      key: 'method', label: t('apidef.reqType', '请求类型'), title: t('apidef.reqType', '请求类型'), dataIndex: 'method', width: 110,
      filters: METHODS.map((m) => ({ text: m, value: m })),
      onFilter: (v, d) => d.method === v,
      render: (m: string) => <Tag color={methodColor(m)} style={{ fontWeight: 600 }}>{m || '—'}</Tag>,
    },
    { key: 'path', label: t('apidef.colPath', '路径'), title: t('apidef.colPath', '路径'), dataIndex: 'path', ellipsis: true, ...columnSearch<ApiDefinition>((d) => d.path || '', t), render: (p: string) => <span className="ms-mono" style={{ color: 'var(--text-2)' }}>{p || '—'}</span> },
    {
      key: 'status', label: t('apidef.colStatus', '状态'), title: t('apidef.colStatus', '状态'), dataIndex: 'status', width: 100,
      filters: API_STATUSES.map((s) => ({ text: s, value: s })),
      onFilter: (v, d) => d.status === v,
      render: (s: string) => <Tag color={statusColor(s)}>{s}</Tag>,
    },
    {
      key: 'module', label: t('apidef.colModule', '模块'), title: t('apidef.colModule', '模块'), dataIndex: 'moduleId', width: 120,
      filters: [{ text: t('apidef.unfiled', '未归类'), value: '__unfiled__' }, ...modules.map((m) => ({ text: m.name, value: m.id }))],
      onFilter: (v, d) => (v === '__unfiled__' ? !d.moduleId : d.moduleId === v),
      render: (mid?: string | null) => {
        const m = modules.find((x) => x.id === mid)
        return m ? <Tag color="geekblue">{m.name}</Tag> : <span style={{ color: 'var(--text-3)' }}>{t('apidef.unfiled', '未归类')}</span>
      },
    },
    {
      key: 'tags', label: t('apidef.tags', '标签'), title: t('apidef.tags', '标签'), dataIndex: 'spec', width: 140,
      filters: [...new Set(defs.flatMap((d) => d.spec?.tags || []))].map((tg) => ({ text: tg, value: tg })),
      onFilter: (v, d) => (d.spec?.tags || []).includes(v as string),
      render: (spec?: ApiDefinition['spec']) => {
        const tags = spec?.tags || []
        return tags.length ? <Space size={[2, 2]} wrap>{tags.map((tg) => <Tag key={tg} style={{ margin: 0 }}>{tg}</Tag>)}</Space> : <span style={{ color: 'var(--text-3)' }}>—</span>
      },
    },
    { key: 'createdBy', label: t('apidef.colCreatedBy', '创建人'), title: t('apidef.colCreatedBy', '创建人'), dataIndex: 'createdBy', width: 110, ellipsis: true, ...columnSearch<ApiDefinition>((d) => d.createdBy || '', t), render: (u?: string) => u ? <span style={{ color: 'var(--text-2)' }}>{u}</span> : <span style={{ color: 'var(--text-3)' }}>—</span> },
    { key: 'createdAt', label: t('apidef.colCreatedAt', '创建时间'), title: t('apidef.colCreatedAt', '创建时间'), dataIndex: 'createdAt', width: 160, render: (ts?: string) => <span style={{ color: 'var(--text-3)', fontSize: 12 }}>{fmtTs(ts)}</span> },
    { key: 'updatedAt', label: t('apidef.updatedAt', '更新时间'), title: t('apidef.updatedAt', '更新时间'), dataIndex: 'updatedAt', width: 160, render: (ts?: string) => <span style={{ color: 'var(--text-3)', fontSize: 12 }}>{fmtTs(ts)}</span> },
    {
      key: 'action', label: t('apidef.colAction', '操作'), title: t('apidef.colAction', '操作'), width: 150, fixed: 'right',
      render: (_, d) => (
        <Space size={0} onClick={(e) => e.stopPropagation()}>
          <Button type="link" size="small" onClick={() => openDef(d.id)}>{t('a.edit', '编辑')}</Button>
          <Button type="link" size="small" onClick={() => openDef(d.id)}>{t('apidef.run', '执行')}</Button>
          <Button type="link" size="small" onClick={() => duplicate(d)}>{t('apidef.duplicate', '复制')}</Button>
          <Dropdown
            trigger={['click']}
            menu={{
              items: [
                { key: 'UNFILED', label: t('apidef.moveToUnfiled', '移到「未归类」') },
                ...modules.map((m) => ({ key: m.id, label: `${t('apidef.moveToPrefix', '移到')}「${m.name}」` })),
                { type: 'divider' as const },
                { key: '__delete__', danger: true, label: t('a.delete', '删除') },
              ],
              onClick: ({ key }) => {
                if (key === '__delete__') removeDef(d)
                else move(d, key === 'UNFILED' ? null : key)
              },
            }}
          >
            <Button type="link" size="small">{t('a.more', '更多')}</Button>
          </Dropdown>
        </Space>
      ),
    },
  ]
  // List toolbar (views/filters/columns/page size/advanced conditions) is delegated to useListView; moduleKey is page-private state stored in view extra.
  const lv = useListView<ApiDefinition>({
    kind: 'apidef',
    projectId,
    searchLabel: t('apidef.searchPlaceholder', '搜索名称 / 路径'),
    searchOf: (d) => `${d.num ?? ''} ${d.name} ${d.path}`,
    // Views saved without a kind also belong to this page; scenario views (kind === 'scenario') are excluded.
    matchKind: (k) => k === 'apidef' || k === undefined,
    systemViews: [
      { key: 'mine', label: t('lv.mine', '我创建的'), pred: (d) => !!d.createdBy && d.createdBy === userIdStore.get() },
    ],
    fields: [
      { key: 'protocol', label: t('apidef.protocol', '协议'), type: 'enum', options: PROTOCOLS.map((p) => ({ value: p, label: p })), get: (d) => d.protocol },
      { key: 'method', label: t('apidef.reqType', '请求类型'), type: 'enum', options: METHODS.map((m) => ({ value: m, label: m })), get: (d) => d.method },
      { key: 'status', label: t('apidef.colStatus', '状态'), type: 'enum', options: API_STATUSES.map((s) => ({ value: s, label: s })), get: (d) => d.status },
      // Below: advanced-condition picker only (duplicates search box / column filters, so not rendered in the declarative filter bar).
      { key: 'num', label: 'ID', type: 'text', advOnly: true, get: (d) => String(d.num ?? '') },
      { key: 'name', label: t('apidef.colName', '名称'), type: 'text', advOnly: true, get: (d) => d.name },
      { key: 'path', label: t('apidef.colPath', '路径'), type: 'text', advOnly: true, get: (d) => d.path },
    ],
    columns: allColumns,
    rows: defs,
    extra: {
      get: () => ({ moduleKey }),
      apply: (v) => {
        if (typeof v.moduleKey === 'string') setModuleKey(v.moduleKey)
      },
    },
  })
  // Module-tree selection filters on top of useListView filtering.
  const visible = lv.rows.filter((d) => (moduleKey === 'ALL' ? true : moduleKey === 'UNFILED' ? !d.moduleId : d.moduleId === moduleKey))

  const listTab = (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '10px 12px', borderBottom: '1px solid var(--border-soft)' }}>
        {/* View mode switch: API / CASE / MOCK (ref: top-left). */}
        <Dropdown
          trigger={['click']}
          menu={{ items: (['API', 'CASE', 'MOCK'] as const).map((m) => ({ key: m, label: m })), onClick: ({ key }) => setViewMode(key as 'API' | 'CASE' | 'MOCK') }}
        >
          <Button>{viewMode} <DownOutlined /></Button>
        </Dropdown>
        <span style={{ fontWeight: 600, color: 'var(--brand)' }}>
          {viewMode === 'API' ? t('apidef.allApis2', '全部') : viewMode === 'CASE' ? t('apidef.allCases', '全部用例') : t('apidef.allMocks', '全部 MOCK')}
        </span>
        <div style={{ flex: 1 }} />
        {/* Toolbar (search/views/filters/columns) renders only in API mode; CASE/MOCK have no search/filter. */}
        {viewMode === 'API' && lv.toolbar}
        <Button icon={<ReloadOutlined />} onClick={load} />
      </div>
      <div style={{ flex: 1, overflow: 'auto', padding: 12 }}>
        {viewMode === 'API' ? (
          <Table<ApiDefinition>
            rowKey="id"
            size="middle"
            loading={loading}
            dataSource={visible}
            columns={lv.columns}
            scroll={{ x: 'max-content' }}
            onRow={(d) => ({ onClick: () => openDef(d.id), style: { cursor: 'pointer' } })}
            pagination={{ ...lv.pagination, showTotal: (total) => `${t('apidef.totalPrefix', '共')} ${total} ${t('apidef.totalSuffix', '个接口')}` }}
            locale={{ emptyText: <Empty description={t('apidef.emptyApis', '暂无接口')} /> }}
          />
        ) : viewMode === 'CASE' ? (
          <Table<ApiCase>
            rowKey="id"
            size="middle"
            loading={viewLoading}
            dataSource={caseRows}
            scroll={{ x: 'max-content' }}
            onRow={(c) => ({ onClick: () => openCase(c), style: { cursor: 'pointer' } })}
            columns={[
              { title: 'ID', dataIndex: 'id', width: 120, render: (v: string) => <a className="ms-mono" style={{ fontSize: 12 }}>{v.slice(0, 8)}</a> },
              { title: t('apidef.colName', '名称'), dataIndex: 'name', ellipsis: true },
              { title: t('apidef.colMethod', '请求类型'), dataIndex: 'method', width: 100, render: (m: string) => <Tag>{m}</Tag> },
              { title: 'URL', dataIndex: 'url', ellipsis: true, render: (v: string) => <span className="ms-mono" style={{ fontSize: 12 }}>{v}</span> },
              { title: t('apidef.colPriority', '优先级'), dataIndex: 'priority', width: 90, render: (v?: string) => v || '—' },
              { title: t('apidef.colStatus', '状态'), dataIndex: 'status', width: 110, render: (v?: string) => v || '—' },
            ]}
            pagination={{ ...lv.pagination, showTotal: (total) => `${t('apidef.totalPrefix', '共')} ${total} ${t('apidef.caseUnit', '个用例')}` }}
            locale={{ emptyText: <Empty description={t('apidef.emptyCases', '暂无用例')} /> }}
          />
        ) : (
          <Table<ProjectMock>
            rowKey="id"
            size="middle"
            loading={viewLoading}
            dataSource={mockRows}
            scroll={{ x: 'max-content' }}
            columns={[
              { title: 'ID', dataIndex: 'id', width: 120, render: (v: string) => <span className="ms-mono" style={{ fontSize: 12 }}>{v.slice(0, 8)}</span> },
              { title: t('apidef.mockName', '期望名称'), dataIndex: 'name', ellipsis: true },
              { title: t('apidef.colMethod', '请求类型'), dataIndex: 'method', width: 100, render: (m: string) => <Tag>{m}</Tag> },
              { title: t('apidef.colProtocol', '协议'), dataIndex: 'protocol', width: 90 },
              { title: t('apidef.colPath', '接口路径'), dataIndex: 'path', ellipsis: true, render: (v: string) => <span className="ms-mono" style={{ fontSize: 12 }}>{v}</span> },
              { title: t('apidef.colTags', '标签'), dataIndex: 'tags', width: 160, render: (tags?: string[]) => (tags?.length ? tags.map((tg) => <Tag key={tg}>{tg}</Tag>) : '—') },
              { title: t('apidef.colStatus', '状态'), dataIndex: 'enabled', width: 90, render: (on: boolean) => <Switch size="small" checked={on} disabled /> },
              { title: t('apidef.colOperator', '操作人'), dataIndex: 'operator', width: 110, render: (v?: string) => v || <span style={{ color: 'var(--text-3)' }}>—</span> },
              { title: t('apidef.colUpdatedAt', '更新时间'), dataIndex: 'updatedAt', width: 160, render: (v?: string) => <span style={{ color: 'var(--text-3)', fontSize: 12 }}>{v ? v.slice(0, 19) : '—'}</span> },
              {
                title: t('apidef.colAction', '操作'),
                width: 170,
                fixed: 'right',
                render: (_v, m: ProjectMock) => (
                  <Space size={0} onClick={(e) => e.stopPropagation()}>
                    <Button type="link" size="small" onClick={() => message.info(t('apidef.mockEditSoon', '编辑 Mock:在所属接口的 MOCK 标签维护'))}>{t('a.edit', '编辑')}</Button>
                    <Dropdown
                      menu={{
                        items: [
                          { key: 'copy', label: t('apidef.copyMockUrl', '复制 Mock 地址') },
                          { key: 'del', danger: true, label: t('a.delete', '删除') },
                        ],
                        onClick: ({ key }) => {
                          if (key === 'copy') { navigator.clipboard?.writeText(`/mock${m.path}`); message.success(t('apidef.copied', '已复制')) }
                          else modal.confirm({
                            title: t('apidef.mockDeleteConfirm', '删除该 Mock?'),
                            okType: 'danger', okText: t('a.delete', '删除'), cancelText: t('a.cancel', '取消'),
                            onOk: async () => {
                              try { await api.deleteMock(m.id); message.success(t('apidef.deleted', '已删除')); setMockRows((rows) => rows.filter((x) => x.id !== m.id)) }
                              catch (e) { message.error(e instanceof ApiError ? e.message : t('apidef.deleteFailed', '删除失败')) }
                            },
                          })
                        },
                      }}
                    >
                      <Button type="link" size="small">···</Button>
                    </Dropdown>
                  </Space>
                ),
              },
            ]}
            pagination={{ ...lv.pagination, showTotal: (total) => `${t('apidef.totalPrefix', '共')} ${total} ${t('apidef.mockUnit', '个 Mock')}` }}
            locale={{ emptyText: <Empty description={t('apidef.emptyMocks', '暂无 Mock')} /> }}
          />
        )}
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
        children: <ApiDetail definition={d} onUpdated={load} />,
      })),
    ...Object.values(openCases).map((c) => ({
      key: `case:${c.id}`,
      label: (
        <Space size={4}>
          <Tag color={methodColor(c.method)} style={{ margin: 0 }}>{c.method}</Tag>
          <span style={{ maxWidth: 120, display: 'inline-block', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', verticalAlign: 'middle' }}>{c.name}</span>
        </Space>
      ),
      children: <CaseDetailTab caseItem={c} projectId={projectId} onClose={() => closeTab(`case:${c.id}`)} onDeleted={() => api.projectCases(projectId).then((r) => setCaseRows(r.items)).catch(() => undefined)} />,
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
      <ResizableSider defaultWidth={252} storageKey="apidef-sider">
        {/* Header: Add API / Import (full-width Compact button pair, matching the scenario page's left panel). */}
        <div style={{ padding: '10px 10px 0' }}>
          <Space.Compact style={{ width: '100%' }}>
            <Button type="primary" icon={<PlusOutlined />} style={{ flex: 1 }} disabled={viewMode !== 'API'} onClick={() => { setCreating(true); setActiveKey(NEW_KEY) }}>{t('apidef.addApi', '添加接口')}</Button>
            <Button icon={<ImportOutlined />} style={{ flex: 1 }} disabled={viewMode !== 'API'} onClick={() => setImportOpen(true)}>{t('a.import', '导入')}</Button>
          </Space.Compact>
        </div>
        {/* Search: module / API name (path). */}
        <div style={{ padding: '10px 10px 6px' }}>
          <Input
            allowClear
            size="small"
            prefix={<SearchOutlined style={{ color: 'var(--text-3)' }} />}
            placeholder={t('apidef.moduleSearch', '请输入模块/接口名称')}
            value={moduleSearch}
            onChange={(e) => setModuleSearch(e.target.value)}
          />
        </div>
        {/* Toolbar: hide empty / show interfaces / collapse all / protocol filter / new module. "All APIs (N)" lives on the tree root, not repeated here. */}
        <div style={{ display: 'flex', alignItems: 'center', gap: 2, padding: '0 8px 8px', borderBottom: '1px solid var(--border-soft)' }}>
          <div style={{ flex: 1 }} />
          <Tooltip title={hideEmpty ? t('apidef.showEmpty', '显示空模块') : t('apidef.hideEmpty', '隐藏空模块')}>
            <Button size="small" type="text" icon={hideEmpty ? <EyeInvisibleOutlined /> : <EyeOutlined />} onClick={() => setHideEmpty((v) => !v)} />
          </Tooltip>
          <Tooltip title={showInterfaces ? t('apidef.hideIfaces', '隐藏接口') : t('apidef.showIfaces', '树内显示接口')}>
            <Button size="small" type="text" icon={<UnorderedListOutlined />} style={{ color: showInterfaces ? 'var(--success)' : undefined }} onClick={() => setShowInterfaces((v) => !v)} />
          </Tooltip>
          <Tooltip title={treeExpanded.length ? t('apidef.collapseAll', '收起全部') : t('apidef.expandAll', '展开全部')}>
            <Button size="small" type="text" icon={<MinusSquareOutlined />} onClick={() => setTreeExpanded(treeExpanded.length ? [] : allExpandableKeys)} />
          </Tooltip>
          <Popover
            trigger="click"
            placement="bottomRight"
            title={t('apidef.protoFilter', '协议过滤')}
            content={
              <div style={{ width: 150 }}>
                <Checkbox checked={protoFilter.length === 0} onChange={(e) => e.target.checked && setProtoFilter([])}>{t('apidef.allProtos', '全部')}</Checkbox>
                <Divider style={{ margin: '8px 0' }} />
                <Space direction="vertical" size={6} style={{ width: '100%' }}>
                  {protoOptions.map((p) => (
                    <Checkbox key={p} checked={protoFilter.includes(p)} onChange={(e) => setProtoFilter((prev) => (e.target.checked ? [...prev, p] : prev.filter((x) => x !== p)))}>{p}</Checkbox>
                  ))}
                </Space>
              </div>
            }
          >
            <Tooltip title={t('apidef.protoFilter', '协议过滤')}>
              <Button size="small" type="text" icon={<FilterOutlined />} style={{ color: protoFilter.length ? 'var(--brand)' : undefined }} />
            </Tooltip>
          </Popover>
          <Tooltip title={t('apidef.newTopModule', '新建顶层模块')}>
            <Button size="small" type="text" icon={<PlusOutlined />} style={{ color: 'var(--success)' }} onClick={() => setModuleForm({ mode: 'create', parentId: null })} />
          </Tooltip>
        </div>
        <div style={{ flex: 1, overflow: 'auto', padding: 8 }}>
          <Tree
            showIcon
            blockNode
            expandedKeys={treeExpanded}
            onExpand={(keys) => setTreeExpanded(keys.map(String))}
            selectedKeys={[moduleKey]}
            treeData={treeData}
            onSelect={(keys) => {
              const k = String(keys[0] ?? '')
              if (!k) return
              if (k.startsWith('api:')) openDef(k.slice(4))
              else { setModuleKey(k); setActiveKey(LIST_KEY) }
            }}
          />
        </div>
      </ResizableSider>

      <div style={{ flex: 1, minWidth: 0, background: 'var(--panel)' }}>
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

      <ImportModal open={importOpen} onClose={() => setImportOpen(false)} projectId={projectId} modules={modules} onDone={() => { setImportOpen(false); load() }} />

      <ModuleFormModal
        state={moduleForm}
        projectId={projectId}
        onClose={() => setModuleForm(null)}
        onDone={() => { setModuleForm(null); load() }}
      />
    </div>
  )
}

/** Interface leaf in the tree: method tag + name (click opens detail). */
function InterfaceLeaf({ d }: { d: ApiDefinition }) {
  return (
    <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6, width: '100%', minWidth: 0 }} title={d.name}>
      <Tag color={methodColor(d.method)} style={{ margin: 0, fontWeight: 600, fontSize: 11, lineHeight: '16px', padding: '0 5px', flexShrink: 0 }}>
        {(d.protocol === 'HTTP' ? d.method : d.protocol) || 'GET'}
      </Tag>
      <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', fontSize: 13 }}>{d.name}</span>
    </span>
  )
}

function ModuleTitle({ name, count, onAction }: { name: string; count?: number; onAction: (a: string) => void }) {
  const { t } = useI18n()
  return (
    <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6, width: '100%', minWidth: 0 }}>
      <FolderOutlined style={{ color: 'var(--text-3)', flexShrink: 0 }} />
      <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{name}</span>
      {count != null && <span style={{ color: 'var(--text-3)', fontSize: 12, flexShrink: 0 }}>{count}</span>}
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
        <MoreOutlined onClick={(e) => e.stopPropagation()} style={{ padding: '0 4px', color: 'var(--text-3)' }} />
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

// Case detail tab (opened via CASE row ID, ref mock): header + request headers/body-JSON + response + server run.
// Case edit drawer (ref #22): name + priority/status/tags + headers/body/query/REST/assertions/auth → PUT /api/case/{id}.
function CaseEditDrawer({ open, caseItem, onClose, onSaved }: { open: boolean; caseItem: ApiCase; onClose: () => void; onSaved: (updated: ApiCase) => void }) {
  const { t } = useI18n()
  const blankKv = (): KVRow => ({ key: '', value: '' })
  const [name, setName] = useState(caseItem.name)
  const [method, setMethod] = useState(caseItem.method)
  const [url, setUrl] = useState(caseItem.url)
  const [priority, setPriority] = useState(caseItem.priority || 'P0')
  const [status, setStatus] = useState(caseItem.status || '进行中')
  const [tags, setTags] = useState<string[]>(caseItem.tags || [])
  const [tagInput, setTagInput] = useState('')
  const [headers, setHeaders] = useState<KVRow[]>(caseItem.headers?.length ? caseItem.headers : [blankKv()])
  const [query, setQuery] = useState<KVRow[]>(caseItem.queryParams?.length ? caseItem.queryParams : [blankKv()])
  const [rest, setRest] = useState<KVRow[]>(caseItem.restParams?.length ? caseItem.restParams : [blankKv()])
  const [body, setBody] = useState(caseItem.body || '')
  const [authType, setAuthType] = useState<'none' | 'bearer' | 'basic'>((caseItem.auth?.type as 'none' | 'bearer' | 'basic') || 'none')
  const [authToken, setAuthToken] = useState(caseItem.auth?.token || '')
  const [assertions, setAssertions] = useState<unknown[]>((caseItem.assertions as unknown[]) || [])
  const [saving, setSaving] = useState(false)

  // Reset the form from the latest case values when reopening or switching cases.
  useEffect(() => {
    if (!open) return
    setName(caseItem.name); setMethod(caseItem.method); setUrl(caseItem.url)
    setPriority(caseItem.priority || 'P0'); setStatus(caseItem.status || '进行中'); setTags(caseItem.tags || [])
    setHeaders(caseItem.headers?.length ? caseItem.headers : [blankKv()])
    setQuery(caseItem.queryParams?.length ? caseItem.queryParams : [blankKv()])
    setRest(caseItem.restParams?.length ? caseItem.restParams : [blankKv()])
    setBody(caseItem.body || ''); setAuthType((caseItem.auth?.type as 'none' | 'bearer' | 'basic') || 'none')
    setAuthToken(caseItem.auth?.token || ''); setAssertions((caseItem.assertions as unknown[]) || [])
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, caseItem])

  const clean = (rows: KVRow[]) => rows.filter((r) => r.key.trim())
  const save = async () => {
    if (!name.trim()) return message.warning(t('apidef.caseNameRequired', '请输入用例名称'))
    if (!url.trim()) return message.warning(t('editor.urlRequired', '请输入 URL'))
    const auth = authType === 'none' ? { type: 'none' } : { type: authType, token: authToken }
    const payload = { name, method, url, body: body || null, assertions, processors: caseItem.processors, priority, status, tags, headers: clean(headers), queryParams: clean(query), restParams: clean(rest), auth }
    setSaving(true)
    try {
      await api.updateCase(caseItem.id, payload)
      message.success(t('apidef.caseUpdated', '用例已更新'))
      onSaved({ ...caseItem, ...payload, body: body || null, auth })
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('apidef.saveFailed', '保存失败'))
    } finally {
      setSaving(false)
    }
  }

  const kvTab = (key: string, label: string, rows: KVRow[], set: (r: KVRow[]) => void) => ({ key, label: `${label}${clean(rows).length ? ` (${clean(rows).length})` : ''}`, children: <KVEditor rows={rows} onChange={set} /> })
  const tabs = [
    kvTab('headers', t('apidef.requestHeaders', '请求头'), headers, setHeaders),
    { key: 'body', label: t('apidef.requestBody', '请求体'), children: <Input.TextArea rows={8} value={body} onChange={(e) => setBody(e.target.value)} className="ms-mono" placeholder='{"k":"v"}' /> },
    kvTab('query', 'Query', query, setQuery),
    kvTab('rest', 'REST', rest, setRest),
    { key: 'assert', label: `${t('apidef.assertions', '断言')}${assertions.length ? ` (${assertions.length})` : ''}`, children: <AssertionEditor value={assertions as never} onChange={(v) => setAssertions(v)} /> },
    { key: 'auth', label: t('apidef.auth', '认证'), children: (
      <Space direction="vertical" style={{ width: '100%' }}>
        <Select value={authType} onChange={setAuthType} style={{ width: 200 }} options={[{ value: 'none', label: t('apidef.authNone', '无') }, { value: 'bearer', label: 'Bearer Token' }, { value: 'basic', label: 'Basic Auth' }]} />
        {authType !== 'none' && <Input value={authToken} onChange={(e) => setAuthToken(e.target.value)} placeholder="token" className="ms-mono" />}
      </Space>
    ) },
  ]

  return (
    <Drawer
      open={open}
      onClose={onClose}
      width={720}
      title={t('apidef.updateCase', '更新用例')}
      footer={<div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}><Button onClick={onClose}>{t('a.cancel', '取消')}</Button><Button type="primary" loading={saving} onClick={save}>{t('a.update', '更新')}</Button></div>}
    >
      <Input value={name} onChange={(e) => setName(e.target.value)} maxLength={255} showCount placeholder={t('apidef.caseName', '用例名称')} style={{ marginBottom: 12 }} />
      <Space style={{ marginBottom: 12 }} wrap>
        <Select value={priority} onChange={setPriority} style={{ width: 120 }} options={['P0', 'P1', 'P2', 'P3'].map((p) => ({ value: p, label: p }))} />
        <Select value={status} onChange={setStatus} style={{ width: 140 }} options={CASE_STATUSES.map((s) => ({ value: s, label: t(caseStatusKey(s), s) }))} />
      </Space>
      <Space size={[6, 6]} wrap style={{ marginBottom: 12 }}>
        {tags.map((tg) => <Tag key={tg} closable onClose={() => setTags(tags.filter((x) => x !== tg))}>{tg}</Tag>)}
        <Input size="small" style={{ width: 120 }} value={tagInput} onChange={(e) => setTagInput(e.target.value)} onPressEnter={() => { const v = tagInput.trim(); if (v && !tags.includes(v)) setTags([...tags, v]); setTagInput('') }} placeholder={t('apidef.addTag', '+ 标签')} />
      </Space>
      <Space.Compact style={{ width: '100%', marginBottom: 12 }}>
        <Select value={method} onChange={setMethod} style={{ width: 110 }} options={['GET', 'POST', 'PUT', 'DELETE', 'PATCH', 'HEAD', 'OPTIONS'].map((m) => ({ value: m, label: m }))} />
        <Input value={url} onChange={(e) => setUrl(e.target.value)} className="ms-mono" placeholder="/api/..." />
      </Space.Compact>
      <Tabs className="ms-detail-tabs" size="small" items={tabs} />
    </Drawer>
  )
}

function CaseDetailTab({ caseItem, projectId, onClose, onDeleted }: { caseItem: ApiCase; projectId: string; onClose: () => void; onDeleted?: () => void }) {
  const { t } = useI18n()
  const [c, setC] = useState<ApiCase>(caseItem) // local copy so edits reflect immediately
  const [editOpen, setEditOpen] = useState(false)
  const [envs, setEnvs] = useState<Environment[]>([])
  const [envId, setEnvId] = useState<string>('')
  const [hdrView, setHdrView] = useState<'table' | 'raw'>('table')
  const [running, setRunning] = useState(false)
  const [resp, setResp] = useState<DebugResponse | null>(null)
  const [err, setErr] = useState('')
  const [lastReq, setLastReq] = useState<SentRequest | null>(null)

  useEffect(() => { api.environments(projectId).then(setEnvs).catch(() => undefined) }, [projectId])
  const env = envs.find((e) => e.id === envId)
  const headers = c.headers || []
  const tags = c.tags || []

  const resolveUrl = (u: string) => {
    if (/^https?:\/\//i.test(u)) return u
    const b = env?.baseUrl?.trim().replace(/\/+$/, '')
    return b ? `${b}${u.startsWith('/') ? '' : '/'}${u}` : u
  }
  const run = async () => {
    const url = resolveUrl(c.url)
    if (!/^https?:\/\//i.test(url)) return message.warning(t('editor.needEnvOrAbs', '相对路径需先选择带 baseUrl 的环境,或填写绝对 URL(http(s)://)'))
    const hdrs = [...headers]
    if (c.auth?.token && (c.auth.type === 'bearer' || c.auth.type === 'basic')) hdrs.push({ key: 'Authorization', value: `${c.auth.type === 'bearer' ? 'Bearer' : 'Basic'} ${c.auth.token}` })
    const req: SentRequest = { method: c.method, url, headers: hdrs, body: c.body || undefined }
    setLastReq(req); setRunning(true); setErr(''); setResp(null)
    try {
      setResp(await api.debugSend({ ...req, assertions: (c.assertions as unknown[]) || [], processors: (c.processors as unknown[]) || [] }))
    } catch (e) {
      setErr(e instanceof ApiError ? e.message : t('editor.sendFail', '发送失败'))
    } finally {
      setRunning(false)
    }
  }

  const hdrRaw = headers.filter((h) => h.key).map((h) => `${h.key}: ${h.value}`).join('\n')
  const detailTab = (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 18 }}>
      <div>
        <div style={{ display: 'flex', alignItems: 'center', marginBottom: 8 }}>
          <span style={{ fontWeight: 600, fontSize: 13 }}>{t('apidef.requestHeaders', '请求头')} <Tag style={{ marginLeft: 4 }}>{headers.length}</Tag></span>
          <div style={{ flex: 1 }} />
          {headers.length > 0 && <Segmented size="small" value={hdrView} onChange={(v) => setHdrView(v as 'table' | 'raw')} options={[{ label: 'Table', value: 'table' }, { label: 'Raw', value: 'raw' }]} />}
        </div>
        {headers.length === 0 ? (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('apidef.none', '无')} style={{ margin: '8px 0' }} />
        ) : hdrView === 'raw' ? (
          <pre className="ms-mono" style={{ background: 'var(--panel-2)', padding: 12, borderRadius: 6, margin: 0, fontSize: 12 }}>{hdrRaw}</pre>
        ) : (
          <Table size="small" rowKey={(_, i) => String(i)} pagination={false} dataSource={headers} columns={[{ title: t('env.varName', '参数名'), dataIndex: 'key', width: '40%' }, { title: t('env.varValue', '参数值'), dataIndex: 'value', render: (v: string) => <span className="ms-mono">{v}</span> }]} />
        )}
      </div>
      <div>
        <div style={{ display: 'flex', alignItems: 'center', marginBottom: 8 }}>
          <span style={{ fontWeight: 600, fontSize: 13 }}>{t('apidef.requestBodyJson', '请求体-JSON')}</span>
          <div style={{ flex: 1 }} />
          {c.body && <Button size="small" icon={<CopyOutlined />} onClick={() => navigator.clipboard?.writeText(c.body || '')}>{t('a.copy', '复制')}</Button>}
        </div>
        {c.body ? (
          <pre className="ms-mono" style={{ background: 'var(--panel-2)', padding: 12, borderRadius: 6, margin: 0, fontSize: 12, maxHeight: 360, overflow: 'auto' }}>{c.body}</pre>
        ) : (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('apidef.noBody', '请求没有 Body')} style={{ margin: '8px 0' }} />
        )}
      </div>
      <div>
        <span style={{ fontWeight: 600, fontSize: 13 }}>{t('apidef.responseContent', '响应内容')}</span>
        <DebugResultPanel running={running} resp={resp} err={err} req={lastReq} isHttp assertions={(c.assertions as Record<string, unknown>[]) || undefined} extractors={(c.processors as Record<string, unknown>[]) || undefined} />
      </div>
    </div>
  )

  return (
    <div style={{ padding: '12px 16px', height: '100%', overflow: 'auto' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 6 }}>
        <span style={{ color: 'var(--error)', fontSize: 12, fontWeight: 600 }}>{c.priority || 'P0'}</span>
        <span className="ms-mono" style={{ color: 'var(--text-3)', fontSize: 12 }}>[{c.id.slice(0, 8)}]</span>
        <span style={{ fontWeight: 600, fontSize: 15 }}>{c.name}</span>
        <div style={{ flex: 1 }} />
        <Select size="small" value={envId || undefined} onChange={setEnvId} allowClear placeholder={t('apidef.selectEnv', '选择环境')} style={{ width: 160 }} options={envs.map((e) => ({ value: e.id, label: e.name }))} />
        <Button
          danger
          onClick={() => modal.confirm({
            title: t('apidef.caseDeleteConfirm', '删除该用例?'), okType: 'danger', okText: t('a.delete', '删除'), cancelText: t('a.cancel', '取消'),
            onOk: async () => {
              try { await api.deleteCase(c.id); message.success(t('apidef.deleted', '已删除')); onClose(); onDeleted?.() }
              catch (e) { message.error(e instanceof ApiError ? e.message : t('apidef.deleteFailed', '删除失败')) }
            },
          })}
        >{t('a.delete', '删除')}</Button>
        <Button onClick={() => setEditOpen(true)}>{t('a.edit', '编辑')}</Button>
        <Button type="primary" icon={<ThunderboltOutlined />} loading={running} onClick={run}>{t('apidef.serverRun', '服务端执行')}</Button>
        <Button onClick={onClose}>{t('a.close', '关闭')}</Button>
      </div>
      <CaseEditDrawer open={editOpen} caseItem={c} onClose={() => setEditOpen(false)} onSaved={(u) => { setC(u); setEditOpen(false); onDeleted?.() }} />
      <div style={{ display: 'flex', alignItems: 'center', gap: 16, marginBottom: 12, fontSize: 12, color: 'var(--text-3)', flexWrap: 'wrap' }}>
        <span>{t('apidef.colMethod', '请求类型')} <Tag color={methodColor(c.method)} style={{ margin: 0 }}>{c.method}</Tag></span>
        <span>{t('apidef.colPath', '路径')} <span className="ms-mono" style={{ color: 'var(--text)' }}>{c.url}</span></span>
        {tags.length > 0 && <span>{t('apidef.colTags', '标签')} {tags.map((tg) => <Tag key={tg}>{tg}</Tag>)}</span>}
      </div>
      <Tabs
        className="ms-detail-tabs"
        defaultActiveKey="detail"
        items={[
          { key: 'detail', label: t('apidef.caseDetail', '用例详情'), children: detailTab },
          { key: 'refs', label: t('apidef.references', '引用关系'), children: <Empty description={t('apidef.refEmpty', '暂无数据')} style={{ marginTop: 40 }} /> },
          { key: 'exec', label: t('apidef.execHistory', '执行历史'), children: <Empty description={t('apidef.none', '无')} style={{ marginTop: 40 }} /> },
          { key: 'change', label: t('apidef.changeHistory', '变更历史'), children: <Empty description={t('apidef.none', '无')} style={{ marginTop: 40 }} /> },
        ]}
      />
    </div>
  )
}

function ApiDetail({ definition, onUpdated }: { definition: ApiDefinition; onUpdated?: () => void }) {
  const { t } = useI18n()
  // Top-level tabs + define/debug switch inside the Define tab (debug is a mode of Define).
  const [tab, setTab] = useState('preview')
  const [defMode, setDefMode] = useState<'define' | 'debug'>('define')
  const specRef = useRef<ApiSpecPanelHandle>(null)
  const [modules, setModules] = useState<ApiModule[]>([])
  const [meta, setMeta] = useState<{ tags: string[]; description?: string; spec?: ApiSpec }>({ tags: [] })
  // Request-line name/method/path (cURL import back-fills method/path); initialized from the definition, persisted with base fields on save.
  const [reqName, setReqName] = useState(definition.name || '')
  const [reqMethod, setReqMethod] = useState(definition.method || 'GET')
  const [reqPath, setReqPath] = useState(definition.path || '')
  const isHttp = (definition.protocol || 'HTTP').toUpperCase() === 'HTTP'
  const [curlOpen, setCurlOpen] = useState(false)
  const [curlText, setCurlText] = useState('')
  // Debug exec mode: server proxy vs. direct browser request.
  const [execMode, setExecMode] = useState<ExecMode>('server')
  // Debug environment (picked in the top bar; supplies baseUrl/default headers/variables).
  const [envs, setEnvs] = useState<Environment[]>([])
  const [envId, setEnvId] = useState<string>('')
  // Bumped after "save as new case" to reload the Cases tab.
  const [casesRefresh, setCasesRefresh] = useState(0)

  useEffect(() => {
    if (defMode !== 'debug' || envs.length) return
    let alive = true
    api
      .environments(definition.projectId)
      .then((list) => {
        if (!alive) return
        const arr = Array.isArray(list) ? list : []
        setEnvs(arr)
        setEnvId((cur) => cur || arr.find((e) => e.enabled !== false)?.id || '')
      })
      .catch(() => alive && setEnvs([]))
    return () => {
      alive = false
    }
  }, [defMode, definition.projectId, envs.length])

  useEffect(() => {
    let alive = true
    api.modules(definition.projectId).then((m) => alive && setModules(Array.isArray(m) ? m : [])).catch(() => undefined)
    api
      .getDefinition(definition.id)
      .then((d) => alive && setMeta({ tags: d.spec?.tags || [], description: d.spec?.description, spec: d.spec }))
      .catch(() => undefined)
    return () => {
      alive = false
    }
  }, [definition.id, definition.projectId])

  const moduleName = modules.find((m) => m.id === definition.moduleId)?.name || t('apidef.unfiled', '未归类')

  // Preview header: status/method/[id]/name + meta row (ref #1).
  const previewHeader = (
    <div style={{ marginBottom: 16 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 10, flexWrap: 'wrap' }}>
        <Tag color={statusColor(definition.status)} style={{ margin: 0 }}>{definition.status}</Tag>
        <Tag color={methodColor(definition.method)} style={{ margin: 0, fontWeight: 600 }}>{definition.method || definition.protocol}</Tag>
        <span className="ms-mono" style={{ color: 'var(--text-3)', fontSize: 12 }}>[{definition.num ?? '—'}]</span>
        <span style={{ fontWeight: 600, fontSize: 15, color: 'var(--text)' }}>{definition.name}</span>
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

  // Preview sub-tabs: detail / references / change history (ref #1).
  // Flex column fill: fixed header + flex:1 sub-tabs so the reference graph fills the available height.
  const previewTab = (
    <div style={{ height: '100%', display: 'flex', flexDirection: 'column', minHeight: 0 }}>
      {previewHeader}
      <Tabs
        className="ms-detail-tabs ms-fill-tabs"
        size="small"
        style={{ flex: 1, minHeight: 0 }}
        items={[
          { key: 'detail', label: t('apidef.detail', '详情'), children: <ApiSpecPanel definition={definition} mode="preview" /> },
          { key: 'refs', label: t('apidef.references', '引用关系'), children: <ReferencesPanel definition={definition} /> },
          { key: 'history', label: t('apidef.changeHistory', '变更历史'), children: <ChangeHistoryPanel definition={definition} /> },
        ]}
      />
    </div>
  )

  // Define/debug share one editor shell: request line (protocol/method/path) + cURL import + inline define/debug switch
  // + run/save (ref #6/#7). Debug appends a response panel below the sub-tabs.
  const importCurl = () => {
    const parsed = parseCurl(curlText)
    if (!parsed) {
      message.error(t('apidef.curlParseFail', 'cURL 解析失败,请检查命令'))
      return
    }
    setReqMethod(parsed.method)
    setReqPath(parsed.url)
    specRef.current?.applyCurl(parsed)
    setCurlOpen(false)
    setCurlText('')
    message.success(t('apidef.curlImported', '已导入 cURL'))
  }
  const defineTab = (
    <div>
      {!isHttp && (
        <div style={{ marginBottom: 8, color: 'var(--text-3)', fontSize: 12 }}>
          {t('apidef.nonHttpDetailHint', '该协议当前仅登记/存储,不支持执行/调试;此处仅查看与编辑定义。')}
        </div>
      )}
      <div style={{ display: 'flex', gap: 8, alignItems: 'center', marginBottom: 12 }}>
        <Tag color="blue" style={{ margin: 0, padding: '4px 10px' }}>{definition.protocol}</Tag>
        {isHttp && <Select value={reqMethod} onChange={setReqMethod} style={{ width: 100 }} popupMatchSelectWidth={false} options={METHODS.map((m) => ({ value: m, label: m }))} />}
        <Input value={reqPath} onChange={(e) => setReqPath(e.target.value)} className="ms-mono" style={{ flex: 1, minWidth: 200 }} placeholder="/api/..." />
        {/* Right-side actions grouped in a Space (excluded from flex growth so the path input fills the middle). */}
        <Space size={8}>
          <Tooltip title={t('apidef.importCurl', '导入 cURL')}>
            <Button icon={<CodeOutlined />} onClick={() => setCurlOpen(true)} />
          </Tooltip>
          {/* Non-HTTP protocols can't execute yet → no debug switch, define only. */}
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
          {defMode === 'debug' ? (
            <>
              {/* Run: main button uses the current mode; dropdown switches server/local and runs immediately. */}
              <Dropdown.Button
                type="primary"
                icon={<DownOutlined />}
                onClick={() => specRef.current?.execute(execMode)}
                menu={{
                  selectable: true,
                  selectedKeys: [execMode],
                  items: [
                    { key: 'server', label: t('apidef.serverRun', '服务端执行') },
                    { key: 'local', label: t('apidef.localRun', '本地执行') },
                  ],
                  onClick: ({ key }) => {
                    setExecMode(key as ExecMode)
                    specRef.current?.execute(key as ExecMode)
                  },
                }}
              >
                <ThunderboltOutlined /> {execMode === 'local' ? t('apidef.localRun', '本地执行') : t('apidef.serverRun', '服务端执行')}
              </Dropdown.Button>
              {/* Debug "Save": main button saves the definition (same spec as define mode); dropdown = save as new case. */}
              <Dropdown.Button
                icon={<DownOutlined />}
                onClick={() => specRef.current?.save()}
                menu={{
                  items: [{ key: 'case', label: t('apidef.saveAsCase', '保存为新用例') }],
                  onClick: ({ key }) => {
                    if (key === 'case') specRef.current?.saveAsCase()
                  },
                }}
              >
                <SaveOutlined /> {t('a.save', '保存')}
              </Dropdown.Button>
            </>
          ) : (
            <Button icon={<SaveOutlined />} onClick={() => specRef.current?.save()}>{t('a.save', '保存')}</Button>
          )}
        </Space>
      </div>
      {/* API name: editable, persisted with base fields (method/path) on save. */}
      <Input
        value={reqName}
        onChange={(e) => setReqName(e.target.value)}
        placeholder={t('apidef.nameInputPlaceholder', '请输入接口名称')}
        style={{ marginBottom: 12 }}
      />
      <ApiSpecPanel
        ref={specRef}
        definition={definition}
        mode={defMode === 'define' ? 'define' : 'debug'}
        reqName={reqName}
        reqMethod={reqMethod}
        reqPath={reqPath}
        onSaved={onUpdated}
        execMode={execMode}
        env={envs.find((e) => e.id === envId)}
        onCaseSaved={() => { setCasesRefresh((n) => n + 1); setTab('cases') }}
        hideSave
      />
      <Modal
        title={t('apidef.importCurl', '导入 cURL')}
        open={curlOpen}
        onCancel={() => setCurlOpen(false)}
        onOk={importCurl}
        okText={t('a.import', '导入')}
        cancelText={t('a.cancel', '取消')}
        width={680}
        destroyOnHidden
      >
        <Input.TextArea
          rows={10}
          value={curlText}
          onChange={(e) => setCurlText(e.target.value)}
          placeholder={"curl -X POST 'https://api.example.com/login' \\\n  -H 'Content-Type: application/json' \\\n  -d '{\"user\":\"admin\"}'"}
          className="ms-mono"
        />
      </Modal>
    </div>
  )

  return (
    <div style={{ padding: '12px 16px', height: '100%', overflow: 'auto' }}>
      <Tabs
        className="ms-detail-tabs"
        activeKey={tab}
        onChange={setTab}
        tabBarExtraContent={
          tab === 'preview' ? (
            <Space size={4}>
              <Button size="small" onClick={() => setTab('define')}>{t('a.edit', '编辑')}</Button>
              <Button type="primary" size="small" onClick={() => { setTab('define'); setDefMode('debug') }}>{t('apidef.serverRun', '服务端执行')}</Button>
            </Space>
          ) : tab === 'define' && defMode === 'debug' ? (
            // Debug mode: env selector shares the tab bar row (top right).
            <Select
              size="small"
              value={envId || undefined}
              onChange={setEnvId}
              style={{ width: 240 }}
              placeholder={t('editor.selectEnv', '选择环境')}
              allowClear
              options={envs.map((e) => ({ value: e.id, label: e.baseUrl ? `${e.name} · ${e.baseUrl}` : e.name }))}
              notFoundContent={t('editor.noEnvConfigured', '未配置环境,去「环境」页新建')}
            />
          ) : undefined
        }
        items={[
          { key: 'preview', label: t('apidef.preview', '预览'), children: previewTab },
          { key: 'define', label: t('apidef.define', '定义'), children: defineTab },
          { key: 'cases', label: t('apidef.casesTab', '用例'), children: <CasesPanel definition={definition} refreshToken={casesRefresh} /> },
          { key: 'mock', label: 'MOCK', children: <MocksPanel definition={definition} /> },
        ]}
      />
    </div>
  )
}

function Meta({ label, value }: { label: string; value: ReactNode }) {
  return (
    <span style={{ color: 'var(--text-3)' }}>
      {label} <span style={{ color: 'var(--text-2)' }}>{value}</span>
    </span>
  )
}

/** New-API work tab (ref mock: protocol/method/path request line + name + description + save/cancel). */
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
  // Debug reuses the detail page's ApiSpecPanel (controlled draft; no id so nothing persists); envs lazy-load on entering debug.
  const panelRef = useRef<ApiSpecPanelHandle>(null)
  const [envs, setEnvs] = useState<Environment[]>([])
  const [envId, setEnvId] = useState('')
  useEffect(() => {
    if (defMode !== 'debug' || envs.length) return
    api.environments(projectId).then((list) => {
      const arr = Array.isArray(list) ? list : []
      setEnvs(arr)
      setEnvId((cur) => cur || arr.find((e) => e.enabled !== false)?.id || '')
    }).catch(() => setEnvs([]))
  }, [defMode, projectId, envs.length])

  const save = async () => {
    if (!name.trim()) return message.warning(t('apidef.nameRequired', '请填接口名称'))
    setSaving(true)
    try {
      const d = await api.createDefinition({ projectId, name: name.trim(), protocol, method: isHttp ? method : '', path })
      if (moduleId) await api.moveDefinition(d.id, moduleId).catch(() => undefined)
      // Also persist the request/response spec edited in the Define tab.
      await api.updateDefinitionSpec(d.id, spec).catch(() => undefined)
      message.success(t('apidef.apiCreated', '接口已创建'))
      onCreated(d)
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('apidef.createFailed', '创建失败'))
    } finally {
      setSaving(false)
    }
  }

  // Synthetic definition for ApiSpecPanel (create mode): no id, just projectId/protocol so it can render.
  const draftDef = { id: '', num: 0, projectId, name, protocol, method, path, status: 'DRAFT', moduleId, spec } as unknown as ApiDefinition

  return (
    <div style={{ padding: '12px 16px', height: '100%', overflow: 'auto' }}>
      {/* Request line: protocol + method + path + define/debug + save (ref #6/#7) */}
      <div style={{ display: 'flex', gap: 8, marginBottom: 12, alignItems: 'center' }}>
        <Select value={protocol} onChange={setProtocol} style={{ width: 120 }} options={PROTOCOLS.map((p) => ({ value: p, label: p }))} />
        {isHttp && <Select value={method} onChange={setMethod} style={{ width: 100 }} popupMatchSelectWidth={false} options={METHODS.map((m) => ({ value: m, label: m }))} />}
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
        {/* Debug: env + send (server run); method/path/params share the same draft as define. */}
        {isHttp && defMode === 'debug' && (
          <>
            <Select
              value={envId || undefined}
              onChange={setEnvId}
              style={{ width: 200 }}
              placeholder={t('editor.selectEnv', '选择环境')}
              allowClear
              options={envs.map((e) => ({ value: e.id, label: e.baseUrl ? `${e.name} · ${e.baseUrl}` : e.name }))}
              notFoundContent={t('editor.noEnvConfigured', '未配置环境,去「环境」页新建')}
            />
            <Button type="primary" icon={<SendOutlined />} onClick={() => panelRef.current?.execute()}>{t('a.send', '发送')}</Button>
          </>
        )}
        <Button onClick={onCancel}>{t('a.cancel', '取消')}</Button>
        <Button type="primary" loading={saving} icon={<SaveOutlined />} onClick={save}>{t('a.save', '保存')}</Button>
      </div>
      <Input value={name} onChange={(e) => setName(e.target.value)} placeholder={t('apidef.nameInputPlaceholder', '请输入接口名称')} style={{ marginBottom: 12 }} autoFocus />
      {!isHttp ? (
        <span style={{ color: 'var(--text-3)', fontSize: 12 }}>{t('apidef.nonHttpHint', '该协议当前仅登记/存储,执行能力待接入;保存后可在列表查看。')}</span>
      ) : defMode === 'define' ? (
        <ApiSpecPanel definition={draftDef} mode="create" value={spec} onChange={setSpec} />
      ) : (
        <ApiSpecPanel
          ref={panelRef}
          definition={draftDef}
          mode="debug"
          value={spec}
          onChange={setSpec}
          hideSave
          reqMethod={method}
          reqPath={path}
          reqName={name}
          env={envs.find((e) => e.id === envId)}
        />
      )}
    </div>
  )
}

function ImportModal({
  open,
  onClose,
  projectId,
  modules,
  onDone,
}: {
  open: boolean
  onClose: () => void
  projectId: string
  modules: ApiModule[]
  onDone: () => void
}) {
  const { t } = useI18n()
  const SOURCES: { label: string; value: ImportFormat }[] = [
    { label: 'Swagger/OpenAPI', value: 'openapi' },
    { label: 'Postman', value: 'postman' },
    { label: 'HAR', value: 'har' },
    { label: 'JMeter', value: 'jmeter' },
    { label: 'MeterSphere', value: 'metersphere' },
  ]
  const [source, setSource] = useState<ImportFormat>('openapi')
  const [importType, setImportType] = useState('file') // file = one-shot | schedule = recurring import
  const [moduleId, setModuleId] = useState<string | undefined>(undefined)
  const [groupByTag, setGroupByTag] = useState(true) // auto-create sub-modules from source tags/groups (default on)
  const [overwrite, setOverwrite] = useState(true)
  const [syncModule, setSyncModule] = useState(true) // on overwrite, also sync the API's module
  const [way, setWay] = useState('file') // one-shot import via: file | url
  const [text, setText] = useState('')
  const [fileName, setFileName] = useState('')
  const [urlVal, setUrlVal] = useState('')
  const [token, setToken] = useState('')
  const [basicAuth, setBasicAuth] = useState(false)
  const [saving, setSaving] = useState(false)
  const [cron, setCron] = useState('0 0 2 * * *') // 6-field cron, default daily 02:00
  const [scheduleName, setScheduleName] = useState('')
  const [schedules, setSchedules] = useState<ImportSchedule[]>([])
  const isJmeter = source === 'jmeter'

  const readFile = (file: File) => {
    const reader = new FileReader()
    reader.onload = () => { setText(String(reader.result || '')); setFileName(file.name) }
    reader.readAsText(file)
    return false
  }

  const loadSchedules = async () => {
    try { setSchedules(await api.importSchedules(projectId)) } catch { /* ignore load failure */ }
  }
  useEffect(() => {
    if (open && importType === 'schedule') void loadSchedules()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, importType, projectId])

  const importResultMsg = (r: { created: number; updated: number; skipped: number }) =>
    `${t('apidef.importSuccessNew', '导入成功:新增接口')} ${r.created}` +
    `${t('apidef.importUpdated', ',覆盖更新')} ${r.updated}` +
    `${t('apidef.importSkipped', ',跳过')} ${r.skipped}`

  const doImport = async () => {
    const opts = { format: source, moduleId: moduleId || undefined, groupByTag, overwrite, syncModule } as const
    // URL import: fetched server-side (avoids CORS).
    if (way === 'url') {
      if (!urlVal.trim()) return message.warning(t('apidef.importUrlRequired', '请输入来源 URL'))
      setSaving(true)
      try {
        const r = await api.importFromUrl(projectId, urlVal.trim(), { ...opts, token: token || undefined, basicAuth })
        message.success(importResultMsg(r))
        setUrlVal(''); onDone()
      } catch (e) {
        message.error(e instanceof ApiError ? e.message : t('apidef.importFailed', '导入失败'))
      } finally { setSaving(false) }
      return
    }
    // File/paste import.
    if (!text.trim()) return message.warning(t('apidef.importContentRequired', '请上传文件或粘贴文档内容'))
    let content: unknown = text
    if (!isJmeter) {
      // JMeter is raw XML; everything else must be valid JSON.
      try { content = JSON.parse(text) } catch {
        return message.error(t('apidef.invalidJson', '不是合法 JSON(上传文件或粘贴文档内容)'))
      }
    }
    setSaving(true)
    try {
      const r = await api.importDefinitions(projectId, content, opts)
      message.success(importResultMsg({ created: r.created.length, updated: r.updated, skipped: r.skipped }))
      setText(''); setFileName(''); onDone()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('apidef.importFailed', '导入失败'))
    } finally { setSaving(false) }
  }

  const doCreateSchedule = async () => {
    if (!urlVal.trim()) return message.warning(t('apidef.importUrlRequired', '请输入来源 URL'))
    if (!cron.trim()) return message.warning(t('apidef.importCronRequired', '请输入 cron 表达式'))
    setSaving(true)
    try {
      await api.createImportSchedule({
        projectId, name: scheduleName || undefined, url: urlVal.trim(), cron: cron.trim(),
        format: source, token: token || undefined, basicAuth,
        moduleId: moduleId || undefined, groupByTag, overwrite, syncModule, enabled: true,
      })
      message.success(t('apidef.scheduleCreated', '定时导入已创建,将按 cron 自动拉取导入'))
      setUrlVal(''); setScheduleName(''); void loadSchedules()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('apidef.scheduleCreateFailed', '创建定时导入失败'))
    } finally { setSaving(false) }
  }

  const toggleSchedule = async (s: ImportSchedule, enabled: boolean) => {
    try { await api.setImportScheduleEnabled(s.id, enabled); void loadSchedules() }
    catch (e) { message.error(e instanceof ApiError ? e.message : t('a.failed', '操作失败')) }
  }
  const runSchedule = async (s: ImportSchedule) => {
    try {
      const r = await api.runImportSchedule(s.id)
      message.success(`${t('apidef.scheduleRan', '已执行:')}${r.result}`); void loadSchedules()
    } catch (e) { message.error(e instanceof ApiError ? e.message : t('apidef.importFailed', '导入失败')) }
  }
  const removeSchedule = (s: ImportSchedule) => {
    modal.confirm({
      title: t('apidef.scheduleDeleteConfirm', '删除该定时导入?'),
      content: s.sourceUrl,
      okButtonProps: { danger: true },
      onOk: async () => { await api.deleteImportSchedule(s.id); void loadSchedules() },
    })
  }

  const Field = ({ label, children }: { label: React.ReactNode; children: React.ReactNode }) => (
    <div style={{ marginBottom: 16 }}>
      <div style={{ fontSize: 13, color: 'var(--text)', marginBottom: 8 }}>{label}</div>
      {children}
    </div>
  )

  const isSchedule = importType === 'schedule'
  // Scheduled imports must be URL-based (re-fetched on schedule); one-shot can be file or URL.
  const showUrlInputs = isSchedule || way === 'url'
  const importDisabled = isSchedule
    ? !urlVal.trim() || !cron.trim()
    : way === 'file' ? !text.trim() : !urlVal.trim()

  const scheduleColumns: ColumnsType<ImportSchedule> = [
    {
      title: t('apidef.scheduleName', '名称/来源'),
      dataIndex: 'name',
      render: (_: unknown, s: ImportSchedule) => (
        <div>
          <div>{s.name || <span style={{ color: 'var(--text-3)' }}>{t('apidef.unnamed', '未命名')}</span>}</div>
          <div style={{ color: 'var(--text-3)', fontSize: 12, wordBreak: 'break-all' }}>{s.sourceUrl}</div>
        </div>
      ),
    },
    { title: t('apidef.importSource', '来源'), dataIndex: 'format', width: 90, render: (f: string) => <Tag>{f}</Tag> },
    { title: 'cron', dataIndex: 'cron', width: 120, render: (c: string) => <span className="ms-mono" style={{ fontSize: 12 }}>{c}</span> },
    {
      title: t('apidef.scheduleEnabled', '启用'), dataIndex: 'enabled', width: 70,
      render: (en: boolean, s: ImportSchedule) => <Switch size="small" checked={en} onChange={(v) => toggleSchedule(s, v)} />,
    },
    {
      title: t('apidef.scheduleLastRun', '最近运行'), dataIndex: 'lastResult', width: 160,
      render: (r: string, s: ImportSchedule) => (
        <div style={{ fontSize: 12 }}>
          <div>{r || <span style={{ color: 'var(--text-3)' }}>{t('apidef.notRun', '尚未运行')}</span>}</div>
          {s.lastRunAt && <div style={{ color: 'var(--text-3)' }}>{s.lastRunAt}</div>}
        </div>
      ),
    },
    {
      // Operator: who triggered the last run. Manual "run now" records the user; cron runs record '' → shown as "Auto".
      title: t('apidef.colOperator', '操作人'), dataIndex: 'lastRunBy', width: 100, ellipsis: true,
      render: (u: string | undefined, s: ImportSchedule) =>
        u ? <span style={{ color: 'var(--text-2)' }}>{u}</span>
          : s.lastRunAt ? <Tag color="blue">{t('apidef.autoRun', '自动')}</Tag>
          : <span style={{ color: 'var(--text-3)' }}>—</span>,
    },
    {
      title: t('apidef.colCreatedBy', '创建人'), dataIndex: 'createdBy', width: 110, ellipsis: true,
      render: (u?: string) => u ? <span style={{ color: 'var(--text-2)' }}>{u}</span> : <span style={{ color: 'var(--text-3)' }}>—</span>,
    },
    {
      title: t('a.actions', '操作'), width: 110,
      render: (_: unknown, s: ImportSchedule) => (
        <Space size={4}>
          <Button size="small" type="link" onClick={() => runSchedule(s)}>{t('apidef.scheduleRunNow', '立即执行')}</Button>
          <Button size="small" type="link" danger onClick={() => removeSchedule(s)}>{t('a.delete', '删除')}</Button>
        </Space>
      ),
    },
  ]

  return (
    <Drawer
      title={t('apidef.importTitle2', '导入接口')}
      open={open}
      onClose={onClose}
      width={680}
      footer={
        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
          <Button onClick={onClose}>{t('a.cancel', '取消')}</Button>
          <Button type="primary" loading={saving} disabled={importDisabled} onClick={isSchedule ? doCreateSchedule : doImport}>
            {isSchedule ? t('apidef.scheduleCreate', '创建定时导入') : t('a.import', '导入')}
          </Button>
        </div>
      }
    >
      <Field label={t('apidef.importSource', '来源')}>
        <Segmented value={source} onChange={(v) => setSource(v as ImportFormat)} options={SOURCES} />
      </Field>
      <Field label={t('apidef.importType', '导入类型')}>
        <Segmented
          value={importType}
          onChange={(v) => setImportType(String(v))}
          options={[
            { label: t('apidef.importTypeFile', '一次性导入'), value: 'file' },
            { label: t('apidef.importTypeSchedule', '定时导入'), value: 'schedule' },
          ]}
        />
      </Field>
      <Field label={t('apidef.importModule', '所属模块')}>
        <Select
          style={{ width: '100%' }}
          value={moduleId || ''}
          onChange={(v) => setModuleId(v || '')}
          placeholder={t('apidef.unfiled', '未归类')}
          options={[{ value: '', label: t('apidef.unfiled', '未归类') }, ...modules.map((m) => ({ value: m.id, label: m.name }))]}
        />
      </Field>
      <Field label={t('apidef.importGroupByTag', '按标签建模块')}>
        <Checkbox checked={groupByTag} onChange={(e) => setGroupByTag(e.target.checked)}>
          {t('apidef.importGroupByTagHint2', '按来源分组(OpenAPI 标签 / Postman 文件夹 / HAR 域名 / MeterSphere 模块)自动建/复用子模块并归类(上方「所属模块」作为其父级)')}
        </Checkbox>
      </Field>
      <Field label={t('apidef.importMode', '导入模式')}>
        <Radio.Group value={overwrite ? 'cover' : 'keep'} onChange={(e) => setOverwrite(e.target.value === 'cover')}>
          <Radio value="cover">
            {t('apidef.importCover', '覆盖')}
            <Tooltip title={t('apidef.importCoverHint', '同 方法+路径 已存在则刷新其规格')}><QuestionCircleOutlined style={{ color: 'var(--text-3)', marginLeft: 4 }} /></Tooltip>
          </Radio>
          <Radio value="keep">
            {t('apidef.importKeep', '不覆盖')}
            <Tooltip title={t('apidef.importKeepHint', '同 方法+路径 已存在则跳过')}><QuestionCircleOutlined style={{ color: 'var(--text-3)', marginLeft: 4 }} /></Tooltip>
          </Radio>
        </Radio.Group>
      </Field>
      {overwrite && (
        <Field label="">
          <Space>
            <Switch checked={syncModule} onChange={setSyncModule} />
            <span style={{ fontSize: 13 }}>{t('apidef.importSyncModule', '同步更新接口所在目录')}</span>
          </Space>
        </Field>
      )}
      {!isSchedule && (
        <Field label={t('apidef.importWay', '导入方式')}>
          <Segmented
            value={way}
            onChange={(v) => setWay(String(v))}
            options={[
              { label: t('apidef.importTypeFileOnly', '文件/粘贴'), value: 'file' },
              { label: t('apidef.importWayUrl', 'URL 导入'), value: 'url' },
            ]}
          />
        </Field>
      )}
      {isSchedule && (
        <>
          <Field label={t('apidef.scheduleName', '名称')}>
            <Input value={scheduleName} onChange={(e) => setScheduleName(e.target.value)} placeholder={t('apidef.scheduleNamePlaceholder', '可选,如:每日同步用户中心接口')} />
          </Field>
          <Field label={<span><span style={{ color: 'var(--error)', marginRight: 4 }}>*</span>cron</span>}>
            <Input value={cron} onChange={(e) => setCron(e.target.value)} className="ms-mono" placeholder="0 0 2 * * *" />
            <p style={{ color: 'var(--text-3)', fontSize: 12, margin: '4px 0 0' }}>
              {t('apidef.cronHint', '6 段表达式(秒 分 时 日 月 周)。例:0 0 2 * * * = 每天 02:00')}
            </p>
          </Field>
        </>
      )}
      {showUrlInputs ? (
        <>
          <Field label={<span><span style={{ color: 'var(--error)', marginRight: 4 }}>*</span>{t('apidef.importSourceUrl', '来源 URL')}</span>}>
            <Input value={urlVal} onChange={(e) => setUrlVal(e.target.value)} className="ms-mono" placeholder={t('apidef.importUrlPlaceholder', '请输入文档/集合的 URL')} />
          </Field>
          <Field label="token">
            <Input.TextArea rows={2} value={token} onChange={(e) => setToken(e.target.value)} className="ms-mono" placeholder={t('apidef.tokenPlaceholder', '可选:作为 Authorization 头(默认 Bearer)')} />
          </Field>
          <Space>
            <Switch checked={basicAuth} onChange={setBasicAuth} />
            <span style={{ fontSize: 13 }}>{t('apidef.basicAuth', 'Basic Auth 认证')}</span>
          </Space>
        </>
      ) : (
        <>
          <Upload.Dragger accept=".json,.yaml,.yml,.har,.jmx,.xml" beforeUpload={readFile} showUploadList={false} style={{ marginBottom: 10 }}>
            <p style={{ margin: 0 }}><InboxOutlined style={{ fontSize: 28, color: 'var(--brand)' }} /></p>
            <p style={{ margin: '6px 0 0' }}>{t('apidef.uploadHint2', '拖拽或点击此区域选择文件')}</p>
            <p style={{ color: 'var(--text-3)', fontSize: 12, margin: '4px 0 0' }}>
              {isJmeter
                ? t('apidef.uploadHintJmeter', '支持 JMeter .jmx(XML)文件')
                : t('apidef.uploadHintGeneric', '支持 Swagger/OpenAPI · Postman · HAR · MeterSphere 的 JSON 文件')}
            </p>
            {fileName && <p style={{ color: 'var(--brand)', margin: '4px 0 0' }}>{t('apidef.selectedFile', '已选:')}{fileName}</p>}
          </Upload.Dragger>
          <Input.TextArea rows={6} value={text} onChange={(e) => setText(e.target.value)} placeholder={t('apidef.pasteHint', '也可直接粘贴文档内容')} className="ms-mono" />
        </>
      )}
      <p style={{ color: 'var(--text-3)', fontSize: 12, margin: '10px 0 0' }}>
        {t('apidef.importAutoHint', '导入将解析请求参数/必填/响应,并为每个接口自动生成带断言的默认用例。')}
      </p>
      {isSchedule && (
        <>
          <Divider style={{ margin: '16px 0' }}>{t('apidef.scheduleList', '已配置的定时导入')}</Divider>
          <Table
            size="small"
            rowKey="id"
            pagination={false}
            dataSource={schedules}
            columns={scheduleColumns}
            locale={{ emptyText: t('apidef.scheduleEmpty', '暂无定时导入') }}
          />
        </>
      )}
    </Drawer>
  )
}
