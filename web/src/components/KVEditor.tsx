import { Button, Input, Space } from 'antd'
import { PlusOutlined, DeleteOutlined } from '@ant-design/icons'
import { useI18n } from '../i18n'

export type KVRow = { key: string; value: string }

/** 键值对编辑器(请求头 / Query / REST / 响应头 共用):一行 名 + 值 + 删除,底部加一行。 */
export default function KVEditor({
  rows,
  onChange,
  namePlaceholder,
  valuePlaceholder,
}: {
  rows: KVRow[]
  onChange: (rows: KVRow[]) => void
  namePlaceholder?: string
  valuePlaceholder?: string
}) {
  const { t } = useI18n()
  const set = (i: number, patch: Partial<KVRow>) => onChange(rows.map((r, idx) => (idx === i ? { ...r, ...patch } : r)))
  return (
    <Space direction="vertical" style={{ width: '100%' }}>
      {rows.map((r, i) => (
        <Space.Compact key={i} style={{ width: '100%' }}>
          <Input
            placeholder={namePlaceholder || t('case.headerName', '名')}
            value={r.key}
            onChange={(e) => set(i, { key: e.target.value })}
            style={{ width: 240 }}
            className="ms-mono"
          />
          <Input
            placeholder={valuePlaceholder || t('case.headerValue', '值')}
            value={r.value}
            onChange={(e) => set(i, { value: e.target.value })}
          />
          <Button icon={<DeleteOutlined />} onClick={() => onChange(rows.filter((_, idx) => idx !== i))} />
        </Space.Compact>
      ))}
      <Button icon={<PlusOutlined />} size="small" onClick={() => onChange([...rows, { key: '', value: '' }])}>
        {t('case.addRow', '加一行')}
      </Button>
    </Space>
  )
}
