import { useCallback, useEffect, useState, type ReactNode } from 'react'
import { Button, Card, Checkbox, Divider, Dropdown, Segmented, Select, Table, Tag, Tooltip } from 'antd'
import type { ColumnsType } from 'antd/es/table'
import { DownOutlined, ReloadOutlined } from '@ant-design/icons'
import { useNavigate } from 'react-router-dom'
import { api, type Bug, type CaseReviewSummary, type FunctionalCase, type PlanStats } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'
import { SelectProjectEmpty } from './Page'
import type { RegItem } from '../registry'
import { fetchPlanItems, isGroup } from './plan/planLocal'
import { funcCaseStatusLabel, priorityColor } from './tags'

// Shared body for the 待办 / 关注 workbench pages: stacked full-width section
// tables with a project switcher, a section-visibility dropdown and a refresh
// button, per the reference layout.
//   todo   → three sections (plans / reviews / bugs), pending items only
//   follow → six sections (plans / functional cases / reviews / API cases /
//            API scenarios / bugs), followed items only; plans via localStorage
//            set, the rest via /follow/mine per entity type.

type SectionKey = 'plan' | 'funcCase' | 'review' | 'apiCase' | 'apiScenario' | 'bug'

type PlanScope = 'all' | 'plan' | 'group'

type PlanStatus = 'notStarted' | 'inProgress' | 'done'

interface PlanRow {
  id: string
  name: string
  status: PlanStatus
  passRate: number
  total: number
  createdBy: string
  createdAt: number
  group: boolean
}

// Placeholder rows for the API case / scenario sections (no follow wiring yet,
// tables stay empty); typed so the columns compile.
interface ApiCaseRow {
  id: string
  displayId: string
  name: string
  priority: string
  status: string
  execResult: string
  env: string
  createdBy: string
  createdAt: number
}

const PLAN_FOLLOW_KEY = 'shepherd.planFollow'

// Same storage as PlanDetailHeader's follow toggle.
function planFollowSet(): Set<string> {
  try {
    return new Set(JSON.parse(localStorage.getItem(PLAN_FOLLOW_KEY) || '[]') as string[])
  } catch {
    return new Set()
  }
}

const bugColor = (s: string) => {
  const v = (s || '').toUpperCase()
  if (v === 'RESOLVED' || v === 'CLOSED') return 'green'
  if (v === 'REJECTED') return 'red'
  if (v === 'NEW' || v === 'REOPENED') return 'orange'
  return 'blue'
}
const BUG_ZH: Record<string, string> = {
  NEW: '新建', RESOLVED: '已解决', CLOSED: '已关闭', REOPENED: '重新打开', REJECTED: '已拒绝',
}

