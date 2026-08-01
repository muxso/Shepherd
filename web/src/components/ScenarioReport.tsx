import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { Button, Cascader, Empty, Input, Segmented, Space, Tabs, Tag, Tooltip, Typography } from 'antd'
import { ShareAltOutlined } from '@ant-design/icons'
import { message } from '../feedback'
import ResizableDrawer from './ResizableDrawer'
import { api, type ApiCase, type AssertionResult, type DebugResponse, type ReportResultItem, type ScenarioReportDetail, type ScenarioStep } from '../api'
import { methodColor, outcomeColor, execStatusLabel } from './tags'
import { DebugResultPanel, type SentRequest } from './ApiSpecPanel'
import { LatencyStat } from './TimingBreakdown'
import { useI18n } from '../i18n'

export type TFn = (key: string, fallback?: string) => string

// Reference id to readable name (case/sub-scenario); falls back to a short id instead of full UUIDs.
export type NameOf = (id: string) => string

/* Humanized units for report stats: ms flips to seconds past 1s, bytes climb KB/MB/GB. */
export const fmtDuration = (ms: number) =>
  ms >= 60_000 ? `${(ms / 60_000).toFixed(1)} min` : ms >= 1000 ? `${(ms / 1000).toFixed(2)} s` : `${ms} ms`
export const fmtSize = (b: number) =>
  b >= 1 << 30 ? `${(b / (1 << 30)).toFixed(2)} GB`
  : b >= 1 << 20 ? `${(b / (1 << 20)).toFixed(2)} MB`
  : b >= 1024 ? `${(b / 1024).toFixed(1)} KB`
  : `${b} bytes`

const runOutcomeLabel = execStatusLabel

