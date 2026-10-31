import { useEffect, useMemo, useState } from 'react'
import { Button, Card, Col, Empty, Progress, Row, Select, Spin, Statistic, Table, Tag, Tooltip, Segmented } from 'antd'
import {
  ApiOutlined,
  PartitionOutlined,
  ProfileOutlined,
  ScheduleOutlined,
  FileTextOutlined,
  BugOutlined,
  SettingOutlined,
  ThunderboltOutlined,
  SafetyCertificateOutlined,
  FileDoneOutlined,
  AuditOutlined,
  RobotOutlined,
  ExperimentOutlined,
  ArrowRightOutlined,
  SyncOutlined,
  HistoryOutlined,
} from '@ant-design/icons'
import { useScopedNavigate } from '../scope'
import { api, type ApiCase, type ApiDefinition, type Bug, type CaseExecSummary, type ExecTrendPoint, type PlanStats } from '../api'
import { useApp } from '../context'
import { regList, type RegItem } from '../registry'
import { fetchPlanItems, isGroup } from '../components/plan/planLocal'
import { useI18n } from '../i18n'
import Donut from '../components/Donut'
import type { CollabStats } from '../api'
import ContributionGrid from '../components/ContributionGrid'
import GroupedBars, { type BarRow } from '../components/GroupedBars'
import CardSettings, { CARD_DEFAULT_SIZE, type CardSize } from '../components/CardSettings'

interface Counts {
  def: number
  scenario: number
  apiCase: number
  funcCase: number
  plan: number
  req: number
  bug: number
}

// Muted palette shared by all Home charts: reduced alpha so large areas aren't glaring saturated primaries.
const C = {
  blue: 'rgba(22, 100, 255, 0.72)',
  skyblue: 'rgba(22, 119, 255, 0.68)',
  cyan: 'rgba(19, 194, 194, 0.68)',
  green: 'rgba(82, 196, 26, 0.68)',
  orange: 'rgba(250, 140, 22, 0.75)',
  pink: 'rgba(235, 47, 150, 0.72)',
  red: 'rgba(245, 34, 45, 0.72)',
  grey: '#8a9099',
  human: 'rgba(255, 154, 46, 0.8)',
}

const PROTO_COLORS = [C.blue, C.skyblue, C.cyan, C.green, C.orange, C.pink, C.red, C.grey]

// Asset series for the project-comparison bars; colors match the asset-distribution donut.
const PROJECT_SERIES = [
  { key: 'def', label: '接口定义', color: C.blue },
  { key: 'scenario', label: '场景用例', color: C.skyblue },
  { key: 'apiCase', label: '接口用例', color: C.cyan },
  { key: 'funcCase', label: '功能用例', color: C.green },
]

// Card registry (drag-sortable in the card-settings editor): layout array order = display order; not in array = hidden.
const ALL_CARDS = ['collab', 'projectBars', 'assets', 'apiStats', 'caseStats', 'execTrend', 'quality', 'shortcuts', 'planOverview', 'apiChanges'] as const
// New cards stay out of the layout by default (v7: not in array = hidden); users drag them in from the card-settings library.
const DEFAULT_HIDDEN: readonly string[] = ['planOverview', 'apiChanges']
const TREND_DAYS = 7
type CardKey = (typeof ALL_CARDS)[number]
interface CardLayout {
  key: CardKey
  size: CardSize
}
const CARDS_KEY = 'shepherd.home.cards.v7'
const LEGACY_CARDS_KEY = 'shepherd.home.cards.v6'

/** Load persisted layout. v7 = {key,size}[]; legacy v6 ({key,shown}[]) migrates: shown kept, sizes default. */
function loadLayout(): CardLayout[] {
  const isKey = (k: unknown): k is CardKey => (ALL_CARDS as readonly string[]).includes(k as string)
  const withSize = (k: CardKey, size?: unknown): CardLayout => ({
    key: k,
    size: size === 'half' || size === 'full' ? size : CARD_DEFAULT_SIZE[k] ?? 'half',
  })
  try {
    const raw = localStorage.getItem(CARDS_KEY)
    if (raw) {
      const parsed = JSON.parse(raw) as { key?: unknown; size?: unknown }[]
      if (Array.isArray(parsed)) {
        const seen = new Set<string>()
        return parsed
          .filter((p): p is { key: CardKey; size?: unknown } => isKey(p?.key) && !seen.has(p.key as string) && (seen.add(p.key as string), true))
          .map((p) => withSize(p.key, p.size))
      }
    }
    const legacy = localStorage.getItem(LEGACY_CARDS_KEY)
    if (legacy) {
      const parsed = JSON.parse(legacy) as { key?: unknown; shown?: unknown }[]
      if (Array.isArray(parsed)) {
        const kept = parsed.filter((p) => p?.shown === true && isKey(p.key)).map((p) => p.key as CardKey)
        const missing = ALL_CARDS.filter((k) => !DEFAULT_HIDDEN.includes(k) && !parsed.some((p) => p?.key === k))
        return [...kept, ...missing].map((k) => withSize(k))
      }
    }
  } catch {
    /* ignore */
  }
  return ALL_CARDS.filter((k) => !DEFAULT_HIDDEN.includes(k)).map((k) => withSize(k))
}

