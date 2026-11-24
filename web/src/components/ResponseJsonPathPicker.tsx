import { useMemo, useState } from 'react'
import { Alert, Button, Input, Modal, Select, Space, Typography } from 'antd'

type Segment = string | number
export type SelectedResponseField = { expression: string; variable: string }

function fieldExpression(segments: Segment[]) {
  if (segments.some(segment => typeof segment === 'string' && !/^[\p{L}_$][\p{L}\p{N}_$]*$/u.test(segment))) {
    return '/' + segments.map(segment => String(segment).replace(/~/g, '~0').replace(/\//g, '~1')).join('/')
  }
  return '$' + segments.map(segment => typeof segment === 'number' ? `[${segment}]` : `.${segment}`).join('')
}

export function responseField(segments: Segment[]): SelectedResponseField {
  const variable = segments.map(String).join('_').replace(/[^\p{L}\p{N}_]/gu, '_') || 'response'
  return { expression: fieldExpression(segments), variable: /^\d/.test(variable) ? `response_${variable}` : variable }
}

export function readJson(source: string) {
  if (!source.trim()) return { error: '请先执行请求获取 JSON 响应，或粘贴 JSON 样例。' }
  if (source.length > 1_000_000) return { error: '响应过大，请粘贴需要提取的 JSON 样例。' }
  try {
    return { value: JSON.parse(source) as unknown }
  } catch {
    return { error: '内容不是有效的 JSON，请检查或粘贴 JSON 样例。' }
  }
}

function childKeys(value: unknown): Segment[] {
  if (Array.isArray(value)) return value.map((_, index) => index)
  return value !== null && typeof value === 'object' ? Object.keys(value) : []
}

export default function ResponseJsonPathPicker({ responseBody, onSelect, label = '选择字段' }: {
  responseBody?: string
  onSelect: (field: SelectedResponseField) => void
  label?: string
}) {
  const [open, setOpen] = useState(false)
  const [sample, setSample] = useState<string>()
  const [segments, setSegments] = useState<Segment[]>([])
  const parsed = useMemo<{ value?: unknown; error?: string }>(() => open ? readJson(sample ?? responseBody ?? '') : {}, [open, sample, responseBody])
  const current = segments.reduce<unknown>((value, key) =>
    value !== null && typeof value === 'object' && Object.prototype.hasOwnProperty.call(value, key)
      ? (value as Record<string, unknown>)[key]
      : undefined, parsed.value)
  const options = useMemo(() => childKeys(current).map(key => ({
    value: key,
    label: typeof key === 'number' ? `[${key}]` : key || '（空字段名）',
  })), [current])
  const field = responseField(segments)
  const { expression } = field
  const preview = current === undefined ? '' : JSON.stringify(current)
  const selectField = () => {
    onSelect(field)
    setOpen(false)
  }

  return <>
    <Button onClick={() => { setSample(undefined); setSegments([]); setOpen(true) }}>{label}</Button>
    <Modal title="选择响应字段" open={open} onCancel={() => setOpen(false)} onOk={selectField}
      okText="应用到提取规则" cancelText="取消" okButtonProps={{ disabled: current === undefined }}>
      <Space direction="vertical" size={12} style={{ width: '100%' }}>
        {sample !== undefined || parsed.error ? <Input.TextArea aria-label="JSON 样例" placeholder="粘贴 JSON 样例"
          autoSize={{ minRows: 3, maxRows: 8 }} value={sample ?? ''} onChange={event => {
            setSample(event.target.value)
            setSegments([])
          }} /> : <Button size="small" onClick={() => { setSample(''); setSegments([]) }}>使用 JSON 样例</Button>}
        {sample !== undefined && responseBody && <Button size="small" onClick={() => { setSample(undefined); setSegments([]) }}>使用最近响应</Button>}
        {parsed.error && <Alert type="info" message={parsed.error} />}
        {!parsed.error && <>
          <Space wrap>
            <Button size="small" disabled={!segments.length} onClick={() => setSegments(segments.slice(0, -1))}>上一级</Button>
            <Button size="small" disabled={!segments.length} onClick={() => setSegments([])}>根节点</Button>
          </Space>
          {options.length > 0 && <Select showSearch optionFilterProp="label" aria-label="下一级字段"
            placeholder="选择下一级字段或数组索引" value={null} options={options} style={{ width: '100%' }}
            onChange={key => { if (key !== null) setSegments([...segments, key]) }} />}
          <Typography.Text>{expression.startsWith('/') ? 'JSON Pointer（字段名含特殊字符）' : 'JSONPath'}</Typography.Text>
          <Input readOnly aria-label="提取路径" value={expression} className="ms-mono" />
          <Typography.Text type="secondary">值预览</Typography.Text>
          <Input.TextArea readOnly aria-label="值预览" value={preview.length > 800 ? preview.slice(0, 800) + '…' : preview}
            autoSize={{ minRows: 1, maxRows: 5 }} className="ms-mono" />
        </>}
      </Space>
    </Modal>
  </>
}
