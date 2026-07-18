import { useEffect, useState } from 'react'
import { Button, Empty, Segmented, Select, Space, Tabs, Tag } from 'antd'
import { Input } from 'antd'
import { message, modal } from '../../feedback'
import EditDrawer from '../EditDrawer'
import ResizableDrawer from '../ResizableDrawer'
import { PlayCircleOutlined, FileMarkdownOutlined, FileTextOutlined, LinkOutlined, ClockCircleOutlined } from '@ant-design/icons'
import { api, ApiError, type ApiCase, type PlanCase, type PlanStats, type PlanStepResult, type Scenario } from '../../api'
import { outcomeColor } from '../tags'
import Donut from '../Donut'
import { fmtDurationMs } from '../TimingBreakdown'
import { ScenarioReportModal } from '../ScenarioReport'
import { useI18n } from '../../i18n'
import PlanMindmap from './PlanMindmap'
import PlanEditDrawer from './PlanEditDrawer'
import PlanDetailHeader, { PlanRunsTable } from './PlanDetailHeader'
import PlanCasesPanel, { planCaseStatusLabel } from './PlanCasesPanel'

// Plan detail (workspace tab content): header + tabs (测试规划 mind-map | 场景用例 | 缺陷列表 | 执行历史).
export default function PlanDetail({ planId, name, projectId }: { planId: string; name: string; projectId: string }) {
  const { t } = useI18n()
  const [stats, setStats] = useState<PlanStats | null>(null)
  const [cases, setCases] = useState<PlanCase[]>([])
  const [loading, setLoading] = useState(false)
  const [linkOpen, setLinkOpen] = useState(false)
  const [running, setRunning] = useState(false)
  const [reportOpen, setReportOpen] = useState(false)
  const [editOpen, setEditOpen] = useState(false)
  const [planName, setPlanName] = useState(name)

  const load = async () => {
    setLoading(true)
    try {
      const [s, c] = await Promise.all([api.planStats(planId), api.planCases(planId)])
      setStats(s)
      setCases(Array.isArray(c) ? c : c.items)
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('plan.loadFail', '加载计划失败'))
    } finally {
      setLoading(false)
    }
  }
  useEffect(() => {
    load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [planId])

  const run = async () => {
    setRunning(true)
    try {
      const r = await api.runPlan(planId)
      message.success(`${t('plan.runDone', '执行完成')}:${r.executed}/${r.total}`)
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? `${t('plan.runFail', '执行失败')}:${e.status}` : t('plan.runFail', '执行失败'))
    } finally {
      setRunning(false)
    }
  }
  const schedule = () => {
    let cron = '0 0 * * * *'
    const dlg = modal.confirm({
      title: t('plan.scheduleTitle', '配置定时执行(cron)'),
      content: (
        <div style={{ marginTop: 8 }}>
          <Input defaultValue={cron} onChange={(e) => (cron = e.target.value)} className="ms-mono" />
          <Button
            danger
            type="link"
            size="small"
            style={{ marginTop: 8, paddingLeft: 0 }}
            onClick={async () => {
              try {
                await api.deletePlanSchedule(planId)
                message.success(t('plan.scheduleRemoved', '定时已移除'))
              } catch (e) {
                if (e instanceof ApiError && e.status === 404) message.info(t('plan.scheduleNone', '该计划没有定时任务'))
                else message.error(e instanceof ApiError ? e.message : t('plan.scheduleFail', '配置失败'))
              }
              dlg.destroy()
            }}
          >
            {t('plan.scheduleRemove', '移除定时')}
          </Button>
        </div>
      ),
      onOk: async () => {
        try {
          await api.planSchedule(planId, cron)
          message.success(t('plan.scheduleDone', '定时已配置'))
        } catch (e) {
          message.error(e instanceof ApiError ? e.message : t('plan.scheduleFail', '配置失败'))
        }
      },
    })
  }

  // 场景用例 tab body: left 测试点/模块 tree + enriched case table (reference layout);
  // the original action buttons stay in the panel toolbar.
  const casesTab = (
    <PlanCasesPanel
      planId={planId}
      projectId={projectId}
      cases={cases}
      loading={loading}
      reload={load}
      toolbar={
        <Space size={8} wrap>
          <Button icon={<LinkOutlined />} size="small" onClick={() => setLinkOpen(true)}>{t('plan.linkCase', '挂用例')}</Button>
          <Button type="primary" icon={<PlayCircleOutlined />} size="small" loading={running} onClick={run}>{t('plan.runPlan', '执行计划')}</Button>
          <Button icon={<ClockCircleOutlined />} size="small" onClick={schedule}>{t('plan.schedule', '定时')}</Button>
          <Button icon={<FileTextOutlined />} size="small" onClick={() => setReportOpen(true)}>{t('plan.viewReport', '查看报告')}</Button>
          {stats && <Tag color={stats.isPass ? 'green' : 'red'}>{stats.isPass ? t('plan.pass', '通过') : t('plan.notPass', '未通过')}</Tag>}
        </Space>
      }
    />
  )

  return (
    <div style={{ height: '100%', display: 'flex', flexDirection: 'column' }}>
      <PlanDetailHeader
        planId={planId}
        projectId={projectId}
        name={planName}
        stats={stats}
        onEdit={() => setEditOpen(true)}
        onReport={() => setReportOpen(true)}
        onSchedule={schedule}
        onRefresh={load}
      />
      <Tabs
        className="ms-detail-tabs ms-fill-tabs"
        tabBarStyle={{ margin: 0, paddingInline: 16 }}
        style={{ flex: 1, minHeight: 0 }}
        items={[
          { key: 'planning', label: t('plan.tabPlanning', '测试规划'), children: <PlanMindmap planId={planId} projectId={projectId} /> },
          { key: 'cases', label: `${t('plan.tabCases', '场景用例')} (${cases.length})`, children: casesTab },
          {
            key: 'bugs',
            label: t('plan.tabBugs', '缺陷列表'),
            children: <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('plan.noBugs', '暂无关联缺陷')} style={{ marginTop: 48 }} />,
          },
          { key: 'history', label: t('plan.tabHistory', '执行历史'), children: <div style={{ padding: '12px 16px' }}><PlanRunsTable planId={planId} /></div> },
        ]}
      />
      <LinkCaseModal open={linkOpen} planId={planId} projectId={projectId} onClose={() => setLinkOpen(false)} onLinked={() => { setLinkOpen(false); load() }} />
      <PlanReportDrawer open={reportOpen} planId={planId} name={planName} projectId={projectId} onClose={() => setReportOpen(false)} />
      <PlanEditDrawer
        open={editOpen}
        planId={planId}
        projectId={projectId}
        onClose={() => setEditOpen(false)}
        onSaved={(d) => { setPlanName(d.name); load() }}
      />
    </div>
  )
}

