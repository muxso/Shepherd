import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import { Link, useSearchParams } from 'react-router-dom'
import { AutoComplete, Button, Divider, Dropdown, Empty, Form, Input, Modal, Popover, Radio, Segmented, Select, Space, Switch, Table, Tabs, Tag, Tooltip, Typography, Upload } from 'antd'
import ResizableDrawer from '../components/ResizableDrawer'
import EditDrawer from '../components/EditDrawer'
import { message, modal } from '../feedback'
import { PlayCircleOutlined, PlusOutlined, SaveOutlined, ThunderboltOutlined, DownOutlined, LinkOutlined, SwapOutlined, DeleteOutlined, FullscreenOutlined, CloseOutlined, SearchOutlined, FilterOutlined, ReloadOutlined, MoreOutlined, ImportOutlined, InboxOutlined, EyeOutlined, SettingOutlined, ShareAltOutlined, EditOutlined, StarFilled, StarOutlined } from '@ant-design/icons'
import { api, ApiError, runEventsWsUrl, type ApiCase, type ApiDefinition, type ApiModule, type ApiView, type AssertionResult, type DebugResponse, type Environment, type ReportResultItem, type ResourcePool, type RunEvent, type Scenario, type ScenarioChange, type ScenarioExecution, type ScenarioRunResult, type ScenarioStep } from '../api'
import type { ColumnsType } from 'antd/es/table'
import { useApp } from '../context'
import { methodColor, statusColor, outcomeColor, priorityColor, statusLabel, execStatusLabel, caseStatusLabel } from '../components/tags'
import { Workspace, useWorkTabs, useWorkspaceExtraSlot, useOpenParam } from '../components/Workspace'
import { ModuleTreePanel, inSelectedModule } from '../components/ModuleTreePanel'
import { columnSearch } from '../components/ListView'
import AssertionEditor from '../components/AssertionEditor'
import ProcessorEditor from '../components/ProcessorEditor'
import KVEditor, { type KVRow } from '../components/KVEditor'
import { DebugResultPanel, type SentRequest } from '../components/ApiSpecPanel'
import { LatencyStat } from '../components/TimingBreakdown'
import { SelectProjectEmpty } from '../components/Page'
import type { RegItem } from '../registry'
import { fetchPlanItems, groupIdOf, isGroup } from '../components/plan/planLocal'
import { ScenarioReportModal, fmtDuration, fmtSize, makeStepMeta, type NameOf, type TFn } from '../components/ScenarioReport'
import { useI18n } from '../i18n'

// Editable form state + scenario param rows (persisted in scenario.meta).
type ScenarioParam = { name: string; type: string; value: string; tags: string; desc: string }
type ScenarioForm = { name: string; status: string; description: string; tags: string[]; priority: string; params: ScenarioParam[]; csv: string; moduleId: string; disabledSteps: string[]; preProcessors: unknown[]; postProcessors: unknown[]; assertions: unknown[]; envCookie: boolean; sharedCookie: boolean }
const SCENARIO_STATUSES = ['DRAFT', 'DEBUGGING', 'COMPLETED', 'DEPRECATED']
const SCENARIO_PRIORITIES = ['P0', 'P1', 'P2', 'P3']

// Localized labels for scenario status / run outcome (shared helpers in components/tags.ts).
const scStatusLabel = statusLabel
const runOutcomeLabel = execStatusLabel

