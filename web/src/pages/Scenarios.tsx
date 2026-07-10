import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import { useSearchParams } from 'react-router-dom'
import { Button, Divider, Drawer, Dropdown, Empty, Form, Input, Modal, Popover, Radio, Segmented, Select, Space, Switch, Table, Tabs, Tag, Tooltip, Typography, Upload } from 'antd'
import { message, modal } from '../feedback'
import { PlayCircleOutlined, PlusOutlined, SaveOutlined, ThunderboltOutlined, DownOutlined, LinkOutlined, SwapOutlined, DeleteOutlined, FullscreenOutlined, CloseOutlined, SearchOutlined, FilterOutlined, ReloadOutlined, MoreOutlined, ImportOutlined, InboxOutlined, EyeOutlined, SettingOutlined, ShareAltOutlined } from '@ant-design/icons'
import { api, ApiError, type ApiCase, type ApiDefinition, type ApiModule, type ApiView, type AssertionResult, type DebugResponse, type Environment, type ReportResultItem, type Scenario, type ScenarioChange, type ScenarioExecution, type ScenarioReportDetail, type ScenarioRunResult, type ScenarioStep } from '../api'
import type { ColumnsType } from 'antd/es/table'
import { useApp } from '../context'
import { methodColor, statusColor, outcomeColor, priorityColor } from '../components/tags'
import { Workspace, useWorkTabs, useWorkspaceExtraSlot, useOpenParam } from '../components/Workspace'
import { ModuleTreePanel, inSelectedModule } from '../components/ModuleTreePanel'
import AssertionEditor from '../components/AssertionEditor'
import ProcessorEditor from '../components/ProcessorEditor'
import KVEditor, { type KVRow } from '../components/KVEditor'
import { DebugResultPanel, type SentRequest } from '../components/ApiSpecPanel'
import { SelectProjectEmpty } from '../components/Page'
import { useI18n } from '../i18n'

type TFn = (key: string, fallback?: string) => string
// 可编辑表单 + 场景参数行(存入 scenario.meta)。
type ScenarioParam = { name: string; type: string; value: string; tags: string; desc: string }
type ScenarioForm = { name: string; status: string; description: string; tags: string[]; priority: string; params: ScenarioParam[]; csv: string; moduleId: string; disabledSteps: string[]; preProcessors: unknown[]; postProcessors: unknown[]; assertions: unknown[]; envCookie: boolean; sharedCookie: boolean }
const SCENARIO_STATUSES = ['DRAFT', 'DEBUGGING', 'COMPLETED', 'DEPRECATED']
const SCENARIO_PRIORITIES = ['P0', 'P1', 'P2', 'P3']

// 场景状态 / 执行结果的本地化标签(避免在中文界面直出 DRAFT/ERROR 等原始枚举)。
const scStatusLabel = (s: string, t: TFn): string =>
  (({ DRAFT: t('scenario.stDraft', '草稿'), DEBUGGING: t('scenario.stDebugging', '调试中'), COMPLETED: t('scenario.stCompleted', '已完成'), DEPRECATED: t('scenario.stDeprecated', '已废弃') } as Record<string, string>)[s] || s)
const runOutcomeLabel = (o: string, t: TFn): string => {
  const v = (o || '').toUpperCase()
  if (v === 'SUCCESS' || v.includes('PASS') || v === 'OK') return t('scenario.runSuccess', '成功')
  if (v === 'ERROR' || v.includes('FAIL')) return t('scenario.runError', '失败')
  return o
}

// —— 列表「视图 + 高级筛选」(对齐接口定义页;客户端过滤)——
// 视图存进同一张 ms_api_view 表(无页面区分列),用 config.kind 标记归属、列表加载时按 kind 过滤。
const SC_VIEW_KIND = 'scenario'
/** 高级筛选条件:字段 + 操作符 + 值。 */
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
  const [importOpen, setImportOpen] = useState(false) // 导入场景抽屉
  // 列表:分页大小 / 列显隐 / 高级筛选 / 视图(全部客户端,对齐接口定义页)。
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
  const tabs = useWorkTabs()
  useOpenParam((id) => tabs.open(id)) // 支持 ?open=<scenarioId> 深链(引用关系图点击场景跳转)
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
      // 执行结果由后端 list_scenarios 直接带出(lastResult);无需逐场景请求。
      // 仅展示归属本页(config.kind === 'scenario')的视图,避免与接口定义视图混淆。
      setViews(Array.isArray(vs) ? vs.filter((v) => (v.config as ScViewConfig)?.kind === SC_VIEW_KIND) : [])
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('scenario.loadFailed', '加载场景失败'))
    } finally {
      setLoading(false)
    }
  }

  // 视图快照:当前筛选/列/分页 → config;反向 applyConfig 写回各状态。
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
  // 深链 ?view=<id>:视图加载后命中即应用,然后清参数。
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
      // 高级筛选:所有=全部命中(AND);任一=任一命中(OR)。
      const adv = conds.length === 0 ? true : advApplied.logic === 'all' ? conds.every((c) => scCondMatch(s, c)) : conds.some((c) => scCondMatch(s, c))
      return inMod && hit && adv
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [list, search, selModule, modules, advApplied])

  if (!projectId) return <SelectProjectEmpty />

  // 左侧:新建/导入(header)+ 复用 ModuleTreePanel(模块搜索 + 层级模块树 + 模块增删改)+ 回收站(footer)。
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
          <Space.Compact style={{ width: '100%' }}>
            <Button type="primary" icon={<PlusOutlined />} style={{ flex: 1 }} onClick={() => tabs.open(NEW_KEY)}>{t('scenario.newScenario', '新建场景')}</Button>
            <Button icon={<ImportOutlined />} style={{ flex: 1 }} onClick={() => setImportOpen(true)}>{t('scenario.importScenario', '导入场景')}</Button>
          </Space.Compact>
        </div>
      }
      footer={<div style={{ padding: '8px 14px', borderTop: '1px solid var(--border-soft)', color: 'var(--text-3)', fontSize: 12 }}>🗑 {t('scenario.recycleBin', '回收站')}</div>}
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
  const muted = (v?: string) => <span style={{ color: 'var(--text-3)' }}>{v || '—'}</span>
  const richCols: ColumnsType<Scenario> = [
    { key: 'id', title: 'ID', dataIndex: 'id', width: 110, render: (v: string) => <span className="ms-mono" style={{ fontSize: 12 }}>{v.slice(0, 8)}</span> },
    { key: 'name', title: t('scenario.colSceneName', '场景名称'), dataIndex: 'name', ellipsis: true, render: (v: string) => <span style={{ fontWeight: 500 }}>{v}</span> },
    { key: 'priority', title: t('scenario.priority', '场景等级'), width: 110, render: (_v, s) => { const p = (s.meta?.priority as string) || 'P0'; return <span style={{ color: priorityColor(p) }}>● {p}</span> } },
    { key: 'status', title: t('scenario.colStatus', '状态'), dataIndex: 'status', width: 110, render: (s: string) => <Tag color={statusColor(s)}>{scStatusLabel(s, t)}</Tag> },
    { key: 'execResult', title: t('scenario.colExecResult', '执行结果'), width: 110, render: (_v, s) => (s.lastResult ? <Tag color={outcomeColor(s.lastResult)} style={{ margin: 0 }}>{runOutcomeLabel(s.lastResult, t)}</Tag> : muted()) },
    { key: 'tags', title: t('scenario.tags', '标签'), width: 160, render: (_v, s) => { const tags = (s.meta?.tags as string[] | undefined) || []; return tags.length ? <Space size={[4, 4]} wrap>{tags.map((tg) => <Tag key={tg} style={{ margin: 0 }}>{tg}</Tag>)}</Space> : muted() } },
    { key: 'sceneEnv', title: t('scenario.colSceneEnv', '场景环境'), width: 130, render: (_v, s) => { const en = s.meta?.envName as string | undefined; return en ? <Tag color="blue" style={{ margin: 0 }}>{en}</Tag> : muted() } },
    { key: 'createdBy', title: t('scenario.createdBy', '创建人'), dataIndex: 'createdBy', width: 110, render: (v?: string) => muted(v || undefined) },
    { key: 'updatedBy', title: t('scenario.updatedBy', '更新人'), width: 110, render: (_v, s) => muted(s.createdBy || undefined) },
    {
      key: 'action',
      title: t('apidef.colAction', '操作'),
      width: 160,
      fixed: 'right',
      render: (_v, s) => (
        <Space size={4} onClick={(e) => e.stopPropagation()}>
          <Button type="link" size="small" onClick={() => tabs.open(s.id)}>{t('a.edit', '编辑')}</Button>
          <Button type="link" size="small" onClick={(e) => runFromList(s, e)}>{t('apidef.run', '执行')}</Button>
          <Button type="link" size="small" onClick={() => message.info(t('scenario.copySoon', '复制场景即将接入'))}>{t('a.copy', '复制')}</Button>
          <Dropdown menu={{ items: [{ key: 'del', label: t('a.delete', '删除'), danger: true }], onClick: ({ key }) => { if (key === 'del') removeScenario(s) } }}><Button type="link" size="small" icon={<MoreOutlined />} /></Dropdown>
        </Space>
      ),
    },
  ]
  // 列显隐:ID/名称/操作 固定;其余可在「表格设置」开关。
  const columns = richCols.filter((c) => !hiddenCols.includes(String(c.key)))
  const TOGGLE_COLS = richCols.filter((c) => !['id', 'name', 'action'].includes(String(c.key))).map((c) => ({ key: String(c.key), label: String(c.title) }))

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
          rowSelection={{ type: 'checkbox' }}
          onRow={(s) => ({ onClick: () => tabs.open(s.id), style: { cursor: 'pointer' } })}
          pagination={{ pageSize, size: 'small', showSizeChanger: true, pageSizeOptions: ['10', '20', '30', '50'], onShowSizeChange: (_, s) => setPageSize(s), showTotal: (total) => `${t('apidef.totalPrefix', '共')} ${total} ${t('scenario.unit', '条')}` }}
          locale={{ emptyText: <Empty description={t('scenario.empty', '暂无场景')} /> }}
        />
      </div>
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
      {/* 高级筛选抽屉(条件组合 所有/任一 + 字段/操作符/值,客户端过滤)。 */}
      <Drawer
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
      </Drawer>
    </>
  )
}