// Link picker: mount an API case or a whole scenario onto the plan (both go
// through the same plan-case link; the runner tells them apart at execution).
function LinkCaseModal({ open, planId, projectId, onClose, onLinked }: { open: boolean; planId: string; projectId: string; onClose: () => void; onLinked: () => void }) {
  const { t } = useI18n()
  const [source, setSource] = useState<'case' | 'scenario'>('case')
  const [cases, setCases] = useState<ApiCase[]>([])
  const [scenarios, setScenarios] = useState<Scenario[]>([])
  const [caseId, setCaseId] = useState('')
  const [saving, setSaving] = useState(false)
  useEffect(() => {
    if (!open) return
    setSource('case')
    setCaseId('')
    api.projectCases(projectId).then((p) => setCases(p.items)).catch(() => setCases([]))
    api.scenarios(projectId).then((ss) => setScenarios(Array.isArray(ss) ? ss : [])).catch(() => setScenarios([]))
  }, [open, projectId])
  const link = async () => {
    const picked =
      source === 'case' ? cases.find((x) => x.id === caseId) : scenarios.find((x) => x.id === caseId)
    if (!picked) return
    setSaving(true)
    try {
      await api.linkPlanCase(planId, picked.id, picked.name)
      message.success(t('plan.linked', '已挂入用例'))
      onLinked()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('plan.linkFail', '挂入失败'))
    } finally {
      setSaving(false)
    }
  }
  return (
    <EditDrawer title={t('plan.linkCase', '挂用例')} open={open} onCancel={onClose} onOk={link} confirmLoading={saving} okButtonProps={{ disabled: !caseId }}>
      <Segmented
        value={source}
        onChange={(v) => { setSource(v as 'case' | 'scenario'); setCaseId('') }}
        options={[
          { value: 'case', label: t('plan.srcApiCase', '接口用例') },
          { value: 'scenario', label: t('plan.srcScenario', '场景') },
        ]}
        style={{ marginBottom: 12 }}
      />
      {source === 'case' ? (
        <Select
          style={{ width: '100%' }}
          showSearch
          placeholder={t('plan.selectApiCase', '选择项目下的接口用例')}
          value={caseId || undefined}
          onChange={setCaseId}
          optionFilterProp="label"
          options={cases.map((c) => ({ value: c.id, label: `${c.method} ${c.name}` }))}
          notFoundContent={t('plan.noApiCase', '项目暂无接口用例(先去「接口定义」建用例)')}
        />
      ) : (
        <Select
          style={{ width: '100%' }}
          showSearch
          placeholder={t('plan.selectScenario', '选择项目下的场景')}
          value={caseId || undefined}
          onChange={setCaseId}
          optionFilterProp="label"
          options={scenarios.map((s) => ({ value: s.id, label: `${s.num || s.id.slice(0, 8)} ${s.name}` }))}
          notFoundContent={t('plan.noScenario', '项目暂无场景(先去「接口场景」创建)')}
        />
      )}
    </EditDrawer>
  )
}