export function makeStepMeta(t: TFn): Record<string, { label: string; color: string }> {
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

// Scenario report (ref #26). Latency/size/status/body are only persisted by newer executors;
// missing values render as an em dash.
// 4-state distribution: pass / false positive / fail / not run (mirrors the reference report).
type Dist = { pass: number; falsePos: number; fail: number; skip: number }
const DIST_COLORS = { pass: '#22c55e', falsePos: '#f59e0b', fail: '#ef4444', skip: '#c9cdd4' }
function StatRing({ d, centerLabel, labels }: { d: Dist; centerLabel: string; labels: Record<keyof Dist, string> }) {
  const total = d.pass + d.falsePos + d.fail + d.skip
  const C = 2 * Math.PI * 42
  const order: (keyof Dist)[] = ['pass', 'falsePos', 'fail', 'skip']
  let off = 0
  return (
    <svg width="116" height="116" viewBox="0 0 120 120">
      <g transform="rotate(-90 60 60)">
        <circle cx="60" cy="60" r="42" fill="none" stroke="var(--border-soft)" strokeWidth="14" />
        {total > 0 && order.map((k) => {
          if (d[k] <= 0) return null
          const len = (d[k] / total) * C
          const el = (
            <Tooltip key={k} title={`${labels[k]} ${d[k]} · ${((d[k] / total) * 100).toFixed(2)}%`}>
              <circle cx="60" cy="60" r="42" fill="none" stroke={DIST_COLORS[k]} strokeWidth="14" strokeDasharray={`${len} ${C - len}`} strokeDashoffset={`-${off}`} style={{ cursor: 'pointer' }} />
            </Tooltip>
          )
          off += len
          return el
        })}
      </g>
      <text x="60" y="58" textAnchor="middle" fontSize="22" fontWeight="700" fill="currentColor" style={{ color: 'var(--text)' }}>{total}</text>
      <text x="60" y="76" textAnchor="middle" fontSize="11" fill="#8a9099">{centerLabel}</text>
    </svg>
  )
}
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
        <StatRing d={d} centerLabel={centerLabel} labels={Object.fromEntries(rows.map((r) => [r.k, r.label])) as Record<keyof Dist, string>} />
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

// ---- Report step tree: mirrors the scenario structure; flat results are mapped onto the
// tree by walking it in the same DFS order the compiled plan executes (sub-scenarios expanded
// in place, LOOP bodies repeated `times` times, IF bodies matched leaf-by-leaf). ----
type RTNode = {
  key: string
  kind: string // step kinds + ITER (loop round) + EXTRA (unmatched trailing results)
  content: ReactNode
  /** Plain text for the name search. */
  text: string
  children?: RTNode[]
  /** Executable leaf that consumes one result row during the DFS walk. */
  leaf?: boolean
  /** Leaf identity for conditional (IF) matching: CASE id or REQUEST method+url. */
  match?: { caseId?: string; method?: string; url?: string }
  /** Subtree the executor never runs (ONCE body on loop rounds >1, SCENARIO refs nested in controllers). */
  noExec?: boolean
  result?: ReportResultItem
}

type ReportTreeData = { subs: Record<string, ScenarioStep[]>; names: Record<string, string> }
type ReportTreeCtx = { data: ReportTreeData; t: TFn; nameOf: NameOf; onceRepeat?: boolean }

/** Scenario ids referenced by steps: direct SCENARIO steps + controller children, recursively. */
function collectScenarioRefs(steps: ScenarioStep[]): string[] {
  const out: string[] = []
  const fromChild = (c: any) => {
    if (!c) return
    if (String(c.kind || '').toUpperCase() === 'SCENARIO' && c.refId) out.push(String(c.refId))
    if (Array.isArray(c.children)) c.children.forEach(fromChild)
  }
  for (const s of steps) {
    if (s.scenarioId) out.push(s.scenarioId)
    const ctl = s.control as { children?: unknown[] } | null | undefined
    if (Array.isArray(ctl?.children)) ctl.children.forEach(fromChild)
  }
  return out
}

const reportReqNode = (key: string, method: string, url: string): RTNode => ({
  key,
  kind: 'REQUEST',
  leaf: true,
  match: { method, url },
  text: `${method} ${url}`,
  content: <Space><Tag color={methodColor(method)} style={{ margin: 0 }}>{method}</Tag><span className="ms-mono">{url}</span></Space>,
})

function buildScenarioNodes(id: string, prefix: string, ctx: ReportTreeCtx, depth: number, stack: string[]): RTNode[] {
  // Cycle/depth guard mirrors the fetch cache; missing entries render as an empty group.
  if (depth > 5 || stack.includes(id)) return []
  const steps = ctx.data.subs[id]
  if (!steps) return []
  return [...steps].sort((a, b) => a.order - b.order).map((s, i) => buildReportStepNode(s, `${prefix}.${i}`, ctx, depth, [...stack, id]))
}

function buildReportStepNode(s: ScenarioStep, key: string, ctx: ReportTreeCtx, depth: number, stack: string[]): RTNode {
  const { t, nameOf } = ctx
  if (s.caseId) return { key, kind: 'CASE', leaf: true, match: { caseId: s.caseId }, text: nameOf(s.caseId), content: <span className="ms-mono">{t('scenario.caseRef', '用例')} {nameOf(s.caseId)}</span> }
  if (s.request) return reportReqNode(key, s.request.method, s.request.url)
  if (s.scenarioId) return { key, kind: 'SCENARIO', text: nameOf(s.scenarioId), content: <span className="ms-mono">{t('scenario.subScenario', '子场景')} {nameOf(s.scenarioId)}</span>, children: buildScenarioNodes(s.scenarioId, key, ctx, depth + 1, stack) }
  if (s.control) return buildReportControlNode(s.kind.toUpperCase(), s.control, key, ctx, depth, stack)
  return { key, kind: s.kind.toUpperCase(), text: '', content: '—' }
}

function buildReportChildNode(c: any, key: string, ctx: ReportTreeCtx, depth: number, stack: string[]): RTNode {
  const kind = String(c?.kind || '').toUpperCase()
  const { t, nameOf } = ctx
  if (kind === 'CASE') return { key, kind, leaf: true, match: { caseId: c.refId }, text: nameOf(c.refId), content: <span className="ms-mono">{t('scenario.caseRef', '用例')} {nameOf(c.refId)}</span> }
  if (kind === 'REQUEST') return reportReqNode(key, c.method || 'GET', c.url || '')
  if (kind === 'SCENARIO')
    // The plan compiler drops SCENARIO refs nested in controllers, so this subtree never executes.
    return { key, kind, noExec: true, text: nameOf(c.refId), content: <span className="ms-mono">{t('scenario.subScenario', '子场景')} {nameOf(c.refId)}</span>, children: buildScenarioNodes(String(c.refId), key, ctx, depth + 1, stack) }
  return buildReportControlNode(kind, c, key, ctx, depth, stack)
}

function buildReportControlNode(kind: string, payload: any, key: string, ctx: ReportTreeCtx, depth: number, stack: string[]): RTNode {
  const { t } = ctx
  const raw = Array.isArray(payload?.children) ? payload.children : []
  const buildKids = (prefix: string, onceRepeat: boolean) => raw.map((c: any, i: number) => buildReportChildNode(c, `${prefix}.${i}`, { ...ctx, onceRepeat }, depth, stack))
  if (kind === 'LOOP') {
    const times = Math.max(1, Number(payload?.times ?? 1) || 1)
    const label = `${t('scenario.loopPrefix', '循环')} ${times} ${t('scenario.loopSuffix', '次')}`
    // Rounds >1: ONCE bodies inside run only on the first pass (executor keeps once-done state per run).
    const children = times > 1
      ? Array.from({ length: times }, (_, r) => ({
          key: `${key}#${r}`,
          kind: 'ITER',
          text: '',
          content: `${t('scenario.iterPrefix', '第')} ${r + 1} ${t('scenario.iterSuffix', '次')}`.trim(),
          children: buildKids(`${key}#${r}`, !!ctx.onceRepeat || r > 0),
        }))
      : buildKids(key, !!ctx.onceRepeat)
    return { key, kind: 'LOOP', text: label, content: label, children }
  }
  if (kind === 'IF') {
    const label = `${payload?.variable ?? ''} ${payload?.operator ?? ''} ${payload?.value ?? ''}`.trim()
    return { key, kind: 'IF', text: label, content: <span className="ms-mono">{label}</span>, children: buildKids(key, !!ctx.onceRepeat) }
  }
  if (kind === 'ONCE') return { key, kind: 'ONCE', text: '', content: t('scenario.onceOnly', '仅执行一次'), noExec: ctx.onceRepeat, children: buildKids(key, !!ctx.onceRepeat) }
  if (kind === 'TIMER') return { key, kind: 'TIMER', text: '', content: `${t('scenario.waitPrefix', '等待')} ${payload?.ms ?? 0} ms` }
  return { key, kind, text: '', content: '—' }
}

/** Does the result row belong to this leaf? Used only under IF bodies (skippable at runtime). */
function reportLeafMatches(m: { caseId?: string; method?: string; url?: string }, r: ReportResultItem): boolean {
  if (m.caseId) return r.caseId === m.caseId
  const parsed = /^([A-Z]+)\s+(\S*)/.exec(r.caseId)
  const method = (parsed?.[1] || r.request?.method || '').toUpperCase()
  if (method && m.method && method !== m.method.toUpperCase()) return false
  // Report labels carry merged query/REST params; compare paths loosely.
  const strip = (u?: string) => (u || '').split('?')[0].replace(/\/+$/, '')
  const a = strip(parsed?.[2] || r.request?.url)
  const b = strip(m.url)
  return !a || !b || a === b || a.endsWith(b) || b.endsWith(a)
}

/** DFS walk with a cursor into results[]; leaves inside IF bodies only consume on a match. */
function assignReportResults(nodes: RTNode[], results: ReportResultItem[], cur: { i: number }, conditional: boolean) {
  for (const n of nodes) {
    if (n.noExec) continue
    if (n.leaf && n.match && cur.i < results.length) {
      const r = results[cur.i]
      if (!conditional || reportLeafMatches(n.match, r)) {
        n.result = r
        cur.i++
      }
    }
    if (n.children) assignReportResults(n.children, results, cur, conditional || n.kind === 'IF')
  }
}

/** Keep leaves matching the predicate plus their ancestor rows; no filters = full tree. */
function filterReportTree(nodes: RTNode[], pred: (n: RTNode) => boolean, filtering: boolean): RTNode[] {
  return nodes.flatMap((n) => {
    if (n.children?.length) {
      const kids = filterReportTree(n.children, pred, filtering)
      return kids.length || !filtering ? [{ ...n, children: kids }] : []
    }
    return !filtering || pred(n) ? [n] : []
  })
}

function collectGroupKeys(nodes: RTNode[], out: string[] = []): string[] {
  for (const n of nodes) {
    if (n.children?.length) {
      out.push(n.key)
      collectGroupKeys(n.children, out)
    }
  }
  return out
}

// Per-leaf result cluster (pass/fail tag + status + latency w/ timing waterfall + size); shared by
// the flat report row and the tree row.
function ResultCluster({ r, t }: { r: ReportResultItem; t: TFn }) {
  const ok = r.outcome === 'SUCCESS'
  const muted: React.CSSProperties = { color: 'var(--text-3)', fontSize: 12, whiteSpace: 'nowrap' }
  return (
    <span style={{ display: 'inline-flex', alignItems: 'center', gap: 10 }}>
      <Tag color={ok ? 'green' : 'red'} style={{ margin: 0 }}>{ok ? t('scenario.pass', '通过') : t('scenario.fail', '失败')}</Tag>
      {r.statusCode != null && <Tooltip title={t('scenario.statusTip', '服务端返回的 HTTP 状态码')}><span style={muted}>{t('apidef.statusCode', '状态码')} <span style={{ color: r.statusCode < 400 ? 'var(--success)' : 'var(--error)' }}>{r.statusCode}</span></span></Tooltip>}
      {r.timings ? (
        <LatencyStat totalMs={r.latencyMs ?? 0} timings={r.timings}>
          <span style={muted}>{t('scenario.respTime', '响应时间')} {r.latencyMs != null ? fmtDuration(r.latencyMs) : '—'}</span>
        </LatencyStat>
      ) : (
        <Tooltip title={t('scenario.respTimeTip', '从建立连接到收到服务端完整响应的全链路耗时')}><span style={muted}>{t('scenario.respTime', '响应时间')} {r.latencyMs != null ? fmtDuration(r.latencyMs) : '—'}</span></Tooltip>
      )}
      <Tooltip title={t('scenario.respSizeTip', '响应体大小')}><span style={muted}>{t('scenario.respSize', '响应大小')} {r.respSize != null ? fmtSize(r.respSize) : '—'}</span></Tooltip>
    </span>
  )
}

// Report tree rows: numbered per level, groups expand/collapse; in tab view executed leaves also
// expand an inline response panel (same debug panel synthesis as ReportRow).
function ReportTreeRows({ nodes, depth, view, t, caseOf, expandedKeys, openKeys, onExpand, onOpen }: {
  nodes: RTNode[]
  depth: number
  view: 'flat' | 'tab'
  t: TFn
  caseOf?: (id: string) => ApiCase | undefined
  expandedKeys: Set<string>
  openKeys: Set<string>
  onExpand: (k: string) => void
  onOpen: (k: string) => void
}) {
  const stepMeta = makeStepMeta(t)
  return (
    <>
      {nodes.map((n, i) => {
        const meta = stepMeta[n.kind]
        const hasKids = (n.children?.length ?? 0) > 0
        const isExpanded = expandedKeys.has(n.key)
        const r = n.result
        const hasDetail = !!r && (r.statusCode != null || r.body != null || (r.headers?.length ?? 0) > 0)
        const canOpen = view === 'tab' && hasDetail
        const isOpen = canOpen && openKeys.has(n.key)
        const clickable = hasKids || canOpen
        const req = r && isOpen ? synthSentRequest(r, caseOf) : null
        const resp = r && isOpen ? synthDebugResponse(r) : null
        return (
          <div key={n.key}>
            <div
              onClick={() => { if (hasKids) onExpand(n.key); else if (canOpen) onOpen(n.key) }}
              style={{ marginLeft: depth * 24, border: '1px solid var(--border-soft)', borderRadius: 6, marginBottom: 6, background: depth ? 'var(--panel-2)' : 'var(--panel)', cursor: clickable ? 'pointer' : 'default' }}
            >
              <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '8px 12px' }}>
                {clickable
                  ? <span style={{ color: 'var(--text-3)', fontSize: 11, width: 12 }}>{(hasKids ? isExpanded : isOpen) ? '▾' : '▸'}</span>
                  : <span style={{ width: 12 }} />}
                <span style={{ color: 'var(--text-3)', fontSize: 12, minWidth: 18 }}>{i + 1}</span>
                {meta && <Tag color={meta.color} style={{ margin: 0 }}>{meta.label}</Tag>}
                <span style={{ flex: 1, minWidth: 0, fontWeight: n.kind === 'ITER' ? 500 : undefined }}>{n.content}</span>
                {n.leaf && (r ? <ResultCluster r={r} t={t} /> : <Tag style={{ margin: 0, color: 'var(--text-3)' }}>{t('scenario.skip', '未执行')}</Tag>)}
              </div>
              {r && r.failures.length > 0 && (
                <div style={{ padding: '0 12px 8px 42px' }}>
                  {r.failures.map((f, j) => <div key={j} style={{ color: 'var(--error)', fontSize: 12 }} className="ms-mono">✗ {f}</div>)}
                </div>
              )}
              {isOpen && resp && (
                <div style={{ padding: '0 12px 12px' }} onClick={(e) => e.stopPropagation()}>
                  <DebugResultPanel running={false} resp={resp} err="" req={req} isHttp={!!req} />
                </div>
              )}
            </div>
            {hasKids && isExpanded && (
              <ReportTreeRows nodes={n.children!} depth={depth + 1} view={view} t={t} caseOf={caseOf} expandedKeys={expandedKeys} openKeys={openKeys} onExpand={onExpand} onOpen={onOpen} />
            )}
          </div>
        )
      })}
    </>
  )
}