// -- List views + advanced filter (mirrors the API definition page; client-side filtering) --
// Views share the ms_api_view table (no page column); config.kind marks ownership and load filters by kind.
const SC_VIEW_KIND = 'scenario'
type ScAdvCond = { field: 'id' | 'name' | 'status' | 'priority' | 'tags'; op: 'contains' | 'notContains' | 'equals' | 'notEquals' | 'empty' | 'notEmpty'; value: string }
const SC_ADV_FIELDS: { value: ScAdvCond['field']; tkey: string; fallback: string }[] = [
  { value: 'id', tkey: 'scenario.filterFieldId', fallback: 'ID' },
  { value: 'name', tkey: 'scenario.filterFieldName', fallback: '场景名称' },
  { value: 'status', tkey: 'scenario.status', fallback: '状态' },
  { value: 'priority', tkey: 'scenario.filterFieldPriority', fallback: '场景等级' },
  { value: 'tags', tkey: 'scenario.filterFieldTags', fallback: '标签' },
]
const SC_ADV_OPS: { value: ScAdvCond['op']; tkey: string; fallback: string }[] = [
  { value: 'contains', tkey: 'scenario.opContains', fallback: '包含' },
  { value: 'notContains', tkey: 'scenario.opNotContains', fallback: '不包含' },
  { value: 'equals', tkey: 'scenario.opEq', fallback: '等于' },
  { value: 'notEquals', tkey: 'scenario.opNe', fallback: '不等于' },
  { value: 'empty', tkey: 'scenario.opEmpty', fallback: '为空' },
  { value: 'notEmpty', tkey: 'scenario.opNotEmpty', fallback: '不为空' },
]
function scFieldVal(s: Scenario, f: ScAdvCond['field']): string {
  if (f === 'id') return s.id || ''
  if (f === 'name') return s.name || ''
  if (f === 'status') return s.status || ''
  if (f === 'priority') return (s.meta?.priority as string) || 'P0'
  return ((s.meta?.tags as string[] | undefined) || []).join(',')
}
function scCondMatch(s: Scenario, c: ScAdvCond): boolean {
  const a = scFieldVal(s, c.field).toLowerCase()
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
type ScViewConfig = { kind?: string; search?: string; selModule?: string; pageSize?: number; hiddenCols?: string[]; advLogic?: 'all' | 'any'; advConds?: ScAdvCond[] }

export default function Scenarios() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const [list, setList] = useState<Scenario[]>([])
  const [modules, setModules] = useState<ApiModule[]>([])
  const [loading, setLoading] = useState(false)
  const [search, setSearch] = useState('')
  const [moduleSearch, setModuleSearch] = useState('')
  const [selModule, setSelModule] = useState('ALL') // ALL | UNFILED | <moduleId>
  const [importOpen, setImportOpen] = useState(false)
  // List state: page size / column visibility / advanced filter / views (all client-side, mirrors the API definition page).
  const [pageSize, setPageSize] = useState(20)
  const [hiddenCols, setHiddenCols] = useState<string[]>([])
  const [advOpen, setAdvOpen] = useState(false)
  const [advLogic, setAdvLogic] = useState<'all' | 'any'>('all')
  const [advConds, setAdvConds] = useState<ScAdvCond[]>([])
  const [advApplied, setAdvApplied] = useState<{ logic: 'all' | 'any'; conds: ScAdvCond[] }>({ logic: 'all', conds: [] })
  const [views, setViews] = useState<ApiView[]>([])
  const [activeViewId, setActiveViewId] = useState<string | null>(null)
  const [viewName, setViewName] = useState('')
  const [viewPopOpen, setViewPopOpen] = useState(false)
  const [searchParams, setSearchParams] = useSearchParams()
  // Batch selection + move/copy-to-module dialogs (bottom action bar, mirrors the reference list).
  const [selectedIds, setSelectedIds] = useState<React.Key[]>([])
  const [followedIds, setFollowedIds] = useState<Set<string>>(new Set())
  const [batchModal, setBatchModal] = useState<'move' | 'copy' | null>(null)
  const [batchModule, setBatchModule] = useState('')
  const [batchBusy, setBatchBusy] = useState(false)
  // Batch run dialog: env override / serial-parallel / stop-on-fail / report mode / pool.
  const [batchRunOpen, setBatchRunOpen] = useState(false)
  const [runEnvMode, setRunEnvMode] = useState<'default' | 'new'>('default')
  const [runEnvId, setRunEnvId] = useState<string>()
  const [runMode, setRunMode] = useState<'serial' | 'parallel'>('serial')
  const [stopOnFail, setStopOnFail] = useState(false)
  const [reportMode, setReportMode] = useState<'independent' | 'union'>('independent')
  const [reportName, setReportName] = useState('')
  const [runPool, setRunPool] = useState('')
  const [runEnvs, setRunEnvs] = useState<Environment[]>([])
  const [runPools, setRunPools] = useState<ResourcePool[]>([])
  // Union batch report viewer (list level): case names resolved lazily.
  const [batchReportId, setBatchReportId] = useState<string | null>(null)
  const [listCaseMap, setListCaseMap] = useState<Record<string, ApiCase>>({})
  useEffect(() => {
    if (!batchReportId || Object.keys(listCaseMap).length) return
    api.projectCasesAll(projectId)
      .then((cs) => setListCaseMap(Object.fromEntries(cs.map((c) => [c.id, c]))))
      .catch(() => setListCaseMap({}))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [batchReportId])
  // Schedule-via-plan dialog: mount selected scenarios into a test plan and set the plan cron.
  const [timerOpen, setTimerOpen] = useState(false)
  const [timerPlanId, setTimerPlanId] = useState<string>()
  const [timerNewName, setTimerNewName] = useState('')
  const [timerCron, setTimerCron] = useState('')
  const [timerEnabled, setTimerEnabled] = useState(true)
  const [timerPlans, setTimerPlans] = useState<RegItem[]>([])
  const tabs = useWorkTabs()
  useOpenParam((id) => tabs.open(id)) // deep link ?open=<scenarioId> (reference graph clicks land here)
  const NEW_KEY = '__new_scenario__'

  const load = async () => {
    if (!projectId) { setList([]); setModules([]); setViews([]); return }
    setLoading(true)
    try {
      const [ss, mm, vs] = await Promise.all([
        api.scenarios(projectId),
        api.modules(projectId).catch(() => []),
        api.views(projectId).catch(() => []),
      ])
      setList(Array.isArray(ss) ? ss : [])
      setModules(Array.isArray(mm) ? mm : [])
      // Followed scenario ids for the star action; failure just leaves stars empty.
      api.myFollows(projectId, 'SCENARIO')
        .then((r) => setFollowedIds(new Set(r.entityIds)))
        .catch(() => {})
      // lastResult comes with list_scenarios; no per-scenario request needed.
      // Only show views owned by this page (config.kind === 'scenario') to avoid mixing with API definition views.
      setViews(Array.isArray(vs) ? vs.filter((v) => (v.config as ScViewConfig)?.kind === SC_VIEW_KIND) : [])
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('scenario.loadFailed', '加载场景失败'))
    } finally {
      setLoading(false)
    }
  }

  // View snapshot: current filters/columns/paging into config; applyConfig writes it back.
  const currentConfig = (): ScViewConfig => ({ kind: SC_VIEW_KIND, search, selModule, pageSize, hiddenCols, advLogic: advApplied.logic, advConds: advApplied.conds })
  const applyConfig = (c: ScViewConfig) => {
    if (typeof c.search === 'string') setSearch(c.search)
    if (typeof c.selModule === 'string') setSelModule(c.selModule)
    if (typeof c.pageSize === 'number') setPageSize(c.pageSize)
    if (Array.isArray(c.hiddenCols)) setHiddenCols(c.hiddenCols)
    const logic = c.advLogic ?? 'all'
    const conds = Array.isArray(c.advConds) ? c.advConds : []
    setAdvApplied({ logic, conds })
    setAdvLogic(logic)
    setAdvConds(conds)
  }
  const applyView = (v: ApiView) => {
    applyConfig(v.config as ScViewConfig)
    setActiveViewId(v.id)
    setViewPopOpen(false)
    message.success(t('apidef.viewApplied', '已应用视图') + `「${v.name}」`)
  }
  const saveView = async () => {
    const name = viewName.trim()
    if (!name) return message.warning(t('apidef.viewNameRequired', '请输入视图名称'))
    if (!projectId) return
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
  // Deep link ?view=<id>: apply once views load, then strip the param.
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
    tabs.reset()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId])

  const moduleOf = (s: Scenario) => (s.meta?.moduleId as string) || ''

  const filtered = useMemo(() => {
    const q = search.toLowerCase()
    const conds = advApplied.conds.filter((c) => c.op === 'empty' || c.op === 'notEmpty' || c.value.trim())
    return list.filter((s) => {
      const inMod = inSelectedModule(modules, selModule, moduleOf(s))
      const tags = (s.meta?.tags as string[] | undefined) || []
      const hit = !q || s.name.toLowerCase().includes(q) || s.id.toLowerCase().includes(q) || tags.some((tg) => tg.toLowerCase().includes(q))
      // Advanced filter: all = AND across conditions; any = OR.
      const adv = conds.length === 0 ? true : advApplied.logic === 'all' ? conds.every((c) => scCondMatch(s, c)) : conds.some((c) => scCondMatch(s, c))
      return inMod && hit && adv
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [list, search, selModule, modules, advApplied])

  if (!projectId) return <SelectProjectEmpty />

  // Left panel: new/import (header) + shared ModuleTreePanel (search + tree + module CRUD) + recycle bin (footer).
  const left = (
    <ModuleTreePanel
      projectId={projectId}
      modules={modules}
      items={list}
      getModuleId={moduleOf}
      selectedKey={selModule}
      onSelect={setSelModule}
      allLabel={t('scenario.allScenarios', '全部场景')}
      unfiledLabel={t('scenario.unplanned', '未规划场景')}
      moduleSearch={moduleSearch}
      onModuleSearch={setModuleSearch}
      searchPlaceholder={t('scenario.moduleSearchPh', '请输入模块名称进行搜索')}
      onModulesChanged={load}
      deleteModuleContent={t('scenario.deleteModuleContent', '其下场景将变为未规划(不会删除场景)。')}
      header={
        <div style={{ padding: '10px 10px 0' }}>
          {/* Import is icon-only: two text buttons overflow the left column with the longer English labels. */}
          <Space.Compact style={{ width: '100%' }}>
            <Button type="primary" icon={<PlusOutlined />} style={{ flex: 1, minWidth: 0 }} onClick={() => tabs.open(NEW_KEY)}>{t('scenario.newScenario', '新建场景')}</Button>
            <Tooltip title={t('scenario.importScenario', '导入场景')}>
              <Button icon={<ImportOutlined />} style={{ flex: '0 0 40px' }} onClick={() => setImportOpen(true)} />
            </Tooltip>
          </Space.Compact>
        </div>
      }
      footer={<div style={{ padding: '8px 14px', borderTop: '1px solid var(--border-soft)', fontSize: 12 }}><Link to="/api/scenario/recycle-bin" style={{ color: 'var(--text-3)' }}>🗑 {t('scenario.recycleBin', '回收站')}</Link></div>}
    />
  )

  const runFromList = async (s: Scenario, e: React.MouseEvent) => {
    e.stopPropagation()
    try {
      const r = await api.runScenario(s.id, s.projectId)
      message.success(`${t('scenario.triggered', '场景已触发执行')} · ${r.status}`)
    } catch (err) {
      message.error(err instanceof ApiError ? `${t('scenario.execFailed', '执行失败')}:${err.status}` : t('scenario.execFailed', '执行失败'))
    }
  }
  const removeScenario = (s: Scenario) => {
    Modal.confirm({
      title: t('scenario.deleteConfirmTitle', '删除场景?'),
      content: t('scenario.deleteConfirmBody', '将删除该场景及其全部步骤,且不可恢复。'),
      okType: 'danger',
      okText: t('a.delete', '删除'),
      cancelText: t('a.cancel', '取消'),
      onOk: async () => {
        try {
          await api.deleteScenario(s.id)
          message.success(t('scenario.deleted', '已删除'))
          tabs.close(s.id)
          load()
        } catch (e) {
          message.error(e instanceof ApiError ? e.message : t('scenario.deleteFailed', '删除失败'))
        }
      },
    })
  }
  // Optimistic star toggle; revert and warn on failure.
  const toggleFollow = async (s: Scenario) => {
    const was = followedIds.has(s.id)
    const apply = (on: boolean) =>
      setFollowedIds((prev) => {
        const n = new Set(prev)
        if (on) n.add(s.id)
        else n.delete(s.id)
        return n
      })
    apply(!was)
    const b = { projectId: s.projectId, entityType: 'SCENARIO', entityId: s.id }
    try {
      const st = was ? await api.unfollow(b) : await api.follow(b)
      apply(st.following)
      message.success(st.following ? t('follow.followed', '已关注') : t('follow.unfollowed', '已取消关注'))
    } catch {
      apply(was)
      message.error(t('follow.failed', '关注操作失败'))
    }
  }
  const muted = (v?: string) => <span style={{ color: 'var(--text-3)' }}>{v || '—'}</span>
  // Column-header search/filter (mirrors the API definition page): text columns get magnifier search, enum columns funnel multi-select; stacks with the top search/filter.
  const allSceneTags = [...new Set(filtered.flatMap((s) => ((s.meta?.tags as string[] | undefined) || [])))]
  const allSceneEnvs = [...new Set(filtered.map((s) => (s.meta?.envName as string | undefined) || '').filter(Boolean))]
  const richCols: ColumnsType<Scenario> = [
    { key: 'id', title: 'ID', dataIndex: 'num', width: 110, sorter: (a, b) => (a.num || 0) - (b.num || 0), render: (v: number, s) => <span className="ms-mono" style={{ color: 'var(--brand)', fontSize: 12 }}>{v || s.id.slice(0, 8)}</span>, ...columnSearch<Scenario>((s) => String(s.num || s.id), t) },
    { key: 'name', title: t('scenario.colSceneName', '场景名称'), dataIndex: 'name', ellipsis: true, sorter: (a, b) => a.name.localeCompare(b.name), render: (v: string) => <span style={{ fontWeight: 500 }}>{v}</span>, ...columnSearch<Scenario>((s) => s.name, t) },
    { key: 'priority', title: t('scenario.priority', '场景等级'), width: 110, render: (_v, s) => { const p = (s.meta?.priority as string) || 'P0'; return <span style={{ color: priorityColor(p) }}>● {p}</span> }, filters: SCENARIO_PRIORITIES.map((p) => ({ text: p, value: p })), onFilter: (v, s) => ((s.meta?.priority as string) || 'P0') === v },
    { key: 'status', title: t('scenario.colStatus', '状态'), dataIndex: 'status', width: 110, render: (s: string) => <Tag color={statusColor(s)}>{scStatusLabel(s, t)}</Tag>, filters: SCENARIO_STATUSES.map((s) => ({ text: scStatusLabel(s, t), value: s })), onFilter: (v, s) => s.status === v },
    { key: 'execResult', title: t('scenario.colExecResult', '执行结果'), width: 110, render: (_v, s) => (s.lastResult ? <Tag color={outcomeColor(s.lastResult)} style={{ margin: 0 }}>{runOutcomeLabel(s.lastResult, t)}</Tag> : muted()), filters: [{ text: t('scenario.runSuccess', '成功'), value: 'SUCCESS' }, { text: t('scenario.runError', '失败'), value: 'ERROR' }], onFilter: (v, s) => runOutcomeLabel(s.lastResult || '', t) === runOutcomeLabel(String(v), t) },
    { key: 'tags', title: t('scenario.tags', '标签'), width: 160, render: (_v, s) => { const tags = (s.meta?.tags as string[] | undefined) || []; return tags.length ? <Space size={[4, 4]} wrap>{tags.map((tg) => <Tag key={tg} style={{ margin: 0 }}>{tg}</Tag>)}</Space> : muted() }, filters: allSceneTags.map((tg) => ({ text: tg, value: tg })), filterSearch: allSceneTags.length > 8, onFilter: (v, s) => (((s.meta?.tags as string[] | undefined) || [])).includes(String(v)) },
    { key: 'sceneEnv', title: t('scenario.colSceneEnv', '场景环境'), width: 130, render: (_v, s) => { const en = s.meta?.envName as string | undefined; return en ? <Tag color="blue" style={{ margin: 0 }}>{en}</Tag> : muted() }, filters: allSceneEnvs.map((e) => ({ text: e, value: e })), onFilter: (v, s) => ((s.meta?.envName as string | undefined) || '') === v },
    { key: 'createdBy', title: t('scenario.createdBy', '创建人'), dataIndex: 'createdBy', width: 110, render: (v?: string) => muted(v || undefined), ...columnSearch<Scenario>((s) => s.createdBy || '', t) },
    { key: 'updatedBy', title: t('scenario.updatedBy', '更新人'), width: 110, render: (_v, s) => muted(s.createdBy || undefined) },
    {
      key: 'action',
      title: t('apidef.colAction', '操作'),
      width: 190,
      fixed: 'right',
      render: (_v, s) => (
        <Space size={4} onClick={(e) => e.stopPropagation()}>
          <Tooltip title={followedIds.has(s.id) ? t('follow.unfollow', '取消关注') : t('follow.follow', '关注')}>
            <Button
              type="text"
              size="small"
              icon={followedIds.has(s.id) ? <StarFilled style={{ color: 'var(--warning, #ff7d00)' }} /> : <StarOutlined />}
              onClick={() => toggleFollow(s)}
            />
          </Tooltip>
          <Button type="link" size="small" onClick={() => tabs.open(s.id)}>{t('a.edit', '编辑')}</Button>
          <Button type="link" size="small" onClick={(e) => runFromList(s, e)}>{t('apidef.run', '执行')}</Button>
          <Button type="link" size="small" onClick={async () => { try { await api.copyScenario(s.id); message.success(t('scenario.copied', '已复制')); load() } catch (e2) { message.error(e2 instanceof ApiError ? e2.message : t('scenario.copyFailed', '复制失败')) } }}>{t('a.copy', '复制')}</Button>
          <Dropdown menu={{ items: [{ key: 'del', label: t('a.delete', '删除'), danger: true }], onClick: ({ key }) => { if (key === 'del') removeScenario(s) } }}><Button type="link" size="small" icon={<MoreOutlined />} /></Dropdown>
        </Space>
      ),
    },
  ]
  // Column visibility: ID/name/action are fixed; the rest toggle in table settings.
  const columns = richCols.filter((c) => !hiddenCols.includes(String(c.key)))
  const TOGGLE_COLS = richCols.filter((c) => !['id', 'name', 'action'].includes(String(c.key))).map((c) => ({ key: String(c.key), label: String(c.title) }))

  // Bottom batch actions over the selection: export / edit / run / move-to / copy-to / (timers, delete) / clear.
  const selectedRows = list.filter((s) => selectedIds.includes(s.id))
  const clearSel = () => setSelectedIds([])
  const batchExport = () => {
    const blob = new Blob([JSON.stringify(selectedRows, null, 2)], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = 'scenarios.json'
    a.click()
    URL.revokeObjectURL(url)
  }
  const openBatchRun = () => {
    setRunEnvMode('default'); setRunEnvId(undefined); setRunMode('serial'); setStopOnFail(false)
    setReportMode('independent'); setReportName(''); setRunPool('')
    setBatchRunOpen(true)
    api.environments(projectId).then((e) => setRunEnvs(Array.isArray(e) ? e : [])).catch(() => setRunEnvs([]))
    api.resourcePools().then((p) => setRunPools(Array.isArray(p) ? p : [])).catch(() => setRunPools([]))
  }
  const batchRun = async () => {
    if (runEnvMode === 'new' && !runEnvId) return message.warning(t('scenario.pickNewEnv', '请选择新环境'))
    if (reportMode === 'union' && !reportName.trim()) return message.warning(t('scenario.reportNameRequired', '请输入报告名称'))
    setBatchBusy(true)
    try {
      const r = await api.batchRunScenarios({
        projectId,
        scenarioIds: selectedRows.map((s) => s.id),
        environmentId: runEnvMode === 'new' ? runEnvId : undefined,
        mode: runMode === 'parallel' ? 'PARALLEL' : 'SERIAL',
        stopOnFail,
        unionReport: reportMode === 'union',
        reportName: reportMode === 'union' ? reportName.trim() : undefined,
        poolId: runPool || undefined,
      })
      setBatchRunOpen(false)
      message.success(`${t('scenario.batchRunDone', '批量执行完成')}: ${r.success}/${r.total}`)
      // Union mode: open the combined report right away.
      if (r.reportId) setBatchReportId(r.reportId)
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('scenario.runFailed2', '执行失败'))
    } finally {
      setBatchBusy(false)
    }
  }
  const batchDelete = () => {
    Modal.confirm({
      title: `${t('scenario.batchDeleteConfirm', '确认删除选中的场景?')} (${selectedRows.length})`,
      okType: 'danger',
      okText: t('a.delete', '删除'),
      cancelText: t('a.cancel', '取消'),
      onOk: async () => {
        for (const s of selectedRows) { try { await api.deleteScenario(s.id); tabs.close(s.id) } catch { /* partial delete surfaces via reload */ } }
        clearSel()
        load()
      },
    })
  }
  // Scheduled execution goes through test plans: pick (or create) a plan, mount the
  // selected scenarios as plan cases, then configure the plan cron.
  const NEW_PLAN_VALUE = '__new_plan__'
  const openTimerEdit = () => {
    setTimerPlanId(undefined); setTimerNewName(''); setTimerCron(''); setTimerEnabled(true)
    setTimerPlans([])
    fetchPlanItems(projectId).then(setTimerPlans).catch(() => setTimerPlans([]))
    setTimerOpen(true)
  }
  const saveTimer = async () => {
    if (!timerPlanId) return message.warning(t('scenario.planRequired', '请选择测试计划'))
    if (timerPlanId === NEW_PLAN_VALUE && !timerNewName.trim()) return message.warning(t('scenario.planNameRequired', '请输入计划名称'))
    if (!timerCron.trim()) return message.warning(t('scenario.cronRequired', '请输入任务触发时间'))
    setBatchBusy(true)
    try {
      let planId = timerPlanId
      if (planId === NEW_PLAN_VALUE) {
        const p = await api.createPlan({ projectId, name: timerNewName.trim(), type: 'TEST_PLAN' })
        planId = p.id
      }
      for (const s of selectedRows) await api.linkPlanCase(planId, s.id, s.name)
      await api.planSchedule(planId, timerCron.trim(), timerEnabled)
      message.success(t('scenario.planTimerSaved', '已挂入计划并配置定时'))
      setTimerOpen(false)
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('func.saveFailed', '保存失败'))
    } finally {
      setBatchBusy(false)
    }
  }
  // Plan picker options: plan groups become optgroups; "new plan" pinned on top.
  const timerPlanOptions = (() => {
    const plans = timerPlans.filter((p) => !isGroup(p))
    const groups = timerPlans.filter(isGroup)
    const opt = (p: RegItem) => ({ value: p.id, label: p.label })
    const grouped = groups
      .map((g) => ({ label: g.label, options: plans.filter((p) => groupIdOf(p) === g.id).map(opt) }))
      .filter((g) => g.options.length)
    const ungrouped = plans.filter((p) => !groups.some((g) => g.id === groupIdOf(p))).map(opt)
    return [{ value: NEW_PLAN_VALUE, label: `+ ${t('scenario.newPlanOption', '新建计划')}` }, ...ungrouped, ...grouped]
  })()
  const batchMoveCopy = async () => {
    setBatchBusy(true)
    try {
      for (const s of selectedRows) {
        if (batchModal === 'move') {
          await api.updateScenario(s.id, { name: s.name, status: s.status, meta: { ...(s.meta || {}), moduleId: batchModule || undefined } })
        } else {
          const copy = await api.copyScenario(s.id)
          await api.updateScenario(copy.id, { name: copy.name, status: copy.status, meta: { ...(copy.meta || {}), moduleId: batchModule || undefined } })
        }
      }
      message.success(batchModal === 'move' ? t('scenario.moved', '已移动') : t('scenario.copied', '已复制'))
      setBatchModal(null)
      clearSel()
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('func.saveFailed', '保存失败'))
    } finally {
      setBatchBusy(false)
    }
  }

  const listContent = (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '10px 12px', borderBottom: '1px solid var(--border-soft)' }}>
        <div style={{ flex: 1 }} />
        <Input allowClear prefix={<SearchOutlined style={{ color: 'var(--text-3)' }} />} placeholder={t('scenario.searchByIdNameTag', '通过 ID/名称/标签搜索')} style={{ width: 260 }} value={search} onChange={(e) => setSearch(e.target.value)} />
        <Popover
          trigger="click"
          placement="bottomRight"
          open={viewPopOpen}
          onOpenChange={setViewPopOpen}
          title={t('apidef.views', '视图')}
          content={
            <div style={{ width: 268 }}>
              {views.length === 0 ? (
                <div style={{ color: 'var(--text-3)', fontSize: 12, padding: '2px 0 8px' }}>{t('apidef.noViews', '暂无视图,保存当前筛选为视图')}</div>
              ) : (
                <Space direction="vertical" size={2} style={{ width: '100%' }}>
                  {views.map((v) => (
                    <div key={v.id} style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                      <a style={{ flex: 1, fontWeight: v.id === activeViewId ? 600 : 400, color: v.id === activeViewId ? 'var(--brand)' : undefined, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }} onClick={() => applyView(v)} title={v.name}>
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
            {t('scenario.view', '视图')}{activeViewId ? `: ${views.find((v) => v.id === activeViewId)?.name ?? ''}` : ''}
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
              <div style={{ fontSize: 12, color: 'var(--text-3)', marginBottom: 6 }}>{t('apidef.pageSize', '每页显示数量')}</div>
              <Segmented size="small" value={pageSize} onChange={(v) => setPageSize(Number(v))} options={[10, 20, 30, 50].map((n) => ({ label: String(n), value: n }))} style={{ marginBottom: 12 }} />
              <div style={{ fontSize: 12, color: 'var(--text-3)', marginBottom: 6 }}>{t('apidef.colSettings', '表头设置')}</div>
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
        <Table<Scenario>
          rowKey="id"
          size="middle"
          loading={loading}
          dataSource={filtered}
          columns={columns}
          scroll={{ x: 'max-content' }}
          rowSelection={{ type: 'checkbox', selectedRowKeys: selectedIds, onChange: setSelectedIds }}
          onRow={(s) => ({ onClick: () => tabs.open(s.id), style: { cursor: 'pointer' } })}
          pagination={{ pageSize, size: 'small', showSizeChanger: true, pageSizeOptions: ['10', '20', '30', '50'], onShowSizeChange: (_, s) => setPageSize(s), showTotal: (total) => `${t('apidef.totalPrefix', '共')} ${total} ${t('scenario.unit', '条')}` }}
          locale={{ emptyText: <Empty description={t('scenario.empty', '暂无场景')} /> }}
        />
      </div>
      {selectedIds.length > 0 && (
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '10px 16px', borderTop: '1px solid var(--border-soft)', background: 'var(--panel)' }}>
          <span>{t('scenario.selectedPrefix', '已选择')} {selectedIds.length} {t('scenario.unit', '条')}</span>
          <Button size="small" onClick={batchExport}>{t('func.export', '导出')}</Button>
          <Tooltip title={selectedIds.length !== 1 ? t('scenario.editOneOnly', '仅支持编辑单个场景') : undefined}>
            <Button size="small" disabled={selectedIds.length !== 1} onClick={() => tabs.open(String(selectedIds[0]))}>{t('a.edit', '编辑')}</Button>
          </Tooltip>
          <Button size="small" onClick={openBatchRun}>{t('apidef.run', '执行')}</Button>
          <Button size="small" onClick={() => { setBatchModule(''); setBatchModal('move') }}>{t('scenario.moveTo', '移动到')}</Button>
          <Button size="small" onClick={() => { setBatchModule(''); setBatchModal('copy') }}>{t('scenario.copyTo', '复制到')}</Button>
          <Dropdown
            menu={{
              items: [
                { key: 'timerPlan', label: t('scenario.timerViaPlan', '定时执行(测试计划)') },
                { key: 'del', label: t('a.delete', '删除'), danger: true },
              ],
              onClick: ({ key }) => {
                if (key === 'del') batchDelete()
                else if (key === 'timerPlan') openTimerEdit()
              },
            }}
          >
            <Button size="small" icon={<MoreOutlined />} />
          </Dropdown>
          <Button size="small" type="text" onClick={clearSel}>{t('scenario.clearSel', '清空')}</Button>
        </div>
      )}
      <ScenarioReportModal
        reportId={batchReportId}
        nameOf={(id) => listCaseMap[id]?.name || id}
        caseMap={listCaseMap}
        onClose={() => setBatchReportId(null)}
      />
      <EditDrawer
        open={timerOpen}
        title={`${t('scenario.timerViaPlan', '定时执行(测试计划)')}(${t('scenario.selectedShort', '已选')} ${selectedIds.length} ${t('scenario.unitScene', '条场景')})`}
        onCancel={() => setTimerOpen(false)}
        footer={
          <div style={{ display: 'flex', alignItems: 'center' }}>
            <Switch size="small" checked={timerEnabled} onChange={setTimerEnabled} />
            <span style={{ marginLeft: 8 }}>
              {t('scenario.timerStatus', '任务状态')}{' '}
              <Tooltip title={t('scenario.timerStatusTip', '开启后按触发时间自动执行')}><span style={{ color: 'var(--text-3)' }}>?</span></Tooltip>
            </span>
            <span style={{ flex: 1 }} />
            <Button onClick={() => setTimerOpen(false)}>{t('a.cancel', '取消')}</Button>
            <Button type="primary" style={{ marginLeft: 8 }} loading={batchBusy} onClick={saveTimer}>{t('a.save', '保存')}</Button>
          </div>
        }
      >
        <div style={{ marginBottom: 12, color: 'var(--text-3)', fontSize: 12 }}>
          {t('scenario.timerPlanNote', '所选场景将挂入测试计划,由计划按触发时间统一执行,报告见「测试计划」。')}
        </div>
        <div style={{ marginBottom: 16 }}>
          <div style={{ marginBottom: 8 }}>{t('scenario.pickPlan', '选择计划')} <span style={{ color: 'var(--error)' }}>*</span></div>
          <Select
            style={{ width: '100%' }}
            showSearch
            optionFilterProp="label"
            placeholder={t('a.pleaseSelect', '请选择')}
            value={timerPlanId}
            onChange={setTimerPlanId}
            options={timerPlanOptions}
          />
        </div>
        {timerPlanId === NEW_PLAN_VALUE && (
          <div style={{ marginBottom: 16 }}>
            <div style={{ marginBottom: 8 }}>{t('scenario.newPlanOption', '新建计划')} <span style={{ color: 'var(--error)' }}>*</span></div>
            <Input
              placeholder={t('scenario.planNamePh', '请输入计划名称')}
              value={timerNewName}
              onChange={(e) => setTimerNewName(e.target.value)}
            />
          </div>
        )}
        <div style={{ marginBottom: 8 }}>
          <div style={{ marginBottom: 8 }}>{t('scenario.cronLabel', '任务触发时间')} <span style={{ color: 'var(--error)' }}>*</span></div>
          <AutoComplete
            style={{ width: '100%' }}
            placeholder={t('scenario.cronPh', '可直接输入表达式')}
            value={timerCron}
            onChange={setTimerCron}
            options={[
              { value: '0 0 * * * *', label: `${t('scenario.cronHourly', '每小时')} (0 0 * * * *)` },
              { value: '0 0 0 * * *', label: `${t('scenario.cronDaily', '每天 0 点')} (0 0 0 * * *)` },
              { value: '0 0 12 * * *', label: `${t('scenario.cronNoon', '每天 12 点')} (0 0 12 * * *)` },
              { value: '0 0 0 * * 1', label: `${t('scenario.cronWeekly', '每周一 0 点')} (0 0 0 * * 1)` },
              { value: '0 */30 * * * *', label: `${t('scenario.cronHalfHour', '每 30 分钟')} (0 */30 * * * *)` },
            ]}
          />
        </div>
      </EditDrawer>
      <EditDrawer
        open={batchRunOpen}
        title={`${t('scenario.batchRun', '批量执行')}（${t('scenario.selectedShort', '已选')} ${selectedIds.length} ${t('scenario.unitScene', '条场景')}）`}
        onCancel={() => setBatchRunOpen(false)}
        footer={
          <>
            <Button onClick={() => setBatchRunOpen(false)}>{t('a.cancel', '取消')}</Button>
            <Button type="primary" loading={batchBusy} onClick={batchRun}>{t('apidef.run', '执行')}</Button>
          </>
        }
      >
        <div style={{ marginBottom: 16 }}>
          <div style={{ marginBottom: 8 }}>{t('scenario.envPick', '环境选择')}</div>
          <Radio.Group value={runEnvMode} onChange={(e) => setRunEnvMode(e.target.value)}>
            <Radio value="default">
              {t('scenario.envDefault', '默认环境')}{' '}
              <Tooltip title={t('scenario.envDefaultTip', '使用每个场景自身配置的环境')}><span style={{ color: 'var(--text-3)' }}>?</span></Tooltip>
            </Radio>
            <Radio value="new">{t('scenario.envNew', '新环境')}</Radio>
          </Radio.Group>
        </div>
        {runEnvMode === 'new' && (
          <div style={{ marginBottom: 16 }}>
            <div style={{ marginBottom: 8 }}>{t('scenario.envNew', '新环境')} <span style={{ color: 'var(--error)' }}>*</span></div>
            <Select
              style={{ width: '100%' }}
              placeholder={t('a.pleaseSelect', '请选择')}
              value={runEnvId}
              onChange={setRunEnvId}
              options={runEnvs.map((e) => ({ value: e.id, label: e.name }))}
            />
          </div>
        )}
        <div style={{ marginBottom: 16 }}>
          <div style={{ marginBottom: 8 }}>{t('scenario.runModeLabel', '模式')}</div>
          <Radio.Group value={runMode} onChange={(e) => setRunMode(e.target.value)}>
            <Radio value="serial">{t('scenario.serial', '串行')}</Radio>
            <Radio value="parallel">{t('scenario.parallel', '并行')}</Radio>
          </Radio.Group>
          {runMode === 'serial' && (
            <div style={{ marginTop: 10 }}>
              <Switch size="small" checked={stopOnFail} onChange={setStopOnFail} />
              <span style={{ marginLeft: 8 }}>{t('scenario.stopOnFail', '失败停止')}</span>
            </div>
          )}
        </div>
        <div style={{ marginBottom: 16 }}>
          <div style={{ marginBottom: 8 }}>{t('scenario.reportConfig', '报告配置')}</div>
          <Segmented
            value={reportMode}
            onChange={(v) => setReportMode(v as 'independent' | 'union')}
            options={[
              { value: 'independent', label: t('scenario.reportIndependent', '独立报告') },
              { value: 'union', label: t('scenario.reportUnion', '集合报告') },
            ]}
          />
        </div>
        {reportMode === 'union' && (
          <div style={{ marginBottom: 16 }}>
            <div style={{ marginBottom: 8 }}>{t('scenario.reportName', '报告名称')} <span style={{ color: 'var(--error)' }}>*</span></div>
            <Input placeholder={t('a.pleaseInput', '请输入')} value={reportName} onChange={(e) => setReportName(e.target.value)} />
          </div>
        )}
        <div style={{ marginBottom: 8 }}>
          <div style={{ marginBottom: 8 }}>{t('scenario.poolRun', '资源池运行')} <span style={{ color: 'var(--error)' }}>*</span></div>
          <Select
            style={{ width: '100%' }}
            value={runPool}
            onChange={setRunPool}
            options={[{ value: '', label: t('scenario.defaultPool', '默认资源池') }, ...runPools.map((p) => ({ value: p.id, label: p.name }))]}
          />
        </div>
      </EditDrawer>
      <EditDrawer
        open={!!batchModal}
        title={batchModal === 'move' ? t('scenario.moveTo', '移动到') : t('scenario.copyTo', '复制到')}
        onCancel={() => setBatchModal(null)}
        onOk={batchMoveCopy}
        confirmLoading={batchBusy}
        okText={t('a.confirm', '确定')}
        cancelText={t('a.cancel', '取消')}
      >
        <Select
          style={{ width: '100%' }}
          value={batchModule}
          onChange={setBatchModule}
          options={[{ value: '', label: t('scenario.unplanned', '未规划场景') }, ...modules.map((m) => ({ value: m.id, label: m.name }))]}
        />
      </EditDrawer>
    </div>
  )

  const detailTabs = tabs.openIds.flatMap((id) => {
    if (id === NEW_KEY)
      return [{
        key: NEW_KEY,
        label: t('scenario.newScenario', '新建场景'),
        children: <NewScenarioTab projectId={projectId} modules={modules} active={tabs.activeKey === NEW_KEY} onCreated={(s) => { tabs.close(NEW_KEY); load().then(() => tabs.open(s.id)) }} />,
      }]
    const s = list.find((x) => x.id === id)
    return s ? [{ key: s.id, label: s.name, children: <ScenarioDetail scenario={s} active={tabs.activeKey === s.id} /> }] : []
  })

  return (
    <>
      <Workspace
        left={left}
        leftWidth={252}
        siderKey="scenario-sider"
        listLabel={t('scenario.allScenarios', '全部场景')}
        activeKey={tabs.activeKey}
        onChange={tabs.setActiveKey}
        onClose={tabs.close}
        tabs={detailTabs}
        listContent={listContent}
      />
      <ImportScenarioDrawer open={importOpen} projectId={projectId} modules={modules} onClose={() => setImportOpen(false)} onImported={load} />
      {/* Advanced filter drawer (all/any logic + field/operator/value, client-side filtering). */}
      <ResizableDrawer
        title={t('apidef.filter', '筛选')}
        open={advOpen}
        onClose={() => setAdvOpen(false)}
        width={460}
        footer={
          <div style={{ textAlign: 'right' }}>
            <Space>
              <Button onClick={() => { setAdvConds([]); setAdvApplied({ logic: advLogic, conds: [] }) }}>{t('a.reset', '重置')}</Button>
              <Button type="primary" onClick={() => { setAdvApplied({ logic: advLogic, conds: advConds }); setAdvOpen(false) }}>{t('apidef.applyFilter', '保存并筛选')}</Button>
            </Space>
          </div>
        }
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
          <span style={{ color: 'var(--text-2)' }}>{t('apidef.matchCond', '符合以下条件')}</span>
          <Select value={advLogic} onChange={(v) => setAdvLogic(v)} style={{ width: 90 }} options={[{ value: 'all', label: t('apidef.all', '所有') }, { value: 'any', label: t('apidef.any', '任一') }]} />
        </div>
        <Space direction="vertical" style={{ width: '100%' }} size={8}>
          {advConds.map((c, i) => {
            const set = (p: Partial<ScAdvCond>) => setAdvConds((cs) => cs.map((x, idx) => (idx === i ? { ...x, ...p } : x)))
            const noValue = c.op === 'empty' || c.op === 'notEmpty'
            return (
              <Space.Compact key={i} style={{ width: '100%' }}>
                <Select value={c.field} onChange={(v) => set({ field: v })} style={{ width: 130 }} options={SC_ADV_FIELDS.map((f) => ({ value: f.value, label: t(f.tkey, f.fallback) }))} />
                <Select value={c.op} onChange={(v) => set({ op: v })} style={{ width: 110 }} options={SC_ADV_OPS.map((o) => ({ value: o.value, label: t(o.tkey, o.fallback) }))} />
                <Input value={c.value} disabled={noValue} onChange={(e) => set({ value: e.target.value })} placeholder={noValue ? '—' : t('apidef.filterValue', '值')} />
                <Button icon={<MoreOutlined />} onClick={() => setAdvConds((cs) => cs.filter((_, idx) => idx !== i))} danger />
              </Space.Compact>
            )
          })}
          <Button type="link" icon={<PlusOutlined />} onClick={() => setAdvConds((cs) => [...cs, { field: 'name', op: 'contains', value: '' }])} style={{ paddingLeft: 0 }}>
            {t('apidef.addCond', '添加条件')}
          </Button>
        </Space>
      </ResizableDrawer>
    </>
  )
}

interface Node {
  kind: string
  /** Copy provenance of a materialized request (COPY_CASE / COPY_API / COPY_SCENARIO). */
  source?: string
  content: ReactNode
  children?: Node[]
  /** Raw child step (controller children item / sub-scenario step); used to open the drawer on click. */
  raw?: ScenarioStep
  /** Run result (leaf = direct, parent = aggregated from descendants); drives green/red on child rows. */
  result?: ReportResultItem
}

/** Normalize a raw controller child json / sub-scenario step into a ScenarioStep the drawer can use. */
function rawToStep(c: any): ScenarioStep {
  const kind = String(c?.kind || '').toUpperCase()
  const base = { id: c?.id || `child-${kind}-${c?.refId || c?.url || Math.random().toString(36).slice(2)}`, order: 0, kind, refMode: 'REFERENCE' as const }
  if (kind === 'CASE') return { ...base, caseId: c.refId }
  if (kind === 'SCENARIO') return { ...base, scenarioId: c.refId }
  if (kind === 'REQUEST') return { ...base, request: { method: c.method || 'GET', url: c.url || '', body: c.body ?? null, assertions: c.assertions } }
  return { ...base, control: c }
}

function childToNode(c: any, t: TFn, nameOf: NameOf): Node {
  const kind = String(c?.kind || '').toUpperCase()
  if (kind === 'CASE') return { kind, content: <span className="ms-mono">{t('scenario.caseRef', '用例')} {nameOf(c.refId)}</span>, raw: rawToStep(c) }
  if (kind === 'REQUEST')
    return { kind, content: <Space><Tag color={methodColor(c.method || 'GET')}>{c.method || 'GET'}</Tag><span className="ms-mono">{c.url}</span></Space>, raw: rawToStep(c) }
  return { ...controlToNode(kind, c, t, nameOf), raw: rawToStep(c) }
}

function controlToNode(kind: string, payload: any, t: TFn, nameOf: NameOf): Node {
  const children = Array.isArray(payload?.children) ? payload.children.map((c: any) => childToNode(c, t, nameOf)) : []
  let content: ReactNode = ''
  if (kind === 'LOOP') content = `${t('scenario.loopPrefix', '循环')} ${payload?.times ?? 1} ${t('scenario.loopSuffix', '次')}`
  else if (kind === 'IF') content = <span className="ms-mono">{payload?.variable} {payload?.operator} {payload?.value}</span>
  else if (kind === 'ONCE') content = t('scenario.onceOnly', '仅执行一次')
  else if (kind === 'TIMER') content = `${t('scenario.waitPrefix', '等待')} ${payload?.ms ?? 0} ms`
  return { kind, content, children: kind === 'TIMER' ? undefined : children }
}

function stepToNode(s: ScenarioStep, t: TFn, nameOf: NameOf): Node {
  if (s.request) return { kind: 'REQUEST', source: s.request.source || undefined, content: <Space><Tag color={methodColor(s.request.method)}>{s.request.method}</Tag><span className="ms-mono">{s.request.url}</span></Space> }
  if (s.caseId) return { kind: 'CASE', content: <span className="ms-mono">{t('scenario.caseRef', '用例')} {nameOf(s.caseId)}</span> }
  if (s.scenarioId) return { kind: 'SCENARIO', source: s.refMode === 'COPY' ? 'COPY_SCENARIO' : undefined, content: <span className="ms-mono">{t('scenario.subScenario', '子场景')} {nameOf(s.scenarioId)}</span> }
  if (s.control) return controlToNode(s.kind.toUpperCase(), s.control, t, nameOf)
  return { kind: s.kind, content: '—' }
}

// Count-up duration for a step that is executing right now (live WS mode).
function LiveElapsed({ since }: { since: number }) {
  const [, tick] = useState(0)
  useEffect(() => {
    const id = window.setInterval(() => tick((n) => n + 1), 100)
    return () => window.clearInterval(id)
  }, [])
  return <span className="ms-mono" style={{ fontSize: 12, color: 'var(--brand)', whiteSpace: 'nowrap' }}>{fmtDuration(Math.max(Date.now() - since, 0))}</span>
}

function StepRow({ node, idx, depth, t, result, running, liveSince, seq = 0, enabled = true, onToggle, onRun, actions, hovered, respPreview, expandable, expanded, onChildSelect, onChildDblClick }: { node: Node; idx: number; depth: number; t: TFn; result?: ReportResultItem; running?: boolean; liveSince?: number; seq?: number; enabled?: boolean; onToggle?: () => void; onRun?: () => void; actions?: React.ReactNode; hovered?: boolean; respPreview?: React.ReactNode; expandable?: boolean; expanded?: boolean; onChildSelect?: (raw: ScenarioStep, path: number[]) => void; onChildDblClick?: (raw: ScenarioStep, path: number[]) => void }) {
  const copyLabels: Record<string, string> = {
    COPY_CASE: t('scenario.copyCaseTag', '复制用例'),
    COPY_API: t('scenario.copyApiTag', '复制API'),
    COPY_SCENARIO: t('scenario.copyScnTag', '复制场景'),
  }
  const meta = node.source
    ? { label: copyLabels[node.source] || t('scenario.copyTag', '复制'), color: 'green' }
    : makeStepMeta(t)[node.kind] || { label: node.kind, color: 'default' }
  const ok = result?.outcome === 'SUCCESS'
  const muted: React.CSSProperties = { color: 'var(--text-3)', fontSize: 12, whiteSpace: 'nowrap' }
  const leaf = depth === 0
  return (
    <>
      <div
        style={{
          position: 'relative',
          isolation: 'isolate', // own stacking context so the z-index:-1 progress fill sits above the row background but below content
          overflow: 'hidden',
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          padding: '8px 12px',
          marginLeft: depth * 24,
          border: '1px solid var(--border-soft)',
          borderRadius: 6,
          marginBottom: 6,
          background: depth ? 'var(--panel-2)' : 'var(--panel)',
          opacity: enabled ? 1 : 0.5,
        }}
      >
        {/* Run progress: row background fills left-to-right under the content. Blue while running, staggered by seq; green (pass) / red (fail) when done. */}
        {running ? (
          <span className={liveSince != null ? 'ms-step-fillbg live' : 'ms-step-fillbg run'} style={{ animationDelay: liveSince != null ? undefined : `${seq}s` }} />
        ) : result ? (
          <span className="ms-step-fillbg done" style={{ background: ok ? 'rgba(34,197,94,0.16)' : 'rgba(239,68,68,0.16)' }} />
        ) : null}
        {/* Expandable (sub-scenario/loop/if/once): fold arrow; leaf steps keep a spacer for alignment. */}
        {expandable
          ? <span style={{ color: 'var(--text-3)', fontSize: 11, width: 12, cursor: 'pointer' }}>{expanded ? '▾' : '▸'}</span>
          : <span style={{ width: 12 }} />}
        <span style={{ color: 'var(--text-3)', cursor: 'grab' }}>⠿</span>
        {/* Enable/disable this step (disabled = dimmed); play = single-step server run. Both stop propagation so the drawer stays closed. */}
        <Switch size="small" checked={enabled} disabled={!leaf || !onToggle} onChange={() => onToggle?.()} onClick={(_c, e) => e.stopPropagation()} />
        <PlayCircleOutlined
          style={{ color: leaf && onRun ? 'var(--brand)' : '#c9cdd4', cursor: leaf && onRun ? 'pointer' : 'default' }}
          onClick={(e) => { e.stopPropagation(); if (leaf) onRun?.() }}
        />
        <span style={{ color: 'var(--text-3)', fontSize: 12, minWidth: 18 }}>{idx}</span>
        <Tag color={meta.color} style={{ margin: 0 }}>{meta.label}</Tag>
        <span style={{ flex: 1, minWidth: 0 }}>{node.content}</span>
        {/* Live mode: elapsed time counts up while this step executes. */}
        {running && liveSince != null && <LiveElapsed since={liveSince} />}
        {/* Per-step result after a run (ref #28): pass / status / latency / size.
            Hovering the result cluster pops response details anchored right (where the mouse is). */}
        {!running && result && (() => {
          const cluster = (
            <span style={{ display: 'inline-flex', alignItems: 'center', gap: 8 }} onClick={(e) => e.stopPropagation()}>
              <Tag color={ok ? 'green' : 'red'} style={{ margin: 0 }}>{ok ? t('scenario.pass', '通过') : t('scenario.fail', '失败')}</Tag>
              {result.statusCode != null && <Tooltip title={t('scenario.statusTip', '服务端返回的 HTTP 状态码')}><span style={muted}>{t('apidef.statusCode', '状态码')} <span style={{ color: result.statusCode < 400 ? 'var(--success)' : 'var(--error)' }}>{result.statusCode}</span></span></Tooltip>}
              {result.timings ? (
                <LatencyStat totalMs={result.latencyMs ?? 0} timings={result.timings}>
                  <span style={muted}>{t('scenario.respTime', '响应时间')} {result.latencyMs != null ? fmtDuration(result.latencyMs) : '—'}</span>
                </LatencyStat>
              ) : (
                <Tooltip title={t('scenario.respTimeTip', '从建立连接到收到服务端完整响应的全链路耗时')}><span style={muted}>{t('scenario.respTime', '响应时间')} {result.latencyMs != null ? fmtDuration(result.latencyMs) : '—'}</span></Tooltip>
              )}
              <Tooltip title={t('scenario.respSizeTip', '响应体大小')}><span style={muted}>{t('scenario.respSize', '响应大小')} {result.respSize != null ? fmtSize(result.respSize) : '—'}</span></Tooltip>
            </span>
          )
          return respPreview
            ? <Popover content={respPreview} trigger="hover" placement="bottomRight" mouseEnterDelay={0.35}>{cluster}</Popover>
            : cluster
        })()}
        {/* Shown on hover: insert above / insert below / delete. */}
        {actions && (
          <span
            style={{ display: 'flex', gap: 2, marginLeft: 4, opacity: hovered ? 1 : 0, pointerEvents: hovered ? 'auto' : 'none', transition: 'opacity .15s' }}
            onClick={(e) => e.stopPropagation()}
          >
            {actions}
          </span>
        )}
      </div>
      {/* Children render only when expanded; clicking one opens the drawer (raw step bubbled via onChildSelect). */}
      {expanded && node.children?.map((c, i) => (
        <div
          key={i}
          onClick={(e) => { if (c.raw && onChildSelect) { e.stopPropagation(); onChildSelect(c.raw, [i]) } }}
          onDoubleClick={(e) => { if (c.raw && onChildDblClick) { e.stopPropagation(); onChildDblClick(c.raw, [i]) } }}
          style={{ cursor: c.raw && onChildSelect ? 'pointer' : 'default' }}
        >
          <StepRow
            node={c}
            idx={i + 1}
            depth={depth + 1}
            t={t}
            result={c.result}
            running={running}
            seq={seq + (i + 1) * 0.12}
            onChildSelect={onChildSelect && ((raw, p) => onChildSelect(raw, [i, ...p]))}
            onChildDblClick={onChildDblClick && ((raw, p) => onChildDblClick(raw, [i, ...p]))}
            expandable={(c.children?.length ?? 0) > 0}
            expanded
          />
        </div>
      ))}
    </>
  )
}

// Step response preview (hover popover): status line + body/headers tabs. Used only once the step has run.
function StepRespPreview({ r, t }: { r: ReportResultItem; t: TFn }) {
  const ok = r.outcome === 'SUCCESS'
  const items = [
    { key: 'body', label: t('apidef.respBody', '响应体'), children: (
      <pre className="ms-mono" style={{ margin: 0, maxHeight: 300, overflow: 'auto', fontSize: 12, background: 'var(--panel-2)', padding: 10, borderRadius: 6 }}>{r.body || '—'}</pre>
    ) },
    { key: 'headers', label: t('apidef.respHeaders', '响应头'), children: (
      (r.headers?.length ?? 0)
        ? <Table size="small" pagination={false} rowKey={(_, i) => String(i)} dataSource={(r.headers ?? []).map(([k, v]) => ({ k, v }))} columns={[{ title: t('editor.colName', '名'), dataIndex: 'k', width: 200 }, { title: t('editor.colValue', '值'), dataIndex: 'v', render: (v: string) => <span className="ms-mono">{v}</span> }]} />
        : <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('apidef.none', '无')} />
    ) },
  ]
  return (
    <div style={{ width: 560 }} onClick={(e) => e.stopPropagation()}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 8 }}>
        <Tag color={ok ? 'green' : 'red'} style={{ margin: 0 }}>{ok ? t('scenario.pass', '通过') : t('scenario.fail', '失败')}</Tag>
        {r.statusCode != null && <span style={{ fontSize: 13, fontWeight: 600, color: r.statusCode < 400 ? 'var(--success)' : 'var(--error)' }}>{r.statusCode}</span>}
        <span style={{ fontSize: 12, color: 'var(--text-3)' }}>{r.latencyMs ?? '—'} ms</span>
        <span style={{ fontSize: 12, color: 'var(--text-3)' }}>{r.respSize ?? '—'} bytes</span>
      </div>
      <Tabs size="small" items={items} />
    </div>
  )
}

