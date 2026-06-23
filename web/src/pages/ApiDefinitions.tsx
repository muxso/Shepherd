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
  SettingOutlined,
  ThunderboltOutlined,
  CodeOutlined,
  DownOutlined,
  ShareAltOutlined,
  DeleteOutlined,
  EyeOutlined,
  EyeInvisibleOutlined,
  UnorderedListOutlined,
  MinusSquareOutlined,
  SearchOutlined,
} from '@ant-design/icons'
import type { ColumnsType } from 'antd/es/table'
import { api, ApiError, type ApiCase, type ApiDefinition, type ApiModule, type ApiSpec, type ApiView, type DebugResponse, type Environment, type ProjectMock } from '../api'
import { useApp } from '../context'
import { methodColor, statusColor } from '../components/tags'
import CasesPanel from './CasesPanel'
import MocksPanel from './MocksPanel'
import RequestEditor from '../components/RequestEditor'
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

/** 高级筛选条件:字段 + 操作符 + 值(对齐 MeterSphere 筛选抽屉)。 */
type AdvCond = { field: 'num' | 'name' | 'path' | 'protocol' | 'method' | 'status'; op: 'contains' | 'notContains' | 'equals' | 'notEquals' | 'empty' | 'notEmpty'; value: string }

const ADV_FIELDS: { value: AdvCond['field']; label: string }[] = [
  { value: 'num', label: 'ID' },
  { value: 'name', label: '接口名称' },
  { value: 'path', label: '路径' },
  { value: 'protocol', label: '协议' },
  { value: 'method', label: '请求类型' },
  { value: 'status', label: '状态' },
]
const ADV_OPS: { value: AdvCond['op']; label: string }[] = [
  { value: 'contains', label: '包含' },
  { value: 'notContains', label: '不包含' },
  { value: 'equals', label: '等于' },
  { value: 'notEquals', label: '不等于' },
  { value: 'empty', label: '为空' },
  { value: 'notEmpty', label: '不为空' },
]

