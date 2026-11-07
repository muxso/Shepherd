import { lazy, Suspense, useEffect, useState } from 'react'
import { Alert, Spin } from 'antd'
import { activityApi, type TeamActivityData } from './api'
import './team-activity.css'
const Analysis = lazy(() => import('./RequirementActivityAnalysis'))
export default function RequirementOverview({ sourceId, days, revision, openRequirement }: { sourceId: string; days: number; revision: number; openRequirement: (id: string) => void }) {
  const [result, setResult] = useState<{ key: string; data: TeamActivityData } | null>(null), [loading, setLoading] = useState(false), [error, setError] = useState('')
  const key = `${sourceId}:${days}:${revision}`, data = result?.key === key ? result.data : null
  useEffect(() => {
    let live = true; setLoading(true); setError('')
    activityApi.activity(sourceId, 'code', days || 365).then(data => { if (live) setResult({ key, data }) }).catch(e => { if (live) setError(String(e.message || e)) }).finally(() => { if (live) setLoading(false) })
    return () => { live = false }
  }, [key, sourceId, days])
  return <div aria-label="需求产出可视化总览">{error && <Alert type="error" showIcon title="需求总览读取失败" description={error} />}<Spin spinning={loading}>{data && <Suspense fallback={<Spin />}><Analysis data={data} openRequirement={openRequirement} tableWithChart={false} /></Suspense>}</Spin><p className="yx-management-notes">展示所选区间内有明确代码关联的需求，人员按已有账号归并识别，未识别作者的提交保留代码变动但不计人数；完整需求及产品、测试明细可通过上方搜索进入。需求未关联代码时，仍保留原始需求记录。</p></div>
}