// Scenario action bar (environment + server run + save): shared by the detail and new-scenario
// tabs, portaled into the tab-bar right slot via Workspace (ref #38). runDisabled greys out the
// run button (unsaved new scenario).
function ScenarioActionBar({ envs, envId, onEnv, running, onRun, onLocalRun, saving, onSave, runDisabled, envDisabled, runTitle, viewReport, pools, poolId, onPool, poolOnline, t }: {
  envs: Environment[]
  envId: string
  onEnv: (v: string) => void
  running?: boolean
  onRun?: () => void
  onLocalRun?: () => void
  saving?: boolean
  onSave: () => void
  runDisabled?: boolean
  envDisabled?: boolean
  runTitle?: string
  viewReport?: ReactNode
  pools?: ResourcePool[]
  poolId?: string
  onPool?: (v: string) => void
  poolOnline?: Record<string, number>
  t: TFn
}) {
  return (
    <>
      {viewReport}
      {/* Run target: a pool with connected runners executes remotely; empty = in-process. */}
      {pools && pools.length > 0 && onPool && (
        <Select
          size="small"
          value={poolId || undefined}
          onChange={(v) => onPool(v || '')}
          style={{ width: 190 }}
          allowClear
          placeholder={t('scenario.selectPool', '资源池(本机执行)')}
          options={pools.map((p) => {
            const n = poolOnline?.[p.id] ?? 0
            return { value: p.id, label: `${p.name}${n > 0 ? ` · ${n} ${t('scenario.runnersOnline', '在线')}` : ` · ${t('scenario.noRunner', '无在线执行机')}`}` }
          })}
        />
      )}
      <Select
        size="small"
        value={envId || undefined}
        onChange={onEnv}
        style={{ width: 200 }}
        disabled={envDisabled}
        placeholder={t('editor.selectEnv', '选择环境')}
        allowClear
        options={envs.map((e) => ({ value: e.id, label: e.baseUrl ? `${e.name} · ${e.baseUrl}` : e.name }))}
        notFoundContent={t('editor.noEnvConfigured', '未配置环境,去「环境」页新建')}
      />
      {runDisabled ? (
        <Button type="primary" icon={<ThunderboltOutlined />} disabled title={runTitle}>{t('apidef.serverRun', '服务端执行')}</Button>
      ) : (
        <Dropdown.Button
          type="primary"
          icon={<DownOutlined />}
          loading={running}
          onClick={onRun}
          menu={{ items: [{ key: 'local', label: t('apidef.localRun', '本地执行') }], onClick: () => onLocalRun?.() }}
        >
          <ThunderboltOutlined /> {t('apidef.serverRun', '服务端执行')}
        </Dropdown.Button>
      )}
      <Button type="default" icon={<SaveOutlined />} loading={saving} onClick={onSave}>{t('a.save', '保存')}</Button>
    </>
  )
}

