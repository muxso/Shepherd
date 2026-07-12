import { useEffect, useState, type ReactNode } from 'react'
import { Card, Spin, Tag } from 'antd'
import { AuditOutlined, BugOutlined, CheckCircleOutlined, ScheduleOutlined } from '@ant-design/icons'
import { useNavigate } from 'react-router-dom'
import { api, type Bug, type PlanStats, type Requirement } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'
import { PageBody, PageContainer, PageHeader, SelectProjectEmpty } from '../components/Page'
import { regList, type RegItem } from '../registry'

// Todos = items awaiting the current user in this project, grouped by source:
//   requirements pending review (DRAFT or in AUDIT/REVIEW stage) → /requirement
//   pending bugs (NEW / REOPENED) → /bug
//   in-progress test plans (started but not fully passed) → /test-plan?open=<id>
//   requirements pending acceptance (currentStage=ACCEPTANCE; delivery side has no
//   project-level pending-acceptance list API, so use the requirement stage) → /requirement
// Read-only aggregation over existing APIs, no new endpoints; sections fetch
// independently so one failure doesn't break the page.

interface TodoRow {
  id: string
  title: string
  tag?: ReactNode
}

function Section({ icon, title, count, rows, emptyText, onClick }: {
  icon: ReactNode
  title: string
  count: number
  rows: TodoRow[]
  emptyText: string
  onClick: (row: TodoRow) => void
}) {
  return (
    <Card
      size="small"
      title={
        <span style={{ display: 'inline-flex', alignItems: 'center', gap: 8 }}>
          <span style={{ color: 'var(--brand)' }}>{icon}</span>
          <span>{title}</span>
          <span
            style={{
              minWidth: 20,
              padding: '0 6px',
              borderRadius: 10,
              fontSize: 12,
              lineHeight: '18px',
              textAlign: 'center',
              background: count ? 'var(--brand-soft)' : 'var(--border-soft)',
              color: count ? 'var(--brand)' : 'var(--text-3)',
            }}
          >
            {count}
          </span>
        </span>
      }
      styles={{ body: { padding: rows.length ? '4px 0' : '16px' } }}
    >
      {rows.length === 0 ? (
        <span style={{ color: 'var(--text-3)', fontSize: 13 }}>{emptyText}</span>
      ) : (
        rows.map((r) => (
          <div
            key={r.id}
            onClick={() => onClick(r)}
            style={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              gap: 12,
              padding: '8px 16px',
              cursor: 'pointer',
              borderRadius: 6,
              color: 'var(--text)',
            }}
            onMouseEnter={(e) => { e.currentTarget.style.background = 'var(--brand-soft)' }}
            onMouseLeave={(e) => { e.currentTarget.style.background = 'transparent' }}
          >
            <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{r.title}</span>
            {r.tag}
          </div>
        ))
      )}
    </Card>
  )
}