function fieldVal(d: ApiDefinition, f: AdvCond['field']): string {
  if (f === 'num') return String(d.num ?? '')
  if (f === 'name') return d.name || ''
  if (f === 'path') return d.path || ''
  if (f === 'protocol') return d.protocol || ''
  if (f === 'method') return d.method || ''
  return d.status || ''
}
function condMatch(d: ApiDefinition, c: AdvCond): boolean {
  const a = fieldVal(d, c.field).toLowerCase()
  const v = c.value.trim().toLowerCase()
  switch (c.op) {
    case 'contains': return a.includes(v)
    case 'notContains': return !a.includes(v)
    case 'equals': return a === v
    case 'notEquals': return a !== v
    case 'empty': return a === ''
    case 'notEmpty': return a !== ''
  }
}

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
  // 列表视图模式:API(接口定义)/ CASE(项目用例)/ MOCK(项目 Mock)。
  const [viewMode, setViewMode] = useState<'API' | 'CASE' | 'MOCK'>('API')
  const [caseRows, setCaseRows] = useState<ApiCase[]>([])
  const [mockRows, setMockRows] = useState<ProjectMock[]>([])
  const [viewLoading, setViewLoading] = useState(false)
  const [moduleForm, setModuleForm] = useState<{ mode: 'create' | 'rename'; id?: string; parentId?: string | null; name?: string } | null>(null)
  const [openIds, setOpenIds] = useState<string[]>([])
  const [openCases, setOpenCases] = useState<Record<string, ApiCase>>({}) // 打开的用例详情 tab(key=caseId)
  const [activeKey, setActiveKey] = useState(LIST_KEY)
  // 列表:分页大小 / 列显隐 / 高级筛选(全部客户端,对齐 MeterSphere)。
  const [pageSize, setPageSize] = useState(20)
  const [hiddenCols, setHiddenCols] = useState<string[]>([])
  const [advOpen, setAdvOpen] = useState(false)
  const [advLogic, setAdvLogic] = useState<'all' | 'any'>('all')
  const [advConds, setAdvConds] = useState<AdvCond[]>([])
  const [advApplied, setAdvApplied] = useState<{ logic: 'all' | 'any'; conds: AdvCond[] }>({ logic: 'all', conds: [] })
  // 列表视图(保存的筛选/列/分页快照)。
  const [views, setViews] = useState<ApiView[]>([])
  const [activeViewId, setActiveViewId] = useState<string | null>(null)
  const [viewName, setViewName] = useState('')
  const [viewPopOpen, setViewPopOpen] = useState(false)
  const [searchParams, setSearchParams] = useSearchParams()
  // 模块树:搜索 / 树内展示接口 / 隐藏空模块 / 协议过滤 / 受控展开(收起全部)。
  const [moduleSearch, setModuleSearch] = useState('')
  const [showInterfaces, setShowInterfaces] = useState(false)
  const [hideEmpty, setHideEmpty] = useState(false)
  const [protoFilter, setProtoFilter] = useState<string[]>([])
  const [treeExpanded, setTreeExpanded] = useState<string[]>(['ALL'])

  const load = async () => {
    if (!projectId) {
      setDefs([])
      setModules([])
      setViews([])
      return
    }
    setLoading(true)
    try {
      const [ds, ms, vs] = await Promise.all([api.definitions(projectId), api.modules(projectId), api.views(projectId)])
      setDefs(Array.isArray(ds) ? ds : [])
      setModules(Array.isArray(ms) ? ms : [])
      // 排除归属其他页面的视图(如场景页 config.kind==='scenario');本页视图无 kind 或为 'apidef'。
      setViews(Array.isArray(vs) ? vs.filter((v) => (v.config as ViewConfig & { kind?: string })?.kind !== 'scenario') : [])
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('apidef.loadFailed', '加载失败'))
    } finally {
      setLoading(false)
    }
  }

  // 视图快照:当前筛选/列/分页 → config;反向 applyConfig 把 config 写回各状态。
  type ViewConfig = { kind?: string; search?: string; moduleKey?: string; pageSize?: number; hiddenCols?: string[]; advLogic?: 'all' | 'any'; advConds?: AdvCond[] }
  const currentConfig = (): ViewConfig => ({ kind: 'apidef', search, moduleKey, pageSize, hiddenCols, advLogic: advApplied.logic, advConds: advApplied.conds })
  const applyConfig = (c: ViewConfig) => {
    if (typeof c.search === 'string') setSearch(c.search)
    if (typeof c.moduleKey === 'string') setModuleKey(c.moduleKey)
    if (typeof c.pageSize === 'number') setPageSize(c.pageSize)
    if (Array.isArray(c.hiddenCols)) setHiddenCols(c.hiddenCols)
    const logic = c.advLogic ?? 'all'
    const conds = Array.isArray(c.advConds) ? c.advConds : []
    setAdvApplied({ logic, conds })
    setAdvLogic(logic)
    setAdvConds(conds)
  }
  const applyView = (v: ApiView) => {
    applyConfig(v.config as ViewConfig)
    setActiveViewId(v.id)
    setViewPopOpen(false)
    message.success(t('apidef.viewApplied', '已应用视图') + `「${v.name}」`)
  }
  const saveView = async () => {
    const name = viewName.trim()
    if (!name) return message.warning(t('apidef.viewNameRequired', '请输入视图名称'))
    try {
      const v = await api.createView({ projectId, name, config: currentConfig(), shared: true })
      setViews((vs) => [v, ...vs])
      setActiveViewId(v.id)
      setViewName('')
      message.success(t('apidef.viewSaved', '视图已保存'))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('apidef.saveFailed', '保存失败'))
    }
  }
  const shareView = async (v: ApiView) => {
    const url = `${window.location.origin}${window.location.pathname}?view=${encodeURIComponent(v.id)}`
    try {
      await navigator.clipboard?.writeText(url)
      message.success(t('apidef.viewLinkCopied', '分享链接已复制'))
    } catch {
      message.info(url)
    }
  }
  const removeView = async (v: ApiView) => {
    try {
      await api.deleteView(v.id)
      setViews((vs) => vs.filter((x) => x.id !== v.id))
      if (activeViewId === v.id) setActiveViewId(null)
      message.success(t('apidef.viewDeleted', '视图已删除'))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('apidef.deleteFailed', '删除失败'))
    }
  }

  // 深链 ?view=<id>:视图加载后命中即应用,然后清参数(避免重复)。
  // 切到 CASE/MOCK 视图时按需加载项目级用例 / Mock。
  useEffect(() => {
    if (viewMode === 'API') return
    let alive = true
    setViewLoading(true)
    const p = viewMode === 'CASE' ? api.projectCases(projectId).then((r) => alive && setCaseRows(r.items)) : api.projectMocks(projectId).then((r) => alive && setMockRows(r))
    p.catch(() => undefined).finally(() => alive && setViewLoading(false))
    return () => { alive = false }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [viewMode, projectId])

  useEffect(() => {
    const vid = searchParams.get('view')
    if (!vid || !views.length) return
    const v = views.find((x) => x.id === vid)
    if (v) applyView(v)
    const next = new URLSearchParams(searchParams)
    next.delete('view')
    setSearchParams(next, { replace: true })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [views])

  useEffect(() => {
    load()
    setOpenIds([])
    setActiveKey(LIST_KEY)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId])

  // 协议过滤选项:来自当前项目接口里出现过的协议(动态、即插即用)。
  const protoOptions = useMemo(() => Array.from(new Set(defs.map((d) => d.protocol).filter(Boolean))).sort(), [defs])

  // 模块树:全部接口 > [未归类] + 模块(parentId 嵌套);可选树内展示接口(方法标签),
  // 支持 名称搜索 / 协议过滤 / 隐藏空模块 / 子树计数。
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
      if (moduleSearch && !nameMatch && children.length === 0) return null // 搜索时:无命中则隐藏
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

  // 树内全部可展开的 key(供「收起全部 / 展开全部」)。
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

  const filtered = useMemo(() => {
    const conds = advApplied.conds.filter((c) => c.op === 'empty' || c.op === 'notEmpty' || c.value.trim())
    return defs.filter((d) => {
      const mod =
        moduleKey === 'ALL' ? true : moduleKey === 'UNFILED' ? !d.moduleId : d.moduleId === moduleKey
      const q =
        d.name.toLowerCase().includes(search.toLowerCase()) ||
        d.path.toLowerCase().includes(search.toLowerCase())
      // 高级筛选:所有=全部命中(AND);任一=任一命中(OR)。
      const adv =
        conds.length === 0
          ? true
          : advApplied.logic === 'all'
            ? conds.every((c) => condMatch(d, c))
            : conds.some((c) => condMatch(d, c))
      return mod && q && adv
    })
  }, [defs, search, moduleKey, advApplied])

  const openDef = (id: string) => {
    setOpenIds((ids) => (ids.includes(id) ? ids : [...ids, id]))
    setActiveKey(id)
  }
  // 点击 CASE 行 ID:打开用例详情 tab(对象在打开时捕获,切换视图后仍可见)。
  const openCase = (c: ApiCase) => {
    setOpenCases((m) => ({ ...m, [c.id]: c }))
    setActiveKey(`case:${c.id}`)
  }
  useOpenParam(openDef) // 支持 ?open=<definitionId> 深链打开
  // 支持 ?openCase=<caseId> 深链:引用关系图点击用例跳转 → 拉项目用例找到后打开其详情 tab。
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

  // 复制接口:同协议/方法/路径新建一条「<名> copy」,归入同模块。
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

  const allColumns: ColumnsType<ApiDefinition> = [
    { key: 'num', title: 'ID', dataIndex: 'num', width: 90, render: (num: number | undefined, d) => <span className="ms-mono" style={{ color: '#8a9099', fontSize: 12 }} title={d.id}>{num ?? '—'}</span> },
    { key: 'name', title: t('apidef.colName', '名称'), dataIndex: 'name', ellipsis: true, render: (name: string) => <span style={{ fontWeight: 500 }}>{name}</span> },
    {
      key: 'protocol', title: t('apidef.protocol', '协议'), dataIndex: 'protocol', width: 100,
      filters: PROTOCOLS.map((p) => ({ text: p, value: p })),
      onFilter: (v, d) => d.protocol === v,
      render: (p: string) => <Tag>{p}</Tag>,
    },
    {
      key: 'method', title: t('apidef.reqType', '请求类型'), dataIndex: 'method', width: 110,
      filters: METHODS.map((m) => ({ text: m, value: m })),
      onFilter: (v, d) => d.method === v,
      render: (m: string) => <Tag color={methodColor(m)} style={{ fontWeight: 600 }}>{m || '—'}</Tag>,
    },
    { key: 'path', title: t('apidef.colPath', '路径'), dataIndex: 'path', ellipsis: true, render: (p: string) => <span className="ms-mono" style={{ color: '#5b6470' }}>{p || '—'}</span> },
    {
      key: 'status', title: t('apidef.colStatus', '状态'), dataIndex: 'status', width: 100,
      filters: API_STATUSES.map((s) => ({ text: s, value: s })),
      onFilter: (v, d) => d.status === v,
      render: (s: string) => <Tag color={statusColor(s)}>{s}</Tag>,
    },
    {
      key: 'module', title: t('apidef.colModule', '模块'), dataIndex: 'moduleId', width: 120,
      render: (mid?: string | null) => {
        const m = modules.find((x) => x.id === mid)
        return m ? <Tag color="geekblue">{m.name}</Tag> : <span style={{ color: '#bbb' }}>{t('apidef.unfiled', '未归类')}</span>
      },
    },
    {
      key: 'tags', title: t('apidef.tags', '标签'), dataIndex: 'spec', width: 140,
      render: (spec?: ApiDefinition['spec']) => {
        const tags = spec?.tags || []
        return tags.length ? <Space size={[2, 2]} wrap>{tags.map((tg) => <Tag key={tg} style={{ margin: 0 }}>{tg}</Tag>)}</Space> : <span style={{ color: '#bbb' }}>—</span>
      },
    },
    { key: 'createdBy', title: t('apidef.colCreatedBy', '创建人'), dataIndex: 'createdBy', width: 110, ellipsis: true, render: (u?: string) => u ? <span style={{ color: '#5b6470' }}>{u}</span> : <span style={{ color: '#bbb' }}>—</span> },
    { key: 'createdAt', title: t('apidef.colCreatedAt', '创建时间'), dataIndex: 'createdAt', width: 160, render: (ts?: string) => <span style={{ color: '#8a9099', fontSize: 12 }}>{fmtTs(ts)}</span> },
    { key: 'updatedAt', title: t('apidef.updatedAt', '更新时间'), dataIndex: 'updatedAt', width: 160, render: (ts?: string) => <span style={{ color: '#8a9099', fontSize: 12 }}>{fmtTs(ts)}</span> },
    {
      key: 'action', title: t('apidef.colAction', '操作'), width: 150, fixed: 'right',
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
  // 列显隐:ID/名称/操作 固定;其余可在「表格设置」开关。
  const columns = allColumns.filter((c) => !hiddenCols.includes(String(c.key)))
  // 可切换显隐的列(对齐参考图「表格设置」,ID/名称固定不可关、操作不在列表)。
  const TOGGLE_COLS = allColumns.filter((c) => !['num', 'name', 'action'].includes(String(c.key))).map((c) => ({ key: String(c.key), label: String(c.title) }))

  const listTab = (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '10px 12px', borderBottom: '1px solid #f0f0f0' }}>
        {/* 视图模式切换:API / CASE / MOCK(对齐参考图左上角)。 */}
        <Dropdown
          trigger={['click']}
          menu={{ items: (['API', 'CASE', 'MOCK'] as const).map((m) => ({ key: m, label: m })), onClick: ({ key }) => setViewMode(key as 'API' | 'CASE' | 'MOCK') }}
        >
          <Button>{viewMode} <DownOutlined /></Button>
        </Dropdown>
        <span style={{ fontWeight: 600, color: '#06a561' }}>
          {viewMode === 'API' ? t('apidef.allApis2', '全部接口') : viewMode === 'CASE' ? t('apidef.allCases', '全部用例') : t('apidef.allMocks', '全部 MOCK')}
        </span>
        <div style={{ flex: 1 }} />
        <Input.Search placeholder={t('apidef.searchPlaceholder', '搜索 ID/名称/路径')} allowClear style={{ width: 240 }} value={search} onChange={(e) => setSearch(e.target.value)} />
        <Popover
          trigger="click"
          placement="bottomRight"
          open={viewPopOpen}
          onOpenChange={setViewPopOpen}
          title={t('apidef.views', '视图')}
          content={
            <div style={{ width: 268 }}>
              {views.length === 0 ? (
                <div style={{ color: '#8a9099', fontSize: 12, padding: '2px 0 8px' }}>{t('apidef.noViews', '暂无视图,保存当前筛选为视图')}</div>
              ) : (
                <Space direction="vertical" size={2} style={{ width: '100%' }}>
                  {views.map((v) => (
                    <div key={v.id} style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                      <a style={{ flex: 1, fontWeight: v.id === activeViewId ? 600 : 400, color: v.id === activeViewId ? '#06a561' : undefined, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }} onClick={() => applyView(v)} title={v.name}>
                        {v.name}
                      </a>
                      <Tooltip title={t('apidef.shareView', '分享')}>
                        <Button type="text" size="small" icon={<ShareAltOutlined />} onClick={() => shareView(v)} />
                      </Tooltip>
                      <Tooltip title={t('a.delete', '删除')}>
                        <Button type="text" size="small" danger icon={<DeleteOutlined />} onClick={() => removeView(v)} />
                      </Tooltip>
                    </div>
                  ))}
                </Space>
              )}
              <Divider style={{ margin: '8px 0' }} />
              <Space.Compact style={{ width: '100%' }}>
                <Input size="small" placeholder={t('apidef.viewName', '视图名称')} value={viewName} onChange={(e) => setViewName(e.target.value)} onPressEnter={saveView} />
                <Button size="small" type="primary" onClick={saveView}>{t('apidef.saveCurrent', '保存当前')}</Button>
              </Space.Compact>
            </div>
          }
        >
          <Button icon={<EyeOutlined />}>
            {t('apidef.views', '视图')}{activeViewId ? `: ${views.find((v) => v.id === activeViewId)?.name ?? ''}` : ''}
          </Button>
        </Popover>
        <Button icon={<FilterOutlined />} onClick={() => { setAdvLogic(advApplied.logic); setAdvConds(advApplied.conds.length ? advApplied.conds : [{ field: 'name', op: 'contains', value: '' }]); setAdvOpen(true) }}>
          {t('apidef.filter', '筛选')}{advApplied.conds.length ? ` (${advApplied.conds.length})` : ''}
        </Button>
        <Popover
          trigger="click"
          placement="bottomRight"
          title={t('apidef.tableSettings', '表格设置')}
          content={
            <div style={{ width: 240 }}>
              <div style={{ fontSize: 12, color: '#8a9099', marginBottom: 6 }}>{t('apidef.pageSize', '每页显示数量')}</div>
              <Segmented size="small" value={pageSize} onChange={(v) => setPageSize(Number(v))} options={[10, 20, 30, 50].map((n) => ({ label: String(n), value: n }))} style={{ marginBottom: 12 }} />
              <div style={{ fontSize: 12, color: '#8a9099', marginBottom: 6 }}>{t('apidef.colSettings', '表头设置')}</div>
              <Space direction="vertical" size={6} style={{ width: '100%' }}>
                {TOGGLE_COLS.map((c) => (
                  <div key={c.key} style={{ display: 'flex', alignItems: 'center' }}>
                    <span style={{ flex: 1, fontSize: 13 }}>{c.label}</span>
                    <Switch size="small" checked={!hiddenCols.includes(c.key)} onChange={(on) => setHiddenCols((h) => (on ? h.filter((x) => x !== c.key) : [...h, c.key]))} />
                  </div>
                ))}
              </Space>
            </div>
          }
        >
          <Button icon={<SettingOutlined />} />
        </Popover>
        <Button icon={<ReloadOutlined />} onClick={load} />
      </div>
      <div style={{ flex: 1, overflow: 'auto', padding: 12 }}>
        {viewMode === 'API' ? (
          <Table<ApiDefinition>
            rowKey="id"
            size="middle"
            loading={loading}
            dataSource={filtered}
            columns={columns}
            scroll={{ x: 'max-content' }}
            onRow={(d) => ({ onClick: () => openDef(d.id), style: { cursor: 'pointer' } })}
            pagination={{ pageSize, size: 'small', showSizeChanger: true, pageSizeOptions: ['10', '20', '30', '50'], onShowSizeChange: (_, s) => setPageSize(s), showTotal: (total) => `${t('apidef.totalPrefix', '共')} ${total} ${t('apidef.totalSuffix', '个接口')}` }}
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
            pagination={{ pageSize, size: 'small', showTotal: (total) => `${t('apidef.totalPrefix', '共')} ${total} ${t('apidef.caseUnit', '个用例')}` }}
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
              { title: t('apidef.colOperator', '操作人'), dataIndex: 'operator', width: 110, render: (v?: string) => v || <span style={{ color: '#bbb' }}>—</span> },
              { title: t('apidef.colUpdatedAt', '更新时间'), dataIndex: 'updatedAt', width: 160, render: (v?: string) => <span style={{ color: '#8a9099', fontSize: 12 }}>{v ? v.slice(0, 19) : '—'}</span> },
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
            pagination={{ pageSize, size: 'small', showTotal: (total) => `${t('apidef.totalPrefix', '共')} ${total} ${t('apidef.mockUnit', '个 Mock')}` }}
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
        children: <ApiDetail definition={d} />,
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
        {/* 顶部:添加接口 / 导入(对齐场景用例左栏头部样式:整宽 Compact 双按钮)。 */}
        <div style={{ padding: '10px 10px 0' }}>
          <Space.Compact style={{ width: '100%' }}>
            <Button type="primary" icon={<PlusOutlined />} style={{ flex: 1 }} disabled={viewMode !== 'API'} onClick={() => { setCreating(true); setActiveKey(NEW_KEY) }}>{t('apidef.addApi', '添加接口')}</Button>
            <Button icon={<ImportOutlined />} style={{ flex: 1 }} disabled={viewMode !== 'API'} onClick={() => setImportOpen(true)}>{t('a.import', '导入')}</Button>
          </Space.Compact>
        </div>
        {/* 搜索:模块 / 接口名称(路径)。 */}
        <div style={{ padding: '10px 10px 6px' }}>
          <Input
            allowClear
            size="small"
            prefix={<SearchOutlined style={{ color: '#bbb' }} />}
            placeholder={t('apidef.moduleSearch', '请输入模块/接口名称')}
            value={moduleSearch}
            onChange={(e) => setModuleSearch(e.target.value)}
          />
        </div>
        {/* 工具条:隐藏空模块 / 树内显示接口 / 收起全部 / 协议过滤 / 新建模块(「全部接口 (N)」由下方树根节点承载,不在此重复)。 */}
        <div style={{ display: 'flex', alignItems: 'center', gap: 2, padding: '0 8px 8px', borderBottom: '1px solid #f5f5f5' }}>
          <div style={{ flex: 1 }} />
          <Tooltip title={hideEmpty ? t('apidef.showEmpty', '显示空模块') : t('apidef.hideEmpty', '隐藏空模块')}>
            <Button size="small" type="text" icon={hideEmpty ? <EyeInvisibleOutlined /> : <EyeOutlined />} onClick={() => setHideEmpty((v) => !v)} />
          </Tooltip>
          <Tooltip title={showInterfaces ? t('apidef.hideIfaces', '隐藏接口') : t('apidef.showIfaces', '树内显示接口')}>
            <Button size="small" type="text" icon={<UnorderedListOutlined />} style={{ color: showInterfaces ? '#52c41a' : undefined }} onClick={() => setShowInterfaces((v) => !v)} />
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
              <Button size="small" type="text" icon={<FilterOutlined />} style={{ color: protoFilter.length ? '#06a561' : undefined }} />
            </Tooltip>
          </Popover>
          <Tooltip title={t('apidef.newTopModule', '新建顶层模块')}>
            <Button size="small" type="text" icon={<PlusOutlined />} style={{ color: '#52c41a' }} onClick={() => setModuleForm({ mode: 'create', parentId: null })} />
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

      <ImportModal open={importOpen} onClose={() => setImportOpen(false)} projectId={projectId} modules={modules} onDone={() => { setImportOpen(false); load() }} />

      {/* 高级筛选抽屉(对齐 MeterSphere:条件组合 所有/任一 + 字段/操作符/值,客户端过滤)。 */}
      <Drawer
        title={t('apidef.filter', '筛选')}
        open={advOpen}
        onClose={() => setAdvOpen(false)}
        width={460}
        footer={
          <div style={{ textAlign: 'right' }}>
            <Space>
              <Button onClick={() => { setAdvConds([]); setAdvApplied({ logic: advLogic, conds: [] }); }}>{t('a.reset', '重置')}</Button>
              <Button type="primary" onClick={() => { setAdvApplied({ logic: advLogic, conds: advConds }); setAdvOpen(false) }}>{t('apidef.applyFilter', '保存并筛选')}</Button>
            </Space>
          </div>
        }
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
          <span style={{ color: '#5b6470' }}>{t('apidef.matchCond', '符合以下条件')}</span>
          <Select value={advLogic} onChange={(v) => setAdvLogic(v)} style={{ width: 90 }} options={[{ value: 'all', label: t('apidef.all', '所有') }, { value: 'any', label: t('apidef.any', '任一') }]} />
        </div>
        <Space direction="vertical" style={{ width: '100%' }} size={8}>
          {advConds.map((c, i) => {
            const set = (p: Partial<AdvCond>) => setAdvConds((cs) => cs.map((x, idx) => (idx === i ? { ...x, ...p } : x)))
            const noValue = c.op === 'empty' || c.op === 'notEmpty'
            return (
              <Space.Compact key={i} style={{ width: '100%' }}>
                <Select value={c.field} onChange={(v) => set({ field: v })} style={{ width: 130 }} options={ADV_FIELDS} />
                <Select value={c.op} onChange={(v) => set({ op: v })} style={{ width: 110 }} options={ADV_OPS} />
                <Input value={c.value} disabled={noValue} onChange={(e) => set({ value: e.target.value })} placeholder={noValue ? '—' : t('apidef.filterValue', '值')} />
                <Button icon={<MoreOutlined />} onClick={() => setAdvConds((cs) => cs.filter((_, idx) => idx !== i))} danger />
              </Space.Compact>
            )
          })}
          <Button type="link" icon={<PlusOutlined />} onClick={() => setAdvConds((cs) => [...cs, { field: 'name', op: 'contains', value: '' }])} style={{ paddingLeft: 0 }}>
            {t('apidef.addCond', '添加条件')}
          </Button>
        </Space>
      </Drawer>
      <ModuleFormModal
        state={moduleForm}
        projectId={projectId}
        onClose={() => setModuleForm(null)}
        onDone={() => { setModuleForm(null); load() }}
      />
    </div>
  )
}

/** 树内接口叶子:方法标签 + 名称(点击打开详情)。 */
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
      <FolderOutlined style={{ color: '#8a9099', flexShrink: 0 }} />
      <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{name}</span>
      {count != null && <span style={{ color: '#a8adb5', fontSize: 12, flexShrink: 0 }}>{count}</span>}
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

// 用例详情 tab(点击 CASE 行 ID 打开,对齐参考图):头部 + 请求头/请求体-JSON + 响应内容 + 服务端执行。
// 更新用例抽屉(对齐参考图 #22):名称 + 优先级/状态/标签 + 请求头/请求体/Query/REST/断言/认证 → PUT /api/case/{id}。
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

  // 重新打开/切换用例时,用最新用例值重置表单。
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
        <Select value={status} onChange={setStatus} style={{ width: 140 }} options={['进行中', '已完成', '已废弃'].map((s) => ({ value: s, label: s }))} />
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
  const [c, setC] = useState<ApiCase>(caseItem) // 本地副本:编辑后即时反映
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
          <pre className="ms-mono" style={{ background: '#f6f8fa', padding: 12, borderRadius: 6, margin: 0, fontSize: 12 }}>{hdrRaw}</pre>
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
          <pre className="ms-mono" style={{ background: '#f6f8fa', padding: 12, borderRadius: 6, margin: 0, fontSize: 12, maxHeight: 360, overflow: 'auto' }}>{c.body}</pre>
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
        <span style={{ color: '#ff4d4f', fontSize: 12, fontWeight: 600 }}>{c.priority || 'P0'}</span>
        <span className="ms-mono" style={{ color: '#8a9099', fontSize: 12 }}>[{c.id.slice(0, 8)}]</span>
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
      <div style={{ display: 'flex', alignItems: 'center', gap: 16, marginBottom: 12, fontSize: 12, color: '#8a9099', flexWrap: 'wrap' }}>
        <span>{t('apidef.colMethod', '请求类型')} <Tag color={methodColor(c.method)} style={{ margin: 0 }}>{c.method}</Tag></span>
        <span>{t('apidef.colPath', '路径')} <span className="ms-mono" style={{ color: '#1f2329' }}>{c.url}</span></span>
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

function ApiDetail({ definition }: { definition: ApiDefinition }) {
  const { t } = useI18n()
  // 顶层标签 + 定义页内的 定义/调试 切换(对标 MeterSphere:调试是定义内的模式)。
  const [tab, setTab] = useState('preview')
  const [defMode, setDefMode] = useState<'define' | 'debug'>('define')
  const specRef = useRef<ApiSpecPanelHandle>(null)
  const [modules, setModules] = useState<ApiModule[]>([])
  const [meta, setMeta] = useState<{ tags: string[]; description?: string; spec?: ApiSpec }>({ tags: [] })
  // 请求行方法/路径(cURL 导入会回填);初值取定义。
  const [reqMethod, setReqMethod] = useState(definition.method || 'GET')
  const [reqPath, setReqPath] = useState(definition.path || '')
  const [curlOpen, setCurlOpen] = useState(false)
  const [curlText, setCurlText] = useState('')
  // 调试执行方式:服务端代理(server)/ 浏览器本地直发(local)。
  const [execMode, setExecMode] = useState<ExecMode>('server')
  // 调试环境(顶栏选择;提供 baseUrl/默认头/变量)。
  const [envs, setEnvs] = useState<Environment[]>([])
  const [envId, setEnvId] = useState<string>('')
  // 「保存为新用例」后自增,驱动「用例」标签重新加载。
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
  // flex 列填满:头部固定 + 子标签 flex:1,使「引用关系」关系图整套铺满可用高度。
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

  // 定义/调试共用同一编辑器外壳:请求行(协议/方法/路径)+ cURL 导入 + 行内 定义/调试 切换
  // + 服务端执行/保存(对齐参考图 #6/#7)。调试在子标签下方追加「响应内容」面板。
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
      <div style={{ display: 'flex', gap: 8, alignItems: 'center', marginBottom: 12 }}>
        <Tag color="blue" style={{ margin: 0, padding: '4px 10px' }}>{definition.protocol}</Tag>
        {definition.protocol === 'HTTP' && <Tag color={methodColor(reqMethod)} style={{ margin: 0, padding: '4px 10px', fontWeight: 600 }}>{reqMethod || 'GET'}</Tag>}
        <Input value={reqPath} onChange={(e) => setReqPath(e.target.value)} readOnly={defMode === 'define'} className="ms-mono" style={{ flex: 1, minWidth: 200 }} placeholder="/api/..." />
        {/* 右侧动作统一收进 Space(不参与 flex 伸缩,保证路径输入框占满中段)。 */}
        <Space size={8}>
          <Tooltip title={t('apidef.importCurl', '导入 cURL')}>
            <Button icon={<CodeOutlined />} onClick={() => setCurlOpen(true)} />
          </Tooltip>
          <Segmented
            value={defMode}
            onChange={(v) => setDefMode(v as 'define' | 'debug')}
            options={[
              { label: t('apidef.define', '定义'), value: 'define' },
              { label: t('apidef.debug', '调试'), value: 'debug' },
            ]}
          />
          {defMode === 'debug' ? (
            <>
              {/* 执行:主键跑当前方式;下拉切「服务端执行 / 本地执行」并立即执行。 */}
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
              {/* 调试态「保存」= 主键保存定义(与定义态共享同一份 spec);下拉「保存为新用例」。 */}
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
      <ApiSpecPanel
        ref={specRef}
        definition={definition}
        mode={defMode === 'define' ? 'define' : 'debug'}
        reqMethod={reqMethod}
        reqPath={reqPath}
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
            // 调试态:环境选择器与 预览/定义/用例/MOCK 同一行(右上,对齐 MeterSphere)。
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
  const [source, setSource] = useState('Swagger')
  const [importType, setImportType] = useState('file')
  const [moduleId, setModuleId] = useState<string | undefined>(undefined)
  const [groupByTag, setGroupByTag] = useState(true) // 按 OpenAPI tag 自动建子模块并归类(默认开)
  const [overwrite, setOverwrite] = useState(true) // 覆盖/不覆盖(对齐参考:默认覆盖)
  const [syncModule, setSyncModule] = useState(true) // 同步更新接口所在目录
  const [way, setWay] = useState('file') // file | url
  const [text, setText] = useState('')
  const [fileName, setFileName] = useState('')
  const [urlVal, setUrlVal] = useState('')
  const [token, setToken] = useState('')
  const [basicAuth, setBasicAuth] = useState(false)
  const [saving, setSaving] = useState(false)

  const readFile = (file: File) => {
    const reader = new FileReader()
    reader.onload = () => { setText(String(reader.result || '')); setFileName(file.name) }
    reader.readAsText(file)
    return false
  }

  // 非 Swagger 来源的解析器尚未接入,先禁用(避免误用)。
  const lockedSources = ['Postman', 'Har', 'Jmeter', 'MeterSphere']
  const doImport = async () => {
    let raw = text
    if (way === 'url') {
      if (!urlVal.trim()) return message.warning(t('apidef.importUrlRequired', '请输入 SwaggerURL'))
      try {
        const headers: Record<string, string> = {}
        if (token.trim()) headers['Authorization'] = `${basicAuth ? 'Basic' : 'Bearer'} ${token.trim()}`
        const res = await fetch(urlVal.trim(), { headers })
        raw = await res.text()
      } catch {
        return message.error(t('apidef.importUrlFailed', '拉取 URL 失败(可能跨域,请改用文件/粘贴)'))
      }
    }
    let parsed: any
    try {
      parsed = JSON.parse(raw)
    } catch {
      message.error(t('apidef.invalidJson', '不是合法 JSON(上传文件或粘贴 OpenAPI/Swagger 文档)'))
      return
    }
    setSaving(true)
    try {
      const r = await api.importDefinitions(projectId, parsed, { moduleId: moduleId || undefined, groupByTag, overwrite, syncModule })
      message.success(
        `${t('apidef.importSuccessNew', '导入成功:新增接口')} ${r.created.length}` +
          `${t('apidef.importUpdated', ',覆盖更新')} ${r.updated}` +
          `${t('apidef.importSkipped', ',跳过')} ${r.skipped}`,
      )
      setText(''); setFileName(''); setUrlVal('')
      onDone()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('apidef.importFailed', '导入失败'))
    } finally {
      setSaving(false)
    }
  }

  const Field = ({ label, children }: { label: React.ReactNode; children: React.ReactNode }) => (
    <div style={{ marginBottom: 16 }}>
      <div style={{ fontSize: 13, color: '#1f2329', marginBottom: 8 }}>{label}</div>
      {children}
    </div>
  )
  const importDisabled = way === 'file' ? !text.trim() : !urlVal.trim()

  return (
    <Drawer
      title={t('apidef.importTitle2', '导入接口')}
      open={open}
      onClose={onClose}
      width={640}
      footer={
        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
          <Button onClick={onClose}>{t('a.cancel', '取消')}</Button>
          <Button type="primary" loading={saving} disabled={importDisabled} onClick={doImport}>{t('a.import', '导入')}</Button>
        </div>
      }
    >
      <Field label={t('apidef.importSource', '来源')}>
        <Segmented
          value={source}
          onChange={(v) => setSource(String(v))}
          options={['Swagger', ...lockedSources].map((s) => ({
            label: lockedSources.includes(s) ? <Tooltip title={t('apidef.importSoon', '该来源解析暂未接入')}><span>{s}</span></Tooltip> : s,
            value: s,
            disabled: lockedSources.includes(s),
          }))}
        />
      </Field>
      <Field label={t('apidef.importType', '导入类型')}>
        <Segmented
          value={importType}
          onChange={(v) => setImportType(String(v))}
          options={[
            { label: t('apidef.importTypeFile', '文件导入'), value: 'file' },
            { label: <Tooltip title={t('apidef.importSoon', '暂未接入')}><span>{t('apidef.importTypeSchedule', '定时导入')}</span></Tooltip>, value: 'schedule', disabled: true },
          ]}
        />
      </Field>
      <Field label={t('apidef.importModule', '所属模块')}>
        <Select
          style={{ width: '100%' }}
          value={moduleId || ''}
          onChange={(v) => setModuleId(v || '')}
          placeholder={t('apidef.unfiled', '未归类')}
          // 对齐左侧树:始终含「未归类」+ 各模块。
          options={[{ value: '', label: t('apidef.unfiled', '未归类') }, ...modules.map((m) => ({ value: m.id, label: m.name }))]}
        />
      </Field>
      <Field label={t('apidef.importGroupByTag', '按标签建模块')}>
        <Checkbox checked={groupByTag} onChange={(e) => setGroupByTag(e.target.checked)}>
          {t('apidef.importGroupByTagHint', '按 OpenAPI 标签(tag)自动建/复用子模块并归类(上方「所属模块」作为其父级)')}
        </Checkbox>
      </Field>
      <Field label={t('apidef.importMode', '导入模式')}>
        <Radio.Group value={overwrite ? 'cover' : 'keep'} onChange={(e) => setOverwrite(e.target.value === 'cover')}>
          <Radio value="cover">
            {t('apidef.importCover', '覆盖')}
            <Tooltip title={t('apidef.importCoverHint', '同 方法+路径 已存在则刷新其规格')}><QuestionCircleOutlined style={{ color: '#bbb', marginLeft: 4 }} /></Tooltip>
          </Radio>
          <Radio value="keep">
            {t('apidef.importKeep', '不覆盖')}
            <Tooltip title={t('apidef.importKeepHint', '同 方法+路径 已存在则跳过')}><QuestionCircleOutlined style={{ color: '#bbb', marginLeft: 4 }} /></Tooltip>
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
      <Field label={t('apidef.importWay', '导入方式')}>
        <Segmented
          value={way}
          onChange={(v) => setWay(String(v))}
          options={[
            { label: t('apidef.importTypeFile', '文件导入'), value: 'file' },
            { label: t('apidef.importWayUrl', 'URL 导入'), value: 'url' },
          ]}
        />
      </Field>
      {way === 'url' ? (
        <>
          <Field label={<span><span style={{ color: '#ff4d4f', marginRight: 4 }}>*</span>SwaggerURL</span>}>
            <Input value={urlVal} onChange={(e) => setUrlVal(e.target.value)} className="ms-mono" placeholder={t('apidef.importUrlPlaceholder', '请输入 OpenAPI/URL')} />
          </Field>
          <Field label="token">
            <Input.TextArea rows={2} value={token} onChange={(e) => setToken(e.target.value)} className="ms-mono" placeholder="token" />
          </Field>
          <Space>
            <Switch checked={basicAuth} onChange={setBasicAuth} />
            <span style={{ fontSize: 13 }}>{t('apidef.basicAuth', 'Basic Auth 认证')}</span>
          </Space>
        </>
      ) : (
        <>
          <Upload.Dragger accept=".json,.yaml,.yml" beforeUpload={readFile} showUploadList={false} style={{ marginBottom: 10 }}>
            <p style={{ margin: 0 }}><InboxOutlined style={{ fontSize: 28, color: '#06a561' }} /></p>
            <p style={{ margin: '6px 0 0' }}>{t('apidef.uploadHint2', '拖拽或点击此区域选择文件')}</p>
            <p style={{ color: '#8a9099', fontSize: 12, margin: '4px 0 0' }}>
              {t('apidef.uploadHintSub', '支持 Swagger 3.0 版本的 json 文件,')}
              <span style={{ color: '#fa8c16' }}>{t('apidef.uploadHintConvert', '2.0 文件可以在官网一键转换 3.0,')}</span>
              {t('apidef.uploadHintSize', '大小不超过 50M')}
            </p>
            {fileName && <p style={{ color: '#06a561', margin: '4px 0 0' }}>{t('apidef.selectedFile', '已选:')}{fileName}</p>}
          </Upload.Dragger>
          <Input.TextArea rows={6} value={text} onChange={(e) => setText(e.target.value)} placeholder={t('apidef.pasteHint', '也可直接粘贴文档内容')} className="ms-mono" />
        </>
      )}
      <p style={{ color: '#8a9099', fontSize: 12, margin: '10px 0 0' }}>
        {t('apidef.importAutoHint', '导入将解析请求参数/必填/响应,并为每个接口自动生成带断言的默认用例。')}
      </p>
    </Drawer>
  )
}
