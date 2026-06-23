import { Button, Input, Select, Space, Table } from 'antd'
import { PlusOutlined, DeleteOutlined } from '@ant-design/icons'
import type { ColumnsType } from 'antd/es/table'
import type { BodySchemaNode } from '../api'
import { useI18n } from '../i18n'

const TYPES: BodySchemaNode['type'][] = ['string', 'integer', 'number', 'boolean', 'object', 'array']
const branch = (t: BodySchemaNode['type']) => t === 'object' || t === 'array'

export const emptyNode = (): BodySchemaNode => ({ name: '', type: 'string', value: '', description: '' })

/** 路径式更新(path 为各层 index)。 */
function updateAt(nodes: BodySchemaNode[], path: number[], fn: (n: BodySchemaNode) => BodySchemaNode): BodySchemaNode[] {
  const [i, ...rest] = path
  return nodes.map((n, idx) => {
    if (idx !== i) return n
    if (rest.length === 0) return fn(n)
    return { ...n, children: updateAt(n.children || [], rest, fn) }
  })
}
function removeAt(nodes: BodySchemaNode[], path: number[]): BodySchemaNode[] {
  const [i, ...rest] = path
  if (rest.length === 0) return nodes.filter((_, idx) => idx !== i)
  return nodes.map((n, idx) => (idx === i ? { ...n, children: removeAt(n.children || [], rest) } : n))
}

/** Schema 树 → JSON 示例。 */
export function schemaToJson(nodes: BodySchemaNode[]): unknown {
  const obj: Record<string, unknown> = {}
  for (const n of nodes) {
    if (!n.name.trim()) continue
    obj[n.name.trim()] = nodeValue(n)
  }
  return obj
}
function nodeValue(n: BodySchemaNode): unknown {
  switch (n.type) {
    case 'object': return schemaToJson(n.children || [])
    case 'array': return (n.children || []).length ? [nodeValue((n.children || [])[0])] : []
    case 'integer': return Number(n.value) || 0
    case 'number': return Number(n.value) || 0
    case 'boolean': return n.value === 'true'
    default: return n.value ?? 'string'
  }
}

type Row = BodySchemaNode & { _path: number[]; _key: string; children?: Row[] }
function toRows(nodes: BodySchemaNode[], parent: number[] = []): Row[] {
  return nodes.map((n, i) => {
    const path = [...parent, i]
    return { ...n, _path: path, _key: path.join('.'), children: n.children ? toRows(n.children, path) : undefined }
  })
}

/** JSON 请求体 Schema 树编辑器(对齐 MeterSphere:参数名称/类型/参数值/描述 + 嵌套 + 增删)。 */
export default function BodySchemaTree({ nodes, onChange }: { nodes: BodySchemaNode[]; onChange: (n: BodySchemaNode[]) => void }) {
  const { t } = useI18n()
  const set = (path: number[], p: Partial<BodySchemaNode>) => onChange(updateAt(nodes, path, (n) => ({ ...n, ...p })))
  const addChild = (path: number[]) => onChange(updateAt(nodes, path, (n) => ({ ...n, children: [...(n.children || []), emptyNode()] })))

  const cols: ColumnsType<Row> = [
    {
      title: t('query.colName', '参数名称'), dataIndex: 'name',
      render: (v: string, r) => <Input size="small" value={v} placeholder={t('a.input', '请输入')} onChange={(e) => set(r._path, { name: e.target.value })} className="ms-mono" />,
    },
    {
      title: t('query.colType', '类型'), dataIndex: 'type', width: 130,
      render: (v: BodySchemaNode['type'], r) => <Select size="small" value={v} onChange={(val) => set(r._path, { type: val })} style={{ width: '100%' }} options={TYPES.map((x) => ({ value: x, label: x }))} />,
    },
    {
      title: t('query.colValue', '参数值'), dataIndex: 'value', width: 200,
      render: (v: string, r) => branch(r.type) ? <span style={{ color: '#bbb' }}>—</span> : <Input size="small" value={v} onChange={(e) => set(r._path, { value: e.target.value })} />,
    },
    {
      title: t('query.colDesc', '描述'), dataIndex: 'description',
      render: (v: string, r) => <Input size="small" value={v} onChange={(e) => set(r._path, { description: e.target.value })} />,
    },
    {
      title: '', width: 70,
      render: (_v, r) => (
        <Space size={0}>
          {branch(r.type) && <Button type="text" size="small" icon={<PlusOutlined />} onClick={() => addChild(r._path)} />}
          <Button type="text" size="small" danger icon={<DeleteOutlined />} onClick={() => onChange(removeAt(nodes, r._path))} />
        </Space>
      ),
    },
  ]

  return (
    <div>
      <Table<Row> size="small" rowKey="_key" pagination={false} columns={cols} dataSource={toRows(nodes)} defaultExpandAllRows locale={{ emptyText: t('body.emptySchema', '无字段') }} />
      <Button icon={<PlusOutlined />} size="small" onClick={() => onChange([...nodes, emptyNode()])} style={{ marginTop: 8 }}>
        {t('case.addRow', '加一行')}
      </Button>
    </div>
  )
}