// 步骤类型 → 标签文案 + 颜色。
function makeStepMeta(t: TFn): Record<string, { label: string; color: string }> {
  return {
    REQUEST: { label: t('scenario.stepRequest', '请求'), color: 'blue' },
    CASE: { label: t('scenario.stepCase', '引用用例'), color: 'green' },
    SCENARIO: { label: t('scenario.stepScenario', '引用场景'), color: 'geekblue' },
    LOOP: { label: t('scenario.stepLoop', '循环控制器'), color: 'purple' },
    IF: { label: t('scenario.stepIf', '条件控制器'), color: 'magenta' },
    ONCE: { label: t('scenario.stepOnce', '仅一次控制器'), color: 'cyan' },
    TIMER: { label: t('scenario.stepTimer', '等待时间'), color: 'orange' },
  }
}

interface Node {
  kind: string
  content: ReactNode
  children?: Node[]
  /** 子步骤原始数据(控制器 children 项 / 子场景步骤),用于点击子步骤打开抽屉。 */
  raw?: ScenarioStep
  /** 执行结果(叶子=直接结果,父步骤=后代聚合);用于子步骤行显示绿/红最终状态。 */
  result?: ReportResultItem
}

/** 把控制器子步骤原始 json / 子场景步骤,规整成抽屉可用的 ScenarioStep。 */
function rawToStep(c: any): ScenarioStep {
  const kind = String(c?.kind || '').toUpperCase()
  const base = { id: c?.id || `child-${kind}-${c?.refId || c?.url || Math.random().toString(36).slice(2)}`, order: 0, kind, refMode: 'REFERENCE' as const }
  if (kind === 'CASE') return { ...base, caseId: c.refId }
  if (kind === 'SCENARIO') return { ...base, scenarioId: c.refId }
  if (kind === 'REQUEST') return { ...base, request: { method: c.method || 'GET', url: c.url || '', body: c.body ?? null, assertions: c.assertions } }
  return { ...base, control: c }
}

// 引用 id → 可读名称(用例/子场景);未命中回落短 id,避免满屏 UUID。
type NameOf = (id: string) => string

// 把控制器载荷里的一个子步骤(原始 json)规整为 Node。
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

// 顶层步骤(ScenarioStep)→ Node。
function stepToNode(s: ScenarioStep, t: TFn, nameOf: NameOf): Node {
  if (s.request) return { kind: 'REQUEST', content: <Space><Tag color={methodColor(s.request.method)}>{s.request.method}</Tag><span className="ms-mono">{s.request.url}</span></Space> }
  if (s.caseId) return { kind: 'CASE', content: <span className="ms-mono">{t('scenario.caseRef', '用例')} {nameOf(s.caseId)}</span> }
  if (s.scenarioId) return { kind: 'SCENARIO', content: <span className="ms-mono">{t('scenario.subScenario', '子场景')} {nameOf(s.scenarioId)}</span> }
  if (s.control) return controlToNode(s.kind.toUpperCase(), s.control, t, nameOf)
  return { kind: s.kind, content: '—' }
}

function StepRow({ node, idx, depth, t, result, running, seq = 0, enabled = true, onToggle, onRun, actions, hovered, respPreview, expandable, expanded, onChildSelect }: { node: Node; idx: number; depth: number; t: TFn; result?: ReportResultItem; running?: boolean; seq?: number; enabled?: boolean; onToggle?: () => void; onRun?: () => void; actions?: React.ReactNode; hovered?: boolean; respPreview?: React.ReactNode; expandable?: boolean; expanded?: boolean; onChildSelect?: (raw: ScenarioStep, idx: number) => void }) {
  const meta = makeStepMeta(t)[node.kind] || { label: node.kind, color: 'default' }
  const ok = result?.outcome === 'SUCCESS'
  const muted: React.CSSProperties = { color: 'var(--text-3)', fontSize: 12, whiteSpace: 'nowrap' }
  const leaf = depth === 0
  return (
    <>
      <div
        style={{
          position: 'relative',
          isolation: 'isolate', // 自成层叠上下文,使 z-index:-1 的进度填充落在底色之上、内容之下
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
        {/* 执行进度:整行背景左→右填充(置于内容之下)。执行中蓝色按 seq 错峰起跑;完成后整行变绿(通过)/红(失败)。 */}
        {running ? (
          <span className="ms-step-fillbg run" style={{ animationDelay: `${seq}s` }} />
        ) : result ? (
          <span className="ms-step-fillbg done" style={{ background: ok ? 'rgba(34,197,94,0.16)' : 'rgba(239,68,68,0.16)' }} />
        ) : null}
        {/* 可展开(子场景/循环/条件/仅一次):▸/▾ 折叠箭头;叶子步骤留占位对齐。 */}
        {expandable
          ? <span style={{ color: 'var(--text-3)', fontSize: 11, width: 12, cursor: 'pointer' }}>{expanded ? '▾' : '▸'}</span>
          : <span style={{ width: 12 }} />}
        <span style={{ color: 'var(--text-3)', cursor: 'grab' }}>⠿</span>
        {/* 启用/禁用本步骤(禁用则置灰);播放=单步服务端执行。两者均阻止冒泡(不打开抽屉)。 */}
        <Switch size="small" checked={enabled} disabled={!leaf || !onToggle} onChange={() => onToggle?.()} onClick={(_c, e) => e.stopPropagation()} />
        <PlayCircleOutlined
          style={{ color: leaf && onRun ? 'var(--brand)' : '#c9cdd4', cursor: leaf && onRun ? 'pointer' : 'default' }}
          onClick={(e) => { e.stopPropagation(); if (leaf) onRun?.() }}
        />
        <span style={{ color: 'var(--text-3)', fontSize: 12, minWidth: 18 }}>{idx}</span>
        <Tag color={meta.color} style={{ margin: 0 }}>{meta.label}</Tag>
        <span style={{ flex: 1, minWidth: 0 }}>{node.content}</span>
        {/* 执行后逐步结果(对齐参考图 #28):通过/状态码/响应时间/响应大小。
            悬停「结果区」(行右侧,即鼠标所在处)弹出响应详情 → 锚定右侧,不再飘到左边。 */}
        {!running && result && (() => {
          const cluster = (
            <span style={{ display: 'inline-flex', alignItems: 'center', gap: 8 }} onClick={(e) => e.stopPropagation()}>
              <Tag color={ok ? 'green' : 'red'} style={{ margin: 0 }}>{ok ? t('scenario.pass', '通过') : t('scenario.fail', '失败')}</Tag>
              {result.statusCode != null && <span style={muted}>{t('apidef.statusCode', '状态码')} <span style={{ color: result.statusCode < 400 ? '#52c41a' : '#ff4d4f' }}>{result.statusCode}</span></span>}
              <span style={muted}>{t('scenario.respTime', '响应时间')} {result.latencyMs != null ? `${result.latencyMs} ms` : '—'}</span>
              <span style={muted}>{t('scenario.respSize', '响应大小')} {result.respSize != null ? `${result.respSize} bytes` : '—'}</span>
            </span>
          )
          return respPreview
            ? <Popover content={respPreview} trigger="hover" placement="bottomRight" mouseEnterDelay={0.35}>{cluster}</Popover>
            : cluster
        })()}
        {/* 悬停显示:向上插入 / 向下插入 / 删除(右侧)。 */}
        {actions && (
          <span
            style={{ display: 'flex', gap: 2, marginLeft: 4, opacity: hovered ? 1 : 0, pointerEvents: hovered ? 'auto' : 'none', transition: 'opacity .15s' }}
            onClick={(e) => e.stopPropagation()}
          >
            {actions}
          </span>
        )}
      </div>
      {/* 子步骤:仅展开时渲染;点击子步骤打开右侧抽屉(经 onChildSelect 上抛原始步骤)。 */}
      {expanded && node.children?.map((c, i) => (
        <div
          key={i}
          onClick={(e) => { if (c.raw && onChildSelect) { e.stopPropagation(); onChildSelect(c.raw, i) } }}
          style={{ cursor: c.raw && onChildSelect ? 'pointer' : 'default' }}
        >
          <StepRow node={c} idx={i + 1} depth={depth + 1} t={t} result={c.result} running={running} seq={seq + (i + 1) * 0.12} onChildSelect={onChildSelect} expandable={(c.children?.length ?? 0) > 0} expanded />
        </div>
      ))}
    </>
  )
}

// 步骤响应预览(悬停弹出):状态行 + 响应体 / 响应头 标签。仅在该步已执行时使用。
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
        {r.statusCode != null && <span style={{ fontSize: 13, fontWeight: 600, color: r.statusCode < 400 ? '#52c41a' : '#ff4d4f' }}>{r.statusCode}</span>}
        <span style={{ fontSize: 12, color: 'var(--text-3)' }}>{r.latencyMs ?? '—'} ms</span>
        <span style={{ fontSize: 12, color: 'var(--text-3)' }}>{r.respSize ?? '—'} bytes</span>
      </div>
      <Tabs size="small" items={items} />
    </div>
  )
}

