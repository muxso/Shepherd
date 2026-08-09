import { useState } from 'react'
import { ExperimentOutlined } from '@ant-design/icons'
import { useI18n } from '../i18n'
import RagSettings from './RagSettings'

// System parameters: a grouped left nav (feature) + right config panel, mirroring the project admin
// layout. Each feature renders its own settings on the right. RAG config lives here (moved out of a
// standalone nav item); add future platform-wide parameter groups as new items.

type ParamKey = 'rag'

export default function SystemParams() {
  const { t } = useI18n()
  const [nav, setNav] = useState<ParamKey>('rag')

  const groups: { title: string; items: { key: ParamKey; label: string; icon: React.ReactNode }[] }[] = [
    {
      title: t('sysparam.grpAi', '智能能力'),
      items: [{ key: 'rag', label: t('sys.rag', 'RAG 配置'), icon: <ExperimentOutlined /> }],
    },
  ]

  return (
    <div style={{ display: 'flex', height: '100%' }}>
      {/* Left: grouped feature nav */}
      <div style={{ width: 200, flexShrink: 0, borderRight: '1px solid var(--border-soft)', padding: '12px 8px', overflow: 'auto', background: 'var(--panel)' }}>
        {groups.map((g, gi) => (
          <div key={g.title} style={gi > 0 ? { borderTop: '1px solid var(--border-soft)', marginTop: 8, paddingTop: 8 } : undefined}>
            <div style={{ fontSize: 12, color: 'var(--text-3)', padding: '6px 10px 2px' }}>{g.title}</div>
            {g.items.map((it) => (
              <div
                key={it.key}
                onClick={() => setNav(it.key)}
                style={{
                  display: 'flex', alignItems: 'center', gap: 8,
                  padding: '8px 12px', borderRadius: 6, cursor: 'pointer', fontSize: 13, margin: '2px 0',
                  background: nav === it.key ? 'var(--brand-soft)' : 'transparent',
                  color: nav === it.key ? 'var(--brand)' : 'var(--text)',
                }}
              >
                {it.icon}
                {it.label}
              </div>
            ))}
          </div>
        ))}
      </div>
      {/* Right: selected feature's config */}
      <div style={{ flex: 1, minWidth: 0, overflow: 'auto', padding: 16, background: 'var(--bg)' }}>
        {nav === 'rag' && <RagSettings embedded />}
      </div>
    </div>
  )
}
