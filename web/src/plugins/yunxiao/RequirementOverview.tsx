import { useI18n } from '../../i18n'
import { lazy, Suspense, useEffect, useState } from 'react'
import { Alert, Spin } from 'antd'
import { activityApi, type TeamActivityData } from './api'
import './team-activity.css'
const Analysis = lazy(() => import('./RequirementActivityAnalysis'))
export default function RequirementOverview({ sourceId, days, revision, openRequirement }: { sourceId: string; days: number; revision: number; openRequirement: (id: string) => void }) {
  const { t } = useI18n()
  const [result, setResult] = useState<{ key: string; data: TeamActivityData } | null>(null), [loading, setLoading] = useState(false), [error, setError] = useState('')
  const key = `${sourceId}:${days}:${revision}`, data = result?.key === key ? result.data : null
  useEffect(() => {
    let live = true; setLoading(true); setError('')
    activityApi.activity(sourceId, 'code', days || 365).then(data => { if (live) setResult({ key, data }) }).catch(e => { if (live) setError(String(e.message || e)) }).finally(() => { if (live) setLoading(false) })
    return () => { live = false }
  }, [key, sourceId, days])
  return <div aria-label={t("engineering.requirementDeliveryOverview")}>{error && <Alert type="error" showIcon title={t("engineering.couldNotLoadTheRequirementOverview")} description={error} />}<Spin spinning={loading}>{data && <Suspense fallback={<Spin />}><Analysis data={data} openRequirement={openRequirement} tableWithChart={false} /></Suspense>}</Spin></div>
}