// 场景详情:全标签编辑器外壳(对齐参考图 #20-#24:头部 + 基本信息/步骤/参数/前后置/断言/
// 场景工具栏(环境 + 服务端执行 + 保存):详情页与新建页复用,经 Workspace 插槽
// 投射到 Tab 栏右侧(对齐参考图 #38 红色区域)。runDisabled 时执行按钮置灰(新建未保存)。
function ScenarioActionBar({ envs, envId, onEnv, running, onRun, onLocalRun, saving, onSave, runDisabled, envDisabled, runTitle, viewReport, t }: {
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
  t: TFn
}) {
  return (
    <>
      {viewReport}
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

// 执行历史/变更历史/设置 + 顶部右侧 环境/服务端执行/保存)。步骤详情抽屉(#25)、可编辑元信息
// (需后端 updateScenario)、报告(#26)为后续切片。
function ScenarioDetail({ scenario, active }: { scenario: Scenario; active?: boolean }) {
  const { t } = useI18n()
  const slot = useWorkspaceExtraSlot()
  const [steps, setSteps] = useState<ScenarioStep[]>([])
  const [running, setRunning] = useState(false)
  const [add, setAdd] = useState<string>('') // 当前打开的添加表单类型
  const [importOpen, setImportOpen] = useState(false) // 导入系统请求抽屉
  const [customReqOpen, setCustomReqOpen] = useState(false) // 自定义请求抽屉
  const [hoverStep, setHoverStep] = useState<string | null>(null) // 悬停的步骤(显示行内插入/删除)
  const [expandedSteps, setExpandedSteps] = useState<Set<string>>(new Set()) // 已展开的控制器/子场景步骤
  const [subSteps, setSubSteps] = useState<Record<string, ScenarioStep[]>>({}) // 子场景 id → 其步骤(展开时按需加载)
  // 插入位置:记录目标下标 + 插入前的步骤 id 集合(用于把新增步骤块挪到目标位置)。
  const [pendingInsert, setPendingInsert] = useState<{ at: number; before: string[] } | null>(null)
  const [lastRun, setLastRun] = useState<ScenarioRunResult | null>(null)
  const [lastRunAt, setLastRunAt] = useState<string>('')
  // 执行后逐步结果(按 caseId 归集:REQUEST→"METHOD url",CASE→case_id)+ 报告弹窗。
  const [stepResults, setStepResults] = useState<Record<string, ReportResultItem>>({})
  const [reportModalId, setReportModalId] = useState<string | null>(null)
  const [nameMap, setNameMap] = useState<Record<string, string>>({})
  const [caseMap, setCaseMap] = useState<Record<string, ApiCase>>({})
  const [selStep, setSelStep] = useState<{ step: ScenarioStep; idx: number } | null>(null)
  const [dragIdx, setDragIdx] = useState<number | null>(null)
  // 执行配置:环境 + 步骤失败规则(后端 run 已支持 environment_id/failure_strategy)。
  const [envs, setEnvs] = useState<Environment[]>([])
  const [envId, setEnvId] = useState<string>('')
  const [failureStrategy, setFailureStrategy] = useState<'CONTINUE' | 'STOP'>('CONTINUE')
  // 可编辑基本信息 + 参数(保存走 PATCH /api/scenario/{id};meta 承载描述/标签/等级/参数)。
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
  // 引用名解析:命中用例/子场景名,未命中回落短 id(前 8 位),不再满屏 UUID。
  const nameOf = (id: string) => nameMap[id] || (id ? id.slice(0, 8) : '—')

  const loadSteps = async () => {
    try {
      const s = await api.getScenario(scenario.id)
      setSteps(s.steps || [])
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('scenario.loadStepsFailed', '加载步骤失败'))
    }
  }
  useEffect(() => {
    loadSteps()
    // 拉项目用例 + 场景,建 id→名 映射供步骤展示;拉环境供执行选择。
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
      // 优先用已保存的场景环境(meta.envId);否则回落第一个启用的环境。
      const savedEnvId = scenario.meta?.envId as string | undefined
      setEnvId((cur) => cur || (savedEnvId && environments.some((e) => e.id === savedEnvId) ? savedEnvId : environments.find((e) => e.enabled !== false)?.id || ''))
      setModules(mods)
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scenario.id])

  const run = async () => {
    setRunning(true)
    try {
      const r = await api.runScenario(scenario.id, scenario.projectId, { environmentId: envId || undefined, failureStrategy })
      setLastRun(r)
      setLastRunAt(new Date().toLocaleString())
      // 拉报告明细,把逐步结果(通过/状态码/耗时/大小)映射到步骤行。
      const rep = await api.scenarioReport(r.reportId).catch(() => null)
      if (rep) {
        const map: Record<string, ReportResultItem> = {}
        rep.results.forEach((res) => { map[res.caseId] = res })
        setStepResults(map)
        // 顶层子场景的最终状态需聚合其叶子结果 → 补载未加载的子场景步骤(否则父步骤行执行后会空白)。
        await Promise.all(
          steps
            .filter((s) => s.scenarioId && !subSteps[s.scenarioId])
            .map(async (s) => {
              try {
                const sc = await api.getScenario(s.scenarioId as string)
                setSubSteps((m) => ({ ...m, [s.scenarioId as string]: sc.steps || [] }))
              } catch {
                /* 子场景加载失败:该父步骤聚合不出结果,不阻断 */
              }
            }),
        )
      }
      message.success(`${t('scenario.triggered', '场景已触发执行')} · ${r.status}`)
    } catch (e) {
      message.error(e instanceof ApiError ? `${t('scenario.execFailed', '执行失败')}:${e.status}` : t('scenario.execFailed', '执行失败'))
    } finally {
      setRunning(false)
    }
  }
  // 步骤 → 结果键:CASE 用 case_id;REQUEST 用 "METHOD url"(对齐执行器 label)。
  const stepKey = (s: ScenarioStep): string | null => (s.caseId ? s.caseId : s.request ? `${s.request.method} ${s.request.url}` : null)
  // 顶层「子场景 / 控制器」步骤本身在报告里没有结果(报告只含叶子用例),收集其后代叶子结果键。
  const collectLeafKeys = (s: ScenarioStep): string[] => {
    const k = stepKey(s)
    if (k) return [k]
    if (s.scenarioId) return (subSteps[s.scenarioId] || []).flatMap(collectLeafKeys)
    return []
  }
  // 取某步的执行结果:叶子=直接结果;父步骤=从后代叶子结果聚合(任一失败即失败),
  // 使执行后父步骤行也能保留最终状态(绿=全通过 / 红=有失败),不随渲染消失。
  const resultFor = (s: ScenarioStep): ReportResultItem | undefined => {
    const k = stepKey(s)
    if (k && stepResults[k]) return stepResults[k]
    const rs = collectLeafKeys(s).map((kk) => stepResults[kk]).filter(Boolean) as ReportResultItem[]
    if (!rs.length) return undefined
    const fail = rs.some((r) => r.outcome !== 'SUCCESS')
    return { caseId: s.id, outcome: fail ? 'ERROR' : 'SUCCESS', failures: [], executedAt: '' }
  }
  // 单步执行(点击步骤行播放按钮):组装该步请求走 /api/debug/send,把结果写回该步。
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
  // 拖拽重排:把 from 移到 to,乐观更新本地顺序后 PATCH 落库。
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
  // 添加完成:新步骤总是追加在末尾;若是「行内插入」(pendingInsert),把新增的步骤块挪到目标位置再落库。
  const onAdded = async () => {
    setAdd('')
    const pi = pendingInsert
    setPendingInsert(null)
    const sc = await api.getScenario(scenario.id).catch(() => null)
    let arr = sc ? [...(sc.steps || [])].sort((a, b) => a.order - b.order) : []
    if (pi && arr.length) {
      const added = arr.filter((s) => !pi.before.includes(s.id)) // 新增块(追加在末尾,保序)
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
  }
  // 行内插入:记录目标下标 + 当前步骤 id 快照,然后打开对应添加入口(自定义请求/导入/控制器)。
  const startInsert = (key: string, at: number) => {
    setPendingInsert({ at, before: steps.map((s) => s.id) })
    if (key === 'IMPORT') setImportOpen(true)
    else if (key === 'REQUEST') setCustomReqOpen(true)
    else setAdd(key)
  }
  // 删除单个步骤。
  const removeStep = async (s: ScenarioStep) => {
    try {
      await api.deleteScenarioStep(scenario.id, s.id)
      message.success(t('scenario.stepDeleted', '步骤已删除'))
      loadSteps()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('scenario.deleteFailed', '删除失败'))
    }
  }
  // 步骤类型子菜单(键编码插入位置 at;与底部「添加步骤」一致)。
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
  // 行内插入选择(key 形如 KIND@at)。
  const onInsertPick = (key: string) => {
    const at = key.lastIndexOf('@')
    if (at >= 0) startInsert(key.slice(0, at), Number(key.slice(at + 1)))
  }
  // 复制步骤:构造与原步骤等价的 body,追加后挪到原步骤之后。
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
  // 单行悬停操作:+(在上方/下方插入 → 步骤类型)· ⋯(复制 / 删除),对齐参考设计。
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

  // 展开/收起控制器或子场景步骤;子场景首次展开时按需加载其步骤。
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
        /* 子场景加载失败:展开后显示空,不阻断 */
      }
    }
  }

  // 可展开的顶层步骤(子场景 / 循环 / 条件 / 仅一次);用于「展开全部 / 收起全部」。
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
    // 展开全部时按需加载所有子场景的步骤(首次)。
    for (const s of ordered) {
      if (s.scenarioId && !subSteps[s.scenarioId]) {
        try {
          const sc = await api.getScenario(s.scenarioId)
          setSubSteps((m) => ({ ...m, [s.scenarioId as string]: sc.steps || [] }))
        } catch {
          /* 子场景加载失败:展开后显示空,不阻断 */
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
          // 子场景:展开后注入其步骤为子节点(带各自结果,使子步骤行也显示绿/红);控制器:children 来自其载荷。
          const node = stepToNode(s, t, nameOf)
          if (s.scenarioId && subSteps[s.scenarioId]) {
            node.children = subSteps[s.scenarioId].map((cs) => ({ ...stepToNode(cs, t, nameOf), raw: cs, result: resultFor(cs) }))
          }
          // 可展开:子场景 / 控制器(LOOP/IF/ONCE)。点击展开而非打开抽屉;叶子步骤点击打开抽屉。
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
              onClick={() => (expandable ? toggleExpand(s) : setSelStep({ step: s, idx: i + 1 }))}
              style={{ cursor: 'pointer', opacity: dragIdx === i ? 0.5 : 1 }}
            >
              <StepRow
                node={node}
                idx={i + 1}
                depth={0}
                t={t}
                result={res}
                running={running}
                seq={i * 0.18}
                enabled={!form.disabledSteps.includes(s.id)}
                onToggle={() => toggleStep(s.id)}
                onRun={() => runStep(s)}
                hovered={hoverStep === s.id}
                actions={rowActions(s, i)}
                respPreview={hasResp ? <StepRespPreview r={res!} t={t} /> : undefined}
                expandable={expandable}
                expanded={isExpanded}
                onChildSelect={(raw, ci) => setSelStep({ step: raw, idx: ci + 1 })}
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
      {/* 工具栏(环境 + 服务端执行 + 保存)投射到 Tab 栏右侧;仅活动 Tab 渲染。 */}
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
          t={t}
        />,
        slot,
      )}
      {/* 头部:状态 / 等级 / [id] / 名称 / 标签 / 描述。 */}
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
      />
      <ScenarioReportModal reportId={reportModalId} nameOf={nameOf} caseMap={caseMap} onClose={() => setReportModalId(null)} />
    </div>
  )
}

