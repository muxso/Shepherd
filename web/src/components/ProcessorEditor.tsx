import { useState } from 'react'
import { Button, Dropdown, Empty, Input, InputNumber, Select, Space, Switch, Typography } from 'antd'
import { PlusOutlined, DeleteOutlined } from '@ant-design/icons'
import ScriptActionEditor, { newScriptAction, scriptActionPayload, type ScriptAction } from './ScriptActionEditor'
import PreparationProfileSelect from './PreparationProfileSelect'
import ResponseJsonPathPicker from './ResponseJsonPathPicker'
import { useApp } from '../context'
import { useI18n } from '../i18n'

// Backend processor (api-runner Processor, serde tag=type / content=args).
export type Processor = Record<string, unknown>
export type OpKind = 'wait' | 'extract' | 'script' | 'sql' | 'plugin' | 'sandboxScript'

type Extractor = { variable: string; kind: 'JSON_PATH' | 'REGEX'; expression: string }
type Op =
  | { kind: 'sandboxScript'; enabled: boolean; action: ScriptAction }
  | { kind: 'plugin'; enabled: boolean; profileId: string }
  | { kind: 'wait'; enabled: boolean; ms: number }
  | { kind: 'extract'; enabled: boolean; extractors: Extractor[] }
  | { kind: 'script'; enabled: boolean; lang: string; code: string }
  | { kind: 'sql'; enabled: boolean; name: string; datasource: string; sql: string }

const emptyOp = (kind: OpKind): Op => {
  switch (kind) {
    case 'sandboxScript': return { kind, enabled: true, action: newScriptAction() }
    case 'plugin': return { kind, enabled: true, profileId: '' }
    case 'wait':
      return { kind, enabled: true, ms: 1000 }
    case 'extract':
      return { kind, enabled: true, extractors: [{ variable: '', kind: 'JSON_PATH', expression: '' }] }
    case 'script':
      return { kind, enabled: true, lang: 'JS', code: '' }
    case 'sql':
      return { kind, enabled: true, name: '', datasource: '', sql: '' }
  }
}

function parse(value?: Processor[]): Op[] {
  if (!value) return []
  return value.map((p): Op => {
    const args = p.args as any
    switch (String(p.type)) {
      case 'SandboxScript': return { kind: 'sandboxScript', enabled: true, action: { ...newScriptAction(), ...args, inputText: JSON.stringify(args?.input ?? {}, null, 2) } }
      case 'Plugin': return { kind: 'plugin', enabled: true, profileId: String(args?.profileId || '') }
      case 'Wait':
        return { kind: 'wait', enabled: true, ms: Number(args?.ms) || 0 }
      case 'Extract':
        return { kind: 'extract', enabled: true, extractors: Array.isArray(args?.extractors) ? args.extractors : [] }
      case 'Script':
        return { kind: 'script', enabled: true, lang: String(args?.lang || 'JS'), code: String(args?.code || '') }
      case 'Sql':
        return { kind: 'sql', enabled: true, name: String(args?.name || ''), datasource: String(args?.datasource || ''), sql: String(args?.sql || '') }
      default:
        return emptyOp('wait')
    }
  })
}

function serialize(ops: Op[]): Processor[] {
  return ops
    .filter((o) => o.enabled)
    .map((o): Processor => {
      switch (o.kind) {
        case 'sandboxScript': return { type: 'SandboxScript', args: scriptActionPayload(o.action) ?? (o.action.mode === 'shared' ? { mode: 'shared', scriptId: o.action.scriptId ?? '', revision: o.action.revision ?? 0, input: null, output: o.action.output } : { mode: 'custom', language: o.action.language ?? 'javascript', code: o.action.code ?? '', input: null, output: o.action.output }) }
        case 'plugin': return { type: 'Plugin', args: { profileId: o.profileId } }
        case 'wait':
          return { type: 'Wait', args: { ms: o.ms } }
        case 'extract':
          return { type: 'Extract', args: { extractors: o.extractors.filter((e) => e.variable.trim() && e.expression.trim()) } }
        case 'script':
          return { type: 'Script', args: { lang: o.lang, code: o.code } }
        case 'sql':
          return { type: 'Sql', args: { name: o.name, datasource: o.datasource, sql: o.sql } }
      }
    })
}

/**
 * Pre/post processor editor: add dropdown + processor list left + editor right.
 * New preparation entries reference plugin-owned profiles; legacy inline entries remain readable.
 */
