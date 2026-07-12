import { Button, Input, Select, Space, Tooltip } from 'antd'
import { PlusOutlined, DeleteOutlined } from '@ant-design/icons'
import { useI18n } from '../i18n'

/** One match condition: param name + operator + value. op maps directly to mock-runtime StringMatch. */
export type MatchCond = { logic: 'AND' | 'OR'; name: string; op: 'equals' | 'contains' | 'regex'; value: string }

export const emptyCond = (): MatchCond => ({ logic: 'AND', name: '', op: 'equals', value: '' })

/**
 * Mock match-condition editor: AND/OR + param name + equals/contains/regex + value.
 * Note: the match engine currently ANDs all conditions; the AND/OR selector is display-only until the engine supports OR grouping (hence disabled with a hint).
 */
export default function MatchConditionEditor({
  rows,
  onChange,
}: {
  rows: MatchCond[]
  onChange: (rows: MatchCond[]) => void
}) {
  const { t } = useI18n()
  const set = (i: number, patch: Partial<MatchCond>) => onChange(rows.map((r, idx) => (idx === i ? { ...r, ...patch } : r)))
  const opOptions = [
    { value: 'equals', label: t('mock.opEquals', '等于') },
    { value: 'contains', label: t('mock.opContains', '包含') },
    { value: 'regex', label: t('mock.opRegex', '正则') },
  ]
  return (
    <Space direction="vertical" style={{ width: '100%' }} size={8}>
      {rows.map((r, i) => (
        <Space.Compact key={i} style={{ width: '100%' }}>
          <Tooltip title={i === 0 ? undefined : t('mock.andOnly', '当前引擎按 AND 组合;OR 分组待支持')}>
            <Select
              value={r.logic}
              onChange={(v) => set(i, { logic: v })}
              disabled={i === 0}
              style={{ width: 78 }}
              options={[
                { value: 'AND', label: 'AND' },
                { value: 'OR', label: 'OR' },
              ]}
            />
          </Tooltip>
          <Input
            placeholder={t('mock.paramName', '参数名称')}
            value={r.name}
            onChange={(e) => set(i, { name: e.target.value })}
            style={{ width: 200 }}
            className="ms-mono"
          />
          <Select value={r.op} onChange={(v) => set(i, { op: v })} style={{ width: 100 }} options={opOptions} />
          <Input placeholder={t('mock.condValue', '值')} value={r.value} onChange={(e) => set(i, { value: e.target.value })} />
          <Button icon={<DeleteOutlined />} onClick={() => onChange(rows.filter((_, idx) => idx !== i))} />
        </Space.Compact>
      ))}
      <Button icon={<PlusOutlined />} size="small" onClick={() => onChange([...rows, emptyCond()])}>
        {t('case.addRow', '加一行')}
      </Button>
    </Space>
  )
}
