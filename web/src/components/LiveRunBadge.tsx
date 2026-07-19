import { useEffect, useState } from 'react'
import { fmtDuration } from './ScenarioReport'

/** Count-up duration for a run/step that is executing right now (live WS mode). */
export function LiveElapsed({ since }: { since: number }) {
  const [, tick] = useState(0)
  useEffect(() => {
    const id = window.setInterval(() => tick((n) => n + 1), 100)
    return () => window.clearInterval(id)
  }, [])
  return <span className="ms-mono" style={{ fontSize: 12, color: 'var(--brand)', whiteSpace: 'nowrap' }}>{fmtDuration(Math.max(Date.now() - since, 0))}</span>
}

/** Inline live-run pill: looping brand sweep (.ms-step-fillbg.live) + label + count-up elapsed. */
export function LiveRunBadge({ label, since }: { label: string; since: number }) {
  return (
    <span
      style={{
        position: 'relative',
        isolation: 'isolate', // own stacking context: the z-index:-1 sweep stays under the text
        overflow: 'hidden',
        display: 'inline-flex',
        alignItems: 'center',
        gap: 6,
        padding: '1px 8px',
        borderRadius: 4,
        border: '1px solid var(--brand)',
        fontSize: 12,
        color: 'var(--brand)',
        whiteSpace: 'nowrap',
      }}
    >
      <span className="ms-step-fillbg live" />
      <span>{label}</span>
      <LiveElapsed since={since} />
    </span>
  )
}