// Case status palette shared by the report drawer ring + legend.
const PLAN_STATUS_SEGS = (t: (k: string, d: string) => string) => [
  { key: 'SUCCESS', label: t('plan.resPass', '通过'), color: '#22c55e' },
  { key: 'ERROR', label: t('plan.resFail', '不通过'), color: '#ef4444' },
  { key: 'FAKE_ERROR', label: t('plan.resFake', '误报'), color: '#f59e0b' },
  { key: 'BLOCK', label: t('plan.resBlock', '阻塞'), color: '#722ed1' },
  { key: 'PENDING', label: t('plan.segPending', '未执行'), color: '#c9cdd4' },
]

// Nested step results of a scenario-mounted plan case (controllers render their children indented).
function PlanStepRows({ steps, depth, t }: { steps: PlanStepResult[]; depth: number; t: (k: string, d: string) => string }) {
  return (
    <>
      {steps.map((s, i) => (
        <div key={`${depth}-${i}`}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '6px 12px', marginLeft: depth * 20, border: '1px solid var(--border-soft)', borderRadius: 6, marginBottom: 4, background: 'var(--panel-2)', fontSize: 13 }}>
            <span style={{ color: 'var(--text-3)', fontSize: 12, minWidth: 16 }}>{i + 1}</span>
            <Tag style={{ margin: 0 }}>{s.kind}</Tag>
            <span className="ms-mono" style={{ flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{s.name}</span>
            {s.statusCode != null && <span style={{ color: s.statusCode < 400 ? 'var(--success)' : 'var(--error)', fontSize: 12 }}>{s.statusCode}</span>}
            <span style={{ color: 'var(--text-3)', fontSize: 12 }}>{fmtDurationMs(s.latencyMs)}</span>
            <Tag color={outcomeColor(s.status)} style={{ margin: 0 }}>{planCaseStatusLabel(s.status, t)}</Tag>
          </div>
          {s.children.length > 0 && <PlanStepRows steps={s.children} depth={depth + 1} t={t} />}
        </div>
      ))}
    </>
  )
}

