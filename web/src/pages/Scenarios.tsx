import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { Button, Drawer, Dropdown, Empty, Form, Input, Modal, Radio, Segmented, Select, Space, Switch, Table, Tabs, Tag, Typography, Upload } from 'antd'
import { message, modal } from '../feedback'
import { PlayCircleOutlined, PlusOutlined, SaveOutlined, ThunderboltOutlined, DownOutlined, LinkOutlined, SwapOutlined, DeleteOutlined, FullscreenOutlined, CloseOutlined, SearchOutlined, FilterOutlined, ReloadOutlined, MoreOutlined, ImportOutlined, InboxOutlined } from '@ant-design/icons'
import { api, ApiError, type ApiCase, type ApiDefinition, type ApiModule, type DebugResponse, type Environment, type ReportResultItem, type Scenario, type ScenarioChange, type ScenarioExecution, type ScenarioReportDetail, type ScenarioRunResult, type ScenarioStep } from '../api'
import type { ColumnsType } from 'antd/es/table'
import { useApp } from '../context'
import { methodColor, statusColor, outcomeColor } from '../components/tags'
import { Workspace, useWorkTabs } from '../components/Workspace'
import AssertionEditor from '../components/AssertionEditor'
import ProcessorEditor from '../components/ProcessorEditor'
import { DebugResultPanel, type SentRequest } from '../components/ApiSpecPanel'
import { useI18n } from '../i18n'