// Scenario run report drawer: analysis cards + rings on top; toolbar filter + expand/collapse all;
// detail rows mirror the scenario step tree when the owning scenario is known (scenarioId prop),
// otherwise fall back to the flat list (union batch reports). Rows reuse ReportRow / ReportTreeRows.
// Report body: overview (report/step/request analysis) + detail list. Extracted so the in-app
// modal (fetches by reportId) and the public share page (fed a pre-fetched `data`) render the
// EXACT same layout. The step tree needs auth (api.getScenario), so it's absent on the public
// page (no scenarioId) and gracefully falls back to the flat list.
export function ScenarioReportBody({ data, scenarioId, nameOf, caseMap, fetchScenario }: { data: ScenarioReportDetail; scenarioId?: string; nameOf: NameOf; caseMap?: Record<string, ApiCase>; fetchScenario?: (id: string) => Promise<{ name: string; steps: ScenarioStep[] }> }) {
  const { t } = useI18n()
  const [search, setSearch] = useState('')
  const [openSet, setOpenSet] = useState<Set<number>>(new Set())
  // Flat rows vs tab view (tab = expanding a leaf also opens the response panel); outcome filter is a [scope, outcome] cascader pick.
  const [view, setView] = useState<'flat' | 'tab'>('tab')
  const [filter, setFilter] = useState<string[]>()
  // Step tree of the owning scenario (+ referenced sub-scenarios, fetched recursively).
  const [treeData, setTreeData] = useState<ReportTreeData | null>(null)
  const [expandedKeys, setExpandedKeys] = useState<Set<string>>(new Set())
  const [openKeys, setOpenKeys] = useState<Set<string>>(new Set())
  // Reset transient filters when the underlying report changes.
  useEffect(() => { setOpenSet(new Set()); setSearch(''); setFilter(undefined) }, [data.reportId])
  useEffect(() => {
    setTreeData(null)
    if (!scenarioId) return
    let alive = true
    const subs: Record<string, ScenarioStep[]> = {}
    const names: Record<string, string> = {}
    // Recursive fetch with cycle guard + depth cap; sub-scenario failures leave an empty group.
    const getScenario = fetchScenario || api.getScenario
    const load = async (id: string, depth: number, stack: Set<string>): Promise<void> => {
      if (depth > 5 || stack.has(id) || subs[id]) return
      const sc = await getScenario(id)
      subs[id] = sc.steps || []
      names[id] = sc.name
      const next = new Set(stack).add(id)
      for (const rid of collectScenarioRefs(subs[id])) await load(rid, depth + 1, next).catch(() => undefined)
    }
    load(scenarioId, 0, new Set())
      .then(() => { if (alive) setTreeData({ subs, names }) })
      .catch(() => { if (alive) setTreeData(null) })
    return () => { alive = false }
  }, [scenarioId])
  const all = data.results || []
  const passN = all.filter((r) => r.outcome === 'SUCCESS').length
  const failN = all.length - passN
  const passRate = all.length ? (passN / all.length) * 100 : 0
  // Distribution (backend doesn't track false positives / not-run yet, so 0). Steps and requests share the same source (flat scenario, one request per step).
  const dist: Dist = { pass: passN, falsePos: 0, fail: failN, skip: 0 }
  // Request total = sum of step latencies; report total is roughly the executedAt span + last step latency (falls back to request total without valid timestamps).
  const reqTotalMs = all.reduce((s, r) => s + (r.latencyMs ?? 0), 0)
  // Report total time: prefer backend wall-clock (durationMs, since 0056); old reports fall back to executedAt span / request total.
  const times = all.map((r) => Date.parse((r.executedAt || '').replace(' ', 'T'))).filter((n) => !Number.isNaN(n))
  const span = times.length ? Math.max(...times) - Math.min(...times) : 0
  const reportTotalMs = (data.durationMs != null && data.durationMs >= 0)
    ? data.durationMs
    : (span > 0 ? span + (all.length ? all[all.length - 1].latencyMs ?? 0 : 0) : reqTotalMs)
  // Assertion pass rate (per assertion; falls back to step pass rate without assertion data).
  let asTotal = 0, asPass = 0
  for (const r of all) { const a = (r.assertions as AssertionResult[] | undefined) || []; asTotal += a.length; asPass += a.filter((x) => x.passed).length }
  const asRate = asTotal ? (asPass / asTotal) * 100 : passRate
  // caseId is usually a readable request line (GET http://...) or a case UUID; resolve UUIDs via nameOf.
  const label = (id: string) => (/^[0-9a-f]{8}-/.test(id) ? nameOf(id) : id)
  // Outcome filter predicates: step and request scopes share one source (flat scenario, one request per step).
  const outcomeMatch: Record<string, (r: ReportResultItem) => boolean> = {
    pass: (r) => r.outcome === 'SUCCESS',
    falsePos: () => false,
    fail: (r) => r.outcome !== 'SUCCESS' && r.outcome !== 'ERROR' && r.outcome !== 'SKIPPED',
    skip: (r) => r.outcome === 'SKIPPED',
    scriptError: (r) => r.outcome === 'ERROR',
  }
  const outcomeKey = filter?.[1] as string | undefined
  const rows = all.filter((r) =>
    (!search || label(r.caseId).toLowerCase().includes(search.toLowerCase())) &&
    (!outcomeKey || outcomeMatch[outcomeKey]?.(r) !== false),
  )
  const toggle = (i: number) => setOpenSet((prev) => { const n = new Set(prev); if (n.has(i)) n.delete(i); else n.add(i); return n })
  // Build the step tree + map results by DFS order; null = fall back to the flat list.
  const treeNameOf: NameOf = (id) => treeData?.names[id] || nameOf(id)
  const tree = useMemo(() => {
    if (!treeData || !scenarioId) return null
    const rootSteps = treeData.subs[scenarioId]
    if (!rootSteps?.length) return null
    const ctx: ReportTreeCtx = { data: treeData, t, nameOf: treeNameOf }
    const nodes = [...rootSteps].sort((a, b) => a.order - b.order).map((s, i) => buildReportStepNode(s, `s${i}`, ctx, 0, [scenarioId]))
    const cur = { i: 0 }
    assignReportResults(nodes, data.results, cur, false)
    // Rows left over after the walk (alignment drift): append flat, never hide data.
    data.results.slice(cur.i).forEach((r, j) =>
      nodes.push({ key: `x${j}`, kind: 'EXTRA', leaf: true, text: label(r.caseId), content: <span className="ms-mono">{label(r.caseId)}</span>, result: r }))
    return nodes
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [treeData, data, scenarioId, t])
  // Default: hierarchy fully expanded, response panels closed.
  useEffect(() => {
    setExpandedKeys(new Set(tree ? collectGroupKeys(tree) : []))
    setOpenKeys(new Set())
  }, [tree])
  const filtering = !!search.trim() || !!outcomeKey
  const visibleTree = useMemo(() => {
    if (!tree) return null
    const q = search.trim().toLowerCase()
    const pred = (n: RTNode) => {
      if (q && !n.text.toLowerCase().includes(q)) return false
      if (outcomeKey) {
        if (!n.leaf) return false
        if (outcomeKey === 'skip') return !n.result
        return n.result ? outcomeMatch[outcomeKey]?.(n.result) !== false : false
      }
      return true
    }
    return filterReportTree(tree, pred, filtering)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tree, search, outcomeKey])
  const toggleKey = (set: React.Dispatch<React.SetStateAction<Set<string>>>) => (k: string) =>
    set((prev) => { const n = new Set(prev); if (n.has(k)) n.delete(k); else n.add(k); return n })
  const outcomeOptions = [
    { value: 'pass', label: t('scenario.pass', '通过') },
    { value: 'falsePos', label: t('scenario.falsePos', '误报') },
    { value: 'fail', label: t('scenario.fail', '失败') },
    { value: 'skip', label: t('scenario.skip', '未执行') },
    { value: 'scriptError', label: t('scenario.scriptError', '脚本错误') },
  ]
  const stat = (lbl: ReactNode, val: ReactNode, color?: string) => (
    <div style={{ display: 'flex', justifyContent: 'space-between', padding: '6px 0', fontSize: 14 }}><span style={{ color: 'var(--text-2)' }}>{lbl}</span><b style={{ color }}>{val}</b></div>
  )
  return (
    <>
      {/* Overview: report / step / request analysis (mirrors the reference report). */}
      <div style={{ display: 'flex', gap: 16, flexWrap: 'wrap', marginBottom: 20 }}>
            <div style={{ flex: 1, minWidth: 260, border: '1px solid var(--border-soft)', borderRadius: 10, padding: '14px 18px', background: 'var(--panel)' }}>
              <h3 style={{ margin: '0 0 10px', fontSize: 14, color: 'var(--text-2)', display: 'flex', alignItems: 'center', gap: 8 }}>
                {t('scenario.reportAnalysis', '报告分析')}<Tag color={outcomeColor(data.status)} style={{ margin: 0 }}>{runOutcomeLabel(data.status, t)}</Tag>
              </h3>
              {stat(t('scenario.reportTotalTime', '报告总耗时'), <Tooltip title={t('scenario.reportTotalTip', '从第一步开始到最后一步完成的墙钟总耗时')}><span>{(reportTotalMs / 1000).toFixed(3)} <span style={{ color: 'var(--text-3)', fontWeight: 400 }}>(sec)</span></span></Tooltip>)}
              {stat(t('scenario.reqTotalTime', '请求总耗时'), <Tooltip title={t('scenario.respTimeTip', '从建立连接到收到服务端完整响应的全链路耗时')}><span>{fmtDuration(reqTotalMs)}</span></Tooltip>)}
              {stat(t('scenario.assertPassRate', '断言通过率'), <Tooltip title={t('scenario.assertRateTip', '通过断言数占全部断言数的比例')}><span>{asRate.toFixed(2)} <span style={{ color: 'var(--text-3)', fontWeight: 400 }}>(%)</span></span></Tooltip>, asRate >= 100 ? '#22c55e' : undefined)}
            </div>
            <DistCard title={t('scenario.stepAnalysis', '步骤分析')} d={dist} centerLabel={t('scenario.totalCount', '总数(个)')} t={t} />
            <DistCard title={t('scenario.reqAnalysis', '请求分析')} d={dist} centerLabel={t('scenario.totalCount', '总数(个)')} t={t} />
          </div>
          {/* Report detail: view toggle + name search + outcome cascader + expand/collapse (flat view only). */}
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 10, flexWrap: 'wrap' }}>
            <Typography.Text strong>{t('scenario.reportDetail', '报告明细')}</Typography.Text>
            <Segmented
              size="small"
              value={view}
              onChange={(v) => setView(v as 'flat' | 'tab')}
              options={[
                { value: 'flat', label: t('scenario.flatView', '平铺展示') },
                { value: 'tab', label: t('scenario.tabView', 'Tab展示') },
              ]}
            />
            <div style={{ flex: 1 }} />
            <Input
              size="small"
              allowClear
              style={{ width: 200 }}
              placeholder={t('scenario.searchByName', '通过名称搜索')}
              suffix={<span style={{ color: 'var(--text-3)' }}>⌕</span>}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
            <Cascader
              size="small"
              style={{ width: 200 }}
              placeholder={t('scenario.filterPh', '请选择过滤条件')}
              value={filter}
              onChange={(v) => setFilter(v as string[] | undefined)}
              allowClear
              expandTrigger="hover"
              options={[
                { value: 'step', label: t('scenario.filterStep', '步骤'), children: outcomeOptions },
                { value: 'request', label: t('scenario.filterReq', '请求'), children: outcomeOptions },
              ]}
            />
            {visibleTree ? (() => {
              // Tree mode: toggles the hierarchy only (never auto-opens response panels).
              const keys = collectGroupKeys(visibleTree)
              const allOpen = keys.length > 0 && keys.every((k) => expandedKeys.has(k))
              return keys.length > 0 ? (
                <Button size="small" onClick={() => setExpandedKeys(allOpen ? new Set() : new Set(collectGroupKeys(tree!)))}>
                  {allOpen ? t('scenario.collapseAll', '收起全部') : t('scenario.expandAll', '展开全部')}
                </Button>
              ) : null
            })() : view === 'flat' && (() => {
              const allOpen = rows.length > 0 && openSet.size >= rows.length
              return (
                <Button size="small" onClick={() => setOpenSet(allOpen ? new Set() : new Set(rows.map((_, i) => i)))}>
                  {allOpen ? t('scenario.collapseAll', '收起全部') : t('scenario.expandAll', '展开全部')}
                </Button>
              )
            })()}
          </div>
          <div style={{ marginTop: 10 }}>
            {visibleTree ? (
              visibleTree.length === 0 ? (
                <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('scenario.noStepResult', '无步骤结果')} />
              ) : (
                <ReportTreeRows
                  nodes={visibleTree}
                  depth={0}
                  view={view}
                  t={t}
                  caseOf={(id) => caseMap?.[id]}
                  expandedKeys={expandedKeys}
                  openKeys={openKeys}
                  onExpand={toggleKey(setExpandedKeys)}
                  onOpen={toggleKey(setOpenKeys)}
                />
              )
            ) : rows.length === 0 ? (
              <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('scenario.noStepResult', '无步骤结果')} />
            ) : view === 'flat' ? (
              rows.map((r, i) => <ReportRow key={i} idx={i + 1} r={r} label={label} t={t} open={openSet.has(i)} onToggle={() => toggle(i)} caseOf={(id) => caseMap?.[id]} />)
            ) : (
              <Tabs
                size="small"
                items={rows.map((r, i) => ({
                  key: String(i),
                  label: (
                    <span>
                      <span style={{ color: r.outcome === 'SUCCESS' ? 'var(--success)' : 'var(--error)', marginRight: 6 }}>●</span>
                      {i + 1} {label(r.caseId)}
                    </span>
                  ),
                  children: <ReportRow idx={i + 1} r={r} label={label} t={t} open onToggle={() => undefined} caseOf={(id) => caseMap?.[id]} />,
                }))}
              />
            )}
          </div>
        </>
  )
}