// Scenario detail: tabbed editor shell (refs #20-#24) — basic info / steps / params / pre-post /
// assertions / exec history / change history / settings, plus the action-bar portal.
function ScenarioDetail({ scenario, active }: { scenario: Scenario; active?: boolean }) {
  const { t } = useI18n()
  const slot = useWorkspaceExtraSlot()
  const [steps, setSteps] = useState<ScenarioStep[]>([])
  const [running, setRunning] = useState(false)
  const [add, setAdd] = useState<string>('') // which add-step form is open
  const [importOpen, setImportOpen] = useState(false)
  const [customReqOpen, setCustomReqOpen] = useState(false)
  // Only non-reference steps are editable: inline custom requests (REQUEST kind).
  // Referenced cases/sub-scenarios stay read-only in the detail drawer.
  const [editReqStep, setEditReqStep] = useState<ScenarioStep | null>(null)
  // Controller-nested inline request: edited via the same drawer, saved by patching the parent
  // controller's payload (children have no step id of their own).
  const [editChild, setEditChild] = useState<{ parent: ScenarioStep; path: number[]; step: ScenarioStep } | null>(null)
  const [hoverStep, setHoverStep] = useState<string | null>(null) // hovered step row (shows inline insert/delete)
  const [expandedSteps, setExpandedSteps] = useState<Set<string>>(new Set())
  const [subSteps, setSubSteps] = useState<Record<string, ScenarioStep[]>>({}) // sub-scenario id to its steps (loaded on first expand)
  // Pending insert: target index + snapshot of step ids before the add (used to move the new block into place).
  const [pendingInsert, setPendingInsert] = useState<{ at: number; before: string[] } | null>(null)
  const [lastRun, setLastRun] = useState<ScenarioRunResult | null>(null)
  const [lastRunAt, setLastRunAt] = useState<string>('')
  // Per-step results after a run, keyed by caseId (REQUEST: "METHOD url", CASE: case_id) + report modal.
  const [stepResults, setStepResults] = useState<Record<string, ReportResultItem>>({})
  const [reportModalId, setReportModalId] = useState<string | null>(null)
  const [nameMap, setNameMap] = useState<Record<string, string>>({})
  const [caseMap, setCaseMap] = useState<Record<string, ApiCase>>({})
  // `child`: selected via a nested row (controller/sub-scenario child) — no directly patchable
  // step id, so the read-only drawer must not offer the edit entry (dblclick covers controllers).
  const [selStep, setSelStep] = useState<{ step: ScenarioStep; idx: number; child?: boolean } | null>(null)
  const [dragIdx, setDragIdx] = useState<number | null>(null)
  // Run config: environment + step failure rule (backend run accepts environment_id/failure_strategy).
  const [envs, setEnvs] = useState<Environment[]>([])
  const [envId, setEnvId] = useState<string>('')
  const [failureStrategy, setFailureStrategy] = useState<'CONTINUE' | 'STOP'>('CONTINUE')
  // Pool routing: a pool with a connected runner executes remotely; empty = in-process.
  const [pools, setPools] = useState<ResourcePool[]>([])
  const [poolOnline, setPoolOnline] = useState<Record<string, number>>({})
  const [poolId, setPoolId] = useState<string>('')
  // Live run state (async run + WS events): per-leaf running flag + start time for the count-up.
  const [liveMode, setLiveMode] = useState(false)
  const [liveSteps, setLiveSteps] = useState<Record<string, { running: boolean; startedAt: number }>>({})
  // Editable basic info + params (saved via PATCH /api/scenario/{id}; meta carries description/tags/priority/params).
  const m0 = (scenario.meta || {}) as Record<string, unknown>
  const [form, setForm] = useState<ScenarioForm>({
    name: scenario.name,
    status: scenario.status,
    description: typeof m0.description === 'string' ? m0.description : '',
    tags: Array.isArray(m0.tags) ? (m0.tags as string[]) : [],
    priority: typeof m0.priority === 'string' ? m0.priority : 'P0',
    params: Array.isArray(m0.params) ? (m0.params as ScenarioParam[]) : [],
    csv: typeof m0.csvParams === 'string' ? (m0.csvParams as string) : '',
    moduleId: typeof m0.moduleId === 'string' ? (m0.moduleId as string) : '',
    disabledSteps: Array.isArray(m0.disabledSteps) ? (m0.disabledSteps as string[]) : [],
    preProcessors: Array.isArray(m0.preProcessors) ? (m0.preProcessors as unknown[]) : [],
    postProcessors: Array.isArray(m0.postProcessors) ? (m0.postProcessors as unknown[]) : [],
    assertions: Array.isArray(m0.assertions) ? (m0.assertions as unknown[]) : [],
    envCookie: typeof m0.envCookie === 'boolean' ? (m0.envCookie as boolean) : true,
    sharedCookie: typeof m0.sharedCookie === 'boolean' ? (m0.sharedCookie as boolean) : false,
  })
  const [modules, setModules] = useState<ApiModule[]>([])
  const toggleStep = (id: string) => patchForm({ disabledSteps: form.disabledSteps.includes(id) ? form.disabledSteps.filter((x) => x !== id) : [...form.disabledSteps, id] })
  const [saving, setSaving] = useState(false)
  const patchForm = (p: Partial<ScenarioForm>) => setForm((f) => ({ ...f, ...p }))
  const onSave = async () => {
    if (!form.name.trim()) return message.warning(t('scenario.nameRequired', '请输入场景名'))
    setSaving(true)
    try {
      await api.updateScenario(scenario.id, {
        name: form.name.trim(),
        status: form.status,
        meta: { description: form.description, tags: form.tags, priority: form.priority, params: form.params, csvParams: form.csv, moduleId: form.moduleId, disabledSteps: form.disabledSteps, preProcessors: form.preProcessors, postProcessors: form.postProcessors, assertions: form.assertions, envCookie: form.envCookie, sharedCookie: form.sharedCookie, envId: envId || undefined, envName: envs.find((e) => e.id === envId)?.name },
      })
      message.success(t('scenario.saved', '已保存'))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('scenario.saveFailed', '保存失败'))
    } finally {
      setSaving(false)
    }
  }
  // Resolve reference names; fall back to the first 8 chars of the id instead of full UUIDs.
  const nameOf = (id: string) => nameMap[id] || (id ? id.slice(0, 8) : '—')

  const loadSteps = async () => {
    try {
      const s = await api.getScenario(scenario.id)
      setSteps(s.steps || [])
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('scenario.loadStepsFailed', '加载步骤失败'))
    }
  }
  const loadReferenced = () =>
    // Load project cases + scenarios to build the id-to-name map for step display; load environments for run selection.
    Promise.all([
      api.projectCasesAll(scenario.projectId).catch(() => []),
      api.scenarios(scenario.projectId).then((s) => s).catch(() => []),
      api.environments(scenario.projectId).then((e) => (Array.isArray(e) ? e : [])).catch(() => []),
      api.modules(scenario.projectId).then((mm) => (Array.isArray(mm) ? mm : [])).catch(() => []),
    ]).then(([cases, scns, environments, mods]) => {
      const m: Record<string, string> = {}
      const cm: Record<string, ApiCase> = {}
      cases.forEach((c) => { m[c.id] = `${c.method} ${c.name}`; cm[c.id] = c })
      scns.forEach((s) => (m[s.id] = s.name))
      setNameMap(m)
      setCaseMap(cm)
      setEnvs(environments)
      // Prefer the saved scenario environment (meta.envId); otherwise the first enabled one.
      const savedEnvId = scenario.meta?.envId as string | undefined
      setEnvId((cur) => cur || (savedEnvId && environments.some((e) => e.id === savedEnvId) ? savedEnvId : environments.find((e) => e.enabled !== false)?.id || ''))
      setModules(mods)
    })
  useEffect(() => {
    loadSteps()
    loadReferenced()
    // Enabled pools + live runner counts for the run-target selector.
    api.resourcePools().then((ps) => setPools((ps || []).filter((p) => p.enabled !== false))).catch(() => {})
    api.poolRunnerStatus().then(setPoolOnline).catch(() => {})
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scenario.id])
  const refreshReferenced = async () => {
    await Promise.all([loadSteps(), loadReferenced()])
    message.success(t('scenario.refDataRefreshed', '引用数据已刷新'))
  }

  // Fetch report detail and map per-step results (pass/status/latency/size) onto step rows.
  const applyReport = async (reportId: string): Promise<string | null> => {
    const rep = await api.scenarioReport(reportId).catch(() => null)
    if (!rep) return null
    const map: Record<string, ReportResultItem> = {}
    rep.results.forEach((res) => { map[res.caseId] = res })
    setStepResults(map)
    // A top-level sub-scenario's status aggregates its leaf results, so load missing sub-scenario steps (otherwise the parent row stays blank after a run).
    await Promise.all(
      steps
        .filter((s) => s.scenarioId && !subSteps[s.scenarioId])
        .map(async (s) => {
          try {
            const sc = await api.getScenario(s.scenarioId as string)
            setSubSteps((m) => ({ ...m, [s.scenarioId as string]: sc.steps || [] }))
          } catch {
            /* sub-scenario load failed: parent can't aggregate a result; don't block */
          }
        }),
    )
    return rep.status
  }

  // Live follow of an async run: WS step events animate rows as they execute.
  // Degrades to polling the report when the socket can't be established.
  const followLiveRun = (reportId: string) =>
    new Promise<void>((resolve) => {
      let settled = false
      const finish = () => { if (!settled) { settled = true; resolve() } }
      const poll = async () => {
        for (let i = 0; i < 900 && !settled; i++) {
          const rep = await api.scenarioReport(reportId).catch(() => null)
          if (rep && rep.status !== 'RUNNING') break
          await new Promise((r) => setTimeout(r, 1000))
        }
        finish()
      }
      let ws: WebSocket
      try { ws = new WebSocket(runEventsWsUrl(reportId)) } catch { void poll(); return }
      let opened = false
      ws.onopen = () => { opened = true }
      ws.onmessage = (m) => {
        let ev: RunEvent
        try { ev = JSON.parse(m.data as string) as RunEvent } catch { return }
        const sid = ev.stepId
        if (ev.type === 'stepStarted' && sid) {
          setLiveSteps((prev) => ({ ...prev, [sid]: { running: true, startedAt: Date.now() } }))
        } else if (ev.type === 'stepFinished' && sid) {
          setLiveSteps((prev) => ({ ...prev, [sid]: { running: false, startedAt: prev[sid]?.startedAt ?? Date.now() } }))
          setStepResults((prev) => ({
            ...prev,
            [sid]: {
              ...(prev[sid] || {}),
              caseId: sid,
              outcome: ev.status === 'SUCCESS' ? 'SUCCESS' : 'ERROR',
              failures: ev.failures || [],
              executedAt: '',
              latencyMs: ev.latencyMs ?? prev[sid]?.latencyMs,
            },
          }))
        } else if (ev.type === 'stepDetail' && sid) {
          setStepResults((prev) => {
            const cur = prev[sid]
            if (!cur) return prev
            return { ...prev, [sid]: { ...cur, statusCode: ev.statusCode ?? cur.statusCode, latencyMs: ev.latencyMs ?? cur.latencyMs, timings: ev.timings ?? cur.timings } }
          })
        } else if (ev.type === 'runComplete') {
          ws.close()
          finish()
        }
      }
      ws.onerror = () => { if (!opened) { try { ws.close() } catch { /* already closed */ } void poll() } }
      // Socket died mid-run (server restart/proxy): keep polling to completion.
      ws.onclose = () => { if (opened && !settled) void poll() }
    })

  const run = async () => {
    setRunning(true)
    setStepResults({})
    setLiveSteps({})
    try {
      const r = await api.runScenario(scenario.id, scenario.projectId, { environmentId: envId || undefined, failureStrategy, poolId: poolId || undefined, asyncRun: true })
      setLastRun(r)
      setLastRunAt(new Date().toLocaleString())
      const live = r.status === 'RUNNING'
      setLiveMode(live)
      if (live) await followLiveRun(r.reportId)
      const finalStatus = (await applyReport(r.reportId)) ?? r.status
      setLiveSteps({})
      message.success(`${t('scenario.triggered', '场景已触发执行')} · ${finalStatus}`)
    } catch (e) {
      message.error(e instanceof ApiError ? `${t('scenario.execFailed', '执行失败')}:${e.status}` : t('scenario.execFailed', '执行失败'))
    } finally {
      setRunning(false)
    }
  }
  // Step to result key: CASE uses case_id; REQUEST uses "METHOD url" (matches the executor label).
  const stepKey = (s: ScenarioStep): string | null => (s.caseId ? s.caseId : s.request ? `${s.request.method} ${s.request.url}` : null)
  // Top-level sub-scenario/controller steps have no result of their own (reports only contain leaves); collect descendant leaf keys.
  const collectLeafKeys = (s: ScenarioStep): string[] => {
    const k = stepKey(s)
    if (k) return [k]
    if (s.scenarioId) return (subSteps[s.scenarioId] || []).flatMap(collectLeafKeys)
    return []
  }
  // Result for a step: leaf = direct; parent = aggregated from descendant leaves (any failure fails),
  // so parent rows keep their final green/red state across renders.
  const resultFor = (s: ScenarioStep): ReportResultItem | undefined => {
    const k = stepKey(s)
    if (k && stepResults[k]) return stepResults[k]
    const rs = collectLeafKeys(s).map((kk) => stepResults[kk]).filter(Boolean) as ReportResultItem[]
    if (!rs.length) return undefined
    const fail = rs.some((r) => r.outcome !== 'SUCCESS')
    return { caseId: s.id, outcome: fail ? 'ERROR' : 'SUCCESS', failures: [], executedAt: '' }
  }
  // Single-step run (row play button): build the request, send via /api/debug/send, write the result back to the step.
  const runStep = async (s: ScenarioStep) => {
    const kase = s.caseId ? caseMap[s.caseId] : undefined
    const reqInfo = kase
      ? { method: kase.method, url: kase.url, body: kase.body, headers: kase.headers || [], auth: kase.auth, assertions: kase.assertions, processors: kase.processors }
      : s.request
        ? { method: s.request.method, url: s.request.url, body: s.request.body ?? null, headers: [] as { key: string; value: string }[], auth: undefined, assertions: s.request.assertions, processors: undefined }
        : null
    const k = stepKey(s)
    if (!reqInfo || !k) return
    const env = envs.find((e) => e.id === envId)
    const req = buildStepRequest(reqInfo.method, reqInfo.url, reqInfo.body, reqInfo.headers, reqInfo.auth as { type: string; token?: string } | undefined, env)
    if (!req) return message.warning(t('editor.needEnvOrAbs', '相对路径需先选择带 baseUrl 的环境,或填写绝对 URL(http(s)://)'))
    try {
      const resp = await api.debugSend({ ...req, assertions: reqInfo.assertions as unknown[], processors: reqInfo.processors as unknown[] })
      const fails = (resp.assertions || []).filter((a) => !a.passed)
      setStepResults((prev) => ({ ...prev, [k]: { caseId: k, outcome: fails.length ? 'ERROR' : 'SUCCESS', failures: fails.map((a) => a.reason || a.item), executedAt: '', statusCode: resp.status, latencyMs: resp.latencyMs, respSize: resp.body.length, body: resp.body, headers: resp.headers } }))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('editor.sendFail', '发送失败'))
    }
  }
  // Drag reorder: move from to to, optimistically update local order, then persist via PATCH.
  const moveStep = async (from: number, to: number) => {
    if (from === to) return
    const arr = [...steps].sort((a, b) => a.order - b.order)
    const [m] = arr.splice(from, 1)
    arr.splice(to, 0, m)
    setSteps(arr.map((s, i) => ({ ...s, order: i + 1 })))
    try {
      await api.reorderScenarioSteps(scenario.id, arr.map((s) => s.id))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('scenario.reorderFailed', '排序保存失败'))
      loadSteps()
    }
  }

  const ordered = [...steps].sort((a, b) => a.order - b.order)
  const nextOrder = steps.length ? Math.max(...steps.map((s) => s.order)) + 1 : 1
  // After an add: new steps always append at the end; for inline inserts (pendingInsert), move the new block to the target index and persist.
  const onAdded = async () => {
    setAdd('')
    const pi = pendingInsert
    setPendingInsert(null)
    const sc = await api.getScenario(scenario.id).catch(() => null)
    let arr = sc ? [...(sc.steps || [])].sort((a, b) => a.order - b.order) : []
    if (pi && arr.length) {
      const added = arr.filter((s) => !pi.before.includes(s.id)) // newly added block (appended at the end, order preserved)
      if (added.length) {
        const rest = arr.filter((s) => pi.before.includes(s.id))
        rest.splice(Math.max(0, Math.min(pi.at, rest.length)), 0, ...added)
        arr = rest
        await api.reorderScenarioSteps(scenario.id, arr.map((s) => s.id)).catch(() => undefined)
        const sc2 = await api.getScenario(scenario.id).catch(() => null)
        if (sc2) arr = [...(sc2.steps || [])].sort((a, b) => a.order - b.order)
      }
    }
    setSteps(arr)
    // New references (e.g. scenario copies) need fresh name/case maps to label correctly.
    loadReferenced()
  }
  // Inline insert: record the target index + a snapshot of current step ids, then open the matching add entry (custom request/import/controller).
  const startInsert = (key: string, at: number) => {
    setPendingInsert({ at, before: steps.map((s) => s.id) })
    if (key === 'IMPORT') setImportOpen(true)
    else if (key === 'REQUEST') setCustomReqOpen(true)
    else setAdd(key)
  }
  const removeStep = async (s: ScenarioStep) => {
    try {
      await api.deleteScenarioStep(scenario.id, s.id)
      message.success(t('scenario.stepDeleted', '步骤已删除'))
      loadSteps()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('scenario.deleteFailed', '删除失败'))
    }
  }
  // Step-type submenu (key encodes the insert position `at`; same set as the bottom add-step button).
  const typeChildren = (at: number) => [
    { type: 'group' as const, label: t('scenario.grpRequest', '请求 / 场景'), children: [
      { key: `IMPORT@${at}`, label: t('scenario.importSystem', '导入系统请求') },
      { key: `REQUEST@${at}`, label: t('scenario.customRequest', '自定义请求') },
    ] },
    { type: 'group' as const, label: t('scenario.grpLogic', '逻辑控制'), children: [
      { key: `LOOP@${at}`, label: t('scenario.stepLoop', '循环控制器') },
      { key: `IF@${at}`, label: t('scenario.stepIf', '条件控制器') },
      { key: `ONCE@${at}`, label: t('scenario.stepOnce', '仅一次控制器') },
    ] },
    { type: 'group' as const, label: t('scenario.grpOther', '其他'), children: [{ key: `TIMER@${at}`, label: t('scenario.stepTimer', '等待时间') }] },
  ]
  // Inline insert pick (key is "KIND@at").
  const onInsertPick = (key: string) => {
    const at = key.lastIndexOf('@')
    if (at >= 0) startInsert(key.slice(0, at), Number(key.slice(at + 1)))
  }
  // Copy step: build an equivalent body, append it, then move it right after the original.
  const copyStep = async (s: ScenarioStep, i: number) => {
    let body: StepBody | null = null
    if (s.caseId) body = { kind: 'CASE', order: nextOrder, refId: s.caseId }
    else if (s.scenarioId) body = { kind: 'SCENARIO', order: nextOrder, refId: s.scenarioId }
    else if (s.request) body = { kind: 'REQUEST', order: nextOrder, request: s.request }
    else if (s.control) body = { kind: s.kind, order: nextOrder, control: s.control }
    if (!body) return message.warning(t('scenario.copyUnsupported', '该步骤类型暂不支持复制'))
    setPendingInsert({ at: i + 1, before: steps.map((x) => x.id) })
    try {
      await api.addStep(scenario.id, body)
      await onAdded()
      message.success(t('scenario.stepCopied', '步骤已复制'))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('scenario.copyFailed', '复制失败'))
    }
  }
  // Row hover actions: + (insert above/below, then pick step type) and a copy/delete menu.
  const rowActions = (s: ScenarioStep, i: number) => (
    <>
      <Dropdown
        trigger={['click']}
        placement="bottomRight"
        menu={{
          items: [
            { key: 'above', label: t('scenario.insertAbove', '在上方插入'), children: typeChildren(i) },
            { key: 'below', label: t('scenario.insertBelow', '在下方插入'), children: typeChildren(i + 1) },
          ],
          onClick: ({ key }) => onInsertPick(key),
        }}
      >
        <Button type="text" size="small" title={t('scenario.insert', '插入步骤')} icon={<PlusOutlined style={{ color: 'var(--brand)' }} />} />
      </Dropdown>
      <Dropdown
        trigger={['click']}
        placement="bottomRight"
        menu={{
          items: [
            { key: 'copy', label: t('a.copy', '复制') },
            { key: 'delete', label: t('a.delete', '删除'), danger: true },
          ],
          onClick: ({ key, domEvent }) => { domEvent.stopPropagation(); if (key === 'delete') removeStep(s); else copyStep(s, i) },
        }}
      >
        <Button type="text" size="small" title={t('a.more', '更多')} icon={<MoreOutlined />} />
      </Dropdown>
    </>
  )

  // Expand/collapse a controller or sub-scenario; sub-scenario steps load lazily on first expand.
  const toggleExpand = async (s: ScenarioStep) => {
    setExpandedSteps((prev) => {
      const n = new Set(prev)
      if (n.has(s.id)) n.delete(s.id)
      else n.add(s.id)
      return n
    })
    if (s.scenarioId && !subSteps[s.scenarioId]) {
      try {
        const sc = await api.getScenario(s.scenarioId)
        setSubSteps((m) => ({ ...m, [s.scenarioId as string]: sc.steps || [] }))
      } catch {
        /* sub-scenario load failed: expands to empty; don't block */
      }
    }
  }

  // Double-click on a controller-nested inline request opens the editor. Sub-scenario children
  // belong to the referenced scenario and stay read-only, hence the parent.control guard.
  const openChildEdit = (parent: ScenarioStep, path: number[], raw: ScenarioStep) => {
    if (!parent.control || raw.kind.toUpperCase() !== 'REQUEST') return
    let node: any = parent.control
    for (let d = 0; d < path.length - 1; d++) node = node?.children?.[path[d]]
    const child = node?.children?.[path[path.length - 1]]
    if (!child || String(child.kind).toUpperCase() !== 'REQUEST') return
    setSelStep(null)
    setEditChild({
      parent,
      path,
      step: {
        id: `${parent.id}:${path.join('.')}`,
        order: path[path.length - 1] + 1,
        kind: 'REQUEST',
        refMode: 'REFERENCE',
        request: { method: child.method || 'GET', url: child.url || '', body: child.body ?? null, assertions: child.assertions, headers: child.headers, queryParams: child.queryParams, restParams: child.restParams, auth: child.auth, processors: child.processors },
      },
    })
  }

  // Expandable top-level steps (sub-scenario / loop / if / once); drives expand-all / collapse-all.
  const expandableStepIds = ordered
    .filter((s) => !!s.scenarioId || ['SCENARIO', 'LOOP', 'IF', 'ONCE'].includes(s.kind.toUpperCase()))
    .map((s) => s.id)
  const allExpanded = expandableStepIds.length > 0 && expandableStepIds.every((id) => expandedSteps.has(id))
  const toggleAllExpand = async () => {
    if (allExpanded) {
      setExpandedSteps(new Set())
      return
    }
    setExpandedSteps(new Set(expandableStepIds))
    // Expanding all lazy-loads every sub-scenario's steps (first time only).
    for (const s of ordered) {
      if (s.scenarioId && !subSteps[s.scenarioId]) {
        try {
          const sc = await api.getScenario(s.scenarioId)
          setSubSteps((m) => ({ ...m, [s.scenarioId as string]: sc.steps || [] }))
        } catch {
          /* sub-scenario load failed: expands to empty; don't block */
        }
      }
    }
  }

  const stepsTab = (
    <div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 12, flexWrap: 'wrap' }}>
        <Typography.Text strong style={{ fontSize: 13 }}>{t('scenario.totalPrefix', '共')} {steps.length} {t('scenario.totalSuffix', '个步骤')}</Typography.Text>
        {expandableStepIds.length > 0 && (
          <Tooltip title={allExpanded ? t('scenario.collapseAllSteps', '收起全部子步骤') : t('scenario.expandAllSteps', '展开全部子步骤')}>
            <Button type="text" size="small" icon={<DownOutlined rotate={allExpanded ? 180 : 0} style={{ transition: 'transform .2s' }} />} onClick={toggleAllExpand} />
          </Tooltip>
        )}
        <div style={{ flex: 1 }} />
        <Tooltip title={t('scenario.refreshRefData', '刷新引用场景数据')}>
          <Button size="small" icon={<ReloadOutlined />} onClick={refreshReferenced} />
        </Tooltip>
        {lastRun && (() => {
          const vals = Object.values(stepResults)
          const passN = vals.filter((r) => r.outcome === 'SUCCESS').length
          const failN = vals.length - passN
          return (
            <Space size={16} style={{ fontSize: 12, color: 'var(--text-2)' }}>
              {lastRunAt && <span>{t('scenario.execTime', '执行时间')} <span style={{ color: 'var(--text)' }}>{lastRunAt}</span></span>}
              <span>
                {t('scenario.execResult', '执行结果')}
                <span style={{ color: '#22c55e', marginLeft: 8 }}>{t('scenario.runSuccess', '成功')} {passN}</span>
                <span style={{ color: failN ? '#ef4444' : '#8a9099', marginLeft: 8 }}>{t('scenario.runError', '失败')} {failN}</span>
                <span style={{ color: 'var(--text-3)', marginLeft: 8 }}>{t('scenario.falsePos', '误报')} 0</span>
              </span>
              <Button type="link" size="small" icon={<EyeOutlined />} style={{ padding: 0 }} onClick={() => setReportModalId(lastRun.reportId)}>{t('scenario.viewReport', '查看报告')}</Button>
              <Button type="text" size="small" icon={<ReloadOutlined />} loading={running} onClick={run} title={t('a.refresh', '重新执行')} />
            </Space>
          )
        })()}
      </div>
      {ordered.length === 0 ? (
        <Empty description={t('scenario.emptySteps', '暂无步骤,点「添加步骤」')} />
      ) : (
        ordered.map((s, i) => {
          const res = resultFor(s)
          const hasResp = !!res && (res.statusCode != null || res.body != null || (res.headers?.length ?? 0) > 0)
          // Live mode animates only the step(s) actually executing; legacy sync mode keeps the staggered all-rows animation.
          const liveKeys = liveMode ? collectLeafKeys(s).filter((k) => liveSteps[k]?.running) : []
          const rowRunning = running && (liveMode ? liveKeys.length > 0 : true)
          const liveSince = rowRunning && liveKeys.length ? Math.min(...liveKeys.map((k) => liveSteps[k].startedAt)) : undefined
          // Sub-scenario: once expanded, inject its steps as children (with results so child rows show green/red); controllers get children from their payload.
          const node = stepToNode(s, t, nameOf)
          if (s.scenarioId && subSteps[s.scenarioId]) {
            node.children = subSteps[s.scenarioId].map((cs) => ({ ...stepToNode(cs, t, nameOf), raw: cs, result: resultFor(cs) }))
          }
          // Expandable: sub-scenario / controller (LOOP/IF/ONCE). Click expands instead of opening the drawer; leaf click opens the drawer.
          const expandable = !!s.scenarioId || (node.children?.length ?? 0) > 0 || ['SCENARIO', 'LOOP', 'IF', 'ONCE'].includes(s.kind.toUpperCase())
          const isExpanded = expandedSteps.has(s.id)
          return (
            <div
              key={s.id}
              draggable
              onDragStart={() => setDragIdx(i)}
              onDragOver={(e) => e.preventDefault()}
              onDrop={(e) => { e.preventDefault(); if (dragIdx != null) moveStep(dragIdx, i); setDragIdx(null) }}
              onMouseEnter={() => setHoverStep(s.id)}
              onMouseLeave={() => setHoverStep((h) => (h === s.id ? null : h))}
              onClick={() => {
                if (expandable) return toggleExpand(s)
                // Editable inline requests (incl. copied steps) open straight in the editor.
                if (s.kind.toUpperCase() === 'REQUEST') { setSelStep(null); setEditReqStep(s) } else setSelStep({ step: s, idx: i + 1 })
              }}
              onDoubleClick={() => { if (s.kind.toUpperCase() === 'REQUEST') { setSelStep(null); setEditReqStep(s) } }}
              style={{ cursor: 'pointer', opacity: dragIdx === i ? 0.5 : 1 }}
            >
              <StepRow
                node={node}
                idx={i + 1}
                depth={0}
                t={t}
                result={res}
                running={rowRunning}
                liveSince={liveSince}
                seq={liveMode ? 0 : i * 0.18}
                enabled={!form.disabledSteps.includes(s.id)}
                onToggle={() => toggleStep(s.id)}
                onRun={() => runStep(s)}
                hovered={hoverStep === s.id}
                actions={rowActions(s, i)}
                respPreview={hasResp ? <StepRespPreview r={res!} t={t} /> : undefined}
                expandable={expandable}
                expanded={isExpanded}
                onChildSelect={(raw, path) => setSelStep({ step: raw, idx: (path[path.length - 1] ?? 0) + 1, child: true })}
                onChildDblClick={(raw, path) => openChildEdit(s, path, raw)}
              />
            </div>
          )
        })
      )}
      <div style={{ textAlign: 'center', marginTop: 10 }}>
        <Dropdown
          menu={{
            items: [
              { type: 'group', label: t('scenario.grpRequest', '请求 / 场景'), children: [
                { key: 'IMPORT', label: t('scenario.importSystem', '导入系统请求') },
                { key: 'REQUEST', label: t('scenario.customRequest', '自定义请求') },
              ] },
              { type: 'group', label: t('scenario.grpLogic', '逻辑控制'), children: [
                { key: 'LOOP', label: t('scenario.stepLoop', '循环控制器') },
                { key: 'IF', label: t('scenario.stepIf', '条件控制器') },
                { key: 'ONCE', label: t('scenario.stepOnce', '仅一次控制器') },
              ] },
              { type: 'group', label: t('scenario.grpOther', '其他'), children: [{ key: 'TIMER', label: t('scenario.stepTimer', '等待时间') }] },
            ],
            onClick: ({ key }) => { if (key === 'IMPORT') setImportOpen(true); else if (key === 'REQUEST') setCustomReqOpen(true); else setAdd(key) },
          }}
        >
          <Button type="dashed" icon={<PlusOutlined />} block>{t('scenario.addStep', '添加步骤')}</Button>
        </Dropdown>
      </div>
      <AddStepModal type={add} scenarioId={scenario.id} projectId={scenario.projectId} nextOrder={nextOrder} onClose={() => setAdd('')} onAdded={onAdded} />
      <CustomRequestDrawer open={customReqOpen} scenarioId={scenario.id} nextOrder={nextOrder} env={envs.find((e) => e.id === envId)} onClose={() => setCustomReqOpen(false)} onAdded={onAdded} />
      <CustomRequestDrawer open={!!editReqStep} editStep={editReqStep} scenarioId={scenario.id} nextOrder={nextOrder} env={envs.find((e) => e.id === envId)} onClose={() => setEditReqStep(null)} onAdded={onAdded} />
      <CustomRequestDrawer open={!!editChild} editStep={editChild?.step} editChild={editChild} scenarioId={scenario.id} nextOrder={nextOrder} env={envs.find((e) => e.id === envId)} onClose={() => setEditChild(null)} onAdded={onAdded} />
      <ImportRequestDrawer open={importOpen} scenarioId={scenario.id} projectId={scenario.projectId} nextOrder={nextOrder} onClose={() => setImportOpen(false)} onImported={onAdded} />
    </div>
  )

  const tabs = [
    { key: 'basic', label: t('apidef.basicInfo', '基本信息'), children: <ScenarioBasicInfo scenario={scenario} stepCount={steps.length} form={form} patch={patchForm} modules={modules} /> },
    { key: 'steps', label: t('scenario.stepsTab', '步骤'), children: stepsTab },
    { key: 'params', label: t('scenario.paramsTab', '参数'), children: <ScenarioParams params={form.params} onChange={(params) => patchForm({ params })} csv={form.csv} onCsvChange={(csv) => patchForm({ csv })} /> },
    {
      key: 'prepost',
      label: t('scenario.prePostTab', '前/后置'),
      children: (
        <Space direction="vertical" size={20} style={{ width: '100%' }}>
          <div>
            <Typography.Text strong>{t('apidef.preProcessors', '前置')}</Typography.Text>
            <div style={{ marginTop: 8 }}><ProcessorEditor value={form.preProcessors as Record<string, unknown>[]} onChange={(v) => patchForm({ preProcessors: v })} allowed={['script', 'sql', 'wait']} /></div>
          </div>
          <div>
            <Typography.Text strong>{t('apidef.postProcessors', '后置')}</Typography.Text>
            <div style={{ marginTop: 8 }}><ProcessorEditor value={form.postProcessors as Record<string, unknown>[]} onChange={(v) => patchForm({ postProcessors: v })} allowed={['extract', 'script', 'sql', 'wait']} /></div>
          </div>
        </Space>
      ),
    },
    { key: 'assert', label: t('apidef.assertions', '断言'), children: <AssertionEditor value={form.assertions as Record<string, unknown>[]} onChange={(v) => patchForm({ assertions: v })} /> },
    { key: 'exec', label: t('scenario.execHistoryTab', '执行历史'), children: <ScenarioExecutionsTab scenarioId={scenario.id} nameOf={nameOf} caseMap={caseMap} t={t} /> },
    { key: 'change', label: t('apidef.changeHistory', '变更历史'), children: <ScenarioChangesTab scenarioId={scenario.id} t={t} /> },
    { key: 'settings', label: t('apidef.settings', '设置'), children: <ScenarioSettings failureStrategy={failureStrategy} onFailureStrategy={setFailureStrategy} envCookie={form.envCookie} sharedCookie={form.sharedCookie} onCookie={(p) => patchForm(p)} t={t} /> },
  ]

  return (
    <div style={{ padding: '12px 16px', height: '100%', overflow: 'auto' }}>
      {/* Toolbar (environment + server run + save) portaled into the tab-bar right slot; rendered only for the active tab. */}
      {active && slot && createPortal(
        <ScenarioActionBar
          envs={envs}
          envId={envId}
          onEnv={setEnvId}
          running={running}
          onRun={run}
          onLocalRun={() => message.info(t('scenario.localSoon', '本地执行即将接入'))}
          saving={saving}
          onSave={onSave}
          pools={pools}
          poolId={poolId}
          onPool={setPoolId}
          poolOnline={poolOnline}
          t={t}
        />,
        slot,
      )}
      {/* Header: status / priority / [id] / name / tags. */}
      <div style={{ marginBottom: 14 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8, flexWrap: 'wrap' }}>
          <Tag color={statusColor(form.status)} style={{ margin: 0 }}>{scStatusLabel(form.status, t)}</Tag>
          <span style={{ color: priorityColor(form.priority), fontSize: 12, fontWeight: 600 }}>{form.priority}</span>
          <span className="ms-mono" style={{ color: 'var(--text-3)', fontSize: 12 }}>[{scenario.id.slice(0, 8)}]</span>
          <span style={{ fontWeight: 600, fontSize: 15, color: 'var(--text)' }}>{form.name}</span>
          {form.tags.map((tg) => <Tag key={tg} style={{ margin: 0 }}>{tg}</Tag>)}
          <Tooltip title={t('scenario.copyLink', '复制链接')}>
            <LinkOutlined
              style={{ color: 'var(--text-3)', cursor: 'pointer' }}
              onClick={async () => {
                const url = `${window.location.origin}${window.location.pathname}?scenario=${encodeURIComponent(scenario.id)}`
                try {
                  await navigator.clipboard?.writeText(url)
                  message.success(t('scenario.linkCopied', '链接已复制'))
                } catch {
                  message.info(url)
                }
              }}
            />
          </Tooltip>
        </div>
      </div>
      <Tabs className="ms-detail-tabs" defaultActiveKey="steps" items={tabs} />
      <StepDetailDrawer
        sel={selStep}
        scenarioId={scenario.id}
        caseMap={caseMap}
        nameOf={nameOf}
        env={envs.find((e) => e.id === envId)}
        result={selStep ? stepResults[stepKey(selStep.step) ?? ''] : undefined}
        onClose={() => setSelStep(null)}
        onDeleted={() => { setSelStep(null); loadSteps() }}
        onEdit={selStep?.child ? undefined : (st) => { setSelStep(null); setEditReqStep(st) }}
      />
      <ScenarioReportModal reportId={reportModalId} scenarioId={scenario.id} nameOf={nameOf} caseMap={caseMap} onClose={() => setReportModalId(null)} />
    </div>
  )
}