type TFn = (key: string, fallback?: string) => string
// 可编辑表单 + 场景参数行(存入 scenario.meta)。
type ScenarioParam = { name: string; type: string; value: string; tags: string; desc: string }
type ScenarioForm = { name: string; status: string; description: string; tags: string[]; priority: string; params: ScenarioParam[]; csv: string; moduleId: string; disabledSteps: string[]; preProcessors: unknown[]; postProcessors: unknown[]; assertions: unknown[]; envCookie: boolean; sharedCookie: boolean }
const SCENARIO_STATUSES = ['DRAFT', 'DEBUGGING', 'COMPLETED', 'DEPRECATED']
const SCENARIO_PRIORITIES = ['P0', 'P1', 'P2', 'P3']

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
  const tabs = useWorkTabs()
  const NEW_KEY = '__new_scenario__'

  const load = async () => {
    if (!projectId) { setList([]); setModules([]); return }
    setLoading(true)
    try {
      const [ss, mm] = await Promise.all([api.scenarios(projectId), api.modules(projectId).catch(() => [])])
      setList(Array.isArray(ss) ? ss : [])
      setModules(Array.isArray(mm) ? mm : [])
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('scenario.loadFailed', '加载场景失败'))
    } finally {
      setLoading(false)
    }
  }
  useEffect(() => {
    load()
    tabs.reset()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId])

  const moduleOf = (s: Scenario) => (s.meta?.moduleId as string) || ''
  const countFor = (mid: string) => list.filter((s) => (mid === 'ALL' ? true : mid === 'UNFILED' ? !moduleOf(s) : moduleOf(s) === mid)).length
  const shownModules = useMemo(() => modules.filter((m) => !m.parentId).filter((m) => !moduleSearch || m.name.toLowerCase().includes(moduleSearch.toLowerCase())), [modules, moduleSearch])

  const filtered = useMemo(() => {
    const q = search.toLowerCase()
    return list.filter((s) => {
      const inMod = selModule === 'ALL' ? true : selModule === 'UNFILED' ? !moduleOf(s) : moduleOf(s) === selModule
      const tags = (s.meta?.tags as string[] | undefined) || []
      const hit = !q || s.name.toLowerCase().includes(q) || s.id.toLowerCase().includes(q) || tags.some((tg) => tg.toLowerCase().includes(q))
      return inMod && hit
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [list, search, selModule])

  if (!projectId) return <div style={{ padding: 48 }}><Empty description={t('common.selectProject', '请先在顶部选择项目')} /></div>

  const moduleRow = (key: string, name: string, count: number) => (
    <div
      key={key}
      onClick={() => setSelModule(key)}
      style={{ display: 'flex', alignItems: 'center', padding: '7px 10px', borderRadius: 6, cursor: 'pointer', fontSize: 13, background: selModule === key ? '#f3eaff' : 'transparent', color: selModule === key ? '#7c3aed' : undefined }}
    >
      <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{name}</span>
      <span style={{ color: '#a8adb5', fontSize: 12 }}>{count}</span>
    </div>
  )

  // 左侧:新建/导入 + 模块搜索 + 模块树(计数)+ 回收站(对齐参考图 #33)。
  const left = (
    <>
      <div style={{ padding: '10px 10px 6px' }}>
        <Space.Compact style={{ width: '100%', marginBottom: 8 }}>
          <Button type="primary" icon={<PlusOutlined />} style={{ flex: 1 }} onClick={() => tabs.open(NEW_KEY)}>{t('scenario.newScenario', '新建场景')}</Button>
          <Button icon={<ImportOutlined />} style={{ flex: 1 }} onClick={() => setImportOpen(true)}>{t('scenario.importScenario', '导入场景')}</Button>
        </Space.Compact>
        <Input size="small" allowClear prefix={<SearchOutlined style={{ color: '#bbb' }} />} placeholder={t('scenario.moduleSearchPh', '请输入模块名称进行搜索')} value={moduleSearch} onChange={(e) => setModuleSearch(e.target.value)} />
      </div>
      <div style={{ flex: 1, overflow: 'auto', padding: 8 }}>
        {moduleRow('ALL', `${t('scenario.allScenarios', '全部场景')} (${countFor('ALL')})`, countFor('ALL'))}
        {moduleRow('UNFILED', t('scenario.unplanned', '未规划场景'), countFor('UNFILED'))}
        {shownModules.map((m) => moduleRow(m.id, m.name, countFor(m.id)))}
      </div>
      <div style={{ padding: '8px 14px', borderTop: '1px solid #f5f5f5', color: '#8a9099', fontSize: 12 }}>🗑 {t('scenario.recycleBin', '回收站')}</div>
    </>
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
  const muted = (v?: string) => <span style={{ color: '#bbb' }}>{v || '—'}</span>
  const richCols: ColumnsType<Scenario> = [
    { title: 'ID', dataIndex: 'id', width: 110, render: (v: string) => <span className="ms-mono" style={{ fontSize: 12 }}>{v.slice(0, 8)}</span> },
    { title: t('scenario.colSceneName', '场景名称'), dataIndex: 'name', ellipsis: true, render: (v: string) => <span style={{ fontWeight: 500 }}>{v}</span> },
    { title: t('scenario.priority', '场景等级'), width: 110, render: (_v, s) => { const p = (s.meta?.priority as string) || 'P0'; return <span style={{ color: '#ff4d4f' }}>● {p}</span> } },
    { title: t('scenario.colStatus', '状态'), dataIndex: 'status', width: 110, render: (s: string) => <Tag color={statusColor(s)}>{s}</Tag> },
    { title: t('scenario.colExecResult', '执行结果'), width: 110, render: () => muted() },
    { title: t('scenario.tags', '标签'), width: 160, render: (_v, s) => { const tags = (s.meta?.tags as string[] | undefined) || []; return tags.length ? <Space size={[4, 4]} wrap>{tags.map((tg) => <Tag key={tg} style={{ margin: 0 }}>{tg}</Tag>)}</Space> : muted() } },
    { title: t('scenario.colSceneEnv', '场景环境'), width: 130, render: () => muted() },
    { title: t('scenario.createdBy', '创建人'), dataIndex: 'createdBy', width: 110, render: (v?: string) => muted(v || undefined) },
    { title: t('scenario.updatedBy', '更新人'), width: 110, render: (_v, s) => muted(s.createdBy || undefined) },
    {
      title: t('apidef.colAction', '操作'),
      width: 160,
      fixed: 'right',
      render: (_v, s) => (
        <Space size={4} onClick={(e) => e.stopPropagation()}>
          <Button type="link" size="small" onClick={() => tabs.open(s.id)}>{t('a.edit', '编辑')}</Button>
          <Button type="link" size="small" onClick={(e) => runFromList(s, e)}>{t('apidef.run', '执行')}</Button>
          <Button type="link" size="small" onClick={() => message.info(t('scenario.copySoon', '复制场景即将接入'))}>{t('a.copy', '复制')}</Button>
          <Dropdown menu={{ items: [{ key: 'del', label: t('a.delete', '删除'), danger: true }], onClick: () => message.info(t('scenario.deleteScenarioSoon', '删除场景即将接入')) }}><Button type="link" size="small" icon={<MoreOutlined />} /></Dropdown>
        </Space>
      ),
    },
  ]

  const listContent = (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '10px 12px', borderBottom: '1px solid #f0f0f0' }}>
        <div style={{ flex: 1 }} />
        <Input allowClear prefix={<SearchOutlined style={{ color: '#bbb' }} />} placeholder={t('scenario.searchByIdNameTag', '通过 ID/名称/标签搜索')} style={{ width: 260 }} value={search} onChange={(e) => setSearch(e.target.value)} />
        <Select size="middle" value="all" disabled style={{ width: 150 }} options={[{ value: 'all', label: `${t('scenario.view', '视图')}: ${t('scenario.allData', '全部数据')}` }]} />
        <Button icon={<FilterOutlined />} disabled>{t('apidef.filter', '筛选')}</Button>
        <Button icon={<ReloadOutlined />} onClick={load} />
      </div>
      <div style={{ flex: 1, overflow: 'auto', padding: 12 }}>
        <Table<Scenario>
          rowKey="id"
          size="middle"
          loading={loading}
          dataSource={filtered}
          columns={richCols}
          scroll={{ x: 'max-content' }}
          rowSelection={{ type: 'checkbox' }}
          onRow={(s) => ({ onClick: () => tabs.open(s.id), style: { cursor: 'pointer' } })}
          pagination={{ pageSize: 20, size: 'small', showTotal: (total) => `${t('apidef.totalPrefix', '共')} ${total} ${t('scenario.unit', '条')}` }}
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
        children: <NewScenarioTab projectId={projectId} modules={modules} onCreated={(s) => { tabs.close(NEW_KEY); load().then(() => tabs.open(s.id)) }} />,
      }]
    const s = list.find((x) => x.id === id)
    return s ? [{ key: s.id, label: s.name, children: <ScenarioDetail scenario={s} /> }] : []
  })

  return (
    <>
      <Workspace
        left={left}
        leftWidth={252}
        listLabel={t('scenario.allScenarios', '全部场景')}
        activeKey={tabs.activeKey}
        onChange={tabs.setActiveKey}
        onClose={tabs.close}
        tabs={detailTabs}
        listContent={listContent}
      />
      <ImportScenarioDrawer open={importOpen} projectId={projectId} modules={modules} onClose={() => setImportOpen(false)} onImported={load} />
    </>
  )
}