// In-app modal: fetches the report by id and renders the shared body inside a drawer, plus a
// "share" action that mints a public link. `scenarioId` enables the step-tree view.
export function ScenarioReportModal({ reportId, scenarioId, nameOf, caseMap, onClose }: { reportId: string | null; scenarioId?: string; nameOf: NameOf; caseMap?: Record<string, ApiCase>; onClose: () => void }) {
  const { t } = useI18n()
  const [data, setData] = useState<ScenarioReportDetail | null>(null)
  const [loading, setLoading] = useState(false)
  useEffect(() => {
    if (!reportId) { setData(null); return }
    setLoading(true)
    api.scenarioReport(reportId).then(setData).catch(() => setData(null)).finally(() => setLoading(false))
  }, [reportId])
  return (
    <ResizableDrawer
      open={!!reportId}
      onClose={onClose}
      width="60%"
      title={data?.name ? `${t('scenario.report', '场景报告')} · ${data.name}` : t('scenario.report', '场景报告')}
      styles={{ body: { background: 'var(--panel-2)' } }}
    >
      {loading ? (
        <div style={{ padding: 32, color: 'var(--text-3)' }}>{t('a.loading', '加载中…')}</div>
      ) : !data ? (
        <Empty description={t('scenario.noReport', '暂无报告')} />
      ) : (
        <>
          <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 12 }}>
            <Button
              icon={<ShareAltOutlined />}
              onClick={async () => {
                if (!reportId) return
                try {
                  const { token } = await api.shareScenarioReport(reportId, scenarioId)
                  await navigator.clipboard?.writeText(`${window.location.origin}/share/scenario/${token}`)
                  message.success(t('report.shareCopied', '分享链接已复制,无需登录即可访问'))
                } catch {
                  message.error(t('report.shareFailed', '生成分享链接失败'))
                }
              }}
            >
              {t('report.share', '分享报告')}
            </Button>
          </div>
          <ScenarioReportBody data={data} scenarioId={scenarioId} nameOf={nameOf} caseMap={caseMap} />
        </>
      )}
    </ResizableDrawer>
  )
}