// Resolve {{var}} and prefix baseUrl onto relative paths; assemble a case/inline request into a sendable request (same conventions as debug send).
function buildStepRequest(method: string, url: string, body: string | null | undefined, headers: { key: string; value: string }[], auth: { type: string; token?: string } | undefined, env?: Environment): SentRequest | null {
  const resolveVars = (s: string): string =>
    env?.variables ? s.replace(/\{\{\s*(\w+)\s*\}\}/g, (whole, k: string) => env.variables?.[k] ?? whole) : s
  const path = (url || '').trim()
  if (!path) return null
  const raw = resolveVars(path)
  let base = raw
  if (!/^https?:\/\//i.test(raw)) {
    const baseUrl = env?.baseUrl?.trim().replace(/\/+$/, '')
    if (!baseUrl) return null
    base = `${baseUrl}${raw.startsWith('/') ? '' : '/'}${raw}`
  }
  const hs: { key: string; value: string }[] = []
  for (const eh of env?.headers || []) if (eh.name?.trim()) hs.push({ key: eh.name, value: resolveVars(eh.value || '') })
  for (const h of headers) if (h.key?.trim()) hs.push({ key: h.key, value: resolveVars(h.value || '') })
  if (auth?.type === 'bearer' && auth.token) hs.push({ key: 'Authorization', value: `Bearer ${auth.token}` })
  if (auth?.type === 'basic' && auth.token) hs.push({ key: 'Authorization', value: `Basic ${btoa(auth.token)}` })
  return { method: method || 'GET', url: base, headers: hs, body: body?.trim() ? resolveVars(body) : undefined }
}

// Step detail drawer (ref #25): header + server run + request tabs + response. Referenced cases
// and inline requests are runnable; controllers show config only. Replace still needs backend support.
function StepDetailDrawer({
  sel,
  scenarioId,
  caseMap,
  nameOf,
  env,
  result,
  onClose,
  onDeleted,
  onEdit,
}: {
  sel: { step: ScenarioStep; idx: number } | null
  scenarioId: string
  caseMap: Record<string, ApiCase>
  nameOf: NameOf
  env?: Environment
  result?: ReportResultItem
  onClose: () => void
  onDeleted: () => void
  /** Present for editable (non-reference) steps; referenced cases/sub-scenarios are read-only. */
  onEdit?: (step: ScenarioStep) => void
}) {
  const { t } = useI18n()
  const [full, setFull] = useState(false)
  const [deleting, setDeleting] = useState(false)
  // Replace: swap the referenced case of a CASE step for another project case.
  const [replaceOpen, setReplaceOpen] = useState(false)
  const [replaceSel, setReplaceSel] = useState<string>()
  const [replacing, setReplacing] = useState(false)
  const doReplace = async () => {
    if (!sel || !replaceSel) return message.warning(t('scenario.pickReplaceCase', '请选择用例'))
    setReplacing(true)
    try {
      await api.updateScenarioStep(scenarioId, sel.step.id, { kind: 'CASE', refId: replaceSel })
      message.success(t('scenario.replaced', '已替换'))
      setReplaceOpen(false)
      onDeleted()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('func.saveFailed', '保存失败'))
    } finally {
      setReplacing(false)
    }
  }
  const del = () => {
    if (!sel) return
    modal.confirm({
      title: t('scenario.deleteStepConfirm', '删除该步骤?'),
      okButtonProps: { danger: true },
      onOk: async () => {
        setDeleting(true)
        try {
          await api.deleteScenarioStep(scenarioId, sel.step.id)
          message.success(t('scenario.stepDeleted', '步骤已删除'))
          onDeleted()
        } catch (e) {
          message.error(e instanceof ApiError ? e.message : t('scenario.deleteFailed', '删除失败'))
        } finally {
          setDeleting(false)
        }
      },
    })
  }
  const [running, setRunning] = useState(false)
  const [resp, setResp] = useState<DebugResponse | null>(null)
  const [err, setErr] = useState('')
  const [lastReq, setLastReq] = useState<SentRequest | null>(null)
  const step = sel?.step
  const meta = step ? makeStepMeta(t)[step.kind.toUpperCase()] || { label: step.kind, color: 'default' } : null
  const kase = step?.caseId ? caseMap[step.caseId] : undefined

  // Sendable request for the current step (CASE: its case; REQUEST: inline); controllers have none.
  const reqInfo = (() => {
    if (kase) return { method: kase.method, url: kase.url, body: kase.body, headers: kase.headers || [], auth: kase.auth, assertions: kase.assertions, processors: kase.processors }
    if (step?.request) return { method: step.request.method, url: step.request.url, body: step.request.body ?? null, headers: [], auth: undefined, assertions: step.request.assertions, processors: undefined }
    return null
  })()

  // On step switch, backfill the response panel + actual request from the scenario run result; clear only when there is no result.
  useEffect(() => {
    setErr('')
    const hasResult = !!result && (result.statusCode != null || result.body != null || (result.headers?.length ?? 0) > 0)
    if (hasResult) {
      const asserts = (result!.assertions as AssertionResult[] | undefined) || []
      setResp({
        status: result!.statusCode ?? 0,
        latencyMs: result!.latencyMs ?? 0,
        headers: result!.headers ?? [],
        body: result!.body ?? '',
        assertions: asserts.length ? asserts : undefined,
        extractions: result!.extractions?.length ? result!.extractions : undefined,
      })
      // Backfill actual request / console / cURL: rebuild the request as sent (relative paths joined with env baseUrl, auth folded into an Authorization header).
      if (reqInfo) {
        const resolveUrl = (u: string) => {
          if (/^https?:\/\//i.test(u)) return u
          const b = env?.baseUrl?.trim().replace(/\/+$/, '')
          return b ? `${b}${u.startsWith('/') ? '' : '/'}${u}` : u
        }
        const hdrs = [...(reqInfo.headers || [])]
        const auth = reqInfo.auth as { type?: string; token?: string } | undefined
        if (auth?.token && (auth.type === 'bearer' || auth.type === 'basic')) {
          hdrs.push({ key: 'Authorization', value: `${auth.type === 'bearer' ? 'Bearer' : 'Basic'} ${auth.token}` })
        }
        setLastReq({ method: reqInfo.method, url: resolveUrl(reqInfo.url), headers: hdrs, body: reqInfo.body ?? undefined })
      } else {
        setLastReq(null)
      }
    } else {
      setResp(null)
      setLastReq(null)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sel?.step.id, result, kase?.id])

  const run = async () => {
    if (!reqInfo) return
    const req = buildStepRequest(reqInfo.method, reqInfo.url, reqInfo.body, reqInfo.headers, reqInfo.auth as any, env)
    if (!req) return message.warning(t('editor.needEnvOrAbs', '相对路径需先选择带 baseUrl 的环境,或填写绝对 URL(http(s)://)'))
    setLastReq(req)
    setRunning(true); setErr(''); setResp(null)
    try {
      setResp(await api.debugSend({ ...req, assertions: reqInfo.assertions as unknown[], processors: reqInfo.processors as unknown[] }))
    } catch (e) {
      setErr(e instanceof ApiError ? e.message : t('editor.sendFail', '发送失败'))
    } finally {
      setRunning(false)
    }
  }

  const title = (
    <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
      <span style={{ color: 'var(--text-3)', fontSize: 12 }}>{sel?.idx}</span>
      {meta && <Tag color={meta.color} style={{ margin: 0 }}>{meta.label}</Tag>}
      <span style={{ fontWeight: 600 }}>{kase ? kase.name : step?.scenarioId ? nameOf(step.scenarioId) : step?.request?.url || meta?.label}</span>
    </div>
  )

  const reqTabs = reqInfo
    ? [
        {
          key: 'headers',
          label: `${t('apidef.requestHeaders', '请求头')}${reqInfo.headers.length ? ` (${reqInfo.headers.length})` : ''}`,
          children: reqInfo.headers.length ? (
            <Table size="small" pagination={false} rowKey={(_, i) => String(i)} dataSource={reqInfo.headers} columns={[{ title: t('env.varName', '参数名称'), dataIndex: 'key' }, { title: t('env.varValue', '参数值'), dataIndex: 'value', render: (v: string) => <span className="ms-mono">{v}</span> }]} />
          ) : <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('apidef.none', '无')} />,
        },
        {
          key: 'body',
          label: t('apidef.requestBody', '请求体'),
          children: reqInfo.body ? <pre className="ms-mono" style={{ background: 'var(--panel-2)', padding: 12, borderRadius: 6, margin: 0, fontSize: 12, maxHeight: 240, overflow: 'auto' }}>{reqInfo.body}</pre> : <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('apidef.noBody', '请求没有 Body')} />,
        },
        {
          key: 'assert',
          label: t('apidef.assertions', '断言'),
          children: Array.isArray(reqInfo.assertions) && (reqInfo.assertions as unknown[]).length ? <pre className="ms-mono" style={{ background: 'var(--panel-2)', padding: 12, borderRadius: 6, margin: 0, fontSize: 12 }}>{JSON.stringify(reqInfo.assertions, null, 2)}</pre> : <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('apidef.none', '无')} />,
        },
      ]
    : []

  return (
    <ResizableDrawer
      open={!!sel}
      onClose={onClose}
      width={full ? '92%' : 680}
      title={title}
      closeIcon={false}
      extra={
        <Space>
          {onEdit && step?.kind.toUpperCase() === 'REQUEST' && (
            <Button type="text" size="small" icon={<EditOutlined />} onClick={() => onEdit(step)}>{t('a.edit', '编辑')}</Button>
          )}
          <Button
            type="text"
            size="small"
            icon={<SwapOutlined />}
            disabled={step?.kind.toUpperCase() !== 'CASE'}
            title={step?.kind.toUpperCase() === 'CASE' ? undefined : t('scenario.replaceCaseOnly', '仅引用用例的步骤支持替换')}
            onClick={() => { setReplaceSel(undefined); setReplaceOpen(true) }}
          >
            {t('scenario.replace', '替换')}
          </Button>
          <Button type="text" size="small" danger icon={<DeleteOutlined />} loading={deleting} onClick={del}>{t('a.delete', '删除')}</Button>
          <Button type="text" size="small" icon={<FullscreenOutlined />} onClick={() => setFull((v) => !v)}>{t('scenario.fullscreen', '全屏')}</Button>
          <Button type="text" size="small" icon={<CloseOutlined />} onClick={onClose} />
        </Space>
      }
    >
      {reqInfo ? (
        <>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
            <Tag color={methodColor(reqInfo.method)} style={{ margin: 0, fontWeight: 600 }}>{reqInfo.method}</Tag>
            <Input readOnly value={reqInfo.url} className="ms-mono" style={{ flex: 1 }} />
            <Button type="primary" icon={<ThunderboltOutlined />} loading={running} onClick={run}>{t('apidef.serverRun', '服务端执行')}</Button>
          </div>
          <Tabs className="ms-detail-tabs" size="small" items={reqTabs} />
          <DebugResultPanel running={running} resp={resp} err={err} req={lastReq} isHttp onRun={run} extractors={reqInfo.processors as Record<string, unknown>[] | undefined} assertions={reqInfo.assertions as Record<string, unknown>[] | undefined} />
        </>
      ) : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('scenario.controlStepInfo', '控制器步骤:在步骤列表中查看其配置与子步骤')} style={{ margin: '48px 0' }} />
      )}
      <EditDrawer
        open={replaceOpen}
        title={t('scenario.replaceCase', '替换引用用例')}
        onCancel={() => setReplaceOpen(false)}
        onOk={doReplace}
        confirmLoading={replacing}
        okText={t('a.confirm', '确定')}
        cancelText={t('a.cancel', '取消')}
      >
        <Select
          showSearch
          style={{ width: '100%' }}
          placeholder={t('scenario.pickReplaceCase', '请选择用例')}
          optionFilterProp="label"
          value={replaceSel}
          onChange={setReplaceSel}
          options={Object.values(caseMap)
            .filter((c) => c.id !== step?.caseId)
            .map((c) => ({ value: c.id, label: `${c.method} ${c.name}` }))}
        />
      </EditDrawer>
    </ResizableDrawer>
  )
}

