import { useEffect, useMemo, useState } from 'react'
import { api, type ExecutedOn, type PoolRunnerInfo, type ResourcePool } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'

/** "pool/runner" toast suffix for a completed run; null = local execution. */
export function executedOnLabel(e?: ExecutedOn | null): string | null {
  if (!e) return null
  return e.runner ? `${e.poolName}/${e.runner}` : e.poolName
}

/** Mirrors the server's auto pool pick: enabled pools applicable to the
 * project's organization with online runners, most spare capacity first.
 * Polls the detail endpoint every 5s while active. */
export function useAutoPool(projectId: string, active: boolean) {
  const { projects } = useApp()
  const [pools, setPools] = useState<ResourcePool[]>([])
  const [detail, setDetail] = useState<Record<string, PoolRunnerInfo[]>>({})
  useEffect(() => {
    if (!active) return
    api.resourcePools().then((p) => setPools(Array.isArray(p) ? p : [])).catch(() => setPools([]))
    const tick = () => api.poolRunnerStatusDetail().then(setDetail).catch(() => setDetail({}))
    tick()
    const timer = window.setInterval(tick, 5000)
    return () => window.clearInterval(timer)
  }, [active, projectId])
  return useMemo(() => {
    const orgId = projects.find((p) => p.id === projectId)?.organizationId
    const spare = (id: string) =>
      (detail[id] || []).reduce((s, r) => s + Math.max(0, r.maxConcurrent - r.inFlight), 0)
    const candidates = pools.filter(
      (p) =>
        p.enabled !== false &&
        (p.allOrg || (orgId ? (p.orgIds || []).includes(orgId) : false)) &&
        (detail[p.id]?.length ?? 0) > 0,
    )
    candidates.sort((a, b) => spare(b.id) - spare(a.id))
    const pick = candidates[0]
    return pick ? { pool: pick, online: detail[pick.id]?.length ?? 0 } : null
  }, [pools, detail, projects, projectId])
}

/** Passive run-target indicator: 自动: <pool> · N在线, or 本地执行. */
export default function AutoPoolIndicator({ projectId, active }: { projectId: string; active?: boolean }) {
  const { t } = useI18n()
  const pick = useAutoPool(projectId, active !== false)
  return (
    <span
      style={{ fontSize: 12, color: 'var(--text-3)', whiteSpace: 'nowrap' }}
      title={t('scenario.autoPoolHint', '有在线执行机时自动派发到资源池,否则本地执行')}
    >
      {pick
        ? `${t('scenario.autoPrefix', '自动')}: ${pick.pool.name} · ${pick.online} ${t('scenario.runnersOnline', '在线')}`
        : t('scenario.localExec', '本地执行')}
    </span>
  )
}