// Rebuild the actual request line from the referenced case (mirrors backend to_node: REST substitution / query string / auth header).
// ${var} runtime substitution can't be reproduced client-side, so this shows the case template request (identical when the case has no variables).
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

// Actual request priority: 1) request persisted in the report (since 0060; variables/baseUrl/auth resolved),
// 2) legacy fallback: parse REQUEST steps from caseId="GET http://...", rebuild CASE references from the case template.
function synthSentRequest(r: ReportResultItem, caseOf?: (id: string) => ApiCase | undefined): SentRequest | null {
  if (r.request) return { method: r.request.method, url: r.request.url, headers: (r.request.headers ?? []).map(([key, value]) => ({ key, value })), body: r.request.body ?? undefined }
  const m = /^([A-Z]+)\s+(\S+)/.exec(r.caseId)
  if (m) return { method: m[1], url: m[2], headers: [] }
  const kase = caseOf?.(r.caseId)
  return kase ? caseToSentRequest(kase) : null
}

// Synthesize a DebugResponse from the stored response detail to reuse the debug 7-tab panel.
// Reports persist per-assertion results (incl. passes) since 0048; older reports synthesize failure rows from `failures`.
function synthDebugResponse(r: ReportResultItem): DebugResponse | null {
  const hasDetail = r.statusCode != null || r.body != null || (r.headers?.length ?? 0) > 0
  if (!hasDetail) return null
  const failAsserts: AssertionResult[] = r.failures.map((f) => ({ item: f, condition: '', expected: '', actual: '', passed: false, reason: f }))
  const asserts = r.assertions?.length ? r.assertions : failAsserts
  return { status: r.statusCode ?? 0, latencyMs: r.latencyMs ?? 0, headers: r.headers ?? [], body: r.body ?? '', assertions: asserts.length ? asserts : undefined, extractions: r.extractions?.length ? r.extractions : undefined }
}

