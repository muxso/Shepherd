import { useEffect, useState, type CSSProperties } from 'react'
import { Button, Card, Col, Empty, Row, Segmented, Select, Space, Table, Tag } from 'antd'
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

// Plan detail (workspace tab content): attach cases / run / record results / schedule / Markdown report + analytics cards.
export default function PlanDetail({ planId, name, projectId }: { planId: string; name: string; projectId: string }) {
  const { t } = useI18n()
  const [stats, setStats] = useState<PlanStats | null>(null)
  const [cases, setCases] = useState<PlanCase[]>([])
  const [loading, setLoading] = useState(false)
  const [linkOpen, setLinkOpen] = useState(false)
  const [running, setRunning] = useState(false)
  const [marking, setMarking] = useState('')
  const [reportOpen, setReportOpen] = useState(false)
  // Project scenario ids: mark scenario-mounted plan cases with a tag.
  const [scenarioIds, setScenarioIds] = useState<Set<string>>(new Set())
  useEffect(() => {
    api.scenarios(projectId)
      .then((ss) => setScenarioIds(new Set((Array.isArray(ss) ? ss : []).map((s) => s.id))))
      .catch(() => setScenarioIds(new Set()))
  }, [projectId])

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
  const markResult = async (caseId: string, status: string) => {
    setMarking(caseId)
    try {
      await api.recordPlanCaseResult(planId, caseId, status)
      message.success(t('plan.markDone', '已登记执行结果'))
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? `${t('plan.markFail', '登记失败')}:${e.status}` : t('plan.markFail', '登记失败'))
    } finally {
      setMarking('')
    }
  }
  const schedule = () => {
    let cron = '0 0 * * * *'
    modal.confirm({
      title: t('plan.scheduleTitle', '配置定时执行(cron)'),
      content: <Input defaultValue={cron} onChange={(e) => (cron = e.target.value)} style={{ marginTop: 8 }} className="ms-mono" />,
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

  return (
    <div style={{ padding: '12px 16px', height: '100%', overflow: 'auto' }}>
      <Space style={{ marginBottom: 12 }} wrap>
        <Button icon={<LinkOutlined />} size="small" onClick={() => setLinkOpen(true)}>{t('plan.linkCase', '挂用例')}</Button>
        <Button type="primary" icon={<PlayCircleOutlined />} size="small" loading={running} onClick={run}>{t('plan.runPlan', '执行计划')}</Button>
        <Button icon={<ClockCircleOutlined />} size="small" onClick={schedule}>{t('plan.schedule', '定时')}</Button>
        <Button icon={<FileTextOutlined />} size="small" onClick={() => setReportOpen(true)}>{t('plan.viewReport', '查看报告')}</Button>
        {stats && <Tag color={stats.isPass ? 'green' : 'red'}>{stats.isPass ? t('plan.pass', '通过') : t('plan.notPass', '未通过')}</Tag>}
      </Space>
      <ReportAnalytics stats={stats} cases={cases} />
      <div style={{ height: 16 }} />
      <Table<PlanCase>
        rowKey="caseId"
        size="small"
        loading={loading}
        dataSource={cases}
        pagination={false}
        locale={{ emptyText: <Empty description={t('plan.noLinkedCase', '未挂用例,点「挂用例」')} /> }}
        columns={[
          {
            title: t('plan.caseName', '用例名'),
            dataIndex: 'name',
            ellipsis: true,
            render: (v: string, c) => (
              <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6, minWidth: 0 }}>
                {scenarioIds.has(c.caseId) && <Tag color="processing" style={{ marginInlineEnd: 0 }}>{t('plan.scenarioTag', '场景')}</Tag>}
                <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{v}</span>
              </span>
            ),
          },
          { title: t('plan.colResult', '结果'), dataIndex: 'status', width: 110, render: (s: string) => <Tag color={outcomeColor(s)}>{caseStatusLabel(s, t)}</Tag> },
          { title: t('plan.colLatency', '耗时(ms)'), dataIndex: 'latencyMs', width: 100, render: (v?: number | null) => v ?? '—' },
          { title: t('plan.colStatusCode', '状态码'), dataIndex: 'statusCode', width: 90, render: (v?: number | null) => v ?? '—' },
          {
            title: t('plan.colAction', '操作'),
            width: 130,
            render: (_, c) => (
              <Select
                size="small"
                variant="borderless"
                style={{ width: 110 }}
                placeholder={t('plan.markResult', '登记结果')}
                value={undefined}
                disabled={marking === c.caseId}
                onChange={(s) => s && markResult(c.caseId, s)}
                options={CASE_RESULT_OPTIONS.map((o) => ({ value: o.value, label: t(o.i18nKey, o.fallback) }))}
              />
            ),
          },
        ]}
      />
      <LinkCaseModal open={linkOpen} planId={planId} projectId={projectId} onClose={() => setLinkOpen(false)} onLinked={() => { setLinkOpen(false); load() }} />
      <PlanReportDrawer open={reportOpen} planId={planId} name={name} projectId={projectId} onClose={() => setReportOpen(false)} />
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
            <Tag color={outcomeColor(s.status)} style={{ margin: 0 }}>{caseStatusLabel(s.status, t)}</Tag>
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
                    <Tag color={outcomeColor(c.status)} style={{ margin: 0 }}>{caseStatusLabel(c.status, t)}</Tag>
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

// Plan report analytics cards: report analysis + execution donut + case status bars.
// Status distribution is aggregated client-side from planCases (SUCCESS/ERROR/FAKE_ERROR/BLOCK/PENDING).
function ReportAnalytics({ stats, cases }: { stats: PlanStats | null; cases: PlanCase[] }) {
  const { t } = useI18n()
  const by = (s: string) => cases.filter((c) => (c.status || 'PENDING').toUpperCase() === s).length
  const success = by('SUCCESS')
  const error = by('ERROR')
  const fake = by('FAKE_ERROR')
  const block = by('BLOCK')
  const pending = by('PENDING')
  const total = cases.length || stats?.total || 0
  const pct = (n: number) => (total ? ((n * 100) / total).toFixed(2) : '0.00')
  const segs = [
    { label: t('plan.segSuccess', '成功'), value: success, color: '#2e7d32' },
    { label: t('plan.segError', '失败'), value: error, color: '#c62828' },
    { label: t('plan.segFake', '误报'), value: fake, color: '#ef6c00' },
    { label: t('plan.segBlock', '阻塞'), value: block, color: '#722ed1' },
    { label: t('plan.segPending', '未执行'), value: pending, color: 'var(--text-3)' },
  ]
  return (
    <Row gutter={16}>
      <Col span={12}>
        <Card size="small" title={t('plan.reportAnalysis', '报告分析')}>
          <div style={rowStyle}><span>{t('plan.colPassRate', '通过率')}</span><b style={{ color: '#2e7d32' }}>{((stats?.passRate ?? 0) * 100).toFixed(2)}%</b></div>
          <div style={rowStyle}><span>{t('plan.executeRate', '执行完成率')}</span><b>{((stats?.executeRate ?? 0) * 100).toFixed(2)}%</b></div>
          <div style={rowStyle}><span>{t('plan.totalCases', '用例总数')}</span><b>{total}</b></div>
          <div style={rowStyle}><span>{t('plan.conclusion', '结论')}</span><b style={{ color: stats?.isPass ? '#2e7d32' : '#c62828' }}>{stats?.isPass ? t('plan.pass', '通过') : t('plan.notPass', '未通过')}</b></div>
        </Card>
      </Col>
      <Col span={12}>
        <Card size="small" title={t('plan.execAnalysis', '执行分析')}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 16 }}>
            <Donut segments={segs} />
            <div style={{ flex: 1 }}>
              {segs.map((s) => (
                <div key={s.label} style={rowStyle}>
                  <span style={{ color: s.color }}>● {s.label}</span>
                  <b>{s.value}　{pct(s.value)}%</b>
                </div>
              ))}
            </div>
          </div>
        </Card>
      </Col>
      <Col span={24} style={{ marginTop: 16 }}>
        <Card size="small" title={t('plan.statusDist', '用例状态分布')}>
          {segs.map((s) => (
            <div key={s.label} style={{ display: 'flex', alignItems: 'center', gap: 12, margin: '6px 0' }}>
              <span style={{ width: 48, color: s.color }}>{s.label}</span>
              <div style={{ flex: 1, background: 'var(--bg)', borderRadius: 4, height: 10, overflow: 'hidden' }}>
                <div style={{ width: `${pct(s.value)}%`, background: s.color, height: '100%' }} />
              </div>
              <span style={{ width: 90, textAlign: 'right', color: 'var(--text-2)' }}>{s.value}　{pct(s.value)}%</span>
            </div>
          ))}
        </Card>
      </Col>
    </Row>
  )
}

const rowStyle: CSSProperties = { display: 'flex', justifyContent: 'space-between', padding: '6px 0', fontSize: 14 }

// Manually recordable execution results: pass / fail / blocked / false alarm.
const CASE_RESULT_OPTIONS = [
  { value: 'SUCCESS', i18nKey: 'plan.resPass', fallback: '通过' },
  { value: 'ERROR', i18nKey: 'plan.resFail', fallback: '不通过' },
  { value: 'BLOCK', i18nKey: 'plan.resBlock', fallback: '阻塞' },
  { value: 'FAKE_ERROR', i18nKey: 'plan.resFake', fallback: '误报' },
]

// Case status code → localized label.
function caseStatusLabel(s: string, t: (k: string, d: string) => string): string {
  switch ((s || 'PENDING').toUpperCase()) {
    case 'SUCCESS': return t('plan.resPass', '通过')
    case 'ERROR': return t('plan.resFail', '不通过')
    case 'BLOCK': return t('plan.resBlock', '阻塞')
    case 'FAKE_ERROR': return t('plan.resFake', '误报')
    case 'PENDING': return t('plan.segPending', '未执行')
    default: return s
  }
}