// 解析 {{var}} + 相对路径补 baseUrl,把用例/内联请求组装成可发送请求(复用调试发送的约定)。
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

// 步骤详情抽屉(对齐参考图 #25):点击步骤右侧展开;头部 + 服务端执行 + 请求标签 + 响应内容。
// 引用用例展示其请求并可服务端执行;内联请求同理;控制器展示配置。删除/替换需后端,暂占位。
function StepDetailDrawer({
  sel,
  scenarioId,
  caseMap,
  nameOf,
  env,
  result,
  onClose,
  onDeleted,
}: {
  sel: { step: ScenarioStep; idx: number } | null
  scenarioId: string
  caseMap: Record<string, ApiCase>
  nameOf: NameOf
  env?: Environment
  result?: ReportResultItem
  onClose: () => void
  onDeleted: () => void
}) {
  const { t } = useI18n()
  const [full, setFull] = useState(false)
  const [deleting, setDeleting] = useState(false)
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

  // 当前步骤可发送的请求(CASE→其用例;REQUEST→内联);控制器无。
  const reqInfo = (() => {
    if (kase) return { method: kase.method, url: kase.url, body: kase.body, headers: kase.headers || [], auth: kase.auth, assertions: kase.assertions, processors: kase.processors }
    if (step?.request) return { method: step.request.method, url: step.request.url, body: step.request.body ?? null, headers: [], auth: undefined, assertions: step.request.assertions, processors: undefined }
    return null
  })()

  // 切换步骤时,用场景执行的该步结果回填响应面板 + 实际请求(避免显示「尚未执行」);无结果才清空。
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
      // 回填「实际请求 / 控制台 / cURL」:重建实际发送的请求行(相对路径按环境 baseUrl 拼接,认证转 Authorization 头)。
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
    <Drawer
      open={!!sel}
      onClose={onClose}
      width={full ? '92%' : 680}
      title={title}
      closeIcon={false}
      extra={
        <Space>
          <Button type="text" size="small" icon={<SwapOutlined />} disabled title={t('scenario.replaceSoon', '替换(即将接入)')}>{t('scenario.replace', '替换')}</Button>
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
          <DebugResultPanel running={running} resp={resp} err={err} req={lastReq} isHttp extractors={reqInfo.processors as Record<string, unknown>[] | undefined} assertions={reqInfo.assertions as Record<string, unknown>[] | undefined} />
        </>
      ) : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('scenario.controlStepInfo', '控制器步骤:在步骤列表中查看其配置与子步骤')} style={{ margin: '48px 0' }} />
      )}
    </Drawer>
  )
}