function ReportRow({ idx, r, label, t, open, onToggle, caseOf }: { idx: number; r: ReportResultItem; label: (id: string) => string; t: TFn; open: boolean; onToggle: () => void; caseOf?: (id: string) => ApiCase | undefined }) {
  const hasDetail = r.statusCode != null || r.body != null || (r.headers?.length ?? 0) > 0
  const req = synthSentRequest(r, caseOf)
  const resp = synthDebugResponse(r)
  return (
    <div style={{ border: '1px solid var(--border-soft)', borderRadius: 8, marginBottom: 8, background: 'var(--panel)' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '8px 12px', cursor: hasDetail ? 'pointer' : 'default' }} onClick={() => hasDetail && onToggle()}>
        {hasDetail && <span style={{ color: 'var(--text-3)', fontSize: 11 }}>{open ? '▾' : '▸'}</span>}
        <span style={{ color: 'var(--text-3)', fontSize: 12, minWidth: 18 }}>{idx}</span>
        <span style={{ flex: 1, minWidth: 0 }} className="ms-mono">{label(r.caseId)}</span>
        <ResultCluster r={r} t={t} />
      </div>
      {r.failures.length > 0 && (
        <div style={{ padding: '0 12px 8px 40px' }}>
          {r.failures.map((f, j) => <div key={j} style={{ color: 'var(--error)', fontSize: 12 }} className="ms-mono">✗ {f}</div>)}
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