// Home workbench: test-asset overview for the current project, with customizable card layout.
export default function Home() {
  const { projectId, projects } = useApp()
  const { t } = useI18n()
  const navigate = useScopedNavigate()
  const [c, setC] = useState<Counts | null>(null)
  const [defs, setDefs] = useState<ApiDefinition[]>([])
  const [cases, setCases] = useState<ApiCase[]>([])
  const [projRows, setProjRows] = useState<BarRow[]>([])
  const [exec, setExec] = useState<CaseExecSummary | null>(null)
  const [trend, setTrend] = useState<ExecTrendPoint[]>([])
  const [collab, setCollab] = useState<CollabStats | null>(null)
  const [collabMetric, setCollabMetric] = useState<'total' | 'ai' | 'human'>('total')
  const [loading, setLoading] = useState(false)
  const [layout, setLayout] = useState<CardLayout[]>(loadLayout)
  const [editing, setEditing] = useState(false)
  // Plan-overview card: selected plan + its stats/linked-case count + project bugs (fetched only while the card is in the layout).
  const [planItems, setPlanItems] = useState<RegItem[]>([])
  const [planId, setPlanId] = useState('')
  const [planStat, setPlanStat] = useState<PlanStats | null>(null)
  const [planCaseCount, setPlanCaseCount] = useState(0)
  const [bugs, setBugs] = useState<Bug[]>([])
  // API-changes card: reference counts (cases/scenarios) for recent definitions.
  const [refCounts, setRefCounts] = useState<Record<string, { cases: number; scenarios: number }>>({})

  const hasPlanCard = layout.some((p) => p.key === 'planOverview')
  const hasApiChangesCard = layout.some((p) => p.key === 'apiChanges')
  const planSelKey = `shepherd.home.planOverview.${projectId}`

  const saveLayout = (next: CardLayout[]) => {
    setLayout(next)
    localStorage.setItem(CARDS_KEY, JSON.stringify(next))
  }

  useEffect(() => {
    if (!projectId) {
      setC(null)
      setExec(null)
      setTrend([])
      setCollab(null)
      setPlanItems([])
      return
    }
    api.caseExecSummary(projectId).then(setExec).catch(() => setExec(null))
    api.collabStats(projectId).then(setCollab).catch(() => setCollab(null))
    api.execTrend(projectId, TREND_DAYS).then((t) => setTrend(Array.isArray(t) ? t : [])).catch(() => setTrend([]))
    setLoading(true)
    Promise.all([
      api.definitions(projectId).catch(() => [] as ApiDefinition[]),
      api.scenarios(projectId).then((d) => d.length).catch(() => 0),
      api.projectCases(projectId).then((p) => ({ total: p.total ?? p.items.length, items: p.items ?? [] })).catch(() => ({ total: 0, items: [] as ApiCase[] })),
      api.functionalCases(projectId).then((d) => d.length).catch(() => 0),
      fetchPlanItems(projectId).catch(() => [] as RegItem[]),
    ])
      .then(([defList, scenario, casePage, funcCase, plans]) => {
        const dl = Array.isArray(defList) ? defList : []
        setDefs(dl)
        setCases(casePage.items)
        setPlanItems(plans)
        setC({
          def: dl.length,
          scenario,
          apiCase: casePage.total,
          funcCase,
          plan: plans.length,
          req: regList('requirement', projectId).length,
          bug: regList('bug', projectId).length,
        })
      })
      .finally(() => setLoading(false))
  }, [projectId])

  // Protocol distribution + coverage: definitions grouped by protocol; a definition referenced by any case counts as covered.
  const protocolSegs = useMemo(() => {
    const m = new Map<string, number>()
    defs.forEach((d) => { const k = (d.protocol || 'HTTP').toUpperCase(); m.set(k, (m.get(k) ?? 0) + 1) })
    return [...m.entries()].sort((a, b) => b[1] - a[1]).map(([label, value], i) => ({ label, value, color: PROTO_COLORS[i % PROTO_COLORS.length] }))
  }, [defs])
  const coveredDefs = useMemo(() => {
    const ref = new Set(cases.map((x) => x.apiDefinitionId))
    return defs.filter((d) => ref.has(d.id)).length
  }, [defs, cases])

  // Exec trend, last N days: backend only returns dates that had executions, so backfill a continuous day axis (passed/failed).
  const trendRows = useMemo<BarRow[]>(() => {
    const map = new Map(trend.map((p) => [p.date, p]))
    const today = new Date()
    const rows: BarRow[] = []
    for (let i = TREND_DAYS - 1; i >= 0; i--) {
      const d = new Date(Date.UTC(today.getUTCFullYear(), today.getUTCMonth(), today.getUTCDate() - i))
      const key = d.toISOString().slice(0, 10)
      const p = map.get(key)
      const passed = p?.passed ?? 0
      const executions = p?.executions ?? 0
      rows.push({ name: key.slice(5), values: { passed, failed: Math.max(0, executions - passed) } })
    }
    return rows
  }, [trend])

  // Project comparison: fetch the four asset counts per project concurrently (independent of the selected project).
  useEffect(() => {
    if (!projects.length) { setProjRows([]); return }
    let alive = true
    Promise.all(
      projects.map(async (p) => {
        const [def, scenario, apiCase, funcCase] = await Promise.all([
          api.definitions(p.id).then((d) => d.length).catch(() => 0),
          api.scenarios(p.id).then((d) => d.length).catch(() => 0),
          api.projectCases(p.id).then((x) => x.total ?? x.items.length).catch(() => 0),
          api.functionalCases(p.id).then((d) => d.length).catch(() => 0),
        ])
        return { name: p.name, values: { def, scenario, apiCase, funcCase } } as BarRow
      }),
    ).then((rows) => { if (alive) setProjRows(rows) })
    return () => { alive = false }
  }, [projects])

  // Plan overview: plan list (groups excluded), persisted selection, stats + linked cases, project bugs.
  const planList = useMemo(() => planItems.filter((p) => !isGroup(p)), [planItems])
  useEffect(() => {
    if (!projectId || !planList.length) { setPlanId(''); return }
    const saved = localStorage.getItem(planSelKey)
    setPlanId(saved && planList.some((p) => p.id === saved) ? saved : planList[0].id)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId, planList])
  useEffect(() => {
    if (!hasPlanCard || !planId) { setPlanStat(null); setPlanCaseCount(0); return }
    let alive = true
    api.planStats(planId).then((s) => { if (alive) setPlanStat(s) }).catch(() => { if (alive) setPlanStat(null) })
    api.planCases(planId)
      .then((r) => { if (alive) setPlanCaseCount(Array.isArray(r) ? r.length : r.total ?? r.items.length) })
      .catch(() => { if (alive) setPlanCaseCount(0) })
    return () => { alive = false }
  }, [planId, hasPlanCard])
  useEffect(() => {
    if (!projectId || !hasPlanCard) { setBugs([]); return }
    let alive = true
    api.bugs(projectId).then((b) => { if (alive) setBugs(Array.isArray(b) ? b : []) }).catch(() => { if (alive) setBugs([]) })
    return () => { alive = false }
  }, [projectId, hasPlanCard])

  // API changes: latest 10 definitions by updatedAt (createdAt fallback), then fetch each one's reference counts.
  const recentDefs = useMemo(
    () => [...defs].sort((a, b) => (b.updatedAt ?? b.createdAt ?? '').localeCompare(a.updatedAt ?? a.createdAt ?? '')).slice(0, 10),
    [defs],
  )
  useEffect(() => {
    if (!hasApiChangesCard || !recentDefs.length) { setRefCounts({}); return }
    let alive = true
    Promise.all(
      recentDefs.map((d) =>
        api.definitionReferences(d.id)
          .then((r) => [d.id, { cases: r.cases?.length ?? 0, scenarios: r.scenarios?.length ?? 0 }] as const)
          .catch(() => null),
      ),
    ).then((es) => {
      if (alive) setRefCounts(Object.fromEntries(es.filter(Boolean) as [string, { cases: number; scenarios: number }][]))
    })
    return () => { alive = false }
  }, [recentDefs, hasApiChangesCard])

  const cardTitle: Record<CardKey, string> = {
    collab: t('home.collab', '人机协同人效'),
    projectBars: t('home.projectCompare', '项目资产对比'),
    assets: t('home.assetDist', '测试资产分布'),
    apiStats: t('home.apiStats', '接口数'),
    caseStats: t('home.caseStats', '接口用例数'),
    execTrend: t('home.execTrend', '执行趋势'),
    quality: t('home.quality', '质量概览'),
    shortcuts: t('home.shortcuts', '快捷入口'),
    planOverview: t('home.planOverview', '测试计划概览'),
    apiChanges: t('home.apiChanges', '接口变更'),
  }

  const cards = useMemo(
    () => [
      { key: 'def', label: t('home.def', '接口定义'), value: c?.def ?? 0, icon: <ApiOutlined />, color: C.blue, to: '/api/definition' },
      { key: 'scenario', label: t('home.scenario', '场景用例'), value: c?.scenario ?? 0, icon: <PartitionOutlined />, color: C.skyblue, to: '/api/scenario' },
      { key: 'apiCase', label: t('home.apiCase', '接口用例'), value: c?.apiCase ?? 0, icon: <ProfileOutlined />, color: C.cyan, to: '/api/definition' },
      { key: 'funcCase', label: t('home.funcCase', '功能用例'), value: c?.funcCase ?? 0, icon: <ProfileOutlined />, color: C.green, to: '/functional-case' },
      { key: 'plan', label: t('home.plan', '测试计划'), value: c?.plan ?? 0, icon: <ScheduleOutlined />, color: C.orange, to: '/test-plan' },
      { key: 'req', label: t('home.req', '需求'), value: c?.req ?? 0, icon: <FileTextOutlined />, color: C.pink, to: '/requirement' },
      { key: 'bug', label: t('home.bug', '缺陷'), value: c?.bug ?? 0, icon: <BugOutlined />, color: C.red, to: '/bug' },
    ],
    [c, t],
  )

  const donutSegs = [
    { label: t('home.def', '接口定义'), value: c?.def ?? 0, color: 'var(--brand)' },
    { label: t('home.scenario', '场景用例'), value: c?.scenario ?? 0, color: C.skyblue },
    { label: t('home.apiCase', '接口用例'), value: c?.apiCase ?? 0, color: C.cyan },
    { label: t('home.funcCase', '功能用例'), value: c?.funcCase ?? 0, color: C.green },
  ]
  const totalAssets = donutSegs.reduce((s, x) => s + x.value, 0)
  const totalCases = (c?.apiCase ?? 0) + (c?.funcCase ?? 0)
  const bugRate = totalCases ? ((c?.bug ?? 0) * 100) / totalCases : 0

  if (!projectId)
    return (
      <div style={{ padding: 48 }}>
        <Empty description={t('common.selectProject', '请先在顶部选择项目')} />
      </div>
    )

  const renderCard = (key: CardKey): React.ReactNode => {
    switch (key) {
      case 'assets':
        return (
          <Card title={cardTitle.assets} size="small" style={{ marginBottom: 16 }}>
            {totalAssets === 0 ? (
              <Empty description={t('common.empty', '暂无数据')} />
            ) : (
              <div style={{ display: 'flex', alignItems: 'center', gap: 32, padding: '8px 0' }}>
                <Donut segments={donutSegs} size={140} thickness={20} />
                <div style={{ flex: 1, maxWidth: 360 }}>
                  {donutSegs.map((s) => (
                    <div key={s.label} style={{ display: 'flex', alignItems: 'center', padding: '6px 0', fontSize: 13 }}>
                      <span style={{ width: 10, height: 10, borderRadius: 2, background: s.color, marginRight: 8 }} />
                      <span style={{ flex: 1, color: 'var(--text-2)' }}>{s.label}</span>
                      <b>{s.value}</b>
                      <span style={{ width: 56, textAlign: 'right', color: 'var(--text-3)' }}>
                        {totalAssets ? ((s.value * 100) / totalAssets).toFixed(1) : '0'}%
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </Card>
        )
      case 'collab': {
        // Accounting rule: once a task passes acceptance (VERIFIED), a DELIVERED delivery record involving AI
        // counts as an AI delivery, otherwise human; failed acceptance counts for neither. Workload = task points.
        const AI_COLOR = 'var(--brand)'
        const items = collab?.items ?? []
        const sum = (f: (x: (typeof items)[number]) => number) => items.reduce((n, x) => n + f(x), 0)
        const aiTasks = sum((x) => x.aiTasks)
        const humanTasks = sum((x) => x.humanTasks)
        const aiPoints = sum((x) => x.aiPoints)
        const humanPoints = sum((x) => x.humanPoints)

        // Requirement breakdown: total points desc; fall back to task count when all points are 0.
        const ranked = [...items]
          .map((x) => ({ ...x, tp: x.aiPoints + x.humanPoints, tc: x.aiTasks + x.humanTasks }))
          .sort((a, b) => b.tp - a.tp || b.tc - a.tc)
          .slice(0, 8)
        const legend = (color: string, label: string, n: number, pts: number) => (
          <span style={{ display: 'inline-flex', alignItems: 'center', fontSize: 13, color: 'var(--text-2)', whiteSpace: 'nowrap' }}>
            <span style={{ width: 10, height: 10, borderRadius: 2, background: color, marginRight: 6, flexShrink: 0 }} />
            {label}
            <span style={{ color: 'var(--text)', fontWeight: 600, margin: '0 4px' }}>{n}</span>
            <span style={{ color: 'var(--text-3)' }}>({pts} {t('home.ptsUnit', '点')})</span>
          </span>
        )
        return (
          <Card title={<span><RobotOutlined style={{ color: AI_COLOR, marginRight: 6 }} />{cardTitle.collab}</span>} size="small" style={{ marginBottom: 16 }}>
            {aiTasks + humanTasks === 0 ? (
              <Empty description={t('home.collabEmpty', '暂无已验收任务;任务派发并验收通过后,这里展示 AI/人工 的交付拆分')} />
            ) : (
              <>
                <Row gutter={[24, 16]} align="middle">
                  <Col xs={24} lg={10}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 24 }}>
                      <Donut
                        segments={[
                          { label: t('home.aiDelivered', 'AI 交付'), value: aiTasks, color: C.blue },
                          { label: t('home.humanDelivered', '人工交付'), value: humanTasks, color: C.human },
                        ]}
                        size={120}
                        thickness={16}
                        centerLabel={t('home.byCount', '已验收任务')}
                      />
                      <Donut
                        segments={[
                          { label: t('home.aiDelivered', 'AI 交付'), value: aiPoints, color: C.blue },
                          { label: t('home.humanDelivered', '人工交付'), value: humanPoints, color: C.human },
                        ]}
                        size={120}
                        thickness={16}
                        centerLabel={t('home.byPoints', '工作量(点)')}
                      />
                      <div>
                        <div style={{ marginBottom: 8 }}>{legend(C.blue, t('home.aiDelivered', 'AI 交付'), aiTasks, aiPoints)}</div>
                        <div>{legend(C.human, t('home.humanDelivered', '人工交付'), humanTasks, humanPoints)}</div>
                      </div>
                    </div>
                  </Col>
                  <Col xs={24} lg={14}>
                    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 6 }}>
                      <span style={{ fontSize: 12, color: 'var(--text-3)' }}>{t('home.collabGridTitle', '近一年验收日历(格子=当日验收任务数)')}</span>
                      <Segmented
                        size="small"
                        value={collabMetric}
                        onChange={(v) => setCollabMetric(v as 'total' | 'ai' | 'human')}
                        options={[
                          { label: t('home.metricTotal', '全部'), value: 'total' },
                          { label: 'AI', value: 'ai' },
                          { label: t('home.humanShort', '人'), value: 'human' },
                        ]}
                      />
                    </div>
                    <ContributionGrid days={collab?.daily ?? []} metric={collabMetric} />
                  </Col>
                </Row>
                {/* Per-requirement AI/human workload split (task-count split when points are 0). */}
                <div style={{ marginTop: 16 }}>
                  <div style={{ fontSize: 13, fontWeight: 600, marginBottom: 8 }}>{t('home.reqSplit', '需求内工作量占比')}</div>
                  {ranked.map((r) => {
                    const usePts = r.tp > 0
                    const a = usePts ? r.aiPoints : r.aiTasks
                    const h = usePts ? r.humanPoints : r.humanTasks
                    const total = a + h || 1
                    return (
                      <div key={r.requirementId} style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '4px 0' }}>
                        <span style={{ width: 220, fontSize: 13, color: 'var(--text-2)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }} title={r.title}>{r.title}</span>
                        <div style={{ flex: 1, display: 'flex', height: 14, borderRadius: 4, overflow: 'hidden', background: 'var(--border-soft)' }}>
                          <div title={`AI ${a}`} style={{ width: `${(a * 100) / total}%`, background: C.blue }} />
                          <div title={`${t('home.humanDelivered', '人工交付')} ${h}`} style={{ width: `${(h * 100) / total}%`, background: C.human }} />
                        </div>
                        <span style={{ width: 150, fontSize: 12, color: 'var(--text-3)', textAlign: 'right' }}>
                          AI {a} · {t('home.humanShort', '人')} {h}{usePts ? ` ${t('home.ptsUnit', '点')}` : ''} · {((a * 100) / total).toFixed(0)}%
                        </span>
                      </div>
                    )
                  })}
                </div>
              </>
            )}
          </Card>
        )
      }
      case 'projectBars': {
        // Asset total desc; cap raised to 60 now that the chart scrolls horizontally — enough, and guards against extreme project counts.
        const TOP = 60
        const ranked = projRows
          .map((r) => ({ r, total: Object.values(r.values).reduce((s, v) => s + v, 0) }))
          .filter((x) => x.total > 0)
          .sort((a, b) => b.total - a.total)
        const shown = ranked.slice(0, TOP).map((x) => x.r)
        return (
          <Card
            title={<span><ApiOutlined style={{ color: C.skyblue, marginRight: 6 }} />{cardTitle.projectBars}</span>}
            extra={ranked.length > TOP ? <span style={{ fontSize: 12, color: 'var(--text-3)' }}>{t('home.topN', '资产量前 {n}').replace('{n}', String(TOP))}</span> : ranked.length > 8 ? <span style={{ fontSize: 12, color: 'var(--text-3)' }}>{t('home.scrollHint', '← 左右滑动 →')}</span> : undefined}
            size="small"
            style={{ marginBottom: 16 }}
          >
            {shown.length === 0 ? (
              <Empty description={t('common.empty', '暂无数据')} />
            ) : (
              <GroupedBars
                series={PROJECT_SERIES.map((s) => ({ ...s, label: t(`home.${s.key}`, s.label) }))}
                rows={shown}
              />
            )}
          </Card>
        )
      }
      case 'apiStats': {
        const totalDefs = defs.length
        const uncovered = totalDefs - coveredDefs
        const coverRate = totalDefs ? (coveredDefs * 100) / totalDefs : 0
        return (
          <Card title={<span><ApiOutlined style={{ color: 'var(--brand)', marginRight: 6 }} />{cardTitle.apiStats}</span>} size="small" style={{ marginBottom: 16 }}>
            {totalDefs === 0 ? (
              <Empty description={t('common.empty', '暂无数据')} />
            ) : (
              <Row gutter={[24, 16]} align="middle">
                {/* Protocol distribution: total donut + per-protocol share */}
                <Col xs={24} lg={14}>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 24 }}>
                    <Donut segments={protocolSegs} size={132} thickness={18} centerLabel={t('home.apiTotal', '接口总数')} />
                    <div style={{ flex: 1, minWidth: 0 }}>
                      {protocolSegs.map((s) => (
                        <div key={s.label} style={{ display: 'flex', alignItems: 'center', padding: '5px 0', fontSize: 13 }}>
                          <span style={{ width: 10, height: 10, borderRadius: 2, background: s.color, marginRight: 8 }} />
                          <span style={{ flex: 1, color: 'var(--text-2)' }}>{s.label}</span>
                          <b>{s.value}</b>
                          <span style={{ width: 56, textAlign: 'right', color: 'var(--text-3)' }}>{((s.value * 100) / totalDefs).toFixed(1)}%</span>
                        </div>
                      ))}
                    </div>
                  </div>
                </Col>
                {/* Case coverage: covered / uncovered */}
                <Col xs={24} lg={10}>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 24 }}>
                    <Donut
                      segments={[
                        { label: t('home.covered', '已覆盖'), value: coveredDefs, color: C.green },
                        { label: t('home.uncovered', '未覆盖'), value: uncovered, color: 'var(--text-3)' },
                      ]}
                      size={132}
                      thickness={18}
                      centerLabel={t('home.coverRate', '覆盖率')}
                    />
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div style={{ fontSize: 22, fontWeight: 700, color: coverRate >= 60 ? C.green : coverRate >= 30 ? C.orange : C.red }}>{coverRate.toFixed(1)}%</div>
                      <div style={{ color: 'var(--text-3)', fontSize: 12, marginBottom: 10 }}>{t('home.coverRateHint', '有用例引用的接口占比')}</div>
                      <div style={{ display: 'flex', alignItems: 'center', padding: '5px 0', fontSize: 13 }}>
                        <span style={{ width: 10, height: 10, borderRadius: 2, background: C.green, marginRight: 8 }} />
                        <span style={{ flex: 1, color: 'var(--text-2)' }}>{t('home.covered', '已覆盖')}</span>
                        <b>{coveredDefs}</b>
                      </div>
                      <div style={{ display: 'flex', alignItems: 'center', padding: '5px 0', fontSize: 13 }}>
                        <span style={{ width: 10, height: 10, borderRadius: 2, background: '#d9d9d9', marginRight: 8 }} />
                        <span style={{ flex: 1, color: 'var(--text-2)' }}>{t('home.uncovered', '未覆盖')}</span>
                        <b>{uncovered}</b>
                      </div>
                    </div>
                  </div>
                </Col>
              </Row>
            )}
          </Card>
        )
      }
      case 'caseStats': {
        const totalCases = c?.apiCase ?? 0
        const executedCases = exec?.executedCases ?? 0
        const unexecuted = Math.max(0, totalCases - executedCases)
        const executions = exec?.executions ?? 0
        const passed = exec?.passed ?? 0
        const failed = Math.max(0, executions - passed)
        const execRate = totalCases ? (executedCases * 100) / totalCases : 0
        const passRate = executions ? (passed * 100) / executions : 0
        const metric = (label: string, value: number, color?: string) => (
          <div style={{ minWidth: 96 }}>
            <div style={{ color: 'var(--text-3)', fontSize: 12 }}>{label}</div>
            <div style={{ fontSize: 22, fontWeight: 700, color }}>{value}</div>
          </div>
        )
        const rateBlock = (label: string, rate: number, segs: { label: string; value: number; color: string }[]) => (
          <div style={{ display: 'flex', alignItems: 'center', gap: 16 }}>
            <Donut segments={segs} size={104} thickness={14} centerLabel={label} />
            <div>
              <div style={{ fontSize: 18, fontWeight: 700, color: rate >= 60 ? C.green : rate >= 30 ? C.orange : C.red }}>{rate.toFixed(1)}%</div>
              {segs.map((s) => (
                <div key={s.label} style={{ display: 'flex', alignItems: 'center', fontSize: 12, padding: '2px 0' }}>
                  <span style={{ width: 8, height: 8, borderRadius: 2, background: s.color, marginRight: 6 }} />
                  <span style={{ color: 'var(--text-2)', marginRight: 8 }}>{s.label}</span>
                  <b>{s.value}</b>
                </div>
              ))}
            </div>
          </div>
        )
        return (
          <Card title={<span><ProfileOutlined style={{ color: C.cyan, marginRight: 6 }} />{cardTitle.caseStats}</span>} size="small" style={{ marginBottom: 16 }}>
            <Row gutter={[24, 16]} align="middle">
              <Col xs={24} md={6}>
                <div style={{ display: 'flex', gap: 24, flexWrap: 'wrap' }}>
                  {metric(t('home.caseTotal', '接口用例数'), totalCases, C.cyan)}
                  {metric(t('home.execCount', '执行次数'), executions, C.skyblue)}
                </div>
              </Col>
              <Col xs={24} md={9}>
                {rateBlock(t('home.execRate', '执行率'), execRate, [
                  { label: t('home.executed', '已执行'), value: executedCases, color: C.skyblue },
                  { label: t('home.unexecuted', '未执行'), value: unexecuted, color: 'var(--text-3)' },
                ])}
              </Col>
              <Col xs={24} md={9}>
                {rateBlock(t('home.passRate', '通过率'), passRate, [
                  { label: t('home.passed', '已通过'), value: passed, color: C.green },
                  { label: t('home.failedExec', '未通过'), value: failed, color: C.red },
                ])}
              </Col>
            </Row>
          </Card>
        )
      }
      case 'execTrend': {
        const hasData = trendRows.some((r) => (r.values.passed ?? 0) + (r.values.failed ?? 0) > 0)
        return (
          <Card title={<span><ThunderboltOutlined style={{ color: C.orange, marginRight: 6 }} />{cardTitle.execTrend}</span>} extra={<span style={{ fontSize: 12, color: 'var(--text-3)' }}>{t('home.last7d', '近 7 天')}</span>} size="small" style={{ marginBottom: 16 }}>
            {!hasData ? (
              <Empty description={t('home.noExec', '近 7 天无执行记录')} />
            ) : (
              <GroupedBars
                height={220}
                series={[
                  { key: 'passed', label: t('home.passed', '已通过'), color: C.green },
                  { key: 'failed', label: t('home.failedExec', '未通过'), color: C.red },
                ]}
                rows={trendRows}
              />
            )}
          </Card>
        )
      }
      case 'quality':
        return (
          <Card
            title={
              <span>
                <SafetyCertificateOutlined style={{ color: C.green, marginRight: 6 }} />
                {cardTitle.quality}
              </span>
            }
            size="small"
            style={{ marginBottom: 16 }}
          >
            <Row gutter={[16, 16]}>
              <Col xs={12} sm={6}>
                <Statistic title={t('home.req', '需求')} value={c?.req ?? 0} valueStyle={{ color: C.pink }} />
              </Col>
              <Col xs={12} sm={6}>
                <Statistic title={t('home.bug', '缺陷')} value={c?.bug ?? 0} valueStyle={{ color: C.red }} />
              </Col>
              <Col xs={12} sm={6}>
                <Statistic title={t('home.plan', '测试计划')} value={c?.plan ?? 0} valueStyle={{ color: C.orange }} />
              </Col>
              <Col xs={12} sm={6}>
                <Tooltip title={t('home.bugRateHint', '缺陷数 / 用例总数')}>
                  <Statistic
                    title={t('home.bugRate', '缺陷率')}
                    value={bugRate}
                    precision={1}
                    suffix="%"
                    valueStyle={{ color: bugRate > 20 ? C.red : C.green }}
                  />
                </Tooltip>
              </Col>
            </Row>
          </Card>
        )
      case 'shortcuts':
        return (
          <Card
            title={
              <span>
                <ThunderboltOutlined style={{ color: C.orange, marginRight: 6 }} />
                {cardTitle.shortcuts}
              </span>
            }
            size="small"
            style={{ marginBottom: 16 }}
          >
            <div style={{ display: 'flex', flexWrap: 'wrap', gap: 10 }}>
              <Button icon={<ApiOutlined />} onClick={() => navigate('/api/definition')}>{t('home.def', '接口定义')}</Button>
              <Button icon={<PartitionOutlined />} onClick={() => navigate('/api/scenario')}>{t('home.scenario', '场景用例')}</Button>
              <Button icon={<ProfileOutlined />} onClick={() => navigate('/functional-case')}>{t('home.funcCase', '功能用例')}</Button>
              <Button icon={<ScheduleOutlined />} onClick={() => navigate('/test-plan')}>{t('home.plan', '测试计划')}</Button>
              <Button icon={<FileTextOutlined />} onClick={() => navigate('/requirement')}>{t('home.req', '需求')}</Button>
              <Button icon={<BugOutlined />} onClick={() => navigate('/bug')}>{t('home.bug', '缺陷')}</Button>
            </div>
          </Card>
        )
      case 'planOverview': {
        // Metric sourcing: exec count/status come from planStats (exec = executeRate × total, same as the test-plan page);
        // "API CASE" = plan-linked case count (planCases; PlanCase has no type field, so it can't be split);
        // functional cases / scenarios / bugs are project-level counts (same source as the top asset stats).
        const selPlan = planList.find((p) => p.id === planId)
        const total = planStat?.total ?? 0
        const exec = planStat ? Math.round(planStat.executeRate * planStat.total) : 0
        const execRate = total ? (exec * 100) / total : 0
        const status: 'NOT_STARTED' | 'RUNNING' | 'DONE' = !planStat || total === 0 || exec <= 0 ? 'NOT_STARTED' : exec >= total ? 'DONE' : 'RUNNING'
        const statusMeta: Record<string, { label: string; color: string }> = {
          NOT_STARTED: { label: t('home.planStatusNotStarted', '未开始'), color: 'default' },
          RUNNING: { label: t('home.planStatusRunning', '进行中'), color: 'processing' },
          DONE: { label: t('home.planStatusDone', '已完成'), color: 'success' },
        }
        const closedBugs = bugs.filter((b) => (b.status || '').toUpperCase() === 'CLOSED').length
        const statBlock = (icon: React.ReactNode, color: string, label: string, value: number) => (
          <div style={{ display: 'flex', alignItems: 'center', gap: 10, minWidth: 130 }}>
            <span style={{ width: 34, height: 34, borderRadius: 8, background: 'var(--brand-soft)', color, display: 'inline-flex', alignItems: 'center', justifyContent: 'center', fontSize: 16, flex: '0 0 auto' }}>{icon}</span>
            <div>
              <div style={{ fontSize: 12, color: 'var(--text-3)' }}>{label}</div>
              <div style={{ fontSize: 20, fontWeight: 700, lineHeight: 1.2 }}>{value}</div>
            </div>
          </div>
        )
        // Small exec-rate ring (plain SVG).
        const R = 34
        const CIRC = 2 * Math.PI * R
        const ring = (
          <svg width={88} height={88} viewBox="0 0 88 88" style={{ flex: '0 0 auto' }}>
            <circle cx={44} cy={44} r={R} fill="none" stroke="var(--border-soft)" strokeWidth={10} />
            <circle cx={44} cy={44} r={R} fill="none" stroke="var(--brand)" strokeWidth={10} strokeLinecap="round" strokeDasharray={`${(CIRC * execRate) / 100} ${CIRC}`} transform="rotate(-90 44 44)" />
            <text x={44} y={42} textAnchor="middle" fontSize={14} fontWeight={700} fill="var(--text)">{execRate.toFixed(0)}%</text>
            <text x={44} y={58} textAnchor="middle" fontSize={10} fill="var(--text-3)">{t('home.execRate', '执行率')}</text>
          </svg>
        )
        return (
          <Card
            title={<span><ScheduleOutlined style={{ color: C.orange, marginRight: 6 }} />{cardTitle.planOverview}</span>}
            extra={planList.length > 0 ? (
              <Select
                size="small"
                style={{ width: 200 }}
                value={planId || undefined}
                options={planList.map((p) => ({ value: p.id, label: p.label }))}
                onChange={(v) => { setPlanId(v); localStorage.setItem(planSelKey, v) }}
              />
            ) : undefined}
            size="small"
            style={{ marginBottom: 16 }}
          >
            {!selPlan ? (
              <Empty description={t('home.noPlan', '暂无测试计划,请先在测试计划页创建')} />
            ) : (
              <>
                <Row gutter={[24, 16]} align="middle">
                  <Col xs={24} lg={12}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 10 }}>
                      <span style={{ fontSize: 14, fontWeight: 600, color: 'var(--text)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }} title={selPlan.label}>{selPlan.label}</span>
                      <Tag color={statusMeta[status].color} style={{ flex: '0 0 auto' }}>{statusMeta[status].label}</Tag>
                    </div>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 16 }}>
                      <div style={{ flex: 1, minWidth: 0 }}>
                        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                          <Progress percent={Number(execRate.toFixed(1))} size="small" style={{ flex: 1, margin: 0 }} />
                          <span style={{ flex: '0 0 auto', fontSize: 12, color: 'var(--text-3)' }}>{t('home.passThreshold', '通过阈值 100%')}</span>
                        </div>
                        <div style={{ fontSize: 12, color: 'var(--text-3)', marginTop: 6 }}>
                          {t('home.planExecuted', '已执行')} {exec} / {total}
                        </div>
                      </div>
                      {ring}
                    </div>
                  </Col>
                  <Col xs={24} lg={12}>
                    <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, minmax(130px, 1fr))', gap: '14px 16px' }}>
                      {statBlock(<ProfileOutlined />, C.green, t('home.funcCase', '功能用例'), c?.funcCase ?? 0)}
                      {statBlock(<ApiOutlined />, C.cyan, t('home.apiCaseShort', '接口 CASE'), planCaseCount)}
                      {statBlock(<PartitionOutlined />, C.skyblue, t('home.apiScenario', '接口场景'), c?.scenario ?? 0)}
                      {statBlock(<BugOutlined />, C.red, t('home.bugCount', '缺陷数'), bugs.length)}
                    </div>
                  </Col>
                </Row>
                {/* Per-assignee breakdown (plan cases have no assignee yet — single "unassigned" bucket): assigned/done/bugs filed/bugs closed */}
                <div style={{ marginTop: 16 }}>
                  <GroupedBars
                    height={200}
                    series={[
                      { key: 'assigned', label: t('home.assignedCases', '分配用例'), color: C.blue },
                      { key: 'done', label: t('home.doneCases', '完成用例'), color: C.green },
                      { key: 'submitted', label: t('home.submittedBugs', '提交缺陷'), color: C.orange },
                      { key: 'closed', label: t('home.closedBugs', '关闭缺陷'), color: C.red },
                    ]}
                    rows={[{ name: t('home.unassigned', '未分配'), values: { assigned: planCaseCount, done: exec, submitted: bugs.length, closed: closedBugs } }]}
                  />
                </div>
              </>
            )}
          </Card>
        )
      }
      case 'apiChanges': {
        const fmtTime = (d: ApiDefinition) => (d.updatedAt ?? d.createdAt ?? '').slice(0, 16) || '—'
        return (
          <Card
            title={<span><HistoryOutlined style={{ color: C.skyblue, marginRight: 6 }} />{cardTitle.apiChanges}</span>}
            extra={<span style={{ fontSize: 12, color: 'var(--text-3)' }}>{t('home.recent10', '最近 10 条')}</span>}
            size="small"
            style={{ marginBottom: 16 }}
          >
            {recentDefs.length === 0 ? (
              <Empty description={t('common.empty', '暂无数据')} />
            ) : (
              <Table<ApiDefinition>
                size="small"
                rowKey="id"
                pagination={false}
                dataSource={recentDefs}
                onRow={(d) => ({ onClick: () => navigate(`/api/definition?open=${d.id}`), style: { cursor: 'pointer' } })}
                columns={[
                  { title: t('home.colId', 'ID'), width: 90, render: (_, d) => (d.num != null ? String(d.num) : '—') },
                  { title: t('home.colApiName', '接口名称'), dataIndex: 'name', ellipsis: true },
                  { title: t('home.colPath', '路径'), dataIndex: 'path', ellipsis: true },
                  { title: t('home.colLinkedCases', '关联CASE'), width: 100, render: (_, d) => refCounts[d.id]?.cases ?? '—' },
                  { title: t('home.colLinkedScenarios', '关联场景'), width: 100, render: (_, d) => refCounts[d.id]?.scenarios ?? '—' },
                  { title: t('home.colUpdatedAt', '更新时间'), width: 160, render: (_, d) => fmtTime(d) },
                ]}
              />
            )}
          </Card>
        )
      }
    }
  }

  // Loop hero: requirement → review → dev delivery → test → verify, feeding back into requirements.
  const passRateHero = exec?.executions ? ((exec.passed ?? 0) * 100) / exec.executions : 0
  // Clickable asset counts per stage, grouped by type: requirement → requirements; test → test assets; verify → bugs.
  const chip = (key: string) => cards.find((x) => x.key === key)
  const asset = (keys: string[]) => keys.map(chip).filter(Boolean) as typeof cards
  const loopStages = [
    { key: 'req', name: t('loop.req', '需求'), desc: t('loop.reqDesc', 'MRD 自动转 PRD'), icon: <FileDoneOutlined />, to: '/requirement', assets: asset(['req']) },
    { key: 'review', name: t('loop.review', '评审'), desc: t('loop.reviewDesc', 'AI 参与评审 · 版本留痕'), icon: <AuditOutlined />, to: '/review', assets: [] as typeof cards },
    { key: 'dev', name: t('loop.dev', '研发交付'), desc: t('loop.devDesc', '多 Agent 协同研发'), icon: <RobotOutlined />, to: '/agents', assets: [] as typeof cards },
    { key: 'test', name: t('loop.test', '测试'), desc: t('loop.testDesc', 'TDD 驱动 · 自动化测试'), icon: <ExperimentOutlined />, to: '/api/scenario', assets: asset(['def', 'scenario', 'apiCase', 'funcCase', 'plan']) },
    { key: 'verify', name: t('loop.verify', '验收质量'), desc: t('loop.verifyDesc', '决策链路可视化 · 回归需求'), icon: <SafetyCertificateOutlined />, to: '/bug', assets: asset(['bug']) },
  ]
  const loopHero = (
    <div className="ms-loop-hero" style={{ marginBottom: 16, borderRadius: 12, padding: '18px 20px', border: '1px solid var(--border-soft)', overflow: 'hidden' }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 16, flexWrap: 'wrap', gap: 8 }}>
        <div>
          <div style={{ fontSize: 18, fontWeight: 700, color: 'var(--text)' }}>{t('loop.heroTitle', '从需求到交付的 AI 闭环')}</div>
          <div style={{ color: 'var(--text-3)', fontSize: 12, marginTop: 3 }}>{t('loop.heroSub', 'MRD 自动转 PRD → AI 参与评审(版本留痕)→ 多 Agent 协同研发 → TDD 测试验收 → 决策链路可视化,结果回归需求')}</div>
        </div>
        <div style={{ textAlign: 'right' }}>
          <div style={{ fontSize: 26, fontWeight: 800, lineHeight: 1.1, color: passRateHero >= 60 ? 'var(--success)' : passRateHero >= 30 ? 'var(--warning)' : 'var(--error)' }}>{passRateHero.toFixed(0)}%</div>
          <div style={{ fontSize: 11, color: 'var(--text-3)' }}>{t('loop.health', '闭环健康 · 用例通过率')}</div>
        </div>
      </div>
      <div style={{ display: 'flex', alignItems: 'stretch', flexWrap: 'wrap', gap: 0 }}>
        {loopStages.map((s, i) => (
          <div key={s.key} style={{ display: 'flex', alignItems: 'stretch', flex: '1 1 150px', minWidth: 150 }}>
            <div
              className="ms-hover-card"
              onClick={() => navigate(s.to)}
              style={{ flex: 1, position: 'relative', overflow: 'hidden', cursor: 'pointer', background: 'linear-gradient(135deg, var(--brand-soft) 0%, var(--panel) 55%)', border: '1px solid var(--border-soft)', borderRadius: 10, padding: '12px 14px' }}
            >
              {/* Ghost step number, top-right */}
              <span style={{ position: 'absolute', top: 6, right: 12, fontStyle: 'italic', fontWeight: 800, fontSize: 22, lineHeight: 1, color: 'var(--brand)', opacity: 0.16, pointerEvents: 'none' }}>
                <span style={{ fontSize: 12, fontWeight: 600, marginRight: 2 }}>step</span>{i + 1}
              </span>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 6 }}>
                <span style={{ width: 28, height: 28, borderRadius: 8, background: 'var(--brand-soft)', color: 'var(--brand)', display: 'inline-flex', alignItems: 'center', justifyContent: 'center', fontSize: 15 }}>{s.icon}</span>
                <span style={{ fontWeight: 600, color: 'var(--text)' }}>{s.name}</span>
              </div>
              <div style={{ color: 'var(--text-3)', fontSize: 12, marginBottom: 8 }}>{s.desc}</div>
              {s.assets.length > 0 ? (
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: '2px 12px' }}>
                  {s.assets.map((a) => (
                    <span
                      key={a.key}
                      onClick={(e) => { e.stopPropagation(); navigate(a.to) }}
                      style={{ fontSize: 12, color: 'var(--text-2)', cursor: 'pointer' }}
                      onMouseEnter={(e) => { e.currentTarget.style.color = 'var(--brand)' }}
                      onMouseLeave={(e) => { e.currentTarget.style.color = 'var(--text-2)' }}
                    >
                      {a.label} <b style={{ color: a.color, fontSize: 14 }}>{a.value}</b>
                    </span>
                  ))}
                </div>
              ) : (
                <span style={{ fontSize: 12, color: 'var(--brand)' }}>{t('loop.enter', '进入')} <ArrowRightOutlined style={{ fontSize: 10 }} /></span>
              )}
            </div>
            {i < loopStages.length - 1 && (
              <ArrowRightOutlined style={{ color: 'var(--text-3)', margin: '0 6px', flex: '0 0 auto', alignSelf: 'center' }} />
            )}
          </div>
        ))}
        {/* Loop-closure badge */}
        <div style={{ display: 'flex', alignItems: 'center', flex: '0 0 auto', paddingLeft: 6 }}>
          <Tooltip title={t('loop.backHint', '验证结果自动回归需求,进入下一轮')}>
            <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4, padding: '6px 10px', borderRadius: 20, background: 'var(--brand-soft)', color: 'var(--brand)', fontSize: 12, fontWeight: 600, cursor: 'default' }}>
              <SyncOutlined /> {t('loop.closed', '闭环')}
            </span>
          </Tooltip>
        </div>
      </div>
    </div>
  )

  return (
    <div style={{ padding: 16, height: '100%', overflow: 'auto' }}>
      {loopHero}
      <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 12 }}>
        <Button size="small" icon={<SettingOutlined />} onClick={() => setEditing(true)}>{t('home.cs.edit', '编辑')}</Button>
      </div>
      <Spin spinning={loading}>
        {layout.length === 0 ? (
          <Empty description={t('home.cs.empty', '暂无卡片,点击右上角「编辑」添加')} />
        ) : (
          <Row gutter={16} className="ms-home-grid">
            {layout.map((p) => (
              <Col key={p.key} span={p.size === 'full' ? 24 : 12}>
                {renderCard(p.key)}
              </Col>
            ))}
          </Row>
        )}
      </Spin>
      {editing && (
        <CardSettings
          layout={layout}
          onExit={() => setEditing(false)}
          onSave={(next) => {
            saveLayout(next.filter((x): x is CardLayout => (ALL_CARDS as readonly string[]).includes(x.key)))
            setEditing(false)
          }}
        />
      )}
    </div>
  )
}
