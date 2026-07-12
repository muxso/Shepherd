import { Button, Checkbox, Input, Select, Space, Table } from 'antd'
import { PlusOutlined, DeleteOutlined } from '@ant-design/icons'
import type { ColumnsType } from 'antd/es/table'
import { useI18n } from '../i18n'

/** One query param row: key/value drive execution; type/min/max/description are doc metadata stored with the case. */
export type QueryParam = {
  enabled: boolean
  key: string
  type: string
  value: string
  minLen: string
  maxLen: string
  description: string
}

export const emptyQueryParam = (): QueryParam => ({
  enabled: true,
  key: '',
  type: 'string',
  value: '',
  minLen: '',
  maxLen: '',
  description: '',
})

const TYPES = ['string', 'integer', 'number', 'boolean', 'array', 'file']

/**
 * Structured query-param table: enabled / name / type / value / length range / description.
 * Execution only uses key=value (when enabled); other columns are doc/constraint metadata that round-trips with the case.
 */
export default function QueryParamTable({
  rows,
  onChange,
}: {
  rows: QueryParam[]
  onChange: (rows: QueryParam[]) => void
}) {
  const { t } = useI18n()
  const set = (i: number, patch: Partial<QueryParam>) => onChange(rows.map((r, idx) => (idx === i ? { ...r, ...patch } : r)))
  const data = rows.map((r, i) => ({ ...r, _k: String(i) }))

  const cols: ColumnsType<QueryParam & { _k: string }> = [
    {
      title: '',
      dataIndex: 'enabled',
      width: 40,
      render: (v: boolean, _r, i) => <Checkbox checked={v} onChange={(e) => set(i, { enabled: e.target.checked })} />,
    },
    {
      title: t('query.colName', '参数名称'),
      dataIndex: 'key',
      width: '24%',
      render: (v: string, _r, i) => <Input value={v} placeholder={t('a.input', '请输入')} onChange={(e) => set(i, { key: e.target.value })} className="ms-mono" />,
    },
    {
      title: t('query.colType', '类型'),
      dataIndex: 'type',
      width: 110,
      render: (v: string, _r, i) => <Select value={v} onChange={(val) => set(i, { type: val })} style={{ width: '100%' }} options={TYPES.map((x) => ({ value: x, label: x }))} />,
    },
    {
      title: t('query.colValue', '参数值'),
      dataIndex: 'value',
      render: (v: string, _r, i) => <Input value={v} onChange={(e) => set(i, { value: e.target.value })} />,
    },
    {
      title: t('query.colLen', '长度区间'),
      width: 150,
      render: (_v, _r, i) => (
        <Space.Compact>
          <Input value={rows[i].minLen} placeholder={t('query.min', '最小')} style={{ width: 64 }} onChange={(e) => set(i, { minLen: e.target.value })} />
          <Input value={rows[i].maxLen} placeholder={t('query.max', '最大')} style={{ width: 64 }} onChange={(e) => set(i, { maxLen: e.target.value })} />
        </Space.Compact>
      ),
    },
    {
      title: t('query.colDesc', '描述'),
      dataIndex: 'description',
      render: (v: string, _r, i) => <Input value={v} onChange={(e) => set(i, { description: e.target.value })} />,
    },
    {
      title: '',
      width: 44,
      render: (_v, _r, i) => <Button type="text" danger size="small" icon={<DeleteOutlined />} onClick={() => onChange(rows.filter((_, idx) => idx !== i))} />,
    },
  ]

  return (
    <div>
      <Table size="small" rowKey="_k" pagination={false} columns={cols} dataSource={data} locale={{ emptyText: t('query.empty', '无 Query 参数') }} />
      <Button icon={<PlusOutlined />} size="small" onClick={() => onChange([...rows, emptyQueryParam()])} style={{ marginTop: 8 }}>
        {t('case.addRow', '加一行')}
      </Button>
    </div>
  )
}