// 步骤类型 → 标签文案 + 颜色(对齐 MeterSphere)。
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
}

// 引用 id → 可读名称(用例/子场景);未命中回落短 id,避免满屏 UUID。
type NameOf = (id: string) => string

// 把控制器载荷里的一个子步骤(原始 json)规整为 Node。
function childToNode(c: any, t: TFn, nameOf: NameOf): Node {
  const kind = String(c?.kind || '').toUpperCase()
  if (kind === 'CASE') return { kind, content: <span className="ms-mono">{t('scenario.caseRef', '用例')} {nameOf(c.refId)}</span> }
  if (kind === 'REQUEST')
    return { kind, content: <Space><Tag color={methodColor(c.method || 'GET')}>{c.method || 'GET'}</Tag><span className="ms-mono">{c.url}</span></Space> }
  return controlToNode(kind, c, t, nameOf)
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

function StepRow({ node, idx, depth, t, result, enabled = true, onToggle, onRun }: { node: Node; idx: number; depth: number; t: TFn; result?: ReportResultItem; enabled?: boolean; onToggle?: () => void; onRun?: () => void }) {
  const meta = makeStepMeta(t)[node.kind] || { label: node.kind, color: 'default' }
  const ok = result?.outcome === 'SUCCESS'
  const muted: React.CSSProperties = { color: '#8a9099', fontSize: 12, whiteSpace: 'nowrap' }
  const leaf = depth === 0
  return (
    <>
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          padding: '8px 12px',
          marginLeft: depth * 24,
          border: '1px solid #f0f0f0',
          borderRadius: 6,
          marginBottom: 6,
          background: depth ? '#fafafa' : '#fff',
          opacity: enabled ? 1 : 0.5,
        }}
      >
        <span style={{ color: '#c9cdd4', cursor: 'grab' }}>⠿</span>
        {/* 启用/禁用本步骤(禁用则置灰);播放=单步服务端执行。两者均阻止冒泡(不打开抽屉)。 */}
        <Switch size="small" checked={enabled} disabled={!leaf || !onToggle} onChange={() => onToggle?.()} onClick={(_c, e) => e.stopPropagation()} />
        <PlayCircleOutlined
          style={{ color: leaf && onRun ? '#7c3aed' : '#c9cdd4', cursor: leaf && onRun ? 'pointer' : 'default' }}
          onClick={(e) => { e.stopPropagation(); if (leaf) onRun?.() }}
        />
        <span style={{ color: '#9aa0a6', fontSize: 12, minWidth: 18 }}>{idx}</span>
        <Tag color={meta.color} style={{ margin: 0 }}>{meta.label}</Tag>
        <span style={{ flex: 1, minWidth: 0 }}>{node.content}</span>
        {/* 执行后逐步结果(对齐参考图 #28):通过/状态码/响应时间/响应大小。 */}
        {result && (
          <>
            <Tag color={ok ? 'green' : 'red'} style={{ margin: 0 }}>{ok ? t('scenario.pass', '通过') : t('scenario.fail', '失败')}</Tag>
            {result.statusCode != null && <span style={muted}>{t('apidef.statusCode', '状态码')} <span style={{ color: result.statusCode < 400 ? '#52c41a' : '#ff4d4f' }}>{result.statusCode}</span></span>}
            <span style={muted}>{t('scenario.respTime', '响应时间')} {result.latencyMs != null ? `${result.latencyMs} ms` : '—'}</span>
            <span style={muted}>{t('scenario.respSize', '响应大小')} {result.respSize != null ? `${result.respSize} bytes` : '—'}</span>
          </>
        )}
      </div>
      {node.children?.map((c, i) => <StepRow key={i} node={c} idx={i + 1} depth={depth + 1} t={t} />)}
    </>
  )
}