// Plan report drawer: overview cards (analysis + case status ring) and a per-case
// detail list with expandable step results; Markdown export is the secondary action.
export function PlanReportDrawer({ open, planId, name, projectId, onClose }: { open: boolean; planId: string; name: string; projectId?: string; onClose: () => void }) {
  const { t } = useI18n()
  const [stats, setStats] = useState<PlanStats | null>(null)
  const [cases, setCases] = useState<PlanCase[]>([])
  const [loading, setLoading] = useState(false)
  const [openSet, setOpenSet] = useState<Set<string>>(new Set())
  // Scenario-mounted rows link to their scenario report; opened in the shared report modal.
  const [scnReport, setScnReport] = useState<{ reportId: string; scenarioId: string } | null>(null)
  const [caseMap, setCaseMap] = useState<Record<string, ApiCase>>({})
  const openScnReport = (c: PlanCase) => {
    if (!c.reportId) return
    // Case names resolve report rows; fetched once per drawer, id-slice fallback otherwise.
    if (projectId && !Object.keys(caseMap).length) {
      api.projectCasesAll(projectId)
        .then((cs) => setCaseMap(Object.fromEntries(cs.map((x) => [x.id, x]))))
        .catch(() => undefined)
    }
    setScnReport({ reportId: c.reportId, scenarioId: c.caseId })
  }
  const scnNameOf = (id: string) => caseMap[id]?.name || (id ? id.slice(0, 8) : '—')
  useEffect(() => {
    if (!open || !planId) return
    setLoading(true)
    setOpenSet(new Set())
    setScnReport(null)
    Promise.all([
      api.planStats(planId).catch(() => null),
      api.planCases(planId).then((c) => (Array.isArray(c) ? c : c.items)).catch(() => [] as PlanCase[]),
    ])
      .then(([s, c]) => { setStats(s); setCases(c) })
      .finally(() => setLoading(false))
  }, [open, planId])
  const exportMd = async () => {
    try {
      const md = await api.planReportMd(planId)
      const blob = new Blob([md], { type: 'text/markdown' })
      const url = URL.createObjectURL(blob)
      const a = document.createElement('a')
      a.href = url
      a.download = `${name || 'plan-report'}.md`
      a.click()
      URL.revokeObjectURL(url)
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('plan.reportFail', '获取报告失败'))
    }
  }
  const segs = PLAN_STATUS_SEGS(t)
  const countOf = (k: string) => cases.filter((c) => (c.status || 'PENDING').toUpperCase() === k).length
  const total = cases.length || stats?.total || 0
  const pct = (n: number) => (total ? ((n * 100) / total).toFixed(2) : '0.00')
  const totalMs = cases.reduce((s, c) => s + (c.latencyMs ?? 0), 0)
  const toggle = (id: string) => setOpenSet((prev) => { const n = new Set(prev); if (n.has(id)) n.delete(id); else n.add(id); return n })
  const statRow = (lbl: string, val: React.ReactNode, color?: string) => (
    <div style={{ display: 'flex', justifyContent: 'space-between', padding: '6px 0', fontSize: 14 }}>
      <span style={{ color: 'var(--text-2)' }}>{lbl}</span><b style={{ color }}>{val}</b>
    </div>
  )
  return (
    <ResizableDrawer
      open={open}
      onClose={onClose}
      width="60%"
      title={`${t('plan.reportTitle', '计划报告')} · ${name}`}
      styles={{ body: { background: 'var(--panel-2)' } }}
    >
      {loading ? (
        <div style={{ padding: 32, color: 'var(--text-3)' }}>{t('a.loading', '加载中…')}</div>
      ) : (
        <>
          <div style={{ display: 'flex', gap: 16, flexWrap: 'wrap', marginBottom: 20 }}>
            <div style={{ flex: 1, minWidth: 260, border: '1px solid var(--border-soft)', borderRadius: 10, padding: '14px 18px', background: 'var(--panel)' }}>
              <h3 style={{ margin: '0 0 10px', fontSize: 14, color: 'var(--text-2)', display: 'flex', alignItems: 'center', gap: 8 }}>
                {t('plan.reportAnalysis', '报告分析')}
                {stats && <Tag color={stats.isPass ? 'green' : 'red'} style={{ margin: 0 }}>{stats.isPass ? t('plan.pass', '通过') : t('plan.notPass', '未通过')}</Tag>}
              </h3>
              {statRow(t('plan.duration', '耗时'), fmtDurationMs(totalMs))}
              {statRow(t('plan.colPassRate', '通过率'), `${((stats?.passRate ?? 0) * 100).toFixed(2)} %`, (stats?.passRate ?? 0) >= 1 ? '#22c55e' : undefined)}
              {statRow(t('plan.executeRate', '执行完成率'), `${((stats?.executeRate ?? 0) * 100).toFixed(2)} %`)}
              {statRow(t('plan.totalCases', '用例总数'), total)}
            </div>
            <div style={{ flex: 1, minWidth: 300, border: '1px solid var(--border-soft)', borderRadius: 10, padding: '14px 18px', background: 'var(--panel)' }}>
              <h3 style={{ margin: '0 0 10px', fontSize: 14, color: 'var(--text-2)' }}>{t('plan.statusDist', '用例状态分布')}</h3>
              <div style={{ display: 'flex', alignItems: 'center', gap: 20 }}>
                <Donut segments={segs.map((s) => ({ label: s.label, value: countOf(s.key), color: s.color }))} size={116} thickness={14} centerLabel={t('plan.totalShort', '总数(个)')} />
                <div style={{ flex: 1 }}>
                  {segs.map((s) => (
                    <div key={s.key} style={{ display: 'grid', gridTemplateColumns: '1fr auto auto', alignItems: 'center', columnGap: 18, padding: '4px 0', fontSize: 13 }}>
                      <span style={{ color: 'var(--text-2)' }}><span style={{ color: s.color }}>●</span> {s.label}</span>
                      <b style={{ color: 'var(--text)' }}>{countOf(s.key)}</b>
                      <span style={{ color: 'var(--text-3)', minWidth: 56, textAlign: 'right' }}>{pct(countOf(s.key))}%</span>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 10 }}>
            <b>{t('plan.reportDetail', '报告明细')}</b>
            <div style={{ flex: 1 }} />
            <Button size="small" icon={<FileMarkdownOutlined />} onClick={exportMd}>{t('plan.exportMd', '导出 Markdown')}</Button>
          </div>
          {cases.length === 0 ? (
            <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('plan.noLinkedCase', '未挂用例,点「挂用例」')} />
          ) : (
            cases.map((c) => {
              // Rows with a scenario report id open the shared scenario report modal;
              // the inline steps expansion remains for legacy rows with stored steps.
              const hasReport = !!c.reportId
              const hasSteps = !hasReport && (c.steps?.length ?? 0) > 0
              const isOpen = openSet.has(c.caseId)
              return (
                <div key={c.caseId} style={{ border: '1px solid var(--border-soft)', borderRadius: 8, marginBottom: 8, background: 'var(--panel)' }}>
                  <div
                    onClick={() => { if (hasReport) openScnReport(c); else if (hasSteps) toggle(c.caseId) }}
                    style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '8px 12px', cursor: hasReport || hasSteps ? 'pointer' : 'default' }}
                  >
                    {hasSteps ? <span style={{ color: 'var(--text-3)', fontSize: 11, width: 12 }}>{isOpen ? '▾' : '▸'}</span> : <span style={{ width: 12 }} />}
                    <span style={{ flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                      {(hasReport || hasSteps) && <Tag color="processing">{t('plan.scenarioTag', '场景')}</Tag>}
                      {c.name}
                    </span>
                    {hasReport && (
                      <Button size="small" type="link" style={{ padding: 0, height: 'auto' }} onClick={(e) => { e.stopPropagation(); openScnReport(c) }}>
                        {t('plan.viewScenarioReport', '查看场景报告')}
                      </Button>
                    )}
                    {c.statusCode != null && <span style={{ color: c.statusCode < 400 ? 'var(--success)' : 'var(--error)', fontSize: 12 }}>{c.statusCode}</span>}
                    <span style={{ color: 'var(--text-3)', fontSize: 12 }}>{c.latencyMs != null ? fmtDurationMs(c.latencyMs) : '—'}</span>
                    <Tag color={outcomeColor(c.status)} style={{ margin: 0 }}>{planCaseStatusLabel(c.status, t)}</Tag>
                  </div>
                  {hasSteps && isOpen && (
                    <div style={{ padding: '0 12px 8px 34px' }}>
                      <PlanStepRows steps={c.steps!} depth={0} t={t} />
                    </div>
                  )}
                </div>
              )
            })
          )}
          <ScenarioReportModal
            reportId={scnReport?.reportId || null}
            scenarioId={scnReport?.scenarioId}
            nameOf={scnNameOf}
            caseMap={caseMap}
            onClose={() => setScnReport(null)}
          />
        </>
      )}
    </ResizableDrawer>
  )
}