// 基本信息(可编辑,对齐参考图 #21):名称/等级/状态/标签/描述 + 只读 ID/步骤数。保存走顶部「保存」。
function ScenarioBasicInfo({ scenario, stepCount, form, patch, modules }: { scenario: Scenario; stepCount: number; form: ScenarioForm; patch: (p: Partial<ScenarioForm>) => void; modules: ApiModule[] }) {
  const { t } = useI18n()
  const [tagInput, setTagInput] = useState('')
  const field = (label: string, value: ReactNode, req?: boolean) => (
    <div style={{ marginBottom: 14 }}>
      <div style={{ fontSize: 13, color: 'var(--text-2)', marginBottom: 6 }}>{req && <span style={{ color: '#ff4d4f', marginRight: 4 }}>*</span>}{label}</div>
      {value}
    </div>
  )
  return (
    <div style={{ maxWidth: 560 }}>
      {field(t('scenario.name', '场景名称'), <Input value={form.name} onChange={(e) => patch({ name: e.target.value })} placeholder={t('scenario.namePlaceholder', '如:下单主流程')} />, true)}
      {field(t('scenario.ownerModule', '所属模块'), <Select style={{ width: 280 }} value={form.moduleId || ''} onChange={(v) => patch({ moduleId: v || '' })} placeholder={t('scenario.unplanned', '未规划场景')} options={[{ value: '', label: t('scenario.unplanned', '未规划场景') }, ...modules.map((m) => ({ value: m.id, label: m.name }))]} />)}
      {field(t('scenario.priority', '场景等级'), <Select style={{ width: 200 }} value={form.priority} onChange={(v) => patch({ priority: v })} options={SCENARIO_PRIORITIES.map((p) => ({ value: p, label: <span style={{ color: priorityColor(p) }}>● <span style={{ color: 'var(--text)' }}>{p}</span></span> }))} />)}
      {field(t('scenario.colStatus', '场景状态'), <Select style={{ width: 200 }} value={form.status} onChange={(v) => patch({ status: v })} options={SCENARIO_STATUSES.map((s) => ({ value: s, label: s }))} />)}
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

// 场景参数(对齐参考图 #22:常规参数表 + CSV 参数;变量名称/类型/参数值/标签/描述 + 加一行)。存入 meta.params / meta.csvParams。
function ScenarioParams({ params, onChange, csv, onCsvChange }: { params: ScenarioParam[]; onChange: (p: ScenarioParam[]) => void; csv: string; onCsvChange: (v: string) => void }) {
  const { t } = useI18n()
  const [mode, setMode] = useState<'normal' | 'csv'>('normal')
  const set = (i: number, p: Partial<ScenarioParam>) => onChange(params.map((r, idx) => (idx === i ? { ...r, ...p } : r)))
  return (
    <div>
      <div style={{ background: '#f6ffed', border: '1px solid #b7eb8f', borderRadius: 6, padding: '6px 10px', marginBottom: 12, fontSize: 12, color: '#389e0d' }}>
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

// 设置:步骤执行失败规则(对齐参考图 #24;映射到 run 的 failure_strategy)。Cookie 配置占位。
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

// 执行历史标签(对齐参考图 #23):序号 / 状态 / 用例数 / 时间 / 操作(执行结果→报告)。
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
          { title: t('scenario.colStatus', '执行状态'), dataIndex: 'status', width: 110, render: (s: string) => <Tag color={outcomeColor(s)}>{s}</Tag> },
          { title: t('scenario.caseUnit', '用例'), dataIndex: 'caseCount', width: 80 },
          { title: t('scenario.execTime', '操作时间'), dataIndex: 'createdAt', width: 200, render: (v: string) => <span className="ms-mono" style={{ fontSize: 12 }}>{v?.slice(0, 19)}</span> },
          { title: t('apidef.colAction', '操作'), width: 100, render: (_v, r) => <Button type="link" size="small" disabled={!r.reportId} onClick={() => setReportId(r.reportId)}>{t('scenario.viewResult', '执行结果')}</Button> },
        ]}
      />
      <ScenarioReportModal reportId={reportId} nameOf={nameOf} caseMap={caseMap} onClose={() => setReportId(null)} />
    </>
  )
}