// 场景详情:全标签编辑器外壳(对齐参考图 #20-#24:头部 + 基本信息/步骤/参数/前后置/断言/
// 执行历史/变更历史/设置 + 顶部右侧 环境/服务端执行/保存)。步骤详情抽屉(#25)、可编辑元信息
// (需后端 updateScenario)、报告(#26)为后续切片。
function ScenarioDetail({ scenario }: { scenario: Scenario }) {
  const { t } = useI18n()
  const [steps, setSteps] = useState<ScenarioStep[]>([])
  const [running, setRunning] = useState(false)
  const [add, setAdd] = useState<string>('') // 当前打开的添加表单类型
  const [importOpen, setImportOpen] = useState(false) // 导入系统请求抽屉
  const [lastRun, setLastRun] = useState<ScenarioRunResult | null>(null)
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
        meta: { description: form.description, tags: form.tags, priority: form.priority, params: form.params, csvParams: form.csv, moduleId: form.moduleId, disabledSteps: form.disabledSteps, preProcessors: form.preProcessors, postProcessors: form.postProcessors, assertions: form.assertions, envCookie: form.envCookie, sharedCookie: form.sharedCookie },
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
      api.projectCases(scenario.projectId).then((p) => p.items).catch(() => []),
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
      setEnvId((cur) => cur || environments.find((e) => e.enabled !== false)?.id || '')
      setModules(mods)
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scenario.id])

  const run = async () => {
    setRunning(true)
    try {
      const r = await api.runScenario(scenario.id, scenario.projectId, { environmentId: envId || undefined, failureStrategy })
      setLastRun(r)
      // 拉报告明细,把逐步结果(通过/状态码/耗时/大小)映射到步骤行。
      const rep = await api.scenarioReport(r.reportId).catch(() => null)
      if (rep) {
        const map: Record<string, ReportResultItem> = {}
        rep.results.forEach((res) => { map[res.caseId] = res })
        setStepResults(map)
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
  const onAdded = () => {
    setAdd('')
    loadSteps()
  }

  const stepsTab = (
    <div>
      <Space style={{ marginBottom: 12 }}>
        <Typography.Text strong style={{ fontSize: 13 }}>{t('scenario.totalPrefix', '共')} {steps.length} {t('scenario.totalSuffix', '个步骤')}</Typography.Text>
        {lastRun && <Tag color={outcomeColor(lastRun.status)} style={{ margin: 0 }}>{lastRun.status} · {lastRun.caseCount} {t('scenario.caseUnit', '用例')}</Tag>}
      </Space>
      {ordered.length === 0 ? (
        <Empty description={t('scenario.emptySteps', '暂无步骤,点「添加步骤」')} />
      ) : (
        ordered.map((s, i) => {
          const k = stepKey(s)
          return (
            <div
              key={s.id}
              draggable
              onDragStart={() => setDragIdx(i)}
              onDragOver={(e) => e.preventDefault()}
              onDrop={(e) => { e.preventDefault(); if (dragIdx != null) moveStep(dragIdx, i); setDragIdx(null) }}
              onClick={() => setSelStep({ step: s, idx: i + 1 })}
              style={{ cursor: 'pointer', opacity: dragIdx === i ? 0.5 : 1 }}
            >
              <StepRow
                node={stepToNode(s, t, nameOf)}
                idx={i + 1}
                depth={0}
                t={t}
                result={k ? stepResults[k] : undefined}
                enabled={!form.disabledSteps.includes(s.id)}
                onToggle={() => toggleStep(s.id)}
                onRun={() => runStep(s)}
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
            onClick: ({ key }) => (key === 'IMPORT' ? setImportOpen(true) : setAdd(key)),
          }}
        >
          <Button type="dashed" icon={<PlusOutlined />} block>{t('scenario.addStep', '添加步骤')}</Button>
        </Dropdown>
      </div>
      <AddStepModal type={add} scenarioId={scenario.id} projectId={scenario.projectId} nextOrder={nextOrder} onClose={() => setAdd('')} onAdded={onAdded} />
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
    { key: 'exec', label: t('scenario.execHistoryTab', '执行历史'), children: <ScenarioExecutionsTab scenarioId={scenario.id} nameOf={nameOf} t={t} /> },
    { key: 'change', label: t('apidef.changeHistory', '变更历史'), children: <ScenarioChangesTab scenarioId={scenario.id} t={t} /> },
    { key: 'settings', label: t('apidef.settings', '设置'), children: <ScenarioSettings failureStrategy={failureStrategy} onFailureStrategy={setFailureStrategy} envCookie={form.envCookie} sharedCookie={form.sharedCookie} onCookie={(p) => patchForm(p)} t={t} /> },
  ]

  return (
    <div style={{ padding: '12px 16px', height: '100%', overflow: 'auto' }}>
      {/* 顶部右侧:环境 + 服务端执行 + 保存(对齐参考图 #20)。 */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
        <div style={{ flex: 1 }} />
        <Select
          size="small"
          value={envId || undefined}
          onChange={setEnvId}
          style={{ width: 200 }}
          placeholder={t('editor.selectEnv', '选择环境')}
          allowClear
          options={envs.map((e) => ({ value: e.id, label: e.baseUrl ? `${e.name} · ${e.baseUrl}` : e.name }))}
          notFoundContent={t('editor.noEnvConfigured', '未配置环境,去「环境」页新建')}
        />
        <Dropdown.Button
          type="primary"
          icon={<DownOutlined />}
          loading={running}
          onClick={run}
          menu={{ items: [{ key: 'local', label: t('apidef.localRun', '本地执行') }], onClick: () => message.info(t('scenario.localSoon', '本地执行即将接入')) }}
        >
          <ThunderboltOutlined /> {t('apidef.serverRun', '服务端执行')}
        </Dropdown.Button>
        <Button type="default" icon={<SaveOutlined />} loading={saving} onClick={onSave}>{t('a.save', '保存')}</Button>
      </div>
      {/* 头部:状态 / 等级 / [id] / 名称 / 标签 / 描述。 */}
      <div style={{ marginBottom: 14 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8, flexWrap: 'wrap' }}>
          <Tag color={statusColor(form.status)} style={{ margin: 0 }}>{form.status}</Tag>
          <span style={{ color: '#ff4d4f', fontSize: 12, fontWeight: 600 }}>{form.priority}</span>
          <span className="ms-mono" style={{ color: '#8a9099', fontSize: 12 }}>[{scenario.id.slice(0, 8)}]</span>
          <span style={{ fontWeight: 600, fontSize: 15, color: '#1f2329' }}>{form.name}</span>
          {form.tags.map((tg) => <Tag key={tg} style={{ margin: 0 }}>{tg}</Tag>)}
          <LinkOutlined style={{ color: '#bbb' }} />
          <div style={{ flex: 1 }} />
          {/* 全部步骤执行完成后显示「查看执行报告」(对齐参考图 #28 红色区域)。 */}
          {lastRun && <Button type="link" onClick={() => setReportModalId(lastRun.reportId)}>{t('scenario.viewReport', '查看执行报告')}</Button>}
        </div>
      </div>
      <Tabs className="ms-detail-tabs" defaultActiveKey="steps" items={tabs} />
      <StepDetailDrawer
        sel={selStep}
        scenarioId={scenario.id}
        caseMap={caseMap}
        nameOf={nameOf}
        env={envs.find((e) => e.id === envId)}
        onClose={() => setSelStep(null)}
        onDeleted={() => { setSelStep(null); loadSteps() }}
      />
      <ScenarioReportModal reportId={reportModalId} nameOf={nameOf} onClose={() => setReportModalId(null)} />
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
  onClose,
  onDeleted,
}: {
  sel: { step: ScenarioStep; idx: number } | null
  scenarioId: string
  caseMap: Record<string, ApiCase>
  nameOf: NameOf
  env?: Environment
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
  useEffect(() => { setResp(null); setErr(''); setLastReq(null) }, [sel?.step.id])

  const step = sel?.step
  const meta = step ? makeStepMeta(t)[step.kind.toUpperCase()] || { label: step.kind, color: 'default' } : null
  const kase = step?.caseId ? caseMap[step.caseId] : undefined

  // 当前步骤可发送的请求(CASE→其用例;REQUEST→内联);控制器无。
  const reqInfo = (() => {
    if (kase) return { method: kase.method, url: kase.url, body: kase.body, headers: kase.headers || [], auth: kase.auth, assertions: kase.assertions, processors: kase.processors }
    if (step?.request) return { method: step.request.method, url: step.request.url, body: step.request.body ?? null, headers: [], auth: undefined, assertions: step.request.assertions, processors: undefined }
    return null
  })()

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
      <span style={{ color: '#9aa0a6', fontSize: 12 }}>{sel?.idx}</span>
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
          children: reqInfo.body ? <pre className="ms-mono" style={{ background: '#f6f8fa', padding: 12, borderRadius: 6, margin: 0, fontSize: 12, maxHeight: 240, overflow: 'auto' }}>{reqInfo.body}</pre> : <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('apidef.noBody', '请求没有 Body')} />,
        },
        {
          key: 'assert',
          label: t('apidef.assertions', '断言'),
          children: Array.isArray(reqInfo.assertions) && (reqInfo.assertions as unknown[]).length ? <pre className="ms-mono" style={{ background: '#f6f8fa', padding: 12, borderRadius: 6, margin: 0, fontSize: 12 }}>{JSON.stringify(reqInfo.assertions, null, 2)}</pre> : <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('apidef.none', '无')} />,
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
      <div style={{ fontSize: 13, color: '#5b6470', marginBottom: 6 }}>{req && <span style={{ color: '#ff4d4f', marginRight: 4 }}>*</span>}{label}</div>
      {value}
    </div>
  )
  return (
    <div style={{ maxWidth: 560 }}>
      {field(t('scenario.name', '场景名称'), <Input value={form.name} onChange={(e) => patch({ name: e.target.value })} placeholder={t('scenario.namePlaceholder', '如:下单主流程')} />, true)}
      {field(t('scenario.ownerModule', '所属模块'), <Select style={{ width: 280 }} value={form.moduleId || undefined} onChange={(v) => patch({ moduleId: v || '' })} allowClear placeholder={t('apidef.unfiled', '未归类')} options={modules.map((m) => ({ value: m.id, label: m.name }))} notFoundContent={t('scenario.noModules', '项目暂无模块(在接口定义维护)')} />)}
      {field(t('scenario.priority', '场景等级'), <Select style={{ width: 200 }} value={form.priority} onChange={(v) => patch({ priority: v })} options={SCENARIO_PRIORITIES.map((p) => ({ value: p, label: p }))} />)}
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
function ScenarioExecutionsTab({ scenarioId, nameOf, t }: { scenarioId: string; nameOf: NameOf; t: TFn }) {
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
      <ScenarioReportModal reportId={reportId} nameOf={nameOf} onClose={() => setReportId(null)} />
    </>
  )
}

// 变更历史标签(审计日志):操作 / 详情 / 操作人 / 时间。
const CHANGE_ACTIONS: Record<string, { label: string; color: string }> = {
  CREATE: { label: '创建', color: 'green' },
  UPDATE: { label: '更新', color: 'blue' },
  ADD_STEP: { label: '新增步骤', color: 'geekblue' },
  DELETE_STEP: { label: '删除步骤', color: 'red' },
  REORDER: { label: '调整顺序', color: 'purple' },
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
        { title: t('scenario.changeAction', '操作'), dataIndex: 'action', width: 120, render: (a: string) => { const m = CHANGE_ACTIONS[a] || { label: a, color: 'default' }; return <Tag color={m.color}>{m.label}</Tag> } },
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
function ScenarioReportModal({ reportId, nameOf, onClose }: { reportId: string | null; nameOf: NameOf; onClose: () => void }) {
  const { t } = useI18n()
  const [data, setData] = useState<ScenarioReportDetail | null>(null)
  const [loading, setLoading] = useState(false)
  const [search, setSearch] = useState('')
  useEffect(() => {
    if (!reportId) { setData(null); return }
    setLoading(true)
    api.scenarioReport(reportId).then(setData).catch(() => setData(null)).finally(() => setLoading(false))
  }, [reportId])
  const passN = data?.results.filter((r) => r.outcome === 'SUCCESS').length ?? 0
  const rows = (data?.results || []).filter((r) => !search || r.caseId.toLowerCase().includes(search.toLowerCase()))
  // caseId 多为可读请求行(GET http://...)或用例 UUID;UUID 用 nameOf 解析。
  const label = (id: string) => (/^[0-9a-f]{8}-/.test(id) ? nameOf(id) : id)
  return (
    <Modal open={!!reportId} onCancel={onClose} footer={null} width="80%" title={t('scenario.report', '场景报告')} destroyOnHidden>
      {loading ? (
        <div style={{ padding: 32, color: '#999' }}>{t('a.loading', '加载中…')}</div>
      ) : !data ? (
        <Empty description={t('scenario.noReport', '暂无报告')} />
      ) : (
        <>
          {/* 报告头 */}
          <div style={{ display: 'flex', alignItems: 'center', gap: 16, marginBottom: 12, flexWrap: 'wrap' }}>
            <Space><span style={{ color: '#8a9099', fontSize: 12 }}>{t('scenario.execStatus', '执行状态')}</span><Tag color={outcomeColor(data.status)}>{data.status}</Tag></Space>
            <Space><span style={{ color: '#8a9099', fontSize: 12 }}>{t('scenario.caseUnit', '用例')}</span><span>{passN}/{data.caseCount} {t('scenario.passed', '通过')}</span></Space>
            <span className="ms-mono" style={{ color: '#bbb', fontSize: 12 }}>{data.reportId.slice(0, 12)}</span>
          </div>
          {/* 报告明细 */}
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 10 }}>
            <Typography.Text strong>{t('scenario.reportDetail', '报告明细')}</Typography.Text>
            <Segmented size="small" value="flat" options={[{ label: t('scenario.flatView', '平铺展示'), value: 'flat' }, { label: t('scenario.tabView', 'Tab 展示'), value: 'tab' }]} disabled />
            <div style={{ flex: 1 }} />
            <Input size="small" allowClear style={{ width: 220 }} placeholder={t('scenario.searchByName', '通过名称搜索')} value={search} onChange={(e) => setSearch(e.target.value)} />
          </div>
          <div style={{ marginTop: 10 }}>
            {rows.length === 0 ? <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('scenario.noStepResult', '无步骤结果')} /> : rows.map((r, i) => <ReportRow key={i} idx={i + 1} r={r} label={label} t={t} />)}
          </div>
        </>
      )}
    </Modal>
  )
}

function ReportRow({ idx, r, label, t }: { idx: number; r: ReportResultItem; label: (id: string) => string; t: TFn }) {
  const [open, setOpen] = useState(false)
  const ok = r.outcome === 'SUCCESS'
  const hasDetail = r.statusCode != null || r.body != null || (r.headers?.length ?? 0) > 0
  // 用存储的响应明细合成一个 DebugResponse,复用调试的 7 标签面板。
  const resp: DebugResponse | null = hasDetail
    ? { status: r.statusCode ?? 0, latencyMs: r.latencyMs ?? 0, headers: r.headers ?? [], body: r.body ?? '' }
    : null
  const muted: React.CSSProperties = { color: '#bbb', fontSize: 12 }
  return (
    <div style={{ border: '1px solid #f0f0f0', borderRadius: 6, marginBottom: 6 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '8px 12px', cursor: hasDetail ? 'pointer' : 'default' }} onClick={() => hasDetail && setOpen((v) => !v)}>
        {hasDetail && <span style={{ color: '#bbb', fontSize: 11 }}>{open ? '▾' : '▸'}</span>}
        <span style={{ color: '#9aa0a6', fontSize: 12, minWidth: 18 }}>{idx}</span>
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
          <DebugResultPanel running={false} resp={resp} err="" req={null} isHttp />
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
      style={{ display: 'flex', alignItems: 'center', padding: '6px 8px', borderRadius: 6, cursor: 'pointer', fontSize: 13, background: selModule === key ? '#f3eaff' : 'transparent', color: selModule === key ? '#7c3aed' : undefined }}
    >
      <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{name}</span>
      <span style={{ color: '#a8adb5', fontSize: 12 }}>{count}</span>
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
          <Input size="small" allowClear prefix={<SearchOutlined style={{ color: '#bbb' }} />} placeholder={t('apidef.moduleSearch', '输入模块名称搜索')} value={moduleSearch} onChange={(e) => setModuleSearch(e.target.value)} style={{ marginBottom: 8 }} />
          <div style={{ border: '1px solid #f0f0f0', borderRadius: 6, padding: 4, maxHeight: 460, overflow: 'auto' }}>
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
            <Input allowClear size="small" style={{ width: 240 }} prefix={<SearchOutlined style={{ color: '#bbb' }} />} placeholder={t('scenario.searchByPathName', '通过路径或名称搜索')} value={search} onChange={(e) => setSearch(e.target.value)} />
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
function NewScenarioTab({ projectId, modules, onCreated }: { projectId: string; modules: ApiModule[]; onCreated: (s: Scenario) => void }) {
  const { t } = useI18n()
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
      <div style={{ fontSize: 13, color: '#5b6470', marginBottom: 6 }}>{req && <span style={{ color: '#ff4d4f', marginRight: 4 }}>*</span>}{label}</div>
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
            onClick: ({ key }) => (key === 'IMPORT' ? setImportOpen(true) : setAdd(key)),
          }}
        >
          <Button type="dashed" icon={<PlusOutlined />} block>{t('scenario.addStep', '添加步骤')}</Button>
        </Dropdown>
      </div>
      <AddStepModal type={add} scenarioId="" projectId={projectId} nextOrder={nextOrder} onClose={() => setAdd('')} onAdded={() => setAdd('')} onLocalAdd={(b) => setLocalSteps((prev) => [...prev, b])} />
      <ImportRequestDrawer open={importOpen} scenarioId="" projectId={projectId} nextOrder={nextOrder} onClose={() => setImportOpen(false)} onImported={() => undefined} onLocalImport={(bs) => setLocalSteps((prev) => [...prev, ...bs])} />
    </div>
  )
  return (
    <div style={{ padding: '12px 16px', height: '100%', overflow: 'auto' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
        <div style={{ flex: 1 }} />
        <Select size="small" disabled placeholder={t('editor.selectEnv', '选择环境')} style={{ width: 180 }} options={[]} />
        <Button type="primary" icon={<ThunderboltOutlined />} disabled title={t('scenario.saveFirst', '保存场景后可执行')}>{t('apidef.serverRun', '服务端执行')}</Button>
        <Button type="primary" icon={<SaveOutlined />} loading={saving} onClick={save}>{t('a.save', '保存')}</Button>
      </div>
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
        <div style={{ width: 320, flexShrink: 0, borderLeft: '1px solid #f0f0f0', paddingLeft: 16 }}>
          {field(t('scenario.name', '场景名称'), <Input value={name} onChange={(e) => setName(e.target.value)} placeholder={t('scenario.namePlaceholder2', '请输入场景名称')} />, true)}
          {field(t('scenario.ownerModule', '所属模块'), <Select style={{ width: '100%' }} value={moduleId || undefined} onChange={(v) => setModuleId(v || '')} allowClear placeholder={t('scenario.unplanned', '未规划场景')} options={modules.map((m) => ({ value: m.id, label: m.name }))} />)}
          {field(t('scenario.priority', '场景等级'), <Select style={{ width: '100%' }} value={priority} onChange={setPriority} options={SCENARIO_PRIORITIES.map((p) => ({ value: p, label: p }))} />)}
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
  const label = (s: string) => <div style={{ fontSize: 13, color: '#5b6470', margin: '14px 0 6px' }}>{s}</div>

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
      <Select style={{ width: '100%' }} value={moduleId || undefined} onChange={(v) => setModuleId(v || '')} allowClear placeholder={t('scenario.unplanned', '未规划场景')} options={modules.map((m) => ({ value: m.id, label: m.name }))} />
      {label(t('scenario.importMode', '导入模式'))}
      <Radio.Group value={mode} onChange={(e) => setMode(e.target.value)}>
        <Radio value="cover">{t('scenario.modeCover', '覆盖')}</Radio>
        <Radio value="skip">{t('scenario.modeSkip', '不覆盖')}</Radio>
      </Radio.Group>
      <div style={{ marginTop: 16 }}>
        <Upload.Dragger maxCount={1} accept=".ms,.json,.jmx,.har" beforeUpload={(f) => { setFile(f); return false }} onRemove={() => setFile(null)}>
          <p className="ant-upload-drag-icon"><InboxOutlined style={{ color: '#7c3aed' }} /></p>
          <p className="ant-upload-text">{t('scenario.dropFile', '拖拽或点击此区域选择文件')}</p>
          <p className="ant-upload-hint" style={{ fontSize: 12 }}>{t('scenario.fileHint', 'HAR 直接解析为请求步骤;MeterSphere/Jmeter 解析后续接入')}</p>
        </Upload.Dragger>
      </div>
    </Drawer>
  )
}
