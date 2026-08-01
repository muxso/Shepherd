import { useEffect, useState } from 'react'
import { useParams } from 'react-router-dom'
import { Empty, Spin } from 'antd'
import { api, type ScenarioReportDetail } from '../api'
import { MarkdownRenderer } from '../components/MarkdownRenderer'
import AnimatedLogo from '../components/AnimatedLogo'
import { ScenarioReportBody } from '../components/ScenarioReport'
import { useI18n } from '../i18n'

/**
 * Standalone public report page (no app shell, no login). Rendered for `/share/:kind/:token`
 * before the auth gate. Reads a shared report anonymously via the public endpoints, and renders
 * it with the very same components the in-app views use (ScenarioReportBody / MarkdownRenderer).
 */
export default function PublicReport() {
  const { kind, token } = useParams()
  const { t } = useI18n()
  const [loading, setLoading] = useState(true)
  const [notFound, setNotFound] = useState(false)
  const [scenario, setScenario] = useState<ScenarioReportDetail | null>(null)
  const [planMd, setPlanMd] = useState<string>('')

  useEffect(() => {
    if (!token) return
    setLoading(true)
    setNotFound(false)
    const p =
      kind === 'scenario'
        ? api.publicScenarioReport(token).then(setScenario)
        : kind === 'plan'
          ? api.publicPlanReportMd(token).then(setPlanMd)
          : Promise.reject(new Error('unknown kind'))
    p.catch(() => setNotFound(true)).finally(() => setLoading(false))
  }, [kind, token])

  return (
    <div style={{ minHeight: '100vh', background: 'var(--bg-base)' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '12px 20px', borderBottom: '1px solid var(--border-soft)', background: 'var(--panel)' }}>
        <AnimatedLogo size={26} />
        <span style={{ fontWeight: 700 }}>Shepherd</span>
        <span style={{ color: 'var(--text-3)', fontSize: 13 }}>· {t('public.sharedReport', '分享的报告')}</span>
      </div>
      <div style={{ maxWidth: 1180, margin: '0 auto', padding: '20px', background: 'var(--panel-2)', minHeight: 'calc(100vh - 52px)' }}>
        {loading ? (
          <div style={{ display: 'flex', justifyContent: 'center', paddingTop: 100 }}><Spin size="large" /></div>
        ) : notFound ? (
          <Empty style={{ paddingTop: 80 }} description={t('public.notFound', '分享链接无效或已失效')} />
        ) : kind === 'scenario' && scenario ? (
          <ScenarioReportBody
            data={scenario}
            scenarioId={scenario.scenarioId}
            nameOf={(id) => id}
            fetchScenario={token ? (id) => api.publicScenario(token, id) : undefined}
          />
        ) : kind === 'plan' ? (
          <MarkdownRenderer value={planMd} />
        ) : (
          <Empty style={{ paddingTop: 80 }} description={t('public.notFound', '分享链接无效或已失效')} />
        )}
      </div>
    </div>
  )
}
