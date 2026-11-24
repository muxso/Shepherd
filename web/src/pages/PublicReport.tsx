import { useEffect, useState } from 'react'
import { useParams, useSearchParams } from 'react-router-dom'
import { Empty } from 'antd'
import { PageSkeleton } from '../components/Page'
import { api, ApiError, type ScenarioReportDetail } from '../api'
import { MarkdownRenderer } from '../components/MarkdownRenderer'
import AnimatedLogo from '../components/AnimatedLogo'
import { ScenarioReportBody } from '../components/ScenarioReport'
import { useI18n } from '../i18n'

type SharedReport = { kind: 'scenario'; data: ScenarioReportDetail } | { kind: 'plan'; markdown: string }

async function loadReport(kind: string | undefined, token: string): Promise<SharedReport> {
  if (kind === 'scenario' || kind === 'report') {
    try {
      return { kind: 'scenario', data: await api.publicScenarioReport(token) }
    } catch (error) {
      if (kind !== 'report' || !(error instanceof ApiError) || error.status !== 404) throw error
    }
  } else if (kind !== 'plan') {
    throw new Error('Unknown report type')
  }
  return { kind: 'plan', markdown: await api.publicPlanReportMd(token) }
}

export default function PublicReport() {
  const { kind, token } = useParams()
  const [params] = useSearchParams()
  const internal = kind === 'report' && params.get('access') === 'project'
  const plan = internal && params.get('type') === 'plan'
  const { t } = useI18n()
  const [report, setReport] = useState<SharedReport | null>(null)
  const [loading, setLoading] = useState(Boolean(token))

  useEffect(() => {
    let active = true
    setReport(null)
    setLoading(Boolean(token))
    if (token) {
      const request: Promise<SharedReport> = !internal
        ? loadReport(kind, token)
        : plan
          ? api.planReportMd(token).then((markdown) => ({ kind: 'plan', markdown }))
          : api.scenarioReport(token).then((data) => ({ kind: 'scenario', data }))
      request
        .then((result) => { if (active) setReport(result) })
        .catch(() => { if (active) setReport(null) })
        .finally(() => { if (active) setLoading(false) })
    }
    return () => { active = false }
  }, [kind, token, internal, plan])

  return (
    <div style={{ minHeight: '100vh', background: 'var(--bg-base)' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '12px 20px', borderBottom: '1px solid var(--border-soft)', background: 'var(--panel)' }}>
        <AnimatedLogo size={26} />
        <span style={{ fontWeight: 700 }}>Shepherd</span>
        <span style={{ color: 'var(--text-3)', fontSize: 13 }}>· {internal ? t('public.executionReport', '测试报告') : t('public.sharedReport', '分享的报告')}</span>
      </div>
      <div style={{ maxWidth: 1180, margin: '0 auto', padding: '20px', background: 'var(--panel-2)', minHeight: 'calc(100vh - 52px)' }}>
        {loading ? (
          <PageSkeleton />
        ) : report?.kind === 'scenario' ? (
          <ScenarioReportBody
            data={report.data}
            scenarioId={report.data.scenarioId}
            nameOf={(id) => id}
            fetchScenario={token && !internal ? (id) => api.publicScenario(token, id) : undefined}
          />
        ) : report?.kind === 'plan' ? (
          <section style={{ padding: '16px 20px', background: 'var(--panel)', border: '1px solid var(--border-soft)', borderRadius: 8 }}>
            <MarkdownRenderer value={report.markdown} />
          </section>
        ) : (
          <Empty style={{ paddingTop: 80 }} description={internal ? t('public.reportUnavailable', '无法读取测试报告') : t('public.notFound', '分享链接无效或已失效')} />
        )}
      </div>
    </div>
  )
}