export default function Todos() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const nav = useNavigate()
  const [loading, setLoading] = useState(false)
  const [reqs, setReqs] = useState<Requirement[]>([])
  const [bugs, setBugs] = useState<Bug[]>([])
  const [plans, setPlans] = useState<RegItem[]>([])
  const [statsMap, setStatsMap] = useState<Record<string, PlanStats>>({})

  useEffect(() => {
    if (!projectId) return
    setLoading(true)
    const planList = regList('plan', projectId)
    setPlans(planList)
    Promise.all([
      api.requirements(projectId).then((p) => setReqs(p.items || [])).catch(() => setReqs([])),
      api.bugs(projectId).then(setBugs).catch(() => setBugs([])),
      Promise.all(
        planList.map((p) => api.planStats(p.id).then((s) => [p.id, s] as const).catch(() => null)),
      ).then((entries) => setStatsMap(Object.fromEntries(entries.filter(Boolean) as [string, PlanStats][]))),
    ]).finally(() => setLoading(false))
  }, [projectId])

  if (!projectId) return <SelectProjectEmpty />

  // Chinese stage-label fallback for this view only; canonical labels live in the requirement page.
  const stageZh: Record<string, string> = { AUDIT: '审核', REVIEW: '评审', ACCEPTANCE: '验收' }
  const stageTag = (r: Requirement) => {
    const s = r.currentStage || ''
    return (
      <Tag color={r.status === 'DRAFT' ? 'default' : 'blue'} style={{ marginInlineEnd: 0 }}>
        {r.status === 'DRAFT' && !stageZh[s] ? t('home.todo.reqDraft', '草稿') : t(`home.todo.stage.${s}`, stageZh[s] || s)}
      </Tag>
    )
  }

  // Pending review: drafts (not yet baselined) or stuck in AUDIT/REVIEW stage.
  const reviewReqs = reqs.filter(
    (r) => r.status === 'DRAFT' || r.currentStage === 'AUDIT' || r.currentStage === 'REVIEW',
  )
  // Pending acceptance: requirement ACCEPTANCE stage (delivery/task side has no project-level list API).
  const acceptReqs = reqs.filter((r) => r.currentStage === 'ACCEPTANCE')
  const pendingBugs = bugs.filter((b) => ['NEW', 'REOPENED'].includes((b.status || '').toUpperCase()))
  // In-progress plans: started but not fully passed (same definition as the test-plan page).
  const runningPlans = plans.filter((p) => {
    const s = statsMap[p.id]
    return !!s && s.executeRate > 0 && !s.isPass
  })

  const total = reviewReqs.length + pendingBugs.length + runningPlans.length + acceptReqs.length

  const bugZh: Record<string, string> = { NEW: '新建', REOPENED: '重新打开' }

  return (
    <PageContainer>
      <PageHeader
        title={t('home.todo.title', '待办')}
        subtitle={
          loading
            ? t('home.todo.loading', '统计中…')
            : t('home.todo.total', '共 {n} 项待办').replace('{n}', String(total))
        }
      />
      <PageBody>
        <Spin spinning={loading}>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(360px, 1fr))', gap: 16 }}>
            <Section
              icon={<AuditOutlined />}
              title={t('home.todo.reviewReq', '待评审需求')}
              count={reviewReqs.length}
              emptyText={t('home.todo.empty', '暂无')}
              rows={reviewReqs.map((r) => ({ id: r.id, title: r.title, tag: stageTag(r) }))}
              onClick={() => nav('/requirement')}
            />
            <Section
              icon={<BugOutlined />}
              title={t('home.todo.pendingBug', '待处理缺陷')}
              count={pendingBugs.length}
              emptyText={t('home.todo.empty', '暂无')}
              rows={pendingBugs.map((b) => ({
                id: b.id,
                title: b.title || b.id,
                tag: (
                  <Tag color="orange" style={{ marginInlineEnd: 0 }}>
                    {t(`bug.st.${b.status}`, bugZh[b.status] || b.status)}
                  </Tag>
                ),
              }))}
              onClick={() => nav('/bug')}
            />
            <Section
              icon={<ScheduleOutlined />}
              title={t('home.todo.runningPlan', '进行中的测试计划')}
              count={runningPlans.length}
              emptyText={t('home.todo.empty', '暂无')}
              rows={runningPlans.map((p) => {
                const s = statsMap[p.id]
                return {
                  id: p.id,
                  title: p.label,
                  tag: (
                    <Tag color="blue" style={{ marginInlineEnd: 0 }}>
                      {t('home.todo.executeRate', '执行率')} {Math.round((s?.executeRate ?? 0) * 100)}%
                    </Tag>
                  ),
                }
              })}
              onClick={(row) => nav(`/test-plan?open=${encodeURIComponent(row.id)}`)}
            />
            <Section
              icon={<CheckCircleOutlined />}
              title={t('home.todo.acceptReq', '待验收需求')}
              count={acceptReqs.length}
              emptyText={t('home.todo.empty', '暂无')}
              rows={acceptReqs.map((r) => ({ id: r.id, title: r.title, tag: stageTag(r) }))}
              onClick={() => nav('/requirement')}
            />
          </div>
        </Spin>
      </PageBody>
    </PageContainer>
  )
}