// Basic info (editable, ref #21): name/priority/status/tags/description + read-only id/step count. Persisted via the top Save.
function ScenarioBasicInfo({ scenario, stepCount, form, patch, modules }: { scenario: Scenario; stepCount: number; form: ScenarioForm; patch: (p: Partial<ScenarioForm>) => void; modules: ApiModule[] }) {
  const { t } = useI18n()
  const [tagInput, setTagInput] = useState('')
  const field = (label: string, value: ReactNode, req?: boolean) => (
    <div style={{ marginBottom: 14 }}>
      <div style={{ fontSize: 13, color: 'var(--text-2)', marginBottom: 6 }}>{req && <span style={{ color: 'var(--error)', marginRight: 4 }}>*</span>}{label}</div>
      {value}
    </div>
  )
  return (
    <div style={{ maxWidth: 560 }}>
      {field(t('scenario.name', '场景名称'), <Input value={form.name} onChange={(e) => patch({ name: e.target.value })} placeholder={t('scenario.namePlaceholder', '如:下单主流程')} />, true)}
      {field(t('scenario.ownerModule', '所属模块'), <Select style={{ width: 280 }} value={form.moduleId || ''} onChange={(v) => patch({ moduleId: v || '' })} placeholder={t('scenario.unplanned', '未规划场景')} options={[{ value: '', label: t('scenario.unplanned', '未规划场景') }, ...modules.map((m) => ({ value: m.id, label: m.name }))]} />)}
      {field(t('scenario.priority', '场景等级'), <Select style={{ width: 200 }} value={form.priority} onChange={(v) => patch({ priority: v })} options={SCENARIO_PRIORITIES.map((p) => ({ value: p, label: <span style={{ color: priorityColor(p) }}>● <span style={{ color: 'var(--text)' }}>{p}</span></span> }))} />)}
      {field(t('scenario.colStatus', '场景状态'), <Select style={{ width: 200 }} value={form.status} onChange={(v) => patch({ status: v })} options={SCENARIO_STATUSES.map((s) => ({ value: s, label: scStatusLabel(s, t) }))} />)}
      {field(t('scenario.tags', '标签'), (
        <Space size={[6, 6]} wrap>
          {form.tags.map((tg) => (
            <Tag key={tg} closable onClose={() => patch({ tags: form.tags.filter((x) => x !== tg) })}>{tg}</Tag>
          ))}
          <Input
            size="small"
            style={{ width: 140 }}
            value={tagInput}
            onChange={(e) => setTagInput(e.target.value)}
            onPressEnter={() => { const v = tagInput.trim(); if (v && !form.tags.includes(v)) patch({ tags: [...form.tags, v] }); setTagInput('') }}
            placeholder={t('apidef.addTag', '添加标签,回车结束')}
          />
        </Space>
      ))}
      {field(t('scenario.descLabel', '描述'), <Input.TextArea rows={3} value={form.description} onChange={(e) => patch({ description: e.target.value })} placeholder={t('scenario.descPlaceholder', '请对该场景进行描述')} />)}
      {field(t('scenario.colSteps', '步骤数'), <span>{stepCount}</span>)}
      {field('ID', <span className="ms-mono" style={{ fontSize: 12 }}>{scenario.id}</span>)}
      {field(t('scenario.createdBy', '创建人'), <Input value={scenario.createdBy || '—'} readOnly />)}
      {field(t('scenario.createdAt', '创建时间'), <Input value={scenario.createdAt?.slice(0, 19) || '—'} readOnly className="ms-mono" />)}
      {field(t('scenario.updatedAt', '更新时间'), <Input value={scenario.updatedAt?.slice(0, 19) || '—'} readOnly className="ms-mono" />)}
      <Typography.Text type="secondary" style={{ fontSize: 12 }}>{t('scenario.moduleReuseNote', '所属模块复用项目接口模块(在「接口定义」维护);随保存写入')}</Typography.Text>
    </div>
  )
}

// Scenario params (ref #22): normal param table + CSV params. Stored in meta.params / meta.csvParams.
function ScenarioParams({ params, onChange, csv, onCsvChange }: { params: ScenarioParam[]; onChange: (p: ScenarioParam[]) => void; csv: string; onCsvChange: (v: string) => void }) {
  const { t } = useI18n()
  const [mode, setMode] = useState<'normal' | 'csv'>('normal')
  const set = (i: number, p: Partial<ScenarioParam>) => onChange(params.map((r, idx) => (idx === i ? { ...r, ...p } : r)))
  return (
    <div>
      <div style={{ background: 'var(--panel-2)', border: '1px solid var(--border-soft)', borderRadius: 6, padding: '6px 10px', marginBottom: 12, fontSize: 12, color: 'var(--text-2)' }}>
        {t('scenario.varPriority', '变量优先级:临时参数 > 场景参数 > 环境参数;同名变量场景级 CSV 优先级最高')}
      </div>
      <Segmented
        size="small"
        value={mode}
        onChange={(v) => setMode(v as 'normal' | 'csv')}
        options={[{ label: t('scenario.normalParams', '常规参数'), value: 'normal' }, { label: t('scenario.csvParams', 'CSV 参数'), value: 'csv' }]}
        style={{ marginBottom: 12 }}
      />
      {mode === 'csv' ? (
        <Input.TextArea
          rows={10}
          value={csv}
          onChange={(e) => onCsvChange(e.target.value)}
          placeholder={'name,age\nadmin,20\nguest,18'}
          className="ms-mono"
        />
      ) : (
      <>
      <Table<ScenarioParam & { _i: number }>
        size="small"
        rowKey="_i"
        pagination={false}
        dataSource={params.map((r, i) => ({ ...r, _i: i }))}
        locale={{ emptyText: t('apidef.none', '无') }}
        columns={[
          { title: t('env.varName', '变量名称'), dataIndex: 'name', render: (v: string, _r, i) => <Input size="small" value={v} placeholder={t('a.input', '请输入')} onChange={(e) => set(i, { name: e.target.value })} className="ms-mono" /> },
          { title: t('env.varType', '类型'), dataIndex: 'type', width: 110, render: (v: string, _r, i) => <Select size="small" style={{ width: '100%' }} value={v || '常量'} onChange={(val) => set(i, { type: val })} options={[{ value: '常量', label: t('env.constant', '常量') }, { value: '列表', label: t('env.list', '列表') }]} /> },
          { title: t('env.varValue', '参数值'), dataIndex: 'value', render: (v: string, _r, i) => <Input size="small" value={v} onChange={(e) => set(i, { value: e.target.value })} /> },
          { title: t('env.tags', '标签'), dataIndex: 'tags', width: 160, render: (v: string, _r, i) => <Input size="small" value={v} placeholder={t('apidef.addTag', '添加标签,回车结束')} onChange={(e) => set(i, { tags: e.target.value })} /> },
          { title: t('env.varDesc', '描述'), dataIndex: 'desc', render: (v: string, _r, i) => <Input size="small" value={v} onChange={(e) => set(i, { desc: e.target.value })} /> },
          { title: '', width: 44, render: (_v, _r, i) => <Button type="text" size="small" danger icon={<DeleteOutlined />} onClick={() => onChange(params.filter((_, idx) => idx !== i))} /> },
        ]}
      />
      <Button size="small" icon={<PlusOutlined />} onClick={() => onChange([...params, { name: '', type: '常量', value: '', tags: '', desc: '' }])} style={{ marginTop: 8 }}>
        {t('scenario.addRow', '加一行')}
      </Button>
      </>
      )}
    </div>
  )
}

// Settings: step failure rule (ref #24; maps to run's failure_strategy). Cookie config is a placeholder.
function ScenarioSettings({ failureStrategy, onFailureStrategy, envCookie, sharedCookie, onCookie, t }: { failureStrategy: 'CONTINUE' | 'STOP'; onFailureStrategy: (v: 'CONTINUE' | 'STOP') => void; envCookie: boolean; sharedCookie: boolean; onCookie: (p: { envCookie?: boolean; sharedCookie?: boolean }) => void; t: TFn }) {
  return (
    <Space direction="vertical" size={18} style={{ width: '100%' }}>
      <div>
        <div style={{ fontWeight: 600, marginBottom: 8 }}>{t('scenario.cookieConfig', 'Cookie 配置')}</div>
        <Space direction="vertical" size={8}>
          <Space><Switch checked={envCookie} onChange={(v) => onCookie({ envCookie: v })} /><span>{t('scenario.envCookie', '环境 Cookie')}</span></Space>
          <Space><Switch checked={sharedCookie} onChange={(v) => onCookie({ sharedCookie: v })} /><span>{t('scenario.sharedCookie', '共享 Cookie(步骤间共享会话)')}</span></Space>
        </Space>
        <Typography.Text type="secondary" style={{ fontSize: 12 }}>{t('scenario.cookieSaveNote', '随保存写入 meta;执行期 Cookie 透传需后端会话支持')}</Typography.Text>
      </div>
      <div>
        <div style={{ fontWeight: 600, marginBottom: 8 }}>{t('scenario.failureRule', '步骤执行失败规则')}</div>
        <Radio.Group value={failureStrategy} onChange={(e) => onFailureStrategy(e.target.value)}>
          <Radio value="CONTINUE">{t('scenario.failContinue', '忽略错误,继续执行')}</Radio>
          <Radio value="STOP">{t('scenario.failStop', '停止/结束执行')}</Radio>
        </Radio.Group>
      </div>
    </Space>
  )
}

// Execution history tab (ref #23); result links open the report modal.
function ScenarioExecutionsTab({ scenarioId, nameOf, caseMap, t }: { scenarioId: string; nameOf: NameOf; caseMap?: Record<string, ApiCase>; t: TFn }) {
  const [rows, setRows] = useState<ScenarioExecution[]>([])
  const [loading, setLoading] = useState(false)
  const [reportId, setReportId] = useState<string | null>(null)
  useEffect(() => {
    setLoading(true)
    api.scenarioExecutions(scenarioId).then((p) => setRows(p.items)).catch(() => setRows([])).finally(() => setLoading(false))
  }, [scenarioId])
  return (
    <>
      <Table<ScenarioExecution>
        rowKey="id"
        size="small"
        loading={loading}
        dataSource={rows}
        locale={{ emptyText: <Empty description={t('scenario.noExec', '暂无执行记录')} /> }}
        pagination={{ pageSize: 20, size: 'small' }}
        columns={[
          { title: t('scenario.colSeq', '序号'), dataIndex: 'id', render: (v: string) => <span className="ms-mono" style={{ fontSize: 12 }}>{v.slice(0, 12)}</span> },
          { title: t('scenario.colStatus', '执行状态'), dataIndex: 'status', width: 110, render: (s: string) => <Tag color={outcomeColor(s)}>{execStatusLabel(s, t)}</Tag> },
          { title: t('scenario.caseUnit', '用例'), dataIndex: 'caseCount', width: 80 },
          { title: t('scenario.execTime', '操作时间'), dataIndex: 'createdAt', width: 200, render: (v: string) => <span className="ms-mono" style={{ fontSize: 12 }}>{v?.slice(0, 19)}</span> },
          { title: t('apidef.colAction', '操作'), width: 100, render: (_v, r) => <Button type="link" size="small" disabled={!r.reportId} onClick={() => setReportId(r.reportId)}>{t('scenario.viewResult', '执行结果')}</Button> },
        ]}
      />
      <ScenarioReportModal reportId={reportId} scenarioId={scenarioId} nameOf={nameOf} caseMap={caseMap} onClose={() => setReportId(null)} />
    </>
  )
}

// Change history tab (audit log).
const CHANGE_ACTIONS: Record<string, { tkey: string; fallback: string; color: string }> = {
  CREATE: { tkey: 'scenario.actionCreate', fallback: '创建', color: 'green' },
  UPDATE: { tkey: 'scenario.actionUpdate', fallback: '更新', color: 'blue' },
  ADD_STEP: { tkey: 'scenario.actionAddStep', fallback: '新增步骤', color: 'geekblue' },
  UPDATE_STEP: { tkey: 'scenario.actionUpdateStep', fallback: '更新步骤', color: 'cyan' },
  DELETE_STEP: { tkey: 'scenario.actionDeleteStep', fallback: '删除步骤', color: 'red' },
  REORDER: { tkey: 'scenario.actionReorder', fallback: '调整顺序', color: 'purple' },
}
function ScenarioChangesTab({ scenarioId, t }: { scenarioId: string; t: TFn }) {
  const [rows, setRows] = useState<ScenarioChange[]>([])
  const [loading, setLoading] = useState(false)
  useEffect(() => {
    setLoading(true)
    api.scenarioChanges(scenarioId).then(setRows).catch(() => setRows([])).finally(() => setLoading(false))
  }, [scenarioId])
  return (
    <Table<ScenarioChange>
      rowKey="id"
      size="small"
      loading={loading}
      dataSource={rows}
      locale={{ emptyText: <Empty description={t('scenario.noChanges', '暂无变更记录')} /> }}
      pagination={{ pageSize: 20, size: 'small' }}
      columns={[
        { title: t('scenario.changeAction', '操作'), dataIndex: 'action', width: 120, render: (a: string) => { const m = CHANGE_ACTIONS[a]; return <Tag color={m ? m.color : 'default'}>{m ? t(m.tkey, m.fallback) : a}</Tag> } },
        { title: t('scenario.changeDetail', '详情'), dataIndex: 'detail', render: (v?: string) => v || '—' },
        { title: t('scenario.changeUser', '操作人'), dataIndex: 'userId', width: 140, render: (v?: string) => v || '—' },
        { title: t('scenario.execTime', '时间'), dataIndex: 'createdAt', width: 200, render: (v: string) => <span className="ms-mono" style={{ fontSize: 12 }}>{v?.slice(0, 19)}</span> },
      ]}
    />
  )
}

// Controller child (leaf) builder: CASE reference or inline REQUEST.
type Child = { kind: 'CASE'; refId: string } | { kind: 'REQUEST'; method: string; url: string }

function ChildrenBuilder({ value, onChange, projectCases }: { value: Child[]; onChange: (v: Child[]) => void; projectCases: ApiCase[] }) {
  const { t } = useI18n()
  const add = (c: Child) => onChange([...value, c])
  return (
    <Space direction="vertical" style={{ width: '100%' }} size={6}>
      {value.map((c, i) => (
        <Space.Compact key={i} style={{ width: '100%' }}>
          <Input style={{ width: 70 }} value={c.kind} disabled />
          <Input style={{ flex: 1 }} className="ms-mono" value={c.kind === 'CASE' ? `${t('scenario.caseRef', '用例')} ${c.refId}` : `${c.method} ${c.url}`} disabled />
          <Button onClick={() => onChange(value.filter((_, idx) => idx !== i))}>{t('scenario.del', '删')}</Button>
        </Space.Compact>
      ))}
      <Space>
        <Select
          key={value.length}
          size="small"
          style={{ width: 240 }}
          showSearch
          optionFilterProp="label"
          placeholder={t('scenario.addCaseChild', '+ 加用例子步骤')}
          onChange={(id) => add({ kind: 'CASE', refId: id })}
          options={projectCases.map((c) => ({ value: c.id, label: `${c.method} ${c.name}` }))}
        />
        <Button size="small" onClick={() => add({ kind: 'REQUEST', method: 'GET', url: 'http://127.0.0.1:9180/healthz' })}>{t('scenario.addRequestChild', '+ 加请求子步骤(示例)')}</Button>
      </Space>
    </Space>
  )
}

