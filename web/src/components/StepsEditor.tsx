import { useState } from 'react'
import { Button, Input, Space, Typography } from 'antd'
import { PlusOutlined, DeleteOutlined } from '@ant-design/icons'
import type { CaseStep } from '../api'

// 受控:功能用例的「步骤 + 预期结果」行编辑器。可放进 AntD Form.Item。
export default function StepsEditor({
  value,
  onChange,
}: {
  value?: CaseStep[]
  onChange?: (v: CaseStep[]) => void
}) {
  const [rows, setRows] = useState<CaseStep[]>(() => value || [])
  const push = (next: CaseStep[]) => {
    setRows(next)
    onChange?.(next)
  }
  const update = (i: number, patch: Partial<CaseStep>) =>
    push(rows.map((r, idx) => (idx === i ? { ...r, ...patch } : r)))

  return (
    <Space direction="vertical" style={{ width: '100%' }} size={6}>
      {rows.length === 0 && (
        <Typography.Text type="secondary" style={{ fontSize: 12 }}>
          暂无步骤,点下方添加。
        </Typography.Text>
      )}
      {rows.map((r, i) => (
        <Space.Compact key={i} style={{ width: '100%' }} block>
          <Input
            style={{ width: 36, textAlign: 'center', background: '#fafafa', color: '#999' }}
            value={String(i + 1)}
            disabled
          />
          <Input
            placeholder="步骤描述"
            value={r.step}
            onChange={(e) => update(i, { step: e.target.value })}
          />
          <Input
            placeholder="预期结果"
            value={r.expected}
            onChange={(e) => update(i, { expected: e.target.value })}
          />
          <Button icon={<DeleteOutlined />} onClick={() => push(rows.filter((_, idx) => idx !== i))} />
        </Space.Compact>
      ))}
      <Button icon={<PlusOutlined />} size="small" onClick={() => push([...rows, { step: '', expected: '' }])}>
        添加步骤
      </Button>
    </Space>
  )
}
