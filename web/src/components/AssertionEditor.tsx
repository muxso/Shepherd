import { useState } from 'react'
import { Button, Input, InputNumber, Select, Space, Typography } from 'antd'
import { PlusOutlined, DeleteOutlined } from '@ant-design/icons'

// 后端断言(api-runner Assertion,serde tag=type/content=args)。
export type Assertion = Record<string, unknown>

const TYPES = [
  { value: 'StatusIs', label: '状态码等于' },
  { value: 'BodyContains', label: '响应体包含' },
  { value: 'JsonFieldEquals', label: 'JSON 字段等于' },
  { value: 'HeaderEquals', label: '响应头等于' },
  { value: 'ResponseTime', label: '响应耗时 ≤ (ms)' },
]

// 内部行表示:统一存 n(数字)/ s、s2(字符串),按类型序列化为后端 JSON。
interface Row {
  type: string
  n: number
  s: string
  s2: string
}

function toAssertion(r: Row): Assertion {
  switch (r.type) {
    case 'StatusIs':
      return { type: 'StatusIs', args: r.n }
    case 'BodyContains':
      return { type: 'BodyContains', args: r.s }
    case 'JsonFieldEquals':
      return { type: 'JsonFieldEquals', args: { pointer: r.s, expected: r.s2 } }
    case 'HeaderEquals':
      return { type: 'HeaderEquals', args: { name: r.s, value: r.s2 } }
    case 'ResponseTime':
      return { type: 'ResponseTime', args: { max_ms: r.n } }
    default:
      return { type: 'StatusIs', args: r.n }
  }
}

function fromAssertion(a: Assertion): Row {
  const t = String(a.type || 'StatusIs')
  const args = a.args as any
  const base: Row = { type: t, n: 200, s: '', s2: '' }
  if (t === 'StatusIs') base.n = Number(args) || 200
  else if (t === 'BodyContains') base.s = String(args ?? '')
  else if (t === 'JsonFieldEquals') {
    base.s = String(args?.pointer ?? '')
    base.s2 = String(args?.expected ?? '')
  } else if (t === 'HeaderEquals') {
    base.s = String(args?.name ?? '')
    base.s2 = String(args?.value ?? '')
  } else if (t === 'ResponseTime') base.n = Number(args?.max_ms) || 500
  return base
}

// 受控:value 为后端断言数组,onChange 回传同结构。可放进 AntD Form.Item。
export default function AssertionEditor({
  value,
  onChange,
}: {
  value?: Assertion[]
  onChange?: (v: Assertion[]) => void
}) {
  const [rows, setRows] = useState<Row[]>(() => (value && value.length ? value.map(fromAssertion) : []))

  const push = (next: Row[]) => {
    setRows(next)
    onChange?.(next.map(toAssertion))
  }
  const update = (i: number, patch: Partial<Row>) => push(rows.map((r, idx) => (idx === i ? { ...r, ...patch } : r)))

  return (
    <Space direction="vertical" style={{ width: '100%' }} size={6}>
      {rows.length === 0 && (
        <Typography.Text type="secondary" style={{ fontSize: 12 }}>
          无断言(执行只看传输是否可达)。点下方添加。
        </Typography.Text>
      )}
      {rows.map((r, i) => (
        <Space.Compact key={i} style={{ width: '100%' }}>
          <Select
            value={r.type}
            onChange={(t) => update(i, { type: t })}
            options={TYPES}
            style={{ width: 150 }}
          />
          {r.type === 'StatusIs' || r.type === 'ResponseTime' ? (
            <InputNumber
              value={r.n}
              onChange={(v) => update(i, { n: Number(v) || 0 })}
              style={{ width: 120 }}
              placeholder={r.type === 'StatusIs' ? '200' : '500'}
            />
          ) : r.type === 'BodyContains' ? (
            <Input value={r.s} onChange={(e) => update(i, { s: e.target.value })} placeholder="子串" />
          ) : (
            <>
              <Input
                value={r.s}
                onChange={(e) => update(i, { s: e.target.value })}
                placeholder={r.type === 'JsonFieldEquals' ? 'Pointer 如 /data/id' : '头名 如 Content-Type'}
                className="ms-mono"
              />
              <Input
                value={r.s2}
                onChange={(e) => update(i, { s2: e.target.value })}
                placeholder="期望值"
              />
            </>
          )}
          <Button icon={<DeleteOutlined />} onClick={() => push(rows.filter((_, idx) => idx !== i))} />
        </Space.Compact>
      ))}
      <Button
        icon={<PlusOutlined />}
        size="small"
        onClick={() => push([...rows, { type: 'StatusIs', n: 200, s: '', s2: '' }])}
      >
        添加断言
      </Button>
    </Space>
  )
}