function fmtTime(v?: number | string | null): string {
  if (v == null || v === '') return '-'
  let d: Date
  if (typeof v === 'number') {
    d = new Date(v < 1e12 ? v * 1000 : v) // tolerate second-precision timestamps
  } else {
    d = new Date(v)
  }
  if (isNaN(d.getTime())) return String(v)
  const p = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`
}

const shortId = (id: string) => (id.length > 14 ? id.slice(0, 8) : id)

const MONO = 'ui-monospace, SFMono-Regular, Menlo, monospace'

function RateBar({ rate, text }: { rate: number; text: string }) {
  const pct = Math.min(100, Math.max(0, rate * 100))
  return (
    <span style={{ display: 'inline-flex', alignItems: 'center', gap: 8 }}>
      <span style={{ width: 100, height: 6, borderRadius: 3, background: 'var(--border-soft)', overflow: 'hidden', flex: 'none' }}>
        <span style={{ display: 'block', width: `${pct}%`, height: '100%', borderRadius: 3, background: 'var(--success)' }} />
      </span>
      <span style={{ fontSize: 12, color: 'var(--text-2)' }}>{text}</span>
    </span>
  )
}

export default function WorkbenchSections({ mode }: { mode: 'todo' | 'follow' }) {
  const { t } = useI18n()
  const { projects, projectId } = useApp()
  const nav = useNavigate()
  // Local project scope: defaults to the app-level project, switching it only
  // affects this page's data.
  const [pid, setPid] = useState(projectId)
  useEffect(() => { setPid(projectId) }, [projectId])
  const [visible, setVisible] = useState<SectionKey[]>(
    mode === 'follow'
      ? ['plan', 'funcCase', 'review', 'apiCase', 'apiScenario', 'bug']
      : ['plan', 'review', 'bug'],
  )
  const [loading, setLoading] = useState(false)
  const [planRows, setPlanRows] = useState<PlanRow[]>([])
  const [planScope, setPlanScope] = useState<PlanScope>('all')
  const [funcRows, setFuncRows] = useState<FunctionalCase[]>([])
  const [reviews, setReviews] = useState<CaseReviewSummary[]>([])
  const [bugRows, setBugRows] = useState<Bug[]>([])
  const [apiCaseRows, setApiCaseRows] = useState<ApiCaseRow[]>([])
  const [scenarioRows, setScenarioRows] = useState<ApiCaseRow[]>([])

  const load = useCallback(async () => {
    if (!pid) return
    setLoading(true)
    try {
      const loadPlans = async () => {
        let plans = await fetchPlanItems(pid).catch(() => [] as RegItem[])
        if (mode === 'follow') {
          const fs = planFollowSet()
          plans = plans.filter((p) => fs.has(p.id))
        }
        const entries = await Promise.all(
          plans.map((p) => api.planStats(p.id).then((s) => [p.id, s] as const).catch(() => null)),
        )
        const statsMap = Object.fromEntries(entries.filter(Boolean) as [string, PlanStats][])
        let rows: PlanRow[] = plans.map((p) => {
          const s = statsMap[p.id]
          const status: PlanStatus = !s || s.executeRate === 0 ? 'notStarted' : s.isPass ? 'done' : 'inProgress'
          return {
            id: p.id,
            name: p.label,
            status,
            passRate: s?.passRate ?? 0,
            total: s?.total ?? 0,
            createdBy: p.meta?.createdBy || '-',
            createdAt: p.createdAt,
            group: isGroup(p),
          }
        })
        if (mode === 'todo') {
          // Pending = not finished-and-passed.
          rows = rows.filter((r) => {
            const s = statsMap[r.id]
            return !s || s.executeRate < 1 || !s.isPass
          })
        }
        setPlanRows(rows)
      }
      const loadFuncCases = async () => {
        if (mode !== 'follow') { setFuncRows([]); return }
        const [cases, ids] = await Promise.all([
          api.functionalCases(pid).catch(() => [] as FunctionalCase[]),
          api.myFollows(pid, 'FUNCTIONAL_CASE').then((r) => r.entityIds || []).catch(() => [] as string[]),
        ])
        const idSet = new Set(ids)
        setFuncRows(cases.filter((c) => idSet.has(c.id)))
      }
      const loadReviews = async () => {
        if (mode === 'follow') {
          const [rv, ids] = await Promise.all([
            api.caseReviews(pid).catch(() => [] as CaseReviewSummary[]),
            api.myFollows(pid, 'CASE_REVIEW').then((r) => r.entityIds || []).catch(() => [] as string[]),
          ])
          const idSet = new Set(ids)
          setReviews(rv.filter((r) => idSet.has(r.id)))
          return
        }
        const rv = await api.caseReviews(pid).catch(() => [] as CaseReviewSummary[])
        setReviews(rv.filter((r) => r.passed < r.total || r.total === 0))
      }
      const loadBugs = async () => {
        const bugs = await api.bugs(pid).catch(() => [] as Bug[])
        if (mode === 'follow') {
          const ids = await api.myFollows(pid, 'bug').then((r) => r.entityIds || []).catch(() => [] as string[])
          const idSet = new Set(ids)
          setBugRows(bugs.filter((b) => idSet.has(b.id)))
        } else {
          setBugRows(bugs.filter((b) => ['NEW', 'REOPENED'].includes((b.status || '').toUpperCase())))
        }
      }
      const loadApiCases = async () => {
        if (mode !== 'follow') { setApiCaseRows([]); return }
        const [cases, ids] = await Promise.all([
          api.projectCasesAll(pid).catch(() => []),
          api.myFollows(pid, 'API_CASE').then((r) => r.entityIds || []).catch(() => [] as string[]),
        ])
        const idSet = new Set(ids)
        setApiCaseRows(
          cases.filter((c) => idSet.has(c.id)).map((c) => ({
            id: c.id,
            displayId: shortId(c.id),
            name: c.name,
            priority: c.priority || 'P3',
            status: c.status || '-',
            execResult: '-',
            env: '-',
            createdBy: '-',
            createdAt: 0,
          })),
        )
      }
      const loadScenarios = async () => {
        if (mode !== 'follow') { setScenarioRows([]); return }
        const [scns, ids] = await Promise.all([
          api.scenarios(pid).then((ss) => (Array.isArray(ss) ? ss : [])).catch(() => []),
          api.myFollows(pid, 'SCENARIO').then((r) => r.entityIds || []).catch(() => [] as string[]),
        ])
        const idSet = new Set(ids)
        setScenarioRows(
          scns.filter((s) => idSet.has(s.id)).map((s) => ({
            id: s.id,
            displayId: s.num ? String(s.num) : shortId(s.id),
            name: s.name,
            priority: String(s.meta?.priority || s.meta?.level || 'P3'),
            status: s.status || '-',
            execResult: s.lastResult || 'none',
            env: '-',
            createdBy: s.createdBy || '-',
            createdAt: s.createdAt ? new Date(s.createdAt).getTime() : 0,
          })),
        )
      }
      await Promise.all([loadPlans(), loadFuncCases(), loadReviews(), loadBugs(), loadApiCases(), loadScenarios()])
    } finally {
      setLoading(false)
    }
  }, [pid, mode])

  useEffect(() => { void load() }, [load])

  if (!pid) return <SelectProjectEmpty />

  const sections: { key: SectionKey; label: string }[] =
    mode === 'follow'
      ? [
          { key: 'plan', label: t('home.wb.plan', '测试计划') },
          { key: 'funcCase', label: t('home.wb.funcCase', '测试用例') },
          { key: 'review', label: t('home.wb.review', '用例评审') },
          { key: 'apiCase', label: t('home.wb.apiCase', '接口用例') },
          { key: 'apiScenario', label: t('home.wb.apiScenario', '接口场景') },
          { key: 'bug', label: t('home.wb.bug', '缺陷') },
        ]
      : [
          { key: 'plan', label: t('home.wb.plan', '测试计划') },
          { key: 'review', label: t('home.wb.review', '用例评审') },
          { key: 'bug', label: t('home.wb.bug', '缺陷') },
        ]
  const allChecked = visible.length === sections.length
  const featureLabel = allChecked
    ? t('home.wb.all', '全部')
    : sections.filter((s) => visible.includes(s.key)).map((s) => s.label).join('、') || t('home.wb.none', '无')

  const pager = {
    size: 'small' as const,
    defaultPageSize: 50,
    showSizeChanger: true,
    pageSizeOptions: ['10', '20', '50', '100'],
    showTotal: (n: number) => t('home.wb.total', '共 {n} 条').replace('{n}', String(n)),
  }

  const planStatusTag = (s: PlanStatus) =>
    s === 'notStarted' ? <Tag style={{ marginInlineEnd: 0 }}>{t('home.wb.notStarted', '未开始')}</Tag>
      : s === 'done' ? <Tag color="green" style={{ marginInlineEnd: 0 }}>{t('home.wb.done', '已完成')}</Tag>
      : <Tag color="blue" style={{ marginInlineEnd: 0 }}>{t('home.wb.inProgress', '进行中')}</Tag>

  const planCols: ColumnsType<PlanRow> = [
    {
      title: 'ID',
      dataIndex: 'id',
      width: 120,
      render: (id: string) => (
        <a
          title={id}
          style={{ color: 'var(--brand)', fontFamily: MONO }}
          onClick={(e) => { e.stopPropagation(); nav(`/test-plan?open=${encodeURIComponent(id)}`) }}
        >
          {shortId(id)}
        </a>
      ),
    },
    { title: t('home.wb.planName', '测试计划名称'), dataIndex: 'name', ellipsis: true },
    {
      title: t('home.wb.status', '状态'),
      dataIndex: 'status',
      width: 110,
      filters: [
        { text: t('home.wb.notStarted', '未开始'), value: 'notStarted' },
        { text: t('home.wb.inProgress', '进行中'), value: 'inProgress' },
        { text: t('home.wb.done', '已完成'), value: 'done' },
      ],
      onFilter: (v, r) => r.status === v,
      render: planStatusTag,
    },
    {
      title: t('home.wb.passRate', '通过率'),
      dataIndex: 'passRate',
      width: 200,
      render: (v: number) => <RateBar rate={v} text={`${(v * 100).toFixed(2)}%`} />,
    },
    { title: t('home.wb.caseCount', '用例数'), dataIndex: 'total', width: 90 },
    { title: t('home.wb.createdBy', '创建人'), dataIndex: 'createdBy', width: 120, ellipsis: true },
    {
      title: t('home.wb.createdAt', '创建时间'),
      dataIndex: 'createdAt',
      width: 175,
      sorter: (a, b) => a.createdAt - b.createdAt,
      render: (v: number) => fmtTime(v),
    },
  ]

  const shownPlanRows =
    mode === 'follow' && planScope !== 'all'
      ? planRows.filter((r) => (planScope === 'group' ? r.group : !r.group))
      : planRows

  const PRIORITY_FILTERS = ['P0', 'P1', 'P2', 'P3'].map((p) => ({ text: p, value: p }))
  const prioTag = (p?: string) => {
    const v = p || 'P2'
    return <Tag color={priorityColor(v)} style={{ marginInlineEnd: 0 }}>{v}</Tag>
  }

  const FCASE_STATUSES = ['PREPARED', 'PENDING', 'REVIEWING', 'PASS', 'REJECT']
  const funcCols: ColumnsType<FunctionalCase> = [
    {
      title: 'ID',
      dataIndex: 'num',
      width: 120,
      render: (num: number | undefined, r) => (
        <span title={r.id} style={{ color: 'var(--brand)', fontFamily: MONO }}>{num ?? shortId(r.id)}</span>
      ),
    },
    { title: t('home.wb.caseName', '用例名称'), dataIndex: 'name', ellipsis: true },
    {
      title: t('home.wb.caseLevel', '用例等级'),
      dataIndex: 'priority',
      width: 110,
      filters: PRIORITY_FILTERS,
      onFilter: (v, r) => (r.priority || 'P2') === v,
      render: prioTag,
    },
    {
      title: t('home.wb.reviewResult', '评审结果'),
      dataIndex: 'status',
      width: 130,
      filters: FCASE_STATUSES.map((s) => ({ text: funcCaseStatusLabel(s, t), value: s })),
      onFilter: (v, r) => (r.status || 'PREPARED').toUpperCase() === v,
      render: (s?: string) => <Tag style={{ marginInlineEnd: 0 }}>{funcCaseStatusLabel(s, t)}</Tag>,
    },
    {
      // Functional cases carry no execution result yet; everything reads 未执行.
      title: t('home.wb.execResult', '执行结果'),
      key: 'execResult',
      width: 110,
      filters: [{ text: t('home.wb.notExecuted', '未执行'), value: 'none' }],
      onFilter: () => true,
      render: () => '-',
    },
    { title: t('home.wb.createdBy', '创建人'), dataIndex: 'createdBy', width: 120, ellipsis: true, render: (v?: string) => v || '-' },
    {
      title: t('home.wb.createdAt', '创建时间'),
      dataIndex: 'createdAt',
      width: 175,
      sorter: (a, b) => new Date(a.createdAt || 0).getTime() - new Date(b.createdAt || 0).getTime(),
      render: (v?: string) => fmtTime(v),
    },
  ]

  // API case / scenario sections: followed items via /follow (API_CASE / SCENARIO);
  // env/exec columns show '-' where the list APIs lack the field.
  const API_STATUS_FILTERS = ['进行中', '已完成', '已废弃'].map((s) => ({ text: s, value: s }))
  const EXEC_FILTERS = [
    { text: t('status.success', '成功'), value: 'SUCCESS' },
    { text: t('status.error', '失败'), value: 'ERROR' },
    { text: t('home.wb.notExecuted', '未执行'), value: 'none' },
  ]
  const apiCaseCols: ColumnsType<ApiCaseRow> = [
    {
      title: 'ID',
      dataIndex: 'displayId',
      width: 120,
      sorter: (a, b) => a.displayId.localeCompare(b.displayId),
      render: (v: string, r) => <span title={r.id} style={{ color: 'var(--brand)', fontFamily: MONO }}>{v}</span>,
    },
    { title: t('home.wb.caseName', '用例名称'), dataIndex: 'name', ellipsis: true, sorter: (a, b) => a.name.localeCompare(b.name) },
    { title: t('home.wb.caseLevel', '用例等级'), dataIndex: 'priority', width: 110, filters: PRIORITY_FILTERS, onFilter: (v, r) => r.priority === v, render: prioTag },
    {
      title: t('home.wb.status', '状态'),
      dataIndex: 'status',
      width: 110,
      filters: API_STATUS_FILTERS,
      onFilter: (v, r) => r.status === v,
      sorter: (a, b) => a.status.localeCompare(b.status),
    },
    { title: t('home.wb.execResult', '执行结果'), dataIndex: 'execResult', width: 110, filters: EXEC_FILTERS, onFilter: (v, r) => r.execResult === v },
    { title: t('home.wb.caseEnv', '用例环境'), dataIndex: 'env', width: 140, ellipsis: true },
    { title: t('home.wb.createdBy', '创建人'), dataIndex: 'createdBy', width: 120, ellipsis: true },
    {
      title: t('home.wb.createdAt', '创建时间'),
      dataIndex: 'createdAt',
      width: 175,
      sorter: (a, b) => a.createdAt - b.createdAt,
      render: (v: number) => fmtTime(v),
    },
  ]
  const scenarioCols: ColumnsType<ApiCaseRow> = [
    {
      title: 'ID',
      dataIndex: 'displayId',
      width: 120,
      sorter: (a, b) => a.displayId.localeCompare(b.displayId),
      render: (v: string, r) => <span title={r.id} style={{ color: 'var(--brand)', fontFamily: MONO }}>{v}</span>,
    },
    { title: t('home.wb.scenarioName', '场景名称'), dataIndex: 'name', ellipsis: true, sorter: (a, b) => a.name.localeCompare(b.name) },
    {
      title: t('home.wb.scenarioLevel', '场景等级'),
      dataIndex: 'priority',
      width: 110,
      filters: PRIORITY_FILTERS,
      onFilter: (v, r) => r.priority === v,
      sorter: (a, b) => a.priority.localeCompare(b.priority),
      render: prioTag,
    },
    {
      title: t('home.wb.status', '状态'),
      dataIndex: 'status',
      width: 110,
      filters: API_STATUS_FILTERS,
      onFilter: (v, r) => r.status === v,
      sorter: (a, b) => a.status.localeCompare(b.status),
    },
    { title: t('home.wb.execResult', '执行结果'), dataIndex: 'execResult', width: 110, filters: EXEC_FILTERS, onFilter: (v, r) => r.execResult === v },
    { title: t('home.wb.scenarioEnv', '场景环境'), dataIndex: 'env', width: 140, ellipsis: true },
    { title: t('home.wb.createdBy', '创建人'), dataIndex: 'createdBy', width: 120, ellipsis: true },
    {
      title: t('home.wb.createdAt', '创建时间'),
      dataIndex: 'createdAt',
      width: 175,
      sorter: (a, b) => a.createdAt - b.createdAt,
      render: (v: number) => fmtTime(v),
    },
  ]

  const reviewDone = (r: CaseReviewSummary) => r.passed >= r.total && r.total > 0
  const reviewCols: ColumnsType<CaseReviewSummary> = [
    {
      title: 'ID',
      dataIndex: 'id',
      width: 120,
      render: (id: string) => <span title={id} style={{ color: 'var(--brand)', fontFamily: MONO }}>{shortId(id)}</span>,
    },
    {
      title: t('home.wb.reviewName', '评审名称'),
      dataIndex: 'name',
      key: 'name',
      ellipsis: true,
      // Unnamed legacy reviews fall back to a derived label.
      render: (v: string, r) =>
        v || <span style={{ fontFamily: MONO }}>{t('home.wb.reviewPrefix', '评审')} {shortId(r.id)}</span>,
    },
    {
      title: t('home.wb.reviewStatus', '评审状态'),
      key: 'status',
      width: 110,
      filters: [
        { text: t('home.wb.inProgress', '进行中'), value: 'inProgress' },
        { text: t('home.wb.done', '已完成'), value: 'done' },
      ],
      onFilter: (v, r) => (reviewDone(r) ? 'done' : 'inProgress') === v,
      render: (_, r) =>
        reviewDone(r)
          ? <Tag color="green" style={{ marginInlineEnd: 0 }}>{t('home.wb.done', '已完成')}</Tag>
          : <Tag color="blue" style={{ marginInlineEnd: 0 }}>{t('home.wb.inProgress', '进行中')}</Tag>,
    },
    {
      title: t('home.wb.passRate', '通过率'),
      key: 'passRate',
      width: 200,
      render: (_, r) => {
        const rate = r.total > 0 ? r.passed / r.total : 0
        return <RateBar rate={rate} text={`${Math.round(rate * 100)}%`} />
      },
    },
    { title: t('home.wb.reviewCases', '用例数量'), dataIndex: 'total', width: 100 },
    {
      title: t('home.wb.reviewMode', '评审模式'),
      dataIndex: 'reviewerCount',
      width: 120,
      render: (n: number) =>
        (n ?? 1) <= 1
          ? <Tag color="blue" style={{ marginInlineEnd: 0 }}>{t('home.wb.singleReview', '单人评审')}</Tag>
          : <Tag color="purple" style={{ marginInlineEnd: 0 }}>{t('home.wb.multiReview', '多人评审')}</Tag>,
    },
    { title: t('home.wb.createdBy', '创建人'), dataIndex: 'createdBy', width: 120, ellipsis: true, render: (v?: string | null) => v || '-' },
    {
      title: t('home.wb.createdAt', '创建时间'),
      dataIndex: 'createdAt',
      width: 175,
      sorter: (a, b) => new Date(a.createdAt).getTime() - new Date(b.createdAt).getTime(),
      render: (v: string) => fmtTime(v),
    },
  ]

  const bugCols: ColumnsType<Bug> = [
    {
      title: 'ID',
      dataIndex: 'id',
      width: 120,
      render: (id: string) => <span title={id} style={{ color: 'var(--brand)', fontFamily: MONO }}>{shortId(id)}</span>,
    },
    { title: t('home.wb.bugName', '缺陷名称'), dataIndex: 'title', ellipsis: true, render: (v: string, r) => v || r.id },
    {
      title: t('home.wb.severity', '严重程度'),
      dataIndex: 'severity',
      width: 100,
      filters: PRIORITY_FILTERS,
      onFilter: (v, r) => r.severity === v,
      render: (v?: string | null) => (v ? prioTag(v) : '-'),
    },
    {
      title: t('home.wb.status', '状态'),
      dataIndex: 'status',
      width: 110,
      filters: Object.keys(BUG_ZH).map((s) => ({ text: t(`bug.st.${s}`, BUG_ZH[s]), value: s })),
      onFilter: (v, r) => (r.status || '').toUpperCase() === v,
      render: (s: string) => (
        <Tag color={bugColor(s)} style={{ marginInlineEnd: 0 }}>{t(`bug.st.${s}`, BUG_ZH[s] || s)}</Tag>
      ),
    },
    { title: t('home.wb.handler', '处理人'), dataIndex: 'handler', width: 100, ellipsis: true, render: (v?: string | null) => v || '-' },
    { title: t('home.wb.createdBy', '创建人'), dataIndex: 'createdBy', width: 120, ellipsis: true, render: (v: string | null) => v || '-' },
    {
      title: t('home.wb.createdAt', '创建时间'),
      dataIndex: 'createdAt',
      width: 175,
      sorter: (a, b) => (a.createdAt || 0) - (b.createdAt || 0),
      render: (v?: number) => fmtTime(v),
    },
    { title: t('home.wb.updatedBy', '更新人'), dataIndex: 'updatedBy', width: 100, ellipsis: true, render: (v?: string | null) => v || '-' },
    {
      title: t('home.wb.updatedAt', '更新时间'),
      dataIndex: 'updatedAt',
      width: 175,
      sorter: (a, b) => new Date(a.updatedAt || 0).getTime() - new Date(b.updatedAt || 0).getTime(),
      render: (v?: string | null) => fmtTime(v),
    },
  ]

  const sectionCard = (title: string, table: ReactNode, extra?: ReactNode) => (
    <Card
      size="small"
      title={<span style={{ fontWeight: 600 }}>{title}</span>}
      extra={extra}
      styles={{ body: { padding: '0 8px 4px' } }}
    >
      {table}
    </Card>
  )

  // Follow mode: 全部 | 计划 | 计划组 scope switch on the plan card header.
  const planExtra =
    mode === 'follow' ? (
      <Segmented
        size="small"
        value={planScope}
        onChange={(v) => setPlanScope(v as PlanScope)}
        options={[
          { label: t('home.wb.all', '全部'), value: 'all' },
          { label: t('home.wb.planOnly', '计划'), value: 'plan' },
          { label: t('home.wb.planGroup', '计划组'), value: 'group' },
        ]}
      />
    ) : undefined

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <div style={{ display: 'flex', justifyContent: 'flex-end', alignItems: 'center', gap: 8 }}>
        <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
          <span style={{ color: 'var(--text-3)', fontSize: 13 }}>{t('home.wb.project', '项目')}</span>
          <Select
            value={pid}
            onChange={setPid}
            options={projects.map((p) => ({ value: p.id, label: p.name }))}
            style={{ minWidth: 180 }}
            showSearch
            optionFilterProp="label"
          />
        </span>
        <Dropdown
          trigger={['click']}
          popupRender={() => (
            <div
              style={{
                background: 'var(--panel)',
                border: '1px solid var(--border-soft)',
                borderRadius: 8,
                boxShadow: '0 6px 16px rgba(0,0,0,0.12)',
                padding: '6px 4px',
                minWidth: 160,
              }}
            >
              <div style={{ padding: '4px 12px' }}>
                <Checkbox
                  checked={allChecked}
                  indeterminate={visible.length > 0 && !allChecked}
                  onChange={(e) => setVisible(e.target.checked ? sections.map((s) => s.key) : [])}
                >
                  {t('home.wb.checkAll', '全选')}
                </Checkbox>
              </div>
              <Divider style={{ margin: '4px 0' }} />
              {sections.map((s) => (
                <div key={s.key} style={{ padding: '4px 12px' }}>
                  <Checkbox
                    checked={visible.includes(s.key)}
                    onChange={(e) =>
                      setVisible(e.target.checked ? [...visible, s.key] : visible.filter((k) => k !== s.key))
                    }
                  >
                    {s.label}
                  </Checkbox>
                </div>
              ))}
            </div>
          )}
        >
          <Button>
            <span style={{ color: 'var(--text-3)' }}>{t('home.wb.feature', '功能')}</span>
            <span>{featureLabel}</span>
            <DownOutlined style={{ fontSize: 10, color: 'var(--text-3)' }} />
          </Button>
        </Dropdown>
        <Tooltip title={t('home.wb.refresh', '刷新')}>
          <Button icon={<ReloadOutlined />} onClick={() => void load()} />
        </Tooltip>
      </div>

      {visible.includes('plan') &&
        sectionCard(
          t('home.wb.plan', '测试计划'),
          <Table
            rowKey="id"
            size="small"
            loading={loading}
            columns={planCols}
            dataSource={shownPlanRows}
            pagination={pager}
          />,
          planExtra,
        )}
      {mode === 'follow' && visible.includes('funcCase') &&
        sectionCard(
          t('home.wb.funcCase', '测试用例'),
          <Table
            rowKey="id"
            size="small"
            loading={loading}
            columns={funcCols}
            dataSource={funcRows}
            pagination={pager}
            onRow={() => ({ onClick: () => nav('/functional-case'), style: { cursor: 'pointer' } })}
          />,
        )}
      {visible.includes('review') &&
        sectionCard(
          t('home.wb.review', '用例评审'),
          <Table
            rowKey="id"
            size="small"
            loading={loading}
            columns={reviewCols}
            dataSource={reviews}
            pagination={pager}
            onRow={() => ({ onClick: () => nav('/review'), style: { cursor: 'pointer' } })}
          />,
        )}
      {mode === 'follow' && visible.includes('apiCase') &&
        sectionCard(
          t('home.wb.apiCase', '接口用例'),
          <Table
            rowKey="id"
            size="small"
            loading={loading}
            columns={apiCaseCols}
            dataSource={apiCaseRows}
            pagination={pager}
            onRow={() => ({ onClick: () => nav('/api/definition'), style: { cursor: 'pointer' } })}
          />,
        )}
      {mode === 'follow' && visible.includes('apiScenario') &&
        sectionCard(
          t('home.wb.apiScenario', '接口场景'),
          <Table
            rowKey="id"
            size="small"
            loading={loading}
            columns={scenarioCols}
            dataSource={scenarioRows}
            pagination={pager}
            onRow={(r) => ({ onClick: () => nav(`/api/scenario?scenario=${encodeURIComponent(r.id)}`), style: { cursor: 'pointer' } })}
          />,
        )}
      {visible.includes('bug') &&
        sectionCard(
          t('home.wb.bug', '缺陷'),
          <Table
            rowKey="id"
            size="small"
            loading={loading}
            columns={bugCols}
            dataSource={bugRows}
            pagination={pager}
            onRow={() => ({ onClick: () => nav('/bug'), style: { cursor: 'pointer' } })}
          />,
        )}
    </div>
  )
}