// 变更历史标签(审计日志):操作 / 详情 / 操作人 / 时间。
const CHANGE_ACTIONS: Record<string, { tkey: string; fallback: string; color: string }> = {
  CREATE: { tkey: 'scenario.actionCreate', fallback: '创建', color: 'green' },
  UPDATE: { tkey: 'scenario.actionUpdate', fallback: '更新', color: 'blue' },
  ADD_STEP: { tkey: 'scenario.actionAddStep', fallback: '新增步骤', color: 'geekblue' },
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

// 场景报告(对齐参考图 #26):报告头(状态/用例数)+ 报告明细逐步结果(通过/失败 + 失败原因)。
// 注:响应时间/大小/状态码/响应体当前未持久化(执行器仅记录通过失败 + 失败原因),展示为 — ;
// 完整明细需扩展执行器落库(后续切片)。
// 4 态分布:通过 / 误报 / 失败 / 未执行(对齐参考报告:步骤分析 / 请求分析)。
type Dist = { pass: number; falsePos: number; fail: number; skip: number }
const DIST_COLORS = { pass: '#22c55e', falsePos: '#f59e0b', fail: '#ef4444', skip: '#c9cdd4' }
// 多段 SVG 圆环 + 中心「总数(个) N」。
function StatRing({ d, centerLabel }: { d: Dist; centerLabel: string }) {
  const total = d.pass + d.falsePos + d.fail + d.skip
  const C = 2 * Math.PI * 42
  const order: (keyof Dist)[] = ['pass', 'falsePos', 'fail', 'skip']
  let off = 0
  return (
    <svg width="116" height="116" viewBox="0 0 120 120">
      <g transform="rotate(-90 60 60)">
        <circle cx="60" cy="60" r="42" fill="none" stroke="#f0f2f5" strokeWidth="14" />
        {total > 0 && order.map((k) => {
          const len = (d[k] / total) * C
          const el = <circle key={k} cx="60" cy="60" r="42" fill="none" stroke={DIST_COLORS[k]} strokeWidth="14" strokeDasharray={`${len} ${C - len}`} strokeDashoffset={`-${off}`} />
          off += len
          return el
        })}
      </g>
      <text x="60" y="58" textAnchor="middle" fontSize="22" fontWeight="700" fill="#1f2329">{total}</text>
      <text x="60" y="76" textAnchor="middle" fontSize="11" fill="#8a9099">{centerLabel}</text>
    </svg>
  )
}
// 分布卡:左圆环 + 右四行图例(标签 / 数量 / 百分比)。
function DistCard({ title, d, centerLabel, t }: { title: string; d: Dist; centerLabel: string; t: TFn }) {
  const total = d.pass + d.falsePos + d.fail + d.skip
  const pct = (n: number) => (total ? ((n / total) * 100).toFixed(2) : '0.00')
  const rows: { k: keyof Dist; label: string }[] = [
    { k: 'pass', label: t('scenario.pass', '通过') },
    { k: 'falsePos', label: t('scenario.falsePos', '误报') },
    { k: 'fail', label: t('scenario.fail', '失败') },
    { k: 'skip', label: t('scenario.skip', '未执行') },
  ]
  return (
    <div style={{ flex: 1, minWidth: 300, border: '1px solid var(--border-soft)', borderRadius: 10, padding: '14px 18px', background: 'var(--panel)' }}>
      <h3 style={{ margin: '0 0 10px', fontSize: 14, color: 'var(--text-2)' }}>{title}</h3>
      <div style={{ display: 'flex', alignItems: 'center', gap: 20 }}>
        <StatRing d={d} centerLabel={centerLabel} />
        <div style={{ flex: 1 }}>
          {rows.map(({ k, label }) => (
            <div key={k} style={{ display: 'grid', gridTemplateColumns: '1fr auto auto', alignItems: 'center', columnGap: 18, padding: '5px 0', fontSize: 13 }}>
              <span style={{ color: 'var(--text-2)' }}><span style={{ color: DIST_COLORS[k] }}>●</span> {label}</span>
              <b style={{ color: 'var(--text)' }}>{d[k]}</b>
              <span style={{ color: 'var(--text-3)', minWidth: 56, textAlign: 'right' }}>{pct(d[k])}%</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}

// 场景执行报告:从右侧展开的抽屉。
// 顶部=报告分析卡(步骤数/通过/失败/通过率)+ 圆环;工具栏=过滤 + 展开/收起全部;
// 报告明细沿用现有 ReportRow 结构(逐步可展开,复用调试 7 标签面板)。
function ScenarioReportModal({ reportId, nameOf, caseMap, onClose }: { reportId: string | null; nameOf: NameOf; caseMap?: Record<string, ApiCase>; onClose: () => void }) {
  const { t } = useI18n()
  const [data, setData] = useState<ScenarioReportDetail | null>(null)
  const [loading, setLoading] = useState(false)
  const [search, setSearch] = useState('')
  const [openSet, setOpenSet] = useState<Set<number>>(new Set())
  useEffect(() => {
    if (!reportId) { setData(null); return }
    setOpenSet(new Set())
    setLoading(true)
    api.scenarioReport(reportId).then(setData).catch(() => setData(null)).finally(() => setLoading(false))
  }, [reportId])
  const all = data?.results || []
  const passN = all.filter((r) => r.outcome === 'SUCCESS').length
  const failN = all.length - passN
  const passRate = all.length ? (passN / all.length) * 100 : 0
  // 分布(误报/未执行后端暂未跟踪 → 0)。步骤与请求当前同源(扁平场景一步一请求)。
  const dist: Dist = { pass: passN, falsePos: 0, fail: failN, skip: 0 }
  // 请求总耗时 = 各步延迟之和;报告总耗时 ≈ executedAt 跨度 + 末步延迟(无有效时间戳则回落请求总耗时)。
  const reqTotalMs = all.reduce((s, r) => s + (r.latencyMs ?? 0), 0)
  // 报告总耗时:优先后端 wall-clock(durationMs,0056 后);旧报告回落 executedAt 跨度 / 请求总耗时。
  const times = all.map((r) => Date.parse((r.executedAt || '').replace(' ', 'T'))).filter((n) => !Number.isNaN(n))
  const span = times.length ? Math.max(...times) - Math.min(...times) : 0
  const reportTotalMs = (data?.durationMs != null && data.durationMs >= 0)
    ? data.durationMs
    : (span > 0 ? span + (all.length ? all[all.length - 1].latencyMs ?? 0 : 0) : reqTotalMs)
  // 断言通过率(逐条断言;无断言数据回落步骤通过率)。
  let asTotal = 0, asPass = 0
  for (const r of all) { const a = (r.assertions as AssertionResult[] | undefined) || []; asTotal += a.length; asPass += a.filter((x) => x.passed).length }
  const asRate = asTotal ? (asPass / asTotal) * 100 : passRate
  const rows = all.filter((r) => !search || r.caseId.toLowerCase().includes(search.toLowerCase()))
  const toggle = (i: number) => setOpenSet((prev) => { const n = new Set(prev); if (n.has(i)) n.delete(i); else n.add(i); return n })
  // caseId 多为可读请求行(GET http://...)或用例 UUID;UUID 用 nameOf 解析。
  const label = (id: string) => (/^[0-9a-f]{8}-/.test(id) ? nameOf(id) : id)
  const stat = (lbl: ReactNode, val: ReactNode, color?: string) => (
    <div style={{ display: 'flex', justifyContent: 'space-between', padding: '6px 0', fontSize: 14 }}><span style={{ color: 'var(--text-2)' }}>{lbl}</span><b style={{ color }}>{val}</b></div>
  )
  return (
    <Drawer
      open={!!reportId}
      onClose={onClose}
      width="60%"
      title={t('scenario.report', '场景报告')}
      styles={{ body: { background: 'var(--panel-2)' } }}
    >
      {loading ? (
        <div style={{ padding: 32, color: 'var(--text-3)' }}>{t('a.loading', '加载中…')}</div>
      ) : !data ? (
        <Empty description={t('scenario.noReport', '暂无报告')} />
      ) : (
        <>
          {/* 概览:报告分析 + 步骤分析 + 请求分析(对齐参考报告)。 */}
          <div style={{ display: 'flex', gap: 16, flexWrap: 'wrap', marginBottom: 20 }}>
            <div style={{ flex: 1, minWidth: 260, border: '1px solid var(--border-soft)', borderRadius: 10, padding: '14px 18px', background: 'var(--panel)' }}>
              <h3 style={{ margin: '0 0 10px', fontSize: 14, color: 'var(--text-2)', display: 'flex', alignItems: 'center', gap: 8 }}>
                {t('scenario.reportAnalysis', '报告分析')}<Tag color={outcomeColor(data.status)} style={{ margin: 0 }}>{runOutcomeLabel(data.status, t)}</Tag>
              </h3>
              {stat(t('scenario.reportTotalTime', '报告总耗时'), <>{(reportTotalMs / 1000).toFixed(3)} <span style={{ color: 'var(--text-3)', fontWeight: 400 }}>(sec)</span></>)}
              {stat(t('scenario.reqTotalTime', '请求总耗时'), <>{reqTotalMs} <span style={{ color: 'var(--text-3)', fontWeight: 400 }}>(ms)</span></>)}
              {stat(t('scenario.assertPassRate', '断言通过率'), <>{asRate.toFixed(2)} <span style={{ color: 'var(--text-3)', fontWeight: 400 }}>(%)</span></>, asRate >= 100 ? '#22c55e' : undefined)}
            </div>
            <DistCard title={t('scenario.stepAnalysis', '步骤分析')} d={dist} centerLabel={t('scenario.totalCount', '总数(个)')} t={t} />
            <DistCard title={t('scenario.reqAnalysis', '请求分析')} d={dist} centerLabel={t('scenario.totalCount', '总数(个)')} t={t} />
          </div>
          {/* 报告明细 */}
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 10 }}>
            <Typography.Text strong>{t('scenario.reportDetail', '报告明细')}</Typography.Text>
            <div style={{ flex: 1 }} />
            <Input size="small" allowClear style={{ width: 220 }} placeholder={t('scenario.searchByName', '通过名称搜索')} value={search} onChange={(e) => setSearch(e.target.value)} />
            <Button size="small" onClick={() => setOpenSet(new Set(rows.map((_, i) => i)))}>{t('scenario.expandAll', '展开全部')}</Button>
            <Button size="small" onClick={() => setOpenSet(new Set())}>{t('scenario.collapseAll', '收起全部')}</Button>
          </div>
          <div style={{ marginTop: 10 }}>
            {rows.length === 0 ? <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('scenario.noStepResult', '无步骤结果')} /> : rows.map((r, i) => <ReportRow key={i} idx={i + 1} r={r} label={label} t={t} open={openSet.has(i)} onToggle={() => toggle(i)} caseOf={(id) => caseMap?.[id]} />)}
          </div>
        </>
      )}
    </Drawer>
  )
}

// 据引用用例重建实际请求行(镜像后端 to_node:REST 参数替换 / Query 拼接 / 认证头)。
// 注:${var} 运行期替换无法在前端还原,这里展示用例模板请求(对无变量用例即实际请求)。
function caseToSentRequest(c: ApiCase): SentRequest {
  let url = c.url || ''
  for (const p of c.restParams ?? []) if (p.key) url = url.split(`{${p.key}}`).join(p.value)
  const qs = (c.queryParams ?? []).filter((p) => p.key).map((p) => `${p.key}=${p.value}`).join('&')
  if (qs) url += (url.includes('?') ? '&' : '?') + qs
  const headers = [...(c.headers ?? [])]
  if (c.auth?.token && c.auth.type && c.auth.type !== 'none') {
    headers.push({ key: 'Authorization', value: `${c.auth.type === 'basic' ? 'Basic' : 'Bearer'} ${c.auth.token}` })
  }
  return { method: c.method, url, headers, body: c.body ?? undefined }
}

function ReportRow({ idx, r, label, t, open, onToggle, caseOf }: { idx: number; r: ReportResultItem; label: (id: string) => string; t: TFn; open: boolean; onToggle: () => void; caseOf?: (id: string) => ApiCase | undefined }) {
  const ok = r.outcome === 'SUCCESS'
  const hasDetail = r.statusCode != null || r.body != null || (r.headers?.length ?? 0) > 0
  // 实际请求优先级:① 报告持久化的实际请求(0060 后,变量/baseUrl/认证已解析,100% 还原)
  // ② 旧报告回落:REQUEST 步骤从 caseId="GET http://…" 解析;CASE 引用据用例模板重建。
  const m = /^([A-Z]+)\s+(\S+)/.exec(r.caseId)
  const kase = m ? undefined : caseOf?.(r.caseId)
  const req: SentRequest | null = r.request
    ? { method: r.request.method, url: r.request.url, headers: (r.request.headers ?? []).map(([key, value]) => ({ key, value })), body: r.request.body ?? undefined }
    : m ? { method: m[1], url: m[2], headers: [] } : kase ? caseToSentRequest(kase) : null
  // 0048 起报告持久化逐条断言(含通过项);旧报告无 → 据 failures 合成失败行兜底。
  const failAsserts: AssertionResult[] = r.failures.map((f) => ({ item: f, condition: '', expected: '', actual: '', passed: false, reason: f }))
  const asserts = r.assertions?.length ? r.assertions : failAsserts
  // 用存储的响应明细合成一个 DebugResponse,复用调试的 7 标签面板。
  const resp: DebugResponse | null = hasDetail
    ? { status: r.statusCode ?? 0, latencyMs: r.latencyMs ?? 0, headers: r.headers ?? [], body: r.body ?? '', assertions: asserts.length ? asserts : undefined, extractions: r.extractions?.length ? r.extractions : undefined }
    : null
  const muted: React.CSSProperties = { color: 'var(--text-3)', fontSize: 12 }
  return (
    <div style={{ border: '1px solid var(--border-soft)', borderRadius: 8, marginBottom: 8, background: 'var(--panel)' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '8px 12px', cursor: hasDetail ? 'pointer' : 'default' }} onClick={() => hasDetail && onToggle()}>
        {hasDetail && <span style={{ color: 'var(--text-3)', fontSize: 11 }}>{open ? '▾' : '▸'}</span>}
        <span style={{ color: 'var(--text-3)', fontSize: 12, minWidth: 18 }}>{idx}</span>
        <span style={{ flex: 1, minWidth: 0 }} className="ms-mono">{label(r.caseId)}</span>
        <Tag color={ok ? 'green' : 'red'} style={{ margin: 0 }}>{ok ? t('scenario.pass', '通过') : t('scenario.fail', '失败')}</Tag>
        {r.statusCode != null && <span style={muted}>{t('apidef.statusCode', '状态码')} <span style={{ color: r.statusCode < 400 ? '#52c41a' : '#ff4d4f' }}>{r.statusCode}</span></span>}
        <span style={muted}>{t('scenario.respTime', '响应时间')} {r.latencyMs != null ? `${r.latencyMs} ms` : '—'}</span>
        <span style={muted}>{t('scenario.respSize', '响应大小')} {r.respSize != null ? `${r.respSize} bytes` : '—'}</span>
      </div>
      {r.failures.length > 0 && (
        <div style={{ padding: '0 12px 8px 40px' }}>
          {r.failures.map((f, j) => <div key={j} style={{ color: '#ff4d4f', fontSize: 12 }} className="ms-mono">✗ {f}</div>)}
        </div>
      )}
      {open && resp && (
        <div style={{ padding: '0 12px 12px' }}>
          <DebugResultPanel running={false} resp={resp} err="" req={req} isHttp={!!req} />
        </div>
      )}
    </div>
  )
}

// 控制器子步骤(叶子)构建:CASE 引用 或 内联 REQUEST。
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

// 按类型分发的添加步骤弹窗:CASE/REQUEST/SCENARIO 叶子 + LOOP/IF/ONCE/TIMER 控制器(含子步骤)。
type StepBody = { kind: string; order: number; refId?: string; request?: unknown; control?: unknown }
// 自定义请求抽屉:请求行 + 请求头/请求体/Query/REST/前置/后置/断言/认证
// + 服务端执行 + 响应内容 + 取消/保存并继续添加/确认。后端内联请求已支持完整规格。
function CustomRequestDrawer({
  open,
  scenarioId,
  nextOrder,
  env,
  onClose,
  onAdded,
  onLocalAdd,
}: {
  open: boolean
  scenarioId: string
  nextOrder: number
  env?: Environment
  onClose: () => void
  onAdded: () => void | Promise<void>
  /** 新建场景(无 id)本地模式:不落库,回传步骤体。 */
  onLocalAdd?: (body: StepBody) => void
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
    setMethod('GET'); setUrl(''); setHeaders([blankKv()]); setQuery([blankKv()]); setRest([blankKv()])
    setBody(''); setAuthType('none'); setAuthToken(''); setAssertions([{ type: 'StatusIs', args: 200 }])
    setPre([]); setPost([]); setResp(null); setErr(''); setLastReq(null)
  }
  useEffect(() => { if (open) reset() }, [open]) // eslint-disable-line react-hooks/exhaustive-deps

  const clean = (rows: KVRow[]) => rows.filter((r) => r.key.trim())
  const authObj = () => (authType === 'none' ? undefined : { type: authType, token: authToken })
  const buildRequest = () => ({
    method,
    url: url.trim(),
    body: body || null,
    headers: clean(headers),
    queryParams: clean(query),
    restParams: clean(rest),
    auth: authObj(),
    assertions,
    processors: [...pre, ...post],
  })

  // 服务端执行:组装最终 URL(REST 替换 {key} + Query 拼接)后直发,展示响应。
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
      setResp(await api.debugSend({ ...req, assertions: assertions as unknown[], processors: [...pre, ...post] as unknown[] }))
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
      const stepBody: StepBody = { kind: 'REQUEST', order: nextOrder, request: buildRequest() }
      if (onLocalAdd) onLocalAdd(stepBody)
      else await api.addStep(scenarioId, stepBody)
      message.success(t('scenario.stepAdded', '步骤已添加'))
      await onAdded()
      if (keepOpen) reset()
      else onClose()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('scenario.addFailed', '添加失败'))
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
    <Drawer
      open={open}
      onClose={onClose}
      width={full ? '92%' : 720}
      closeIcon={false}
      title={<span style={{ fontWeight: 600 }}>{t('scenario.customRequest', '自定义请求')}</span>}
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
          <Button loading={saving} onClick={() => save(true)}>{t('scenario.saveAndContinue', '保存并继续添加')}</Button>
          <Button type="primary" loading={saving} onClick={() => save(false)}>{t('a.confirm', '确认')}</Button>
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
      <DebugResultPanel running={running} resp={resp} err={err} req={lastReq} isHttp assertions={assertions as Record<string, unknown>[]} extractors={[...pre, ...post] as Record<string, unknown>[]} />
    </Drawer>
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
  /** 新建场景(无 id)时:不落库,回传步骤体到本地。 */
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
      // CASE / 控制器 需要项目用例下拉;SCENARIO 需要场景下拉。
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
    <Modal title={`${t('scenario.addPrefix', '添加')} · ${title}`} open={!!type} onCancel={onClose} onOk={() => form.submit()} confirmLoading={saving} destroyOnHidden width={620}>
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
    </Modal>
  )
}