// Add-step modal dispatched by type: CASE/REQUEST/SCENARIO leaves + LOOP/IF/ONCE/TIMER controllers (with children).
type StepBody = { kind: string; order: number; refId?: string; refMode?: string; snapshot?: unknown; request?: unknown; control?: unknown }
// Custom request drawer: request line + headers/body/query/REST/pre/post/assertions/auth
// + server run + response. The backend supports the full inline request spec.
function CustomRequestDrawer({
  open,
  scenarioId,
  nextOrder,
  env,
  onClose,
  onAdded,
  onLocalAdd,
  editStep,
  editChild,
}: {
  open: boolean
  scenarioId: string
  nextOrder: number
  env?: Environment
  onClose: () => void
  onAdded: () => void | Promise<void>
  /** New-scenario (no id) local mode: don't persist; hand the step body back. */
  onLocalAdd?: (body: StepBody) => void
  /** Edit mode: prefill from this REQUEST step and PATCH it instead of adding. */
  editStep?: ScenarioStep | null
  /** Controller-child edit: PATCH the parent controller's payload at this children path. */
  editChild?: { parent: ScenarioStep; path: number[] } | null
}) {
  const { t } = useI18n()
  const blankKv = (): KVRow => ({ key: '', value: '' })
  const [method, setMethod] = useState('GET')
  const [url, setUrl] = useState('')
  const [headers, setHeaders] = useState<KVRow[]>([blankKv()])
  const [query, setQuery] = useState<KVRow[]>([blankKv()])
  const [rest, setRest] = useState<KVRow[]>([blankKv()])
  const [body, setBody] = useState('')
  const [authType, setAuthType] = useState<'none' | 'bearer' | 'basic'>('none')
  const [authToken, setAuthToken] = useState('')
  const [assertions, setAssertions] = useState<unknown[]>([{ type: 'StatusIs', args: 200 }])
  const [pre, setPre] = useState<unknown[]>([])
  const [post, setPost] = useState<unknown[]>([])
  const [full, setFull] = useState(false)
  const [saving, setSaving] = useState(false)
  const [running, setRunning] = useState(false)
  const [resp, setResp] = useState<DebugResponse | null>(null)
  const [err, setErr] = useState('')
  const [lastReq, setLastReq] = useState<SentRequest | null>(null)

  const reset = () => {
    const r = editStep?.request
    const kv = (rows?: { key: string; value: string }[]): KVRow[] => (rows?.length ? rows.map((x) => ({ key: x.key, value: x.value })) : [blankKv()])
    setMethod(r?.method || 'GET'); setUrl(r?.url || ''); setHeaders(kv(r?.headers)); setQuery(kv(r?.queryParams)); setRest(kv(r?.restParams))
    setBody(r?.body || '')
    setAuthType(r?.auth?.type === 'bearer' || r?.auth?.type === 'basic' ? r.auth.type : 'none'); setAuthToken(r?.auth?.token || '')
    setAssertions(r ? (Array.isArray(r.assertions) ? r.assertions : []) : [{ type: 'StatusIs', args: 200 }])
    // Split stored processors by args.phase; legacy entries without a phase land in the post tab.
    const stored = (r?.processors?.length ? r.processors : []) as { args?: { phase?: string } }[]
    setPre(stored.filter((p) => p?.args?.phase === 'pre'))
    setPost(stored.filter((p) => p?.args?.phase !== 'pre'))
    setResp(null); setErr(''); setLastReq(null)
  }
  useEffect(() => { if (open) reset() }, [open, editStep?.id]) // eslint-disable-line react-hooks/exhaustive-deps

  const clean = (rows: KVRow[]) => rows.filter((r) => r.key.trim())
  const authObj = () => (authType === 'none' ? undefined : { type: authType, token: authToken })
  // The runner reads one processors array; args.phase only records which editor tab each entry belongs to.
  const withPhase = (list: unknown[], phase: 'pre' | 'post') =>
    list.map((p) => {
      const o = p as { args?: Record<string, unknown> }
      return { ...o, args: { ...(o.args || {}), phase } }
    })
  const mergedProcessors = () => [...withPhase(pre, 'pre'), ...withPhase(post, 'post')]
  const buildRequest = () => ({
    method,
    url: url.trim(),
    body: body || null,
    headers: clean(headers),
    queryParams: clean(query),
    restParams: clean(rest),
    auth: authObj(),
    assertions,
    processors: mergedProcessors(),
  })

  // Server run: build the final URL (REST {key} substitution + query string), send directly, show the response.
  const run = async () => {
    if (!url.trim()) return message.warning(t('editor.urlRequired', '请输入 URL'))
    let u = url.trim()
    for (const r of clean(rest)) u = u.replace(`{${r.key}}`, r.value)
    const qs = clean(query).map((q) => `${q.key}=${q.value}`).join('&')
    if (qs) u += (u.includes('?') ? '&' : '?') + qs
    const req = buildStepRequest(method, u, body || null, clean(headers), authObj(), env)
    if (!req) return message.warning(t('editor.needEnvOrAbs', '相对路径需先选择带 baseUrl 的环境,或填写绝对 URL(http(s)://)'))
    setLastReq(req); setRunning(true); setErr(''); setResp(null)
    try {
      setResp(await api.debugSend({ ...req, assertions: assertions as unknown[], processors: mergedProcessors() as unknown[] }))
    } catch (e) {
      setErr(e instanceof ApiError ? e.message : t('editor.sendFail', '发送失败'))
    } finally {
      setRunning(false)
    }
  }

  const save = async (keepOpen: boolean) => {
    if (!url.trim()) return message.warning(t('editor.urlRequired', '请输入 URL'))
    setSaving(true)
    try {
      if (editChild) {
        // Rewrite the child in a payload clone, then PATCH the whole controller step.
        const payload = JSON.parse(JSON.stringify(editChild.parent.control || {}))
        let node: any = payload
        for (let d = 0; d < editChild.path.length - 1; d++) node = node?.children?.[editChild.path[d]]
        const li = editChild.path[editChild.path.length - 1]
        if (!node?.children?.[li]) throw new ApiError(404, t('scenario.updateFailed', '更新失败'))
        const r = buildRequest()
        node.children[li] = { ...node.children[li], kind: 'REQUEST', method: r.method, url: r.url, body: r.body, assertions: r.assertions, headers: r.headers, queryParams: r.queryParams, restParams: r.restParams, auth: r.auth, processors: r.processors }
        await api.updateScenarioStep(scenarioId, editChild.parent.id, { kind: editChild.parent.kind, control: payload })
        message.success(t('scenario.stepUpdated', '步骤已更新'))
        await onAdded()
        onClose()
        return
      }
      if (editStep) {
        await api.updateScenarioStep(scenarioId, editStep.id, { kind: 'REQUEST', request: buildRequest() })
        message.success(t('scenario.stepUpdated', '步骤已更新'))
        await onAdded()
        onClose()
        return
      }
      const stepBody: StepBody = { kind: 'REQUEST', order: nextOrder, request: buildRequest() }
      if (onLocalAdd) onLocalAdd(stepBody)
      else await api.addStep(scenarioId, stepBody)
      message.success(t('scenario.stepAdded', '步骤已添加'))
      await onAdded()
      if (keepOpen) reset()
      else onClose()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : editStep ? t('scenario.updateFailed', '更新失败') : t('scenario.addFailed', '添加失败'))
    } finally {
      setSaving(false)
    }
  }

  const kvTab = (rows: KVRow[], set: (r: KVRow[]) => void, n: number, label: string) => ({
    key: label,
    label: `${label}${n ? ` (${n})` : ''}`,
    children: <KVEditor rows={rows} onChange={set} />,
  })
  const tabs = [
    kvTab(headers, setHeaders, clean(headers).length, t('apidef.requestHeaders', '请求头')),
    { key: 'body', label: t('apidef.requestBody', '请求体'), children: <Input.TextArea rows={8} value={body} onChange={(e) => setBody(e.target.value)} className="ms-mono" placeholder='{"k":"v"}' /> },
    kvTab(query, setQuery, clean(query).length, 'Query'),
    kvTab(rest, setRest, clean(rest).length, 'REST'),
    { key: 'pre', label: t('scenario.pre', '前置'), children: <ProcessorEditor value={pre as never} onChange={(v) => setPre(v)} allowed={['wait', 'extract', 'script', 'sql']} /> },
    { key: 'post', label: t('scenario.post', '后置'), children: <ProcessorEditor value={post as never} onChange={(v) => setPost(v)} allowed={['wait', 'extract', 'script', 'sql']} /> },
    { key: 'assert', label: t('apidef.assertions', '断言'), children: <AssertionEditor value={assertions as never} onChange={(v) => setAssertions(v)} /> },
    { key: 'auth', label: t('apidef.auth', '认证'), children: (
      <Space direction="vertical" style={{ width: '100%' }}>
        <Select value={authType} onChange={setAuthType} style={{ width: 200 }} options={[{ value: 'none', label: t('apidef.authNone', '无') }, { value: 'bearer', label: 'Bearer Token' }, { value: 'basic', label: 'Basic Auth' }]} />
        {authType !== 'none' && <Input value={authToken} onChange={(e) => setAuthToken(e.target.value)} placeholder="token" className="ms-mono" />}
      </Space>
    ) },
  ]

  return (
    <ResizableDrawer
      open={open}
      onClose={onClose}
      width={full ? '92%' : 720}
      closeIcon={false}
      title={
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          {editStep && <span style={{ color: 'var(--text-3)', fontSize: 12 }}>{editStep.order}</span>}
          <Tag color="blue" style={{ margin: 0 }}>{t('scenario.customRequest', '自定义请求')}</Tag>
          <span style={{ fontWeight: 600 }}>{editStep ? editStep.request?.url : t('scenario.addStep', '添加步骤')}</span>
        </div>
      }
      extra={
        <Space>
          <span style={{ fontSize: 12, color: 'var(--text-3)' }}>{t('apidef.currentEnv', '当前环境')}: {env?.name || t('apidef.noEnv', '不引用')}</span>
          <Button type="text" size="small" icon={<FullscreenOutlined />} onClick={() => setFull((v) => !v)} />
          <Button type="text" size="small" icon={<CloseOutlined />} onClick={onClose} />
        </Space>
      }
      footer={
        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
          <Button onClick={onClose}>{t('a.cancel', '取消')}</Button>
          {!editStep && <Button loading={saving} onClick={() => save(true)}>{t('scenario.saveAndContinue', '保存并继续添加')}</Button>}
          <Button type="primary" loading={saving} onClick={() => save(false)}>{editStep ? t('a.save', '保存') : t('a.confirm', '确认')}</Button>
        </div>
      }
    >
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
        <Select value={method} onChange={setMethod} style={{ width: 110 }} options={['GET', 'POST', 'PUT', 'DELETE', 'PATCH', 'HEAD', 'OPTIONS'].map((m) => ({ value: m, label: m }))} />
        <Input value={url} onChange={(e) => setUrl(e.target.value)} className="ms-mono" style={{ flex: 1 }} placeholder={t('scenario.urlPlaceholder', '请输入包含 http/https 的完整 URL')} />
        <Button type="primary" icon={<ThunderboltOutlined />} loading={running} onClick={run}>{t('apidef.serverRun', '服务端执行')}</Button>
      </div>
      <Tabs className="ms-detail-tabs" size="small" items={tabs} />
      <Divider style={{ margin: '12px 0' }} />
      <Typography.Text strong style={{ fontSize: 13 }}>{t('apidef.respContent', '响应内容')}</Typography.Text>
      <DebugResultPanel running={running} resp={resp} err={err} req={lastReq} isHttp onRun={run} assertions={assertions as Record<string, unknown>[]} extractors={[...pre, ...post] as Record<string, unknown>[]} />
    </ResizableDrawer>
  )
}

function AddStepModal({
  type,
  scenarioId,
  projectId,
  nextOrder,
  onClose,
  onAdded,
  onLocalAdd,
}: {
  type: string
  scenarioId: string
  projectId: string
  nextOrder: number
  onClose: () => void
  onAdded: () => void
  /** New-scenario (no id) mode: don't persist; hand the step body back locally. */
  onLocalAdd?: (body: StepBody) => void
}) {
  const { t } = useI18n()
  const [form] = Form.useForm()
  const [saving, setSaving] = useState(false)
  const [children, setChildren] = useState<Child[]>([])
  const [projCases, setProjCases] = useState<ApiCase[]>([])
  const [scns, setScns] = useState<Scenario[]>([])
  const isControl = ['LOOP', 'IF', 'ONCE'].includes(type)

  useEffect(() => {
    if (type) {
      setChildren([])
      form.resetFields()
      // CASE / controllers need the project-case dropdown; SCENARIO needs the scenario dropdown.
      if (isControl || type === 'CASE') api.projectCases(projectId).then((p) => setProjCases(p.items)).catch(() => undefined)
      if (type === 'SCENARIO') api.scenarios(projectId).then(setScns).catch(() => undefined)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [type])

  const submit = async (v: any) => {
    setSaving(true)
    try {
      const childPayload = children.map((c) => (c.kind === 'CASE' ? { kind: 'CASE', refId: c.refId } : { kind: 'REQUEST', method: c.method, url: c.url, assertions: [] }))
      let body: StepBody | null = null
      if (type === 'CASE' || type === 'SCENARIO') body = { kind: type, order: nextOrder, refId: v.refId }
      else if (type === 'REQUEST') body = { kind: 'REQUEST', order: nextOrder, request: { method: v.method, url: v.url, body: v.body || null, assertions: v.assertions || [] } }
      else if (type === 'TIMER') body = { kind: 'TIMER', order: nextOrder, control: { ms: Number(v.ms) || 1000 } }
      else if (type === 'LOOP') body = { kind: 'LOOP', order: nextOrder, control: { times: Number(v.times) || 1, children: childPayload } }
      else if (type === 'IF') body = { kind: 'IF', order: nextOrder, control: { variable: v.variable, operator: v.operator, value: v.value, children: childPayload } }
      else if (type === 'ONCE') body = { kind: 'ONCE', order: nextOrder, control: { children: childPayload } }
      if (!body) return
      if (onLocalAdd) onLocalAdd(body)
      else await api.addStep(scenarioId, body)
      message.success(t('scenario.stepAdded', '步骤已添加'))
      onAdded()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('scenario.addFailed', '添加失败'))
    } finally {
      setSaving(false)
    }
  }

  const title = (makeStepMeta(t)[type]?.label || t('scenario.step', '步骤'))
  return (
    <EditDrawer title={`${t('scenario.addPrefix', '添加')} · ${title}`} open={!!type} onCancel={onClose} onOk={() => form.submit()} confirmLoading={saving} width={620}>
      <Form form={form} layout="vertical" initialValues={{ method: 'GET', operator: '等于', times: 3, ms: 1000, assertions: [{ type: 'StatusIs', args: 200 }] }} onFinish={submit}>
        {(type === 'CASE' || type === 'SCENARIO') && (
          <Form.Item name="refId" label={type === 'CASE' ? t('scenario.stepCase', '引用用例') : t('scenario.refSubScenario', '引用子场景')} rules={[{ required: true }]}>
            <Select
              showSearch
              optionFilterProp="label"
              placeholder={type === 'CASE' ? t('scenario.selectProjCase', '选择项目接口用例') : t('scenario.selectSubScenario', '选择子场景')}
              options={
                type === 'CASE'
                  ? projCases.map((c) => ({ value: c.id, label: `${c.method} ${c.name}` }))
                  : scns.map((s) => ({ value: s.id, label: s.name }))
              }
              notFoundContent={type === 'CASE' ? t('scenario.noProjCase', '项目暂无接口用例') : t('scenario.noScenario', '项目暂无场景')}
            />
          </Form.Item>
        )}
        {type === 'REQUEST' && (
          <>
            <Space.Compact style={{ width: '100%' }}>
              <Form.Item name="method" label={t('scenario.method', '方法')} style={{ width: 120 }}><Select options={['GET', 'POST', 'PUT', 'DELETE', 'PATCH'].map((m) => ({ value: m, label: m }))} /></Form.Item>
              <Form.Item name="url" label="URL" style={{ flex: 1 }} rules={[{ required: true }]}><Input className="ms-mono" placeholder="http://127.0.0.1:9180/healthz" /></Form.Item>
            </Space.Compact>
            <Form.Item name="body" label={t('scenario.bodyOptional', '请求体(可选)')}><Input.TextArea rows={2} className="ms-mono" /></Form.Item>
            <Form.Item name="assertions" label={t('scenario.assertions', '断言')}><AssertionEditor /></Form.Item>
          </>
        )}
        {type === 'TIMER' && (
          <Form.Item name="ms" label={t('scenario.waitDuration', '等待时长 (ms)')} rules={[{ required: true }]}><Input type="number" /></Form.Item>
        )}
        {type === 'LOOP' && <Form.Item name="times" label={t('scenario.loopTimes', '循环次数')} rules={[{ required: true }]}><Input type="number" /></Form.Item>}
        {type === 'IF' && (
          <Space.Compact style={{ width: '100%' }}>
            <Form.Item name="variable" label={t('scenario.variable', '变量')} style={{ flex: 1 }} rules={[{ required: true }]}><Input className="ms-mono" placeholder="${count}" /></Form.Item>
            <Form.Item name="operator" label={t('scenario.operator', '操作符')} style={{ width: 110 }}><Select options={[['等于', t('scenario.opEq', '等于')], ['不等于', t('scenario.opNe', '不等于')], ['大于', t('scenario.opGt', '大于')], ['小于', t('scenario.opLt', '小于')], ['包含', t('scenario.opContains', '包含')]].map(([v, label]) => ({ value: v, label }))} /></Form.Item>
            <Form.Item name="value" label={t('scenario.value', '值')} style={{ width: 140 }} rules={[{ required: true }]}><Input /></Form.Item>
          </Space.Compact>
        )}
        {isControl && (
          <Form.Item label={t('scenario.childSteps', '子步骤(控制器内执行)')}>
            <ChildrenBuilder value={children} onChange={setChildren} projectCases={projCases} />
          </Form.Item>
        )}
      </Form>
    </EditDrawer>
  )
}

// Import system request (ref #29): browse APIs/cases/scenarios in one drawer, multi-select, then "reference" adds them as steps in bulk.
// API becomes REQUEST (method/path); case becomes a CASE reference; scenario a SCENARIO reference. Simplified: current project + name search only.
function ImportRequestDrawer({
  open,
  scenarioId,
  projectId,
  nextOrder,
  onClose,
  onImported,
  onLocalImport,
}: {
  open: boolean
  scenarioId: string
  projectId: string
  nextOrder: number
  onClose: () => void
  onImported: () => void
  /** New-scenario (no id) mode: don't persist; hand the step bodies back locally. */
  onLocalImport?: (bodies: StepBody[]) => void
}) {
  const { t } = useI18n()
  const [tab, setTab] = useState<'api' | 'case' | 'scenario'>('api')
  const [search, setSearch] = useState('')
  const [protocol, setProtocol] = useState('HTTP')
  const [moduleSearch, setModuleSearch] = useState('')
  const [selModule, setSelModule] = useState('ALL') // ALL | UNFILED | <moduleId>
  const [defs, setDefs] = useState<ApiDefinition[]>([])
  const [cases, setCases] = useState<ApiCase[]>([])
  const [scns, setScns] = useState<Scenario[]>([])
  const [modules, setModules] = useState<ApiModule[]>([])
  const [selApi, setSelApi] = useState<string[]>([])
  const [selCase, setSelCase] = useState<string[]>([])
  const [selScn, setSelScn] = useState<string[]>([])
  const [importing, setImporting] = useState(false)

  const reload = () => {
    api.definitions(projectId).then((d) => setDefs(Array.isArray(d) ? d : [])).catch(() => setDefs([]))
    api.projectCasesAll(projectId).then(setCases).catch(() => setCases([]))
    api.scenarios(projectId).then((s) => setScns(s.filter((x) => x.id !== scenarioId))).catch(() => setScns([]))
    api.modules(projectId).then((m) => setModules(Array.isArray(m) ? m : [])).catch(() => setModules([]))
  }

  useEffect(() => {
    if (!open) return
    setSearch(''); setModuleSearch(''); setSelModule('ALL'); setSelApi([]); setSelCase([]); setSelScn([])
    reload()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, projectId, scenarioId])

  const lc = (s: string) => s.toLowerCase()
  // case: module of its API definition; scenario: meta.moduleId.
  const defModuleMap = Object.fromEntries(defs.map((d) => [d.id, d.moduleId || '']))
  const moduleOf = (x: ApiDefinition | ApiCase | Scenario): string =>
    tab === 'api' ? (x as ApiDefinition).moduleId || '' : tab === 'case' ? defModuleMap[(x as ApiCase).apiDefinitionId] || '' : ((x as Scenario).meta?.moduleId as string) || ''
  const inModule = (x: ApiDefinition | ApiCase | Scenario) => (selModule === 'ALL' ? true : selModule === 'UNFILED' ? !moduleOf(x) : moduleOf(x) === selModule)
  const fDefs = defs.filter((d) => d.protocol === protocol && inModule(d) && (!search || lc(d.name).includes(lc(search)) || lc(d.path).includes(lc(search))))
  const fCases = cases.filter((c) => inModule(c) && (!search || lc(c.name).includes(lc(search)) || lc(c.url || '').includes(lc(search))))
  const fScns = scns.filter((s) => inModule(s) && (!search || lc(s.name).includes(lc(search))))
  const total = selApi.length + selCase.length + selScn.length

  // Left module-tree counts (based on the current tab's data; APIs also filtered by protocol).
  const activeData: (ApiDefinition | ApiCase | Scenario)[] = tab === 'api' ? defs.filter((d) => d.protocol === protocol) : tab === 'case' ? cases : scns
  const countFor = (mid: string) => activeData.filter((x) => (mid === 'ALL' ? true : mid === 'UNFILED' ? !moduleOf(x) : moduleOf(x) === mid)).length
  const shownModules = modules.filter((m) => !m.parentId).filter((m) => !moduleSearch || lc(m.name).includes(lc(moduleSearch)))
  const moduleRow = (key: string, name: string, count: number) => (
    <div
      key={key}
      onClick={() => setSelModule(key)}
      style={{ display: 'flex', alignItems: 'center', padding: '6px 8px', borderRadius: 6, cursor: 'pointer', fontSize: 13, background: selModule === key ? 'var(--brand-soft)' : 'transparent', color: selModule === key ? 'var(--brand)' : undefined }}
    >
      <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{name}</span>
      <span style={{ color: 'var(--text-3)', fontSize: 12 }}>{count}</span>
    </div>
  )
  const totalLabel = tab === 'api' ? t('apidef.allApis', '全部接口') : tab === 'case' ? t('scenario.allCases', '全部用例') : t('scenario.allScenarios', '全部场景')

  const doImport = async () => {
    setImporting(true)
    let order = nextOrder
    try {
      const bodies: StepBody[] = []
      for (const id of selApi) {
        const d = defs.find((x) => x.id === id)
        if (d) bodies.push({ kind: 'REQUEST', order: order++, request: { method: d.method || 'GET', url: d.path || '', assertions: [] } })
      }
      for (const id of selCase) bodies.push({ kind: 'CASE', order: order++, refId: id })
      for (const id of selScn) bodies.push({ kind: 'SCENARIO', order: order++, refId: id })
      if (onLocalImport) onLocalImport(bodies)
      else for (const b of bodies) await api.addStep(scenarioId, b)
      message.success(t('scenario.imported', '已引用') + ` ${total}`)
      onImported()
      onClose()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('scenario.importFailed', '引用失败'))
    } finally {
      setImporting(false)
    }
  }

  // Copy mode: materialize selections into editable inline REQUEST steps
  // (same request payload shape CustomRequestDrawer saves).
  const arr = (v: unknown): unknown[] => (Array.isArray(v) ? v : [])
  const cleanAuth = (a?: { type?: string; token?: string }) => (a?.type && a.type !== 'none' ? { type: a.type, token: a.token } : undefined)
  const caseToRequest = (c: ApiCase) => ({
    method: c.method || 'GET',
    url: c.url || '',
    body: c.body ?? null,
    headers: c.headers ?? [],
    queryParams: c.queryParams ?? [],
    restParams: c.restParams ?? [],
    auth: cleanAuth(c.auth),
    assertions: arr(c.assertions),
    processors: arr(c.processors),
  })
  const specKv = (rows?: { name: string; value?: string }[]) => (rows ?? []).filter((r) => r.name).map((r) => ({ key: r.name, value: r.value ?? '' }))
  // Spec processors carry no editor tab; tag args.phase so they land in the pre/post tabs.
  const tagPhase = (list: unknown[], phase: 'pre' | 'post') =>
    list.map((p) => {
      const o = p as { args?: Record<string, unknown> }
      return { ...o, args: { ...(o.args || {}), phase } }
    })
  const defToRequest = (d: ApiDefinition) => {
    const s = d.spec
    return {
      method: d.method || 'GET',
      url: d.path || '',
      body: s?.requestBody || null,
      headers: specKv(s?.requestHeaders),
      queryParams: specKv(s?.requestQuery),
      restParams: specKv(s?.restParams),
      auth: cleanAuth(s?.auth),
      assertions: arr(s?.assertions).length ? arr(s?.assertions) : [{ type: 'StatusIs', args: 200 }],
      processors: [...tagPhase(arr(s?.preProcessors), 'pre'), ...tagPhase(arr(s?.postProcessors), 'post')],
    }
  }

  const doCopy = async () => {
    setImporting(true)
    let order = nextOrder
    try {
      const bodies: StepBody[] = []
      for (const id of selApi) {
        const d = defs.find((x) => x.id === id)
        if (d) bodies.push({ kind: 'REQUEST', order: order++, request: { ...defToRequest(d), source: 'COPY_API' } })
      }
      for (const id of selCase) {
        const c = cases.find((x) => x.id === id)
        if (c) bodies.push({ kind: 'REQUEST', order: order++, request: { ...caseToRequest(c), source: 'COPY_CASE' } })
      }
      // Scenario: deep-copy into an independent scenario, then mount the copy as a
      // COPY group — it renders like a reference group, but edits stay private.
      for (const id of selScn) {
        const copy = await api.copyScenario(id)
        bodies.push({ kind: 'SCENARIO', order: order++, refId: copy.id, refMode: 'COPY', snapshot: { copiedFrom: id } })
      }
      if (onLocalImport) onLocalImport(bodies)
      else for (const b of bodies) await api.addStep(scenarioId, b)
      message.success(`${t('scenario.copied', '已复制')} ${bodies.length}`)
      onImported()
      onClose()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('scenario.copyFailed', '复制失败'))
    } finally {
      setImporting(false)
    }
  }

  const apiCols: ColumnsType<ApiDefinition> = [
    { title: 'ID', dataIndex: 'num', width: 90, render: (v?: number) => <span className="ms-mono" style={{ fontSize: 12 }}>{v ?? '—'}</span> },
    { title: t('scenario.apiName', '接口名称'), dataIndex: 'name', ellipsis: true },
    { title: t('apidef.colMethod', '请求类型'), dataIndex: 'method', width: 100, render: (m: string, r) => <Tag color={methodColor(m)}>{r.protocol === 'HTTP' ? m || 'GET' : r.protocol}</Tag> },
    { title: t('apidef.colPath', '路径'), dataIndex: 'path', ellipsis: true, render: (v: string) => <span className="ms-mono" style={{ fontSize: 12 }}>{v || '—'}</span> },
    { title: t('apidef.colStatus', '状态'), dataIndex: 'status', width: 100, render: (s: string) => <Tag color={statusColor(s)}>{statusLabel(s, t)}</Tag> },
  ]
  const caseCols: ColumnsType<ApiCase> = [
    { title: t('scenario.colName', '名称'), dataIndex: 'name', ellipsis: true },
    { title: t('apidef.colMethod', '请求类型'), dataIndex: 'method', width: 100, render: (m: string) => <Tag color={methodColor(m)}>{m}</Tag> },
    { title: t('apidef.colPath', '路径'), dataIndex: 'url', ellipsis: true, render: (v: string) => <span className="ms-mono" style={{ fontSize: 12 }}>{v || '—'}</span> },
    { title: t('apidef.colStatus', '状态'), dataIndex: 'status', width: 100, render: (s?: string) => <Tag>{s ? caseStatusLabel(s, t) : '—'}</Tag> },
  ]
  const scnCols: ColumnsType<Scenario> = [
    { title: t('scenario.colName', '名称'), dataIndex: 'name', ellipsis: true },
    { title: t('scenario.colSteps', '步骤数'), dataIndex: 'steps', width: 90, render: (s?: unknown[]) => <Tag color={s?.length ? 'geekblue' : 'default'}>{s?.length ?? 0}</Tag> },
    { title: t('scenario.colStatus', '状态'), dataIndex: 'status', width: 110, render: (s: string) => <Tag color={statusColor(s)}>{statusLabel(s, t)}</Tag> },
  ]

  return (
    <ResizableDrawer
      open={open}
      onClose={onClose}
      width="82%"
      title={t('scenario.importSystem', '导入系统请求')}
      footer={
        <div style={{ display: 'flex', alignItems: 'center', gap: 16 }}>
          <Typography.Text type="secondary" style={{ fontSize: 12 }}>
            {t('scenario.totalSelected', '共选择')} {total} · {t('scenario.tabApi', '接口')} {selApi.length} · {t('scenario.caseUnit', '用例')} {selCase.length} · {t('scenario.scenarioUnit', '场景')} {selScn.length}
          </Typography.Text>
          {total > 0 && (
            <Button type="link" size="small" style={{ padding: 0 }} onClick={() => { setSelApi([]); setSelCase([]); setSelScn([]) }}>{t('scenario.clearSel', '清空')}</Button>
          )}
          <div style={{ flex: 1 }} />
          <Button onClick={onClose}>{t('a.cancel', '取消')}</Button>
          <Button loading={importing} disabled={total === 0} onClick={doCopy}>{t('a.copy', '复制')}</Button>
          <Button type="primary" loading={importing} disabled={total === 0} onClick={doImport}>{t('scenario.doReference', '引用')}</Button>
        </div>
      }
    >
      <Segmented
        value={tab}
        onChange={(v) => setTab(v as 'api' | 'case' | 'scenario')}
        options={[
          { label: t('scenario.tabApi', '接口'), value: 'api' },
          { label: t('scenario.tabCase', '用例'), value: 'case' },
          { label: t('scenario.tabScenario', '场景'), value: 'scenario' },
        ]}
        style={{ marginBottom: 12 }}
      />
      <div style={{ display: 'flex', gap: 12, alignItems: 'flex-start' }}>
        {/* Left filter panel (ref #34): project + protocol + module search + module tree with counts. */}
        <div style={{ width: 240, flexShrink: 0 }}>
          <Space.Compact style={{ width: '100%', marginBottom: 8 }}>
            <Select size="small" value="__cur__" style={{ flex: 1 }} options={[{ value: '__cur__', label: t('scenario.curProject', '当前项目') }]} disabled />
            {tab !== 'scenario' && (
              <Select size="small" value={protocol} onChange={setProtocol} style={{ width: 96 }} options={['HTTP', 'SSH', 'AMQP', 'Redis', 'TCP', 'MongoDB', 'GRPC'].map((p) => ({ value: p, label: p }))} />
            )}
          </Space.Compact>
          <Input size="small" allowClear prefix={<SearchOutlined style={{ color: 'var(--text-3)' }} />} placeholder={t('apidef.moduleSearch', '输入模块名称搜索')} value={moduleSearch} onChange={(e) => setModuleSearch(e.target.value)} style={{ marginBottom: 8 }} />
          <div style={{ border: '1px solid var(--border-soft)', borderRadius: 6, padding: 4, maxHeight: 460, overflow: 'auto' }}>
            {moduleRow('ALL', `${totalLabel} (${countFor('ALL')})`, countFor('ALL'))}
            {moduleRow('UNFILED', t('scenario.unfiled', '未规划'), countFor('UNFILED'))}
            {shownModules.map((m) => moduleRow(m.id, m.name, countFor(m.id)))}
          </div>
        </div>
        {/* Result table */}
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8 }}>
            <Typography.Text strong>{totalLabel} ({tab === 'api' ? fDefs.length : tab === 'case' ? fCases.length : fScns.length})</Typography.Text>
            <div style={{ flex: 1 }} />
            <Input allowClear size="small" style={{ width: 240 }} prefix={<SearchOutlined style={{ color: 'var(--text-3)' }} />} placeholder={t('scenario.searchByPathName', '通过路径或名称搜索')} value={search} onChange={(e) => setSearch(e.target.value)} />
            <Button size="small" icon={<ReloadOutlined />} onClick={reload} title={t('a.refresh', '刷新')} />
          </div>
          {tab === 'api' && (
            <Table<ApiDefinition> rowKey="id" size="small" columns={apiCols} dataSource={fDefs} pagination={{ pageSize: 10, size: 'small' }} rowSelection={{ selectedRowKeys: selApi, onChange: (k) => setSelApi(k.map(String)) }} locale={{ emptyText: <Empty description={t('scenario.noImportData', '暂无可引用数据,可切换项目获取数据')} /> }} />
          )}
          {tab === 'case' && (
            <Table<ApiCase> rowKey="id" size="small" columns={caseCols} dataSource={fCases} pagination={{ pageSize: 10, size: 'small' }} rowSelection={{ selectedRowKeys: selCase, onChange: (k) => setSelCase(k.map(String)) }} locale={{ emptyText: <Empty description={t('scenario.noImportData', '暂无可引用数据,可切换项目获取数据')} /> }} />
          )}
          {tab === 'scenario' && (
            <Table<Scenario> rowKey="id" size="small" columns={scnCols} dataSource={fScns} pagination={{ pageSize: 10, size: 'small' }} rowSelection={{ selectedRowKeys: selScn, onChange: (k) => setSelScn(k.map(String)) }} locale={{ emptyText: <Empty description={t('scenario.noImportData', '暂无可引用数据,可切换项目获取数据')} /> }} />
          )}
        </div>
      </div>
    </ResizableDrawer>
  )
}

