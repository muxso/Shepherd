import { useEffect, useMemo, useRef, useState, type Key, type ReactNode } from 'react'
import { Button, Checkbox, Empty, Input, Popover, Segmented, Select, Space, Table, Tag, Tree } from 'antd'
import type { ColumnsType } from 'antd/es/table'
import type { DataNode } from 'antd/es/tree'
import { FilterOutlined, ReloadOutlined, SearchOutlined, SettingOutlined } from '@ant-design/icons'
import { message, modal } from '../../feedback'
import { api, ApiError, runEventsWsUrl, type ApiCase, type ApiModule, type PlanCase, type PlanningNode, type RunEvent, type Scenario } from '../../api'
import { executedOnLabel } from '../AutoPoolIndicator'
import { LiveRunBadge } from '../LiveRunBadge'
import { outcomeColor, priorityColor } from '../tags'
import { useApp } from '../../context'
import { useI18n } from '../../i18n'

type TFn = (k: string, d: string) => string

/** Plan case status code (SUCCESS/ERROR/BLOCK/FAKE_ERROR/PENDING) → localized label. */
export function planCaseStatusLabel(s: string, t: TFn): string {
  switch ((s || 'PENDING').toUpperCase()) {
    case 'SUCCESS': return t('plan.resPass', '通过')
    case 'ERROR': return t('plan.resFail', '不通过')
    case 'BLOCK': return t('plan.resBlock', '阻塞')
    case 'FAKE_ERROR': return t('plan.resFake', '误报')
    case 'PENDING': return t('plan.segPending', '未执行')
    default: return s
  }
}

const COLS_KEY = 'shepherd.planCases.cols'
const PRIORITIES = ['P0', 'P1', 'P2', 'P3']
const RESULT_STATUSES = ['SUCCESS', 'ERROR', 'BLOCK', 'FAKE_ERROR', 'PENDING']

/** Plan-case table row enriched with scenario/case metadata for display. */
interface Row extends PlanCase {
  num: string
  numVal: number
  isScenario: boolean
  priority: string
  testPoint: string
  moduleId: string
  moduleName: string
  createdAt: string
  updatedAt: string
  createdBy: string
}

const fmtTs = (v?: string) => (v ? v.slice(0, 19).replace('T', ' ') : '')

/** Collects every planning node's own linked ids plus its subtree total. */
function collectNodeIds(node: PlanningNode, into: Map<string, Set<string>>): Set<string> {
  const own = new Set<string>([...(node.caseIds || []), ...(node.scenarioIds || [])])
  for (const c of node.children || []) for (const id of collectNodeIds(c, into)) own.add(id)
  into.set(node.id, own)
  return own
}

/** First planning node that directly links the id (leaf 测试点 wins over categories). */
function pointNameOf(nodes: PlanningNode[], id: string): string {
  for (const n of nodes) {
    const deep = pointNameOf(n.children || [], id)
    if (deep) return deep
    if ((n.caseIds || []).includes(id) || (n.scenarioIds || []).includes(id)) return n.name
  }
  return ''
}

