import { useEffect, useState } from 'react'
import { Button, Radio, Select, Switch } from 'antd'
import type { Environment, PlanningNode, PlanningNodeConfig, ResourcePool } from '../../api'
import { useI18n } from '../../i18n'

/**
 * Right slide-in config panel for a category node (接口用例 / 场景用例):
 * pool + env selects, serial/parallel radio, stop-on-fail / retry switches.
 * No case picker at category level.
 */
export default function PlanCategoryConfig({
  node,
  envs,
  pools,
  onSave,
  onClose,
}: {
  node: PlanningNode
  envs: Environment[]
  pools: ResourcePool[]
  onSave: (config: PlanningNodeConfig) => void
  onClose: () => void
}) {
  const { t } = useI18n()
  const [config, setConfig] = useState<PlanningNodeConfig>({})

  useEffect(() => {
    setConfig({ mode: 'serial', ...(node.config || {}) })
  }, [node])

  const set = (patch: PlanningNodeConfig) => setConfig((c) => ({ ...c, ...patch }))

  const fieldLabel = (text: string) => (
    <div style={{ color: 'var(--text-2)', fontSize: 13, margin: '14px 0 6px' }}>{text}</div>
  )

  return (
    <div
      style={{
        position: 'absolute',
        top: 0,
        right: 0,
        bottom: 0,
        width: 340,
        background: 'var(--panel)',
        borderLeft: '1px solid var(--border-soft)',
        boxShadow: '-6px 0 16px rgba(0,0,0,0.08)',
        display: 'flex',
        flexDirection: 'column',
        zIndex: 5,
      }}
    >
      {/* Title: brand vertical bar + category name */}
      <div style={{ padding: '14px 16px 0', display: 'flex', alignItems: 'center', gap: 8 }}>
        <span style={{ width: 3, height: 14, borderRadius: 2, background: 'var(--brand)', flexShrink: 0 }} />
        <span style={{ fontWeight: 600, fontSize: 14 }}>{node.name}</span>
      </div>
      <div style={{ flex: 1, overflow: 'auto', padding: '0 16px 16px' }}>
        {fieldLabel(t('plan.mm.pool', '资源池'))}
        <Select
          style={{ width: '100%' }}
          allowClear
          placeholder={t('plan.mm.defaultPool', '默认资源池')}
          value={config.poolId || undefined}
          onChange={(v) => set({ poolId: v })}
          options={pools.map((p) => ({ value: p.id, label: p.name }))}
        />
        {fieldLabel(t('plan.mm.env', '环境'))}
        <Select
          style={{ width: '100%' }}
          allowClear
          placeholder={t('plan.mm.defaultEnv', '默认环境')}
          value={config.envId || undefined}
          onChange={(v) => set({ envId: v })}
          options={envs.map((e) => ({ value: e.id, label: e.name }))}
        />
        <div style={{ margin: '18px 0' }}>
          <Radio.Group
            value={config.mode || 'serial'}
            onChange={(e) => set({ mode: e.target.value })}
            options={[
              { value: 'serial', label: t('plan.mm.serialFull', '串行') },
              { value: 'parallel', label: t('plan.mm.parallelFull', '并行') },
            ]}
          />
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, margin: '16px 0' }}>
          <Switch size="small" checked={!!config.stopOnFail} onChange={(v) => set({ stopOnFail: v })} />
          <span style={{ fontSize: 13 }}>{t('plan.mm.stopOnFail', '失败停止')}</span>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, margin: '16px 0' }}>
          <Switch size="small" checked={!!config.retry} onChange={(v) => set({ retry: v })} />
          <span style={{ fontSize: 13 }}>{t('plan.mm.retry', '失败重试')}</span>
        </div>
        {/* Footer: left-aligned save + cancel */}
        <div style={{ display: 'flex', gap: 8, marginTop: 22 }}>
          <Button type="primary" onClick={() => onSave(config)}>
            {t('lv.save', '保存')}
          </Button>
          <Button onClick={onClose}>{t('a.cancel', '取消')}</Button>
        </div>
      </div>
    </div>
  )
}