// New scenario: full-screen tab (ref #38). Left = step editor, right = basic info form; saving creates the scenario and opens its detail.
function NewScenarioTab({ projectId, modules, onCreated, active }: { projectId: string; modules: ApiModule[]; onCreated: (s: Scenario) => void; active?: boolean }) {
  const { t } = useI18n()
  const slot = useWorkspaceExtraSlot()
  const [name, setName] = useState('')
  const [moduleId, setModuleId] = useState('')
  const [priority, setPriority] = useState('P0')
  const [status, setStatus] = useState('DRAFT')
  const [tags, setTags] = useState<string[]>([])
  const [tagInput, setTagInput] = useState('')
  const [desc, setDesc] = useState('')
  const [saving, setSaving] = useState(false)
  const [localSteps, setLocalSteps] = useState<StepBody[]>([])
  const [add, setAdd] = useState('')
  const [importOpen, setImportOpen] = useState(false)
  const [customReqOpen, setCustomReqOpen] = useState(false)
  const nextOrder = localSteps.length + 1
  const save = async () => {
    if (!name.trim()) return message.warning(t('scenario.nameRequired', '请输入场景名'))
    setSaving(true)
    try {
      const s = await api.createScenario(projectId, name.trim())
      for (let i = 0; i < localSteps.length; i++) await api.addStep(s.id, { ...localSteps[i], order: i + 1 })
      await api.updateScenario(s.id, { name: name.trim(), status, meta: { moduleId, priority, tags, description: desc } })
      message.success(t('scenario.created', '场景已创建'))
      onCreated(s)
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('scenario.createFailed', '创建失败'))
    } finally {
      setSaving(false)
    }
  }
  const field = (label: string, node: ReactNode, req?: boolean) => (
    <div style={{ marginBottom: 14 }}>
      <div style={{ fontSize: 13, color: 'var(--text-2)', marginBottom: 6 }}>{req && <span style={{ color: 'var(--error)', marginRight: 4 }}>*</span>}{label}</div>
      {node}
    </div>
  )
  const soon = (label: string) => <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={`${label} · ${t('scenario.saveFirst', '保存场景后可编辑')}`} style={{ margin: '32px 0' }} />
  // Local step body to a display-only pseudo step (no id, only for StepRow rendering).
  const bodyToStep = (b: StepBody, i: number): ScenarioStep => ({
    id: `local-${i}`,
    order: b.order,
    kind: b.kind,
    refMode: 'REFERENCE',
    caseId: b.kind === 'CASE' ? b.refId || null : null,
    scenarioId: b.kind === 'SCENARIO' ? b.refId || null : null,
    request: (b.request as ScenarioStep['request']) || null,
    control: (b.control as ScenarioStep['control']) || null,
  })
  const stepsTab = (
    <div>
      <Typography.Text strong style={{ fontSize: 13 }}>{t('scenario.totalPrefix', '共')} {localSteps.length} {t('scenario.totalSuffix', '个步骤')}</Typography.Text>
      <div style={{ marginTop: 10 }}>
        {localSteps.map((b, i) => (
          <div key={i} style={{ position: 'relative' }}>
            <StepRow node={stepToNode(bodyToStep(b, i), t, (id) => id.slice(0, 8))} idx={i + 1} depth={0} t={t} />
            <Button type="text" size="small" danger icon={<DeleteOutlined />} onClick={() => setLocalSteps(localSteps.filter((_, idx) => idx !== i))} style={{ position: 'absolute', right: 8, top: 8 }} />
          </div>
        ))}
        <Dropdown
          menu={{
            items: [
              { type: 'group', label: t('scenario.grpRequest', '请求 / 场景'), children: [
                { key: 'IMPORT', label: t('scenario.importSystem', '导入系统请求') },
                { key: 'REQUEST', label: t('scenario.customRequest', '自定义请求') },
              ] },
              { type: 'group', label: t('scenario.grpLogic', '逻辑控制'), children: [
                { key: 'LOOP', label: t('scenario.stepLoop', '循环控制器') },
                { key: 'IF', label: t('scenario.stepIf', '条件控制器') },
                { key: 'ONCE', label: t('scenario.stepOnce', '仅一次控制器') },
              ] },
              { type: 'group', label: t('scenario.grpOther', '其他'), children: [{ key: 'TIMER', label: t('scenario.stepTimer', '等待时间') }] },
            ],
            onClick: ({ key }) => { if (key === 'IMPORT') setImportOpen(true); else if (key === 'REQUEST') setCustomReqOpen(true); else setAdd(key) },
          }}
        >
          <Button type="dashed" icon={<PlusOutlined />} block>{t('scenario.addStep', '添加步骤')}</Button>
        </Dropdown>
      </div>
      <AddStepModal type={add} scenarioId="" projectId={projectId} nextOrder={nextOrder} onClose={() => setAdd('')} onAdded={() => setAdd('')} onLocalAdd={(b) => setLocalSteps((prev) => [...prev, b])} />
      <CustomRequestDrawer open={customReqOpen} scenarioId="" nextOrder={nextOrder} onClose={() => setCustomReqOpen(false)} onAdded={() => undefined} onLocalAdd={(b) => setLocalSteps((prev) => [...prev, b])} />
      <ImportRequestDrawer open={importOpen} scenarioId="" projectId={projectId} nextOrder={nextOrder} onClose={() => setImportOpen(false)} onImported={() => undefined} onLocalImport={(bs) => setLocalSteps((prev) => [...prev, ...bs])} />
    </div>
  )
  return (
    <div style={{ padding: '12px 16px', height: '100%', overflow: 'auto' }}>
      {/* Toolbar portaled into the tab-bar right slot; unsaved scenario: env/run disabled, only save works. */}
      {active && slot && createPortal(
        <ScenarioActionBar
          envs={[]}
          envId=""
          onEnv={() => undefined}
          saving={saving}
          onSave={save}
          runDisabled
          envDisabled
          runTitle={t('scenario.saveFirst', '保存场景后可执行')}
          t={t}
        />,
        slot,
      )}
      <div style={{ display: 'flex', gap: 16, alignItems: 'flex-start' }}>
        <div style={{ flex: 1, minWidth: 0 }}>
          <Tabs className="ms-detail-tabs" size="small" items={[
            { key: 'steps', label: t('scenario.stepsTab', '步骤'), children: stepsTab },
            { key: 'params', label: t('scenario.paramsTab', '参数'), children: soon(t('scenario.paramsTab', '参数')) },
            { key: 'prepost', label: t('scenario.prePostTab', '前/后置'), children: soon(t('scenario.prePostTab', '前/后置')) },
            { key: 'assert', label: t('apidef.assertions', '断言'), children: soon(t('apidef.assertions', '断言')) },
            { key: 'settings', label: t('apidef.settings', '设置'), children: soon(t('apidef.settings', '设置')) },
          ]} />
        </div>
        {/* Right basic-info form (ref #38). */}
        <div style={{ width: 320, flexShrink: 0, borderLeft: '1px solid var(--border-soft)', paddingLeft: 16 }}>
          {field(t('scenario.name', '场景名称'), <Input value={name} onChange={(e) => setName(e.target.value)} placeholder={t('scenario.namePlaceholder2', '请输入场景名称')} />, true)}
          {field(t('scenario.ownerModule', '所属模块'), <Select style={{ width: '100%' }} value={moduleId || ''} onChange={(v) => setModuleId(v || '')} placeholder={t('scenario.unplanned', '未规划场景')} options={[{ value: '', label: t('scenario.unplanned', '未规划场景') }, ...modules.map((m) => ({ value: m.id, label: m.name }))]} />)}
          {field(t('scenario.priority', '场景等级'), <Select style={{ width: '100%' }} value={priority} onChange={setPriority} options={SCENARIO_PRIORITIES.map((p) => ({ value: p, label: <span style={{ color: priorityColor(p) }}>● <span style={{ color: 'var(--text)' }}>{p}</span></span> }))} />)}
          {field(t('scenario.sceneStatus', '场景状态'), <Select style={{ width: '100%' }} value={status} onChange={setStatus} options={SCENARIO_STATUSES.map((s) => ({ value: s, label: scStatusLabel(s, t) }))} />)}
          {field(t('scenario.tags', '标签'), (
            <Space size={[6, 6]} wrap>
              {tags.map((tg) => <Tag key={tg} closable onClose={() => setTags(tags.filter((x) => x !== tg))}>{tg}</Tag>)}
              <Input size="small" style={{ width: 140 }} value={tagInput} onChange={(e) => setTagInput(e.target.value)} onPressEnter={() => { const v = tagInput.trim(); if (v && !tags.includes(v)) setTags([...tags, v]); setTagInput('') }} placeholder={t('apidef.addTag', '添加标签,回车结束')} />
            </Space>
          ))}
          {field(t('scenario.descLabel', '描述'), <Input.TextArea rows={3} value={desc} onChange={(e) => setDesc(e.target.value)} placeholder={t('scenario.descPlaceholder', '请对该场景进行描述')} />)}
        </div>
      </div>
    </div>
  )
}

// Import scenario drawer (ref #39): format tabs + module + import mode + file dropzone. Only HAR parses client-side; MeterSphere/Jmeter need the backend.
/** One HAR request entry (only the fields this platform uses). */
type HarEntry = { request?: { method?: string; url?: string; postData?: { text?: string } } }

/** Parse HAR (JSON) log.entries into REQUEST step bodies. Entries without method/url are skipped. */
function parseHarSteps(text: string): StepBody[] {
  const har = JSON.parse(text) as { log?: { entries?: HarEntry[] } }
  const entries = har?.log?.entries
  if (!Array.isArray(entries)) throw new Error('invalid HAR: missing log.entries')
  const steps: StepBody[] = []
  entries.forEach((e, i) => {
    const r = e.request
    if (!r?.method || !r?.url) return
    const raw = r.postData?.text
    const body = typeof raw === 'string' && raw.length > 0 ? raw : undefined
    steps.push({ kind: 'REQUEST', order: i + 1, request: { method: r.method.toUpperCase(), url: r.url, body } })
  })
  return steps
}

function ImportScenarioDrawer({ open, projectId, modules, onClose, onImported }: { open: boolean; projectId: string; modules: ApiModule[]; onClose: () => void; onImported?: () => void }) {
  const { t } = useI18n()
  const [fmt, setFmt] = useState('Har')
  const [moduleId, setModuleId] = useState('')
  const [mode, setMode] = useState('skip')
  const [file, setFile] = useState<File | null>(null)
  const [busy, setBusy] = useState(false)
  const label = (s: string) => <div style={{ fontSize: 13, color: 'var(--text-2)', margin: '14px 0 6px' }}>{s}</div>

  const doImport = async () => {
    if (fmt !== 'Har') {
      message.info(t('scenario.importParseSoon', '该格式解析需后端支持(MeterSphere/Jmeter),后续接入'))
      return
    }
    if (!file) { message.warning(t('scenario.fileRequired', '请先选择文件')); return }
    setBusy(true)
    try {
      const steps = parseHarSteps(await file.text())
      if (!steps.length) { message.warning(t('scenario.harEmpty', 'HAR 中未找到可导入的请求')); return }
      const name = file.name.replace(/\.[^.]+$/, '') || t('scenario.importScenario', '导入场景')
      const s = await api.createScenario(projectId, name)
      for (let i = 0; i < steps.length; i++) await api.addStep(s.id, { ...steps[i], order: i + 1 })
      if (moduleId) await api.updateScenario(s.id, { name, meta: { moduleId } })
      message.success(t('scenario.importedN', '已导入 {n} 个请求').replace('{n}', String(steps.length)))
      setFile(null)
      onImported?.()
      onClose()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : e instanceof Error ? e.message : t('scenario.importFailed', '导入失败'))
    } finally {
      setBusy(false)
    }
  }

  return (
    <ResizableDrawer
      open={open}
      onClose={onClose}
      width={560}
      title={t('scenario.importScenario', '导入场景')}
      footer={<div style={{ textAlign: 'right' }}><Space><Button onClick={onClose}>{t('a.cancel', '取消')}</Button><Button type="primary" loading={busy} onClick={doImport}>{t('scenario.doImport', '导入')}</Button></Space></div>}
    >
      <Segmented value={fmt} onChange={(v) => setFmt(v as string)} options={[{ label: 'MeterSphere', value: 'MeterSphere' }, { label: 'Jmeter', value: 'Jmeter' }, { label: 'Har', value: 'Har' }]} />
      {label(t('scenario.ownerModule', '所属模块'))}
      <Select style={{ width: '100%' }} value={moduleId || ''} onChange={(v) => setModuleId(v || '')} placeholder={t('scenario.unplanned', '未规划场景')} options={[{ value: '', label: t('scenario.unplanned', '未规划场景') }, ...modules.map((m) => ({ value: m.id, label: m.name }))]} />
      {label(t('scenario.importMode', '导入模式'))}
      <Radio.Group value={mode} onChange={(e) => setMode(e.target.value)}>
        <Radio value="cover">{t('scenario.modeCover', '覆盖')}</Radio>
        <Radio value="skip">{t('scenario.modeSkip', '不覆盖')}</Radio>
      </Radio.Group>
      <div style={{ marginTop: 16 }}>
        <Upload.Dragger maxCount={1} accept=".ms,.json,.jmx,.har" beforeUpload={(f) => { setFile(f); return false }} onRemove={() => setFile(null)}>
          <p className="ant-upload-drag-icon"><InboxOutlined style={{ color: 'var(--brand)' }} /></p>
          <p className="ant-upload-text">{t('scenario.dropFile', '拖拽或点击此区域选择文件')}</p>
          <p className="ant-upload-hint" style={{ fontSize: 12 }}>{t('scenario.fileHint', 'HAR 直接解析为请求步骤;MeterSphere/Jmeter 解析后续接入')}</p>
        </Upload.Dragger>
      </div>
    </ResizableDrawer>
  )
}