// 场景用例 tab: left 测试点/模块 tree + case table with per-row run/unlink,
// column settings and batch actions (layout mirrors the reference UI).
export default function PlanCasesPanel({ planId, projectId, cases, loading, reload, toolbar }: {
  planId: string
  projectId: string
  cases: PlanCase[]
  loading: boolean
  reload: () => void
  toolbar: ReactNode
}) {
  const { t } = useI18n()
  const { projects } = useApp()
  const [scenarios, setScenarios] = useState<Scenario[]>([])
  const [apiCases, setApiCases] = useState<ApiCase[]>([])
  const [modules, setModules] = useState<ApiModule[]>([])
  const [planningNodes, setPlanningNodes] = useState<PlanningNode[]>([])
  const [treeMode, setTreeMode] = useState<'point' | 'module'>('point')
  const [treeSearch, setTreeSearch] = useState('')
  const [selectedKey, setSelectedKey] = useState('ALL')
  const [search, setSearch] = useState('')
  const [hiddenCols, setHiddenCols] = useState<string[]>(() => {
    try { return JSON.parse(localStorage.getItem(COLS_KEY) || '[]') } catch { return [] }
  })
  const [pageSize, setPageSize] = useState(50)
  const [selectedIds, setSelectedIds] = useState<Key[]>([])
  const [runningId, setRunningId] = useState('')
  // Live asyncRun scenario rows keyed by caseId; reportId doubles as the
  // terminal marker (the plan case row is recorded with it at completion).
  const [live, setLive] = useState<Record<string, { reportId: string; since: number }>>({})
  const liveWs = useRef<Record<string, WebSocket>>({})
  useEffect(() => () => {
    Object.values(liveWs.current).forEach((w) => { try { w.close() } catch { /* already closed */ } })
  }, [])
  const [batchBusy, setBatchBusy] = useState(false)
  // 筛选 popover: extra priority/result filters on top of column filters.
  const [fltPriority, setFltPriority] = useState<string[]>([])
  const [fltResult, setFltResult] = useState<string[]>([])

  useEffect(() => {
    api.scenarios(projectId).then((ss) => setScenarios(Array.isArray(ss) ? ss : [])).catch(() => setScenarios([]))
    api.projectCasesAll(projectId).then(setApiCases).catch(() => setApiCases([]))
    api.modules(projectId).then((mm) => setModules(Array.isArray(mm) ? mm : [])).catch(() => setModules([]))
  }, [projectId])
  useEffect(() => {
    api.planDetail(planId).then((d) => setPlanningNodes(d.planning?.nodes || [])).catch(() => setPlanningNodes([]))
    // Planning links change together with the case list, so re-fetch alongside it.
  }, [planId, cases])

  const projectName = projects.find((p) => p.id === projectId)?.name || '-'
  const saveCols = (next: string[]) => {
    setHiddenCols(next)
    localStorage.setItem(COLS_KEY, JSON.stringify(next))
  }

  const rows: Row[] = useMemo(() => {
    const scnMap = new Map(scenarios.map((s) => [s.id, s]))
    const caseMap = new Map(apiCases.map((c) => [c.id, c]))
    const modName = (id: string) => modules.find((m) => m.id === id)?.name || ''
    return cases.map((pc) => {
      const s = scnMap.get(pc.caseId)
      const c = caseMap.get(pc.caseId)
      const moduleId = (s?.meta?.moduleId as string) || ''
      return {
        ...pc,
        num: s?.num ? String(s.num) : pc.caseId.slice(0, 8),
        numVal: s?.num || 0,
        isScenario: !!s,
        priority: (s?.meta?.priority as string) || c?.priority || 'P0',
        testPoint: pointNameOf(planningNodes, pc.caseId) || '-',
        moduleId,
        moduleName: modName(moduleId) || (s ? t('plan.pcUnplannedModule', '未规划模块') : '-'),
        createdAt: fmtTs(s?.createdAt),
        updatedAt: fmtTs(s?.updatedAt),
        createdBy: s?.createdBy || '-',
      }
    })
  }, [cases, scenarios, apiCases, modules, planningNodes, t])

  // Left tree (测试点 mode): planning hierarchy with per-node linked counts.
  const pointIdSets = useMemo(() => {
    const m = new Map<string, Set<string>>()
    for (const n of planningNodes) collectNodeIds(n, m)
    return m
  }, [planningNodes])
  const linkedIds = useMemo(() => new Set(cases.map((c) => c.caseId)), [cases])
  const nodeCount = (id: string) => [...(pointIdSets.get(id) || [])].filter((x) => linkedIds.has(x)).length
  const countLabel = (name: string, count: number) => (
    <span style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
      <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{name}</span>
      <span style={{ color: 'var(--text-3)', fontSize: 12 }}>{count}</span>
    </span>
  )
  const treeData: DataNode[] = useMemo(() => {
    const q = treeSearch.trim().toLowerCase()
    if (treeMode === 'point') {
      const build = (nodes: PlanningNode[]): DataNode[] =>
        nodes
          .map((n): DataNode | null => {
            const children = build(n.children || [])
            const hit = !q || n.name.toLowerCase().includes(q)
            if (!hit && !children.length) return null
            return { key: `pt:${n.id}`, title: countLabel(n.name, nodeCount(n.id)), children }
          })
          .filter((x): x is DataNode => !!x)
      return [{ key: 'ALL', title: countLabel(`${t('plan.tabCases', '场景用例')} (${rows.length})`, rows.length), children: build(planningNodes) }]
    }
    // 模块 mode: modules referenced by linked scenarios (flat), plus 未规划模块.
    const counts = new Map<string, number>()
    for (const r of rows) counts.set(r.isScenario ? r.moduleId : '', (counts.get(r.isScenario ? r.moduleId : '') || 0) + 1)
    const children: DataNode[] = [...counts.entries()]
      .map(([id, n]) => {
        const name = id ? modules.find((m) => m.id === id)?.name || id : t('plan.pcUnplannedModule', '未规划模块')
        if (q && !name.toLowerCase().includes(q)) return null
        return { key: `mod:${id}`, title: countLabel(name, n) } as DataNode
      })
      .filter((x): x is DataNode => !!x)
      .sort((a, b) => String(a.key).localeCompare(String(b.key)))
    return [{ key: 'ALL', title: countLabel(`${t('plan.tabCases', '场景用例')} (${rows.length})`, rows.length), children }]
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [treeMode, treeSearch, rows, planningNodes, modules, linkedIds, t])

  const filtered = useMemo(() => {
    let list = rows
    if (selectedKey !== 'ALL') {
      if (selectedKey.startsWith('pt:')) {
        const ids = pointIdSets.get(selectedKey.slice(3)) || new Set()
        list = list.filter((r) => ids.has(r.caseId))
      } else if (selectedKey.startsWith('mod:')) {
        const mid = selectedKey.slice(4)
        list = list.filter((r) => (r.isScenario ? r.moduleId : '') === mid)
      }
    }
    const q = search.trim().toLowerCase()
    if (q) list = list.filter((r) => r.name.toLowerCase().includes(q) || r.num.toLowerCase().includes(q))
    if (fltPriority.length) list = list.filter((r) => fltPriority.includes(r.priority))
    if (fltResult.length) list = list.filter((r) => fltResult.includes((r.status || 'PENDING').toUpperCase()))
    return list
  }, [rows, selectedKey, search, pointIdSets, fltPriority, fltResult])

  const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))
  // The recorded row carries our report id right after the run completes.
  const fetchRecorded = async (caseId: string, reportId: string): Promise<PlanCase | null> => {
    const cs = await api.planCases(planId).catch(() => null)
    const list = Array.isArray(cs) ? cs : cs?.items || []
    const row = list.find((c) => c.caseId === caseId)
    return row?.reportId === reportId ? row : null
  }
  const settleLive = (caseId: string, status?: string) => {
    delete liveWs.current[caseId]
    setLive((m) => {
      const n = { ...m }
      delete n[caseId]
      return n
    })
    if (status) message.success(`${t('plan.runDone', '执行完成')}:${planCaseStatusLabel(status, t)}`)
    reload()
  }
  // Follows one asyncRun row over the run-events WS; degrades to polling the
  // case list every 3s when the socket can't be established (or dies mid-run).
  const followRow = (caseId: string, reportId: string) => {
    let settled = false
    const settle = (status?: string) => {
      if (settled) return
      settled = true
      settleLive(caseId, status)
    }
    const poll = async () => {
      for (let i = 0; i < 600 && !settled; i++) {
        const row = await fetchRecorded(caseId, reportId)
        if (row) return settle(row.status)
        await sleep(3000)
      }
      settle()
    }
    let ws: WebSocket
    try { ws = new WebSocket(runEventsWsUrl(reportId)) } catch { void poll(); return }
    liveWs.current[caseId] = ws
    let opened = false
    ws.onopen = () => { opened = true }
    ws.onmessage = async (m) => {
      let ev: RunEvent
      try { ev = JSON.parse(m.data as string) as RunEvent } catch { return }
      if (ev.type !== 'runComplete' || settled) return
      ws.close()
      // The plan-case record lands right after the run: confirm it briefly so
      // the refreshed row already shows the final result.
      for (let i = 0; i < 10 && !settled; i++) {
        const row = await fetchRecorded(caseId, reportId)
        if (row) return settle(row.status)
        await sleep(300)
      }
      settle(ev.status)
    }
    ws.onerror = () => { if (!opened && !settled) { try { ws.close() } catch { /* already closed */ } void poll() } }
    ws.onclose = () => { if (opened && !settled) void poll() }
  }
  const runOne = async (r: Row) => {
    setRunningId(r.caseId)
    try {
      // Scenario rows run async with a live row badge; plain API cases finish
      // in one round trip either way.
      const res = await api.runPlanCase(planId, r.caseId, r.isScenario ? { asyncRun: true } : undefined)
      if (res.status === 'RUNNING' && res.reportId) {
        setLive((m) => ({ ...m, [r.caseId]: { reportId: res.reportId as string, since: Date.now() } }))
        followRow(r.caseId, res.reportId)
      } else {
        const where = executedOnLabel(res.executedOn)
        message.success(`${t('plan.runDone', '执行完成')}:${planCaseStatusLabel(res.status, t)}${where ? ` · ${where}` : ''}`)
        reload()
      }
    } catch (e) {
      message.error(e instanceof ApiError ? `${t('plan.runFail', '执行失败')}:${e.status}` : t('plan.runFail', '执行失败'))
    } finally {
      setRunningId('')
    }
  }
  const unlinkOne = (row: Row) => {
    modal.confirm({
      title: `${t('plan.pcUnlinkConfirm', '确认取消关联该用例?')}`,
      content: row.name,
      okButtonProps: { danger: true },
      onOk: async () => {
        try {
          await api.unlinkPlanCase(planId, row.caseId)
          message.success(t('plan.pcUnlinked', '已取消关联'))
          reload()
        } catch (e) {
          message.error(e instanceof ApiError ? `${t('plan.pcUnlinkFail', '取消关联失败')}:${e.status}` : t('plan.pcUnlinkFail', '取消关联失败'))
        }
      },
    })
  }
  const batchRun = async () => {
    setBatchBusy(true)
    try {
      for (const id of selectedIds) await api.runPlanCase(planId, String(id)).catch(() => undefined)
      message.success(t('plan.runDone', '执行完成'))
      setSelectedIds([])
      reload()
    } finally {
      setBatchBusy(false)
    }
  }
  const batchUnlink = () => {
    modal.confirm({
      title: `${t('plan.pcUnlinkConfirm', '确认取消关联该用例?')} (${selectedIds.length})`,
      okButtonProps: { danger: true },
      onOk: async () => {
        setBatchBusy(true)
        try {
          for (const id of selectedIds) await api.unlinkPlanCase(planId, String(id)).catch(() => undefined)
          message.success(t('plan.pcUnlinked', '已取消关联'))
          setSelectedIds([])
          reload()
        } finally {
          setBatchBusy(false)
        }
      },
    })
  }

  const muted = (v?: string) => <span style={{ color: 'var(--text-3)' }}>{v || '-'}</span>
  const colSettings = (
    <Popover
      trigger="click"
      placement="bottomRight"
      title={t('apidef.colSettings', '表头设置')}
      content={
        <Space direction="vertical" size={4} style={{ width: 160 }}>
          {TOGGLE_COLS.map((c) => (
            <Checkbox
              key={c.key}
              checked={!hiddenCols.includes(c.key)}
              onChange={(e) => saveCols(e.target.checked ? hiddenCols.filter((x) => x !== c.key) : [...hiddenCols, c.key])}
            >
              {t(c.i18nKey, c.fallback)}
            </Checkbox>
          ))}
        </Space>
      }
    >
      <SettingOutlined style={{ cursor: 'pointer', color: 'var(--text-3)' }} />
    </Popover>
  )
  const richCols: ColumnsType<Row> = [
    {
      key: 'id', title: 'ID', dataIndex: 'num', width: 110,
      sorter: (a, b) => (a.numVal - b.numVal) || a.num.localeCompare(b.num),
      render: (v: string) => <span className="ms-mono" style={{ color: 'var(--brand)', fontSize: 12 }}>{v}</span>,
    },
    {
      key: 'name', title: t('plan.pcColName', '场景名称'), dataIndex: 'name', ellipsis: true,
      sorter: (a, b) => a.name.localeCompare(b.name),
      render: (v: string) => <span style={{ fontWeight: 500 }}>{v}</span>,
    },
    { key: 'testPoint', title: t('plan.pcColTestPoint', '测试点'), dataIndex: 'testPoint', width: 140, ellipsis: true },
    {
      key: 'priority', title: t('plan.pcColPriority', '场景等级'), width: 110,
      render: (_v, r) => <span style={{ color: priorityColor(r.priority) }}>● {r.priority}</span>,
      filters: PRIORITIES.map((p) => ({ text: p, value: p })),
      onFilter: (v, r) => r.priority === v,
    },
    {
      key: 'result', title: t('plan.pcColResult', '执行结果'), width: 130,
      render: (_v, r) => {
        const lv = live[r.caseId]
        if (lv) return <LiveRunBadge label={t('plan.pcRunning', '执行中')} since={lv.since} />
        const s = (r.status || 'PENDING').toUpperCase()
        return s === 'PENDING' ? muted() : <Tag color={outcomeColor(s)} style={{ margin: 0 }}>{planCaseStatusLabel(s, t)}</Tag>
      },
      filters: RESULT_STATUSES.map((s) => ({ text: planCaseStatusLabel(s, t), value: s })),
      onFilter: (v, r) => (r.status || 'PENDING').toUpperCase() === v,
    },
    { key: 'createdAt', title: t('plan.colCreatedAt', '创建时间'), dataIndex: 'createdAt', width: 170, sorter: (a, b) => a.createdAt.localeCompare(b.createdAt), render: (v: string) => <span className="ms-mono" style={{ fontSize: 12 }}>{v || '-'}</span> },
    { key: 'updatedAt', title: t('plan.pcColUpdatedAt', '更新时间'), dataIndex: 'updatedAt', width: 170, sorter: (a, b) => a.updatedAt.localeCompare(b.updatedAt), render: (v: string) => <span className="ms-mono" style={{ fontSize: 12 }}>{v || '-'}</span> },
    { key: 'module', title: t('plan.pcColModule', '所属模块'), dataIndex: 'moduleName', width: 130, ellipsis: true },
    { key: 'bugs', title: t('plan.pcColBugs', '缺陷数'), width: 90, render: () => <span style={{ color: 'var(--brand)' }}>0</span> },
    { key: 'project', title: t('plan.pcColProject', '所属项目'), width: 130, ellipsis: true, render: () => projectName },
    { key: 'createdBy', title: t('plan.colCreatedBy', '创建人'), dataIndex: 'createdBy', width: 110, render: (v: string) => muted(v === '-' ? undefined : v) },
    {
      key: 'executor', title: t('plan.pcColExecutor', '执行人'), width: 100, render: () => muted(),
      filters: [{ text: '-', value: '-' }], onFilter: () => true,
    },
    {
      key: 'action',
      title: (
        <span style={{ display: 'inline-flex', alignItems: 'center', gap: 8 }}>
          {t('plan.colAction', '操作')} {colSettings}
        </span>
      ),
      width: 150,
      fixed: 'right',
      render: (_v, r) => (
        <Space size={0} split={<span style={{ color: 'var(--border-soft)', margin: '0 2px' }}>|</span>}>
          <Button type="link" size="small" style={{ padding: '0 2px' }} loading={runningId === r.caseId} disabled={!!live[r.caseId]} onClick={() => runOne(r)}>{t('plan.exec', '执行')}</Button>
          <Button type="link" size="small" style={{ padding: '0 2px' }} onClick={() => unlinkOne(r)}>{t('plan.pcUnlink', '取消关联')}</Button>
        </Space>
      ),
    },
  ]
  const columns = richCols.filter((c) => !hiddenCols.includes(String(c.key)))

  return (
    <div style={{ height: '100%', minHeight: 0, display: 'flex' }}>
      {/* Left: 测试点 | 模块 tree */}
      <div style={{ width: 230, flexShrink: 0, borderRight: '1px solid var(--border-soft)', display: 'flex', flexDirection: 'column', padding: 12, gap: 8, overflow: 'auto' }}>
        <Segmented
          block
          value={treeMode}
          onChange={(v) => { setTreeMode(v as 'point' | 'module'); setSelectedKey('ALL') }}
          options={[
            { value: 'point', label: t('plan.pcTestPoint', '测试点') },
            { value: 'module', label: t('plan.pcModule', '模块') },
          ]}
        />
        <Input
          allowClear
          placeholder={t('plan.pcSearchName', '请输入名称')}
          suffix={<SearchOutlined style={{ color: 'var(--text-3)' }} />}
          value={treeSearch}
          onChange={(e) => setTreeSearch(e.target.value)}
        />
        <Tree
          key={`${treeMode}:${planningNodes.length}:${modules.length}`}
          blockNode
          defaultExpandAll
          selectedKeys={[selectedKey]}
          onSelect={(keys) => setSelectedKey(keys.length ? String(keys[0]) : 'ALL')}
          treeData={treeData}
        />
      </div>
      {/* Right: toolbar + table */}
      <div style={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '12px 16px', flexWrap: 'wrap' }}>
          {toolbar}
          <div style={{ flex: 1 }} />
          <Input
            allowClear
            size="small"
            style={{ width: 200 }}
            placeholder={t('plan.pcSearchIdName', '通过 ID/名称搜索')}
            suffix={<SearchOutlined style={{ color: 'var(--text-3)' }} />}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          <span style={{ color: 'var(--text-2)', fontSize: 13 }}>{t('plan.pcView', '视图')}</span>
          <Select
            size="small"
            style={{ width: 110 }}
            value="ALL"
            options={[{ value: 'ALL', label: t('plan.pcViewAll', '全部数据') }]}
          />
          <Popover
            trigger="click"
            placement="bottomRight"
            title={t('apidef.filter', '筛选')}
            content={
              <Space direction="vertical" size={8} style={{ width: 220 }}>
                <div>
                  <div style={{ fontSize: 12, color: 'var(--text-3)', marginBottom: 4 }}>{t('plan.pcColPriority', '场景等级')}</div>
                  <Select mode="multiple" allowClear size="small" style={{ width: '100%' }} value={fltPriority} onChange={setFltPriority} options={PRIORITIES.map((p) => ({ value: p, label: p }))} />
                </div>
                <div>
                  <div style={{ fontSize: 12, color: 'var(--text-3)', marginBottom: 4 }}>{t('plan.pcColResult', '执行结果')}</div>
                  <Select mode="multiple" allowClear size="small" style={{ width: '100%' }} value={fltResult} onChange={setFltResult} options={RESULT_STATUSES.map((s) => ({ value: s, label: planCaseStatusLabel(s, t) }))} />
                </div>
              </Space>
            }
          >
            <Button size="small" icon={<FilterOutlined />}>
              {t('apidef.filter', '筛选')}{fltPriority.length + fltResult.length ? ` (${fltPriority.length + fltResult.length})` : ''}
            </Button>
          </Popover>
          <Button size="small" icon={<ReloadOutlined />} onClick={reload} />
        </div>
        <div style={{ flex: 1, minHeight: 0, overflow: 'auto', padding: '0 16px 12px' }}>
          <Table<Row>
            rowKey="caseId"
            size="small"
            loading={loading}
            dataSource={filtered}
            columns={columns}
            scroll={{ x: 'max-content' }}
            rowSelection={{ type: 'checkbox', selectedRowKeys: selectedIds, onChange: setSelectedIds }}
            pagination={{
              pageSize,
              size: 'small',
              showSizeChanger: true,
              pageSizeOptions: ['10', '20', '50', '100'],
              onShowSizeChange: (_, s) => setPageSize(s),
              showTotal: (total) => `${t('apidef.totalPrefix', '共')} ${total} ${t('scenario.unit', '条')}`,
            }}
            locale={{ emptyText: <Empty description={t('plan.noLinkedCase', '未挂用例,点「挂用例」')} /> }}
          />
        </div>
        {selectedIds.length > 0 && (
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '10px 16px', borderTop: '1px solid var(--border-soft)', background: 'var(--panel)' }}>
            <span>{t('scenario.selectedPrefix', '已选择')} {selectedIds.length} {t('scenario.unit', '条')}</span>
            <Button size="small" loading={batchBusy} onClick={batchRun}>{t('plan.pcBatchRun', '批量执行')}</Button>
            <Button size="small" loading={batchBusy} onClick={batchUnlink}>{t('plan.pcBatchUnlink', '批量取消关联')}</Button>
            <Button size="small" type="text" onClick={() => setSelectedIds([])}>{t('scenario.clearSel', '清空')}</Button>
          </div>
        )}
      </div>
    </div>
  )
}

// ID/name/action stay fixed; the rest toggle from the header gear.
const TOGGLE_COLS = [
  { key: 'testPoint', i18nKey: 'plan.pcColTestPoint', fallback: '测试点' },
  { key: 'priority', i18nKey: 'plan.pcColPriority', fallback: '场景等级' },
  { key: 'result', i18nKey: 'plan.pcColResult', fallback: '执行结果' },
  { key: 'createdAt', i18nKey: 'plan.colCreatedAt', fallback: '创建时间' },
  { key: 'updatedAt', i18nKey: 'plan.pcColUpdatedAt', fallback: '更新时间' },
  { key: 'module', i18nKey: 'plan.pcColModule', fallback: '所属模块' },
  { key: 'bugs', i18nKey: 'plan.pcColBugs', fallback: '缺陷数' },
  { key: 'project', i18nKey: 'plan.pcColProject', fallback: '所属项目' },
  { key: 'createdBy', i18nKey: 'plan.colCreatedBy', fallback: '创建人' },
  { key: 'executor', i18nKey: 'plan.pcColExecutor', fallback: '执行人' },
]