export default function ProcessorEditor({
  value,
  onChange,
  allowed,
  responseBody,
  initialSelection = 0,
  environmentId,
}: {
  value?: Processor[]
  onChange?: (v: Processor[]) => void
  allowed: OpKind[]
  responseBody?: string
  initialSelection?: number
  environmentId?: string
}) {
  const { t } = useI18n()
  const { projectId } = useApp()
  const [ops, setOps] = useState<Op[]>(() => parse(value))
  const [sel, setSel] = useState(initialSelection)

  const push = (next: Op[]) => {
    setOps(next)
    onChange?.(serialize(next))
  }
  const update = (i: number, patch: Partial<Op>) => push(ops.map((o, idx) => (idx === i ? ({ ...o, ...patch } as Op) : o)))
  const add = (kind: OpKind) => {
    push([...ops, emptyOp(kind)])
    setSel(ops.length)
  }

  const LABEL: Record<OpKind, string> = {
    sandboxScript: '公共 / 自定义脚本',
    plugin: '前置插件',
    wait: t('proc.wait', '等待时间'),
    extract: t('proc.extract', '提取参数'),
    script: t('proc.script', '脚本操作'),
    sql: t('proc.sql', 'SQL 操作'),
  }
  const addMenu = {
    items: allowed.map((k) => ({ key: k, label: LABEL[k] })),
    onClick: ({ key }: { key: string }) => add(key as OpKind),
  }
  const cur = ops[sel]

  return (
    <div>
      <Dropdown menu={addMenu} trigger={['click']}>
        <Button type="primary" ghost icon={<PlusOutlined />} size="small" style={{ marginBottom: 10 }}>
          {t('proc.add', '操作')}
        </Button>
      </Dropdown>
      {allowed.includes('extract') && <span style={{ marginLeft: 8 }}>
        <ResponseJsonPathPicker label="从响应提取" responseBody={responseBody} onSelect={field => {
          const extractor: Extractor = { ...field, kind: 'JSON_PATH' }
          if (cur?.kind === 'extract' && cur.enabled) {
            const emptyIndex = cur.extractors.findIndex(row => !row.variable && !row.expression)
            update(sel, { extractors: emptyIndex < 0 ? [...cur.extractors, extractor]
              : cur.extractors.map((row, index) => index === emptyIndex ? extractor : row) })
          } else {
            push([...ops, { kind: 'extract', enabled: true, extractors: [extractor] }])
            setSel(ops.length)
          }
        }} />
      </span>}
      {ops.length === 0 ? (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('proc.empty', '无操作')} />
      ) : (
        <div style={{ display: 'flex', gap: 12, border: '1px solid var(--border-soft)', borderRadius: 6 }}>
          {/* Left: processor name stays on one line (ellipsized); index/switch/delete never shrink. */}
          <div style={{ width: 216, flexShrink: 0, borderRight: '1px solid var(--border-soft)', padding: 8 }}>
            <Space direction="vertical" style={{ width: '100%' }} size={4}>
              {ops.map((o, i) => (
                <div
                  key={i}
                  onClick={() => setSel(i)}
                  style={{ display: 'flex', alignItems: 'center', gap: 6, padding: '6px 8px', borderRadius: 4, cursor: 'pointer', background: i === sel ? 'var(--brand-soft)' : 'transparent' }}
                >
                  <span style={{ width: 18, height: 18, flexShrink: 0, borderRadius: '50%', background: 'var(--brand)', color: '#fff', fontSize: 11, display: 'inline-flex', alignItems: 'center', justifyContent: 'center' }}>{i + 1}</span>
                  <span style={{ flex: 1, minWidth: 0, fontSize: 13, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{LABEL[o.kind]}</span>
                  <Switch size="small" checked={o.enabled} onChange={(c) => update(i, { enabled: c })} onClick={(_, e) => e.stopPropagation()} />
                  <Button type="text" size="small" danger style={{ width: 22, minWidth: 22, padding: 0, flexShrink: 0 }} icon={<DeleteOutlined />} onClick={(e) => { e.stopPropagation(); const next = ops.filter((_, idx) => idx !== i); push(next); setSel(Math.max(0, Math.min(sel, next.length - 1))) }} />
                </div>
              ))}
            </Space>
          </div>
          <div style={{ flex: 1, minWidth: 0, padding: 12 }}>{cur?.kind === 'sandboxScript' ? <ScriptActionEditor projectId={projectId} value={cur.action} onChange={action => update(sel, { action })} /> : cur?.kind === 'plugin' ? <PreparationProfileSelect projectId={projectId} environmentId={environmentId} value={cur.profileId} onChange={profileId => update(sel, { profileId })} /> : cur && <OpEditor op={cur} onChange={(p) => update(sel, p)} t={t} responseBody={responseBody} preparation={!allowed.includes('extract')} />}</div>
        </div>
      )}
    </div>
  )
}

function OpEditor({ op, onChange, t, preparation, responseBody }: { op: Op; onChange: (p: Partial<Op>) => void; t: (k: string, f?: string) => string; preparation: boolean; responseBody?: string }) {
  if (op.kind === 'sandboxScript' || op.kind === 'plugin') return null
  if (op.kind === 'wait') {
    return (
      <Space>
        <span>{t('case.waitMs', '等待(ms)')}</span>
        <InputNumber min={0} max={600000} value={op.ms} onChange={(v) => onChange({ ms: Number(v) || 0 })} style={{ width: 140 }} />
      </Space>
    )
  }
  if (op.kind === 'extract') {
    const setE = (i: number, patch: Partial<Extractor>) => onChange({ extractors: op.extractors.map((e, idx) => (idx === i ? { ...e, ...patch } : e)) })
    return (
      <Space direction="vertical" style={{ width: '100%' }} size={8}>
        {op.extractors.map((e, i) => (
          <div key={i}>
            <Space.Compact style={{ width: '100%' }}>
              <Input placeholder={t('case.varName', '变量名')} value={e.variable} onChange={(ev) => setE(i, { variable: ev.target.value })} style={{ width: '29%', flexShrink: 0 }} className="ms-mono" />
              <Select value={e.kind} onChange={(v) => setE(i, { kind: v })} style={{ width: 120 }} options={[{ value: 'JSON_PATH', label: 'JSONPath' }, { value: 'REGEX', label: t('proc.regex', '正则') }]} />
              <Input placeholder={e.kind === 'JSON_PATH' ? '$.data.id' : '"id":\\s*"(\\w+)"'} value={e.expression} onChange={(ev) => setE(i, { expression: ev.target.value })} className="ms-mono" />
              <Button icon={<DeleteOutlined />} onClick={() => onChange({ extractors: op.extractors.filter((_, idx) => idx !== i) })} />

            </Space.Compact>
            {e.kind === 'JSON_PATH' && <div style={{ marginTop: 4 }}>
              <ResponseJsonPathPicker responseBody={responseBody}
                onSelect={field => setE(i, { expression: field.expression, variable: e.variable.trim() ? e.variable : field.variable })} />
            </div>}
          </div>
        ))}
        <Button icon={<PlusOutlined />} size="small" onClick={() => onChange({ extractors: [...op.extractors, { variable: '', kind: 'JSON_PATH', expression: '' }] })}>{t('case.addRow', '加一行')}</Button>
      </Space>
    )
  }
  if (op.kind === 'script') {
    return (
      <Space direction="vertical" style={{ width: '100%' }} size={8}>
        <Typography.Text type="warning" style={{ fontSize: 12 }}>{preparation ? '服务端 HTTP 调试和场景步骤可执行 JavaScript 前置处理。使用 vars.name 读写字符串变量。' : '后置脚本暂不支持，执行时会明确报错。'}</Typography.Text>
        <Select value={op.lang} onChange={(v) => onChange({ lang: v })} style={{ width: 200 }} options={[{ value: 'JS', label: 'JavaScript' }, { value: 'BEANSHELL', label: 'BeanShell（暂不支持）', disabled: true }]} />
        <Input.TextArea rows={8} value={op.code} onChange={(e) => onChange({ code: e.target.value })} placeholder="// script" className="ms-mono" />
      </Space>
    )
  }
  // sql
  return (
    <Space direction="vertical" style={{ width: '100%' }} size={8}>
      <Typography.Text type="warning" style={{ fontSize: 12 }}>{preparation ? '填当前环境的数据库配置 ID。SQL 默认只读，名称作为结果变量。例如名称填 baseline，断言预期值可填 {{baseline#/rows/0/0}}。' : '后置 SQL 暂不支持，执行时会明确报错。'}</Typography.Text>
      <Input placeholder={t('proc.sqlName', 'SQL 操作名称')} value={op.name} onChange={(e) => onChange({ name: e.target.value })} />
      <Input placeholder={t('proc.datasource', '数据源')} value={op.datasource} onChange={(e) => onChange({ datasource: e.target.value })} />
      <Input.TextArea rows={6} value={op.sql} onChange={(e) => onChange({ sql: e.target.value })} placeholder="SELECT ..." className="ms-mono" />
    </Space>
  )
}
