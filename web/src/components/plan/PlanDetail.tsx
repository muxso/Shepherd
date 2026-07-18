import { useEffect, useState, type CSSProperties } from 'react'
import { Button, Card, Col, Empty, Modal, Row, Select, Space, Table, Tag } from 'antd'
import { Input } from 'antd'
import { message, modal } from '../../feedback'
import EditDrawer from '../EditDrawer'
import { PlayCircleOutlined, FileMarkdownOutlined, LinkOutlined, ClockCircleOutlined } from '@ant-design/icons'
import { api, ApiError, type ApiCase, type PlanCase, type PlanStats } from '../../api'
import { outcomeColor } from '../tags'
import Donut from '../Donut'
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
  const [mdOpen, setMdOpen] = useState(false)
  const [md, setMd] = useState('')

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
  const openReport = async () => {
    try {
      setMd(await api.planReportMd(planId))
      setMdOpen(true)
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('plan.reportFail', '获取报告失败'))
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
        <Button icon={<FileMarkdownOutlined />} size="small" onClick={openReport}>{t('plan.mdReport', 'Markdown 报告')}</Button>
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
          { title: t('plan.caseName', '用例名'), dataIndex: 'name', ellipsis: true },
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
      <ReportMdModal open={mdOpen} name={name} md={md} onClose={() => setMdOpen(false)} />
    </div>
  )
}

// Markdown report modal, shared by the detail page and the report tab.
export function ReportMdModal({ open, name, md, onClose }: { open: boolean; name: string; md: string; onClose: () => void }) {
  const { t } = useI18n()
  return (
    <Modal
      title={`${t('plan.mdReport', 'Markdown 报告')} · ${name}`}
      open={open}
      onCancel={onClose}
      width={760}
      footer={<Button type="primary" onClick={onClose}>{t('plan.close', '关闭')}</Button>}
    >
      <pre style={{ background: 'var(--panel-2)', color: 'var(--text)', border: '1px solid var(--border-soft)', padding: 12, borderRadius: 6, maxHeight: 520, overflow: 'auto', fontSize: 12 }}>{md}</pre>
    </Modal>
  )
}

function LinkCaseModal({ open, planId, projectId, onClose, onLinked }: { open: boolean; planId: string; projectId: string; onClose: () => void; onLinked: () => void }) {
  const { t } = useI18n()
  const [cases, setCases] = useState<ApiCase[]>([])
  const [caseId, setCaseId] = useState('')
  const [saving, setSaving] = useState(false)
  useEffect(() => {
    if (open) api.projectCases(projectId).then((p) => setCases(p.items)).catch(() => setCases([]))
  }, [open, projectId])
  const link = async () => {
    const c = cases.find((x) => x.id === caseId)
    if (!c) return
    setSaving(true)
    try {
      await api.linkPlanCase(planId, c.id, c.name)
      message.success(t('plan.linked', '已挂入用例'))
      onLinked()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('plan.linkFail', '挂入失败'))
    } finally {
      setSaving(false)
    }
  }
  return (
    <EditDrawer title={t('plan.linkApiCase', '挂入接口用例')} open={open} onCancel={onClose} onOk={link} confirmLoading={saving} okButtonProps={{ disabled: !caseId }}>
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
    </EditDrawer>
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
