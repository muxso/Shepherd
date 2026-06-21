import { useEffect, useState } from 'react'
import { Empty, List, Tag, Spin } from 'antd'
import { ApiOutlined, PartitionOutlined } from '@ant-design/icons'
import { api, type ApiDefinition } from '../api'
import { useI18n } from '../i18n'

type RefItem = { id: string; name: string }

/** 接口定义「引用关系」:被哪些接口用例 / 场景引用。 */
export default function ReferencesPanel({ definition }: { definition: ApiDefinition }) {
  const { t } = useI18n()
  const [loading, setLoading] = useState(true)
  const [cases, setCases] = useState<RefItem[]>([])
  const [scenarios, setScenarios] = useState<RefItem[]>([])

  useEffect(() => {
    let alive = true
    setLoading(true)
    api
      .definitionReferences(definition.id)
      .then((r) => {
        if (!alive) return
        setCases(r.cases || [])
        setScenarios(r.scenarios || [])
      })
      .catch(() => alive && (setCases([]), setScenarios([])))
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

  const section = (title: string, icon: React.ReactNode, items: RefItem[], color: string) => (
    <div style={{ marginBottom: 20 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 8, fontWeight: 600, fontSize: 13 }}>
        {icon}
        {title}
        <Tag color="default" style={{ marginLeft: 4 }}>{items.length}</Tag>
      </div>
      {items.length === 0 ? (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('apidef.refNone', '暂无引用')} style={{ margin: '8px 0' }} />
      ) : (
        <List
          size="small"
          bordered
          dataSource={items}
          renderItem={(it) => (
            <List.Item>
              <Tag color={color} style={{ marginRight: 8 }}>{it.id.slice(0, 8)}</Tag>
              <span>{it.name}</span>
            </List.Item>
          )}
        />
      )}
    </div>
  )

  return (
    <div>
      {section(t('apidef.refByCases', '被接口用例引用'), <ApiOutlined style={{ color: '#7c3aed' }} />, cases, 'purple')}
      {section(t('apidef.refByScenarios', '被场景引用'), <PartitionOutlined style={{ color: '#1677ff' }} />, scenarios, 'geekblue')}
    </div>
  )
}
