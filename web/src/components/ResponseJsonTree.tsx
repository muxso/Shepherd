import { useMemo, useState, type CSSProperties } from 'react'
import { Button, Popover, Space } from 'antd'
import { readJson, responseField, type SelectedResponseField } from './ResponseJsonPathPicker'

type Segment = string | number
type Line = { text: string; field?: Segment[] }
type Props = {
  body: string
  style: CSSProperties
  onAssertion: (field: SelectedResponseField) => void
  onExtraction: (field: SelectedResponseField) => void
}

function* jsonLines(value: unknown, path: Segment[] = [], comma = ''): Generator<Line> {
  const indent = '  '.repeat(path.length)
  const key = path[path.length - 1]
  const prefix = typeof key === 'string' ? `${indent}${JSON.stringify(key)}: ` : indent
  const array = Array.isArray(value)
  const object = value !== null && typeof value === 'object'
  if (!object || path.length >= 100) {
    yield { text: prefix + JSON.stringify(value) + comma, field: path }
    return
  }
  const entries = Object.entries(value)
  const opening = array ? '[' : '{'
  const closing = array ? ']' : '}'
  if (!entries.length) {
    yield { text: prefix + opening + closing + comma, field: path }
    return
  }
  yield { text: prefix + opening, field: path }
  for (const [index, [name, child]] of entries.entries()) {
    yield* jsonLines(child, [...path, array ? Number(name) : name], index + 1 === entries.length ? '' : ',')
  }
  yield { text: indent + closing + comma }
}

export default function ResponseJsonTree({ body, style, onAssertion, onExtraction }: Props) {
  const parsed = useMemo(() => readJson(body), [body])
  const [visible, setVisible] = useState(200)
  const [selection, setSelection] = useState<{ body: string; index: number }>()
  const lines = useMemo(() => {
    if (!('value' in parsed)) return []
    const rows: Line[] = []
    for (const line of jsonLines(parsed.value)) {
      rows.push(line)
      if (rows.length > visible) break
    }
    return rows
  }, [parsed, visible])
  if (!lines.length) return <pre style={style}>{body || '(空)'}</pre>

  return <div style={style}>
    <pre style={{ font: 'inherit', fontFamily: 'monospace', whiteSpace: 'pre-wrap', overflowWrap: 'anywhere', margin: 0 }}
      onMouseUp={event => {
        const selectedText = window.getSelection()
        if (!selectedText || selectedText.isCollapsed) return
        const rowOf = (node: Node | null) => (node instanceof Element ? node : node?.parentElement)?.closest('[data-json-line]')
        const start = rowOf(selectedText.anchorNode)
        if (start && start === rowOf(selectedText.focusNode) && event.currentTarget.contains(start)) {
          setSelection({ body, index: Number(start.getAttribute('data-json-line')) })
        }
      }}>
      {lines.slice(0, visible).map(({ text, field: path }, index) => {
        if (!path) return <span key={index}>{text}{'\n'}</span>
        return <Popover key={index} trigger="click" placement="topLeft"
          open={selection?.body === body && selection.index === index}
          onOpenChange={open => setSelection(current => open ? { body, index } : current?.index === index ? undefined : current)}
          content={() => {
            const field = responseField(path)
            return <Space direction="vertical">
              <code style={{ overflowWrap: 'anywhere', display: 'block', maxWidth: 300 }}>{field.expression}</code>
              <Space wrap>
                <Button size="small" onClick={() => { setSelection(undefined); onAssertion(field) }}>添加断言</Button>
                <Button size="small" onClick={() => { setSelection(undefined); onExtraction(field) }}>添加后置提取</Button>
              </Space>
            </Space>
          }}>
          <span role="button" tabIndex={0} data-json-line={index} style={{ cursor: 'pointer' }}
            onClickCapture={event => {
              if (window.getSelection()?.isCollapsed === false) event.stopPropagation()
            }}
            onKeyDown={event => {
              if (event.key === 'Enter' || event.key === ' ') {
                event.preventDefault()
                setSelection({ body, index })
              }
            }}>{text}{'\n'}</span>
        </Popover>
      })}
    </pre>
    {lines.length > visible && <Button size="small" onClick={() => setVisible(visible + 200)}>显示更多</Button>}
  </div>
}