// 导入系统请求(对齐参考图 #29):统一以抽屉浏览 接口/用例/场景,多选后「引用」批量加为步骤。
// 接口→REQUEST(方法/路径);用例→CASE 引用;场景→SCENARIO 引用。简化:仅当前项目 + 名称搜索。
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
  /** 新建场景(无 id)时:不落库,回传步骤体数组到本地。 */
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

  useEffect(() => {
    if (!open) return
    setSearch(''); setModuleSearch(''); setSelModule('ALL'); setSelApi([]); setSelCase([]); setSelScn([])
    api.definitions(projectId).then((d) => setDefs(Array.isArray(d) ? d : [])).catch(() => setDefs([]))
    api.projectCases(projectId).then((p) => setCases(p.items)).catch(() => setCases([]))
    api.scenarios(projectId).then((s) => setScns(s.filter((x) => x.id !== scenarioId))).catch(() => setScns([]))
    api.modules(projectId).then((m) => setModules(Array.isArray(m) ? m : [])).catch(() => setModules([]))
  }, [open, projectId, scenarioId])

  const lc = (s: string) => s.toLowerCase()
  // case → 其接口定义所属模块;scenario → meta.moduleId。
  const defModuleMap = Object.fromEntries(defs.map((d) => [d.id, d.moduleId || '']))
  const moduleOf = (x: ApiDefinition | ApiCase | Scenario): string =>
    tab === 'api' ? (x as ApiDefinition).moduleId || '' : tab === 'case' ? defModuleMap[(x as ApiCase).apiDefinitionId] || '' : ((x as Scenario).meta?.moduleId as string) || ''
  const inModule = (x: ApiDefinition | ApiCase | Scenario) => (selModule === 'ALL' ? true : selModule === 'UNFILED' ? !moduleOf(x) : moduleOf(x) === selModule)
  const fDefs = defs.filter((d) => d.protocol === protocol && inModule(d) && (!search || lc(d.name).includes(lc(search)) || lc(d.path).includes(lc(search))))
  const fCases = cases.filter((c) => inModule(c) && (!search || lc(c.name).includes(lc(search)) || lc(c.url || '').includes(lc(search))))
  const fScns = scns.filter((s) => inModule(s) && (!search || lc(s.name).includes(lc(search))))
  const total = selApi.length + selCase.length + selScn.length

  // 左侧模块树计数(按当前标签数据;接口受协议过滤)。
  const activeData: (ApiDefinition | ApiCase | Scenario)[] = tab === 'api' ? defs.filter((d) => d.protocol === protocol) : tab === 'case' ? cases : scns
  const countFor = (mid: string) => activeData.filter((x) => (mid === 'ALL' ? true : mid === 'UNFILED' ? !moduleOf(x) : moduleOf(x) === mid)).length
  const shownModules = modules.filter((m) => !m.parentId).filter((m) => !moduleSearch || lc(m.name).includes(lc(moduleSearch)))
  const moduleRow = (key: string, name: string, count: number) => (
    <div
      key={key}
      onClick={() => setSelModule(key)}
      style={{ display: 'flex', alignItems: 'center', padding: '6px 8px', borderRadius: 6, cursor: 'pointer', fontSize: 13, background: selModule === key ? '#f3eaff' : 'transparent', color: selModule === key ? 'var(--brand)' : undefined }}
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

  const apiCols: ColumnsType<ApiDefinition> = [
    { title: 'ID', dataIndex: 'num', width: 90, render: (v?: number) => <span className="ms-mono" style={{ fontSize: 12 }}>{v ?? '—'}</span> },
    { title: t('scenario.apiName', '接口名称'), dataIndex: 'name', ellipsis: true },
    { title: t('apidef.colMethod', '请求类型'), dataIndex: 'method', width: 100, render: (m: string, r) => <Tag color={methodColor(m)}>{r.protocol === 'HTTP' ? m || 'GET' : r.protocol}</Tag> },
    { title: t('apidef.colPath', '路径'), dataIndex: 'path', ellipsis: true, render: (v: string) => <span className="ms-mono" style={{ fontSize: 12 }}>{v || '—'}</span> },
    { title: t('apidef.colStatus', '状态'), dataIndex: 'status', width: 100, render: (s: string) => <Tag color={statusColor(s)}>{s}</Tag> },
  ]
  const caseCols: ColumnsType<ApiCase> = [
    { title: t('scenario.colName', '名称'), dataIndex: 'name', ellipsis: true },
    { title: t('apidef.colMethod', '请求类型'), dataIndex: 'method', width: 100, render: (m: string) => <Tag color={methodColor(m)}>{m}</Tag> },
    { title: t('apidef.colPath', '路径'), dataIndex: 'url', ellipsis: true, render: (v: string) => <span className="ms-mono" style={{ fontSize: 12 }}>{v || '—'}</span> },
    { title: t('apidef.colStatus', '状态'), dataIndex: 'status', width: 100, render: (s?: string) => <Tag>{s || '—'}</Tag> },
  ]
  const scnCols: ColumnsType<Scenario> = [
    { title: t('scenario.colName', '名称'), dataIndex: 'name', ellipsis: true },
    { title: t('scenario.colSteps', '步骤数'), dataIndex: 'steps', width: 90, render: (s?: unknown[]) => <Tag color={s?.length ? 'geekblue' : 'default'}>{s?.length ?? 0}</Tag> },
    { title: t('scenario.colStatus', '状态'), dataIndex: 'status', width: 110, render: (s: string) => <Tag color={statusColor(s)}>{s}</Tag> },
  ]

  return (
    <Drawer
      open={open}
      onClose={onClose}
      width="82%"
      title={t('scenario.importSystem', '导入系统请求')}
      footer={
        <div style={{ display: 'flex', alignItems: 'center', gap: 16 }}>
          <Typography.Text type="secondary" style={{ fontSize: 12 }}>
            {t('scenario.totalSelected', '共选择')} {total} · {t('scenario.apiName', '接口')} {selApi.length} · {t('scenario.caseUnit', '用例')} {selCase.length} · {t('scenario.scenarioUnit', '场景')} {selScn.length}
          </Typography.Text>
          <div style={{ flex: 1 }} />
          <Button onClick={onClose}>{t('a.cancel', '取消')}</Button>
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
        {/* 左侧筛选栏(对齐参考图 #34:项目 + 协议 + 模块搜索 + 模块树计数)。 */}
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
        {/* 右侧结果表 */}
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8 }}>
            <Typography.Text strong>{totalLabel} ({tab === 'api' ? fDefs.length : tab === 'case' ? fCases.length : fScns.length})</Typography.Text>
            <div style={{ flex: 1 }} />
            <Input allowClear size="small" style={{ width: 240 }} prefix={<SearchOutlined style={{ color: 'var(--text-3)' }} />} placeholder={t('scenario.searchByPathName', '通过路径或名称搜索')} value={search} onChange={(e) => setSearch(e.target.value)} />
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
    </Drawer>
  )
}

