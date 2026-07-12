import { useEffect, useState } from 'react'
import { Empty, Spin, Tag, Timeline } from 'antd'
import { api, type ApiDefinition, type ApiDefinitionChange } from '../api'
import { useI18n } from '../i18n'

const ACTION_COLOR: Record<string, string> = {
  CREATE: 'green',
  UPDATE_SPEC: 'blue',
  MOVE_MODULE: 'gold',
  RENAME: 'purple',
}

/** API definition change history (audit): action timeline, newest first. */
export default function ChangeHistoryPanel({ definition }: { definition: ApiDefinition }) {
  const { t } = useI18n()
  const [loading, setLoading] = useState(true)
  const [changes, setChanges] = useState<ApiDefinitionChange[]>([])

  useEffect(() => {
    let alive = true
    setLoading(true)
    api
      .definitionChanges(definition.id)
      .then((c) => alive && setChanges(Array.isArray(c) ? c : []))
      .catch(() => alive && setChanges([]))
      .finally(() => alive && setLoading(false))
    return () => {
      alive = false
    }
  }, [definition.id])

  if (loading)
    return (
      <div style={{ padding: 32, textAlign: 'center' }}>
        <Spin />
      </div>
    )

  const actionLabel = (a: string) =>
    t(`apidef.action.${a}`, a === 'CREATE' ? '创建' : a === 'UPDATE_SPEC' ? '更新规格' : a === 'MOVE_MODULE' ? '移动模块' : a === 'RENAME' ? '重命名' : a)

  if (changes.length === 0)
    return <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('apidef.changeNone', '暂无变更记录')} style={{ marginTop: 32 }} />

  return (
    <Timeline
      style={{ paddingTop: 8 }}
      items={changes.map((c) => ({
        color: ACTION_COLOR[c.action] || 'gray',
        children: (
          <div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <Tag color={ACTION_COLOR[c.action] || 'default'} style={{ margin: 0 }}>{actionLabel(c.action)}</Tag>
              {c.detail && <span style={{ color: 'var(--text-2)' }}>{c.detail}</span>}
            </div>
            <div style={{ color: 'var(--text-3)', fontSize: 12, marginTop: 2 }}>
              {c.actor || '—'} · <span className="ms-mono">{c.createdAt}</span>
            </div>
          </div>
        ),
      }))}
    />
  )
}
