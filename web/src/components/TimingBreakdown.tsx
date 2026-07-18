import type { ReactNode } from 'react'
import { Popover } from 'antd'
import type { PhaseTimings } from '../api'
import { useI18n } from '../i18n'

/* Humanized duration: ms flips to seconds past 1s, minutes past 60s. */
export const fmtDurationMs = (ms: number) =>
  ms >= 60_000 ? `${(ms / 60_000).toFixed(1)} min` : ms >= 1000 ? `${(ms / 1000).toFixed(2)} s` : `${ms} ms`

/** Chrome-style timing waterfall for one HTTP exchange: DNS / TTFB / download. */
export function TimingBreakdown({ totalMs, timings }: { totalMs: number; timings?: PhaseTimings | null }) {
  const { t } = useI18n()
  const dns = timings?.dnsMs ?? null
  const ttfb = timings?.ttfbMs ?? null
  const dl = timings?.downloadMs ?? null
  const span = Math.max(totalMs, (ttfb ?? 0) + (dl ?? 0), 1)
  const rows: { label: string; ms: number; offset: number; color: string }[] = []
  if (dns != null) rows.push({ label: t('timing.dns', 'DNS 解析'), ms: dns, offset: 0, color: 'var(--warning, #ff7d00)' })
  if (ttfb != null) rows.push({ label: t('timing.ttfb', '等待响应 TTFB(含建连)'), ms: ttfb, offset: 0, color: 'var(--brand)' })
  if (dl != null) rows.push({ label: t('timing.download', '内容下载'), ms: dl, offset: ttfb ?? 0, color: 'var(--success)' })
  return (
    <div style={{ width: 320 }}>
      {rows.map((r) => (
        <div key={r.label} style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '3px 0', fontSize: 12 }}>
          <span style={{ width: 128, color: 'var(--text-2)' }}>{r.label}</span>
          <span style={{ flex: 1, height: 6, borderRadius: 3, background: 'var(--panel-2)', position: 'relative', overflow: 'hidden' }}>
            <span
              style={{
                position: 'absolute',
                left: `${(r.offset / span) * 100}%`,
                width: `${Math.max((r.ms / span) * 100, 1.5)}%`,
                top: 0,
                bottom: 0,
                borderRadius: 3,
                background: r.color,
              }}
            />
          </span>
          <span className="ms-mono" style={{ width: 62, textAlign: 'right' }}>{fmtDurationMs(r.ms)}</span>
        </div>
      ))}
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '4px 0 0', fontSize: 12, borderTop: '1px solid var(--border-soft)', marginTop: 4 }}>
        <span style={{ width: 128, color: 'var(--text-2)', fontWeight: 600 }}>{t('timing.total', '总计')}</span>
        <span style={{ flex: 1 }} />
        <span className="ms-mono" style={{ width: 62, textAlign: 'right', fontWeight: 600 }}>{fmtDurationMs(totalMs)}</span>
      </div>
      <div style={{ marginTop: 6, fontSize: 11, color: 'var(--text-3)' }}>
        {t('timing.note', '从建立连接到收到完整响应的全链路分解;DNS 为独立计时(受系统缓存影响)。')}
      </div>
    </div>
  )
}

/** Latency value that pops the waterfall on hover (theme-following Popover). */
export function LatencyStat({ totalMs, timings, children }: { totalMs: number; timings?: PhaseTimings | null; children?: ReactNode }) {
  const body = children ?? <span className="ms-mono">{fmtDurationMs(totalMs)}</span>
  if (!timings || (timings.dnsMs == null && timings.ttfbMs == null && timings.downloadMs == null)) return <>{body}</>
  return (
    <Popover content={<TimingBreakdown totalMs={totalMs} timings={timings} />} placement="bottomRight" mouseEnterDelay={0.2}>
      <span style={{ cursor: 'default' }}>{body}</span>
    </Popover>
  )
}