// 新建场景:全屏 tab(对齐参考图 #38)。左=步骤编辑器占位 + 右=基本信息表单;保存创建场景后转入详情。
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
      <div style={{ fontSize: 13, color: 'var(--text-2)', marginBottom: 6 }}>{req && <span style={{ color: '#ff4d4f', marginRight: 4 }}>*</span>}{label}</div>
      {node}
    </div>
  )
  const soon = (label: string) => <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={`${label} · ${t('scenario.saveFirst', '保存场景后可编辑')}`} style={{ margin: '32px 0' }} />
  // 本地步骤体 → 展示用伪步骤(无 id,仅用于 StepRow 渲染)。
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
      {/* 工具栏投射到 Tab 栏右侧;未保存场景:环境/执行置灰,仅保存可用。 */}
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
        {/* 右侧基本信息表单(对齐 #38)。 */}
        <div style={{ width: 320, flexShrink: 0, borderLeft: '1px solid var(--border-soft)', paddingLeft: 16 }}>
          {field(t('scenario.name', '场景名称'), <Input value={name} onChange={(e) => setName(e.target.value)} placeholder={t('scenario.namePlaceholder2', '请输入场景名称')} />, true)}
          {field(t('scenario.ownerModule', '所属模块'), <Select style={{ width: '100%' }} value={moduleId || ''} onChange={(v) => setModuleId(v || '')} placeholder={t('scenario.unplanned', '未规划场景')} options={[{ value: '', label: t('scenario.unplanned', '未规划场景') }, ...modules.map((m) => ({ value: m.id, label: m.name }))]} />)}
          {field(t('scenario.priority', '场景等级'), <Select style={{ width: '100%' }} value={priority} onChange={setPriority} options={SCENARIO_PRIORITIES.map((p) => ({ value: p, label: <span style={{ color: priorityColor(p) }}>● <span style={{ color: 'var(--text)' }}>{p}</span></span> }))} />)}
          {field(t('scenario.sceneStatus', '场景状态'), <Select style={{ width: '100%' }} value={status} onChange={setStatus} options={SCENARIO_STATUSES.map((s) => ({ value: s, label: s }))} />)}
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

// 导入场景抽屉(对齐参考图 #39):格式标签 + 所属模块 + 导入模式 + 文件拖拽。解析需后端,先占位。
/** HAR 一个请求条目(只取本平台用得到的字段)。 */
type HarEntry = { request?: { method?: string; url?: string; postData?: { text?: string } } }

/** 解析 HAR(JSON)的 log.entries → REQUEST 步骤体。无 method/url 的条目跳过。 */
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
    <Drawer
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
    </Drawer>
  )
}
