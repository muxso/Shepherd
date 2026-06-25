import { useState } from 'react'
import { Button, Dropdown, Empty, Input, InputNumber, Segmented, Select, Space, Typography } from 'antd'
import { PlusOutlined, DeleteOutlined } from '@ant-design/icons'
import { useI18n } from '../i18n'

// 后端断言(api-runner Assertion,serde tag=type/content=args)。
export type Assertion = Record<string, unknown>

type Cat = 'status' | 'header' | 'body' | 'variable' | 'time'

// 内部行:统一承载各类断言字段,按类别序列化为后端 JSON。
interface Row {
  cat: Cat
  name: string // 响应头名 / 变量名
  bodyMode: 'jsonpath' | 'whole' // 响应体:JSONPath 取值 或 整体
  path: string // JSONPath 表达式
  condition: string // MatchCondition(SCREAMING_SNAKE)
  expected: string
}

// value=后端 MatchCondition(数据,不翻译);labelKey/fallback 在 render 时经 t() 解析为双语。
const NUMERIC_CONDS = [
  { value: 'EQUALS', labelKey: 'assert.opEquals', fallback: '等于' },
  { value: 'NOT_EQUALS', labelKey: 'assert.opNotEquals', fallback: '不等于' },
  { value: 'GT', labelKey: 'assert.opGt', fallback: '大于' },
  { value: 'GT_OR_EQUALS', labelKey: 'assert.opGte', fallback: '大于等于' },
  { value: 'LT', labelKey: 'assert.opLt', fallback: '小于' },
  { value: 'LT_OR_EQUALS', labelKey: 'assert.opLte', fallback: '小于等于' },
  { value: 'UNCHECKED', labelKey: 'assert.opUnchecked', fallback: '不校验' },
]
const TEXT_CONDS = [
  { value: 'CONTAINS', labelKey: 'assert.opContains', fallback: '包含' },
  { value: 'NOT_CONTAINS', labelKey: 'assert.opNotContains', fallback: '不包含' },
  { value: 'EQUALS', labelKey: 'assert.opEquals', fallback: '等于' },
  { value: 'NOT_EQUALS', labelKey: 'assert.opNotEquals', fallback: '不等于' },
  { value: 'REGEX', labelKey: 'assert.opRegex', fallback: '正则' },
]

const emptyRow = (cat: Cat): Row => ({
  cat,
  name: cat === 'header' ? 'Content-Type' : '',
  bodyMode: 'jsonpath',
  path: '',
  condition: cat === 'header' || cat === 'body' ? 'CONTAINS' : 'EQUALS',
  expected: cat === 'status' ? '200' : '',
})

function toAssertion(r: Row): Assertion {
  switch (r.cat) {
    case 'status':
      return { type: 'ResponseCode', args: { condition: r.condition, expected: r.expected } }
    case 'header':
      return { type: 'ResponseHeader', args: { name: r.name, condition: r.condition, expected: r.expected } }
    case 'variable':
      return { type: 'Variable', args: { name: r.name, condition: r.condition, expected: r.expected } }
    case 'time':
      return { type: 'ResponseTime', args: { max_ms: Number(r.expected) || 0 } }
    case 'body':
      return r.bodyMode === 'jsonpath'
        ? { type: 'JsonPath', args: { path: r.path, condition: r.condition, expected: r.expected } }
        : { type: 'ResponseBody', args: { condition: r.condition, expected: r.expected } }
  }
}

function fromAssertion(a: Assertion): Row {
  const ty = String(a.type || 'StatusIs')
  const args = a.args as any
  const base = emptyRow('status')
  switch (ty) {
    case 'StatusIs':
      return { ...base, cat: 'status', condition: 'EQUALS', expected: String(args ?? 200) }
    case 'ResponseCode':
      return { ...base, cat: 'status', condition: String(args?.condition || 'EQUALS'), expected: String(args?.expected ?? '') }
    case 'HeaderEquals':
      return { ...emptyRow('header'), name: String(args?.name ?? ''), condition: 'EQUALS', expected: String(args?.value ?? '') }
    case 'ResponseHeader':
      return { ...emptyRow('header'), name: String(args?.name ?? ''), condition: String(args?.condition || 'CONTAINS'), expected: String(args?.expected ?? '') }
    case 'Variable':
      return { ...emptyRow('variable'), name: String(args?.name ?? ''), condition: String(args?.condition || 'EQUALS'), expected: String(args?.expected ?? '') }
    case 'ResponseTime':
      return { ...emptyRow('time'), expected: String(args?.max_ms ?? 500) }
    case 'BodyContains':
      return { ...emptyRow('body'), bodyMode: 'whole', condition: 'CONTAINS', expected: String(args ?? '') }
    case 'ResponseBody':
      return { ...emptyRow('body'), bodyMode: 'whole', condition: String(args?.condition || 'CONTAINS'), expected: String(args?.expected ?? '') }
    case 'JsonFieldEquals':
      return { ...emptyRow('body'), bodyMode: 'jsonpath', path: String(args?.pointer ?? ''), condition: 'EQUALS', expected: String(args?.expected ?? '') }
    case 'JsonPath':
      return { ...emptyRow('body'), bodyMode: 'jsonpath', path: String(args?.path ?? ''), condition: String(args?.condition || 'EQUALS'), expected: String(args?.expected ?? '') }
    default:
      return base
  }
}

// 受控:value 为后端断言数组,onChange 回传同结构。断言矩阵(左列类别 + 右侧编辑)。
export default function AssertionEditor({
  value,
  onChange,
}: {
  value?: Assertion[]
  onChange?: (v: Assertion[]) => void
}) {
  const { t } = useI18n()
  const [rows, setRows] = useState<Row[]>(() => (value && value.length ? value.map(fromAssertion) : []))
  const [sel, setSel] = useState(0)

  const push = (next: Row[]) => {
    setRows(next)
    onChange?.(next.map(toAssertion))
  }
  const update = (i: number, patch: Partial<Row>) => push(rows.map((r, idx) => (idx === i ? { ...r, ...patch } : r)))
  const add = (cat: Cat) => {
    push([...rows, emptyRow(cat)])
    setSel(rows.length)
  }

  const CAT_LABEL: Record<Cat, string> = {
    status: t('assert.catStatus', '状态码'),
    header: t('assert.catHeader', '响应头'),
    body: t('assert.catBody', '响应体'),
    variable: t('assert.catVariable', '变量'),
    time: t('assert.catTime', '响应耗时'),
  }
  const addMenu = {
    items: (['status', 'header', 'body', 'variable', 'time'] as Cat[]).map((c) => ({ key: c, label: CAT_LABEL[c] })),
    onClick: ({ key }: { key: string }) => add(key as Cat),
  }

  const cur = rows[sel]

  return (
    <div>
      <Dropdown menu={addMenu} trigger={['click']}>
        <Button type="primary" ghost icon={<PlusOutlined />} size="small" style={{ marginBottom: 10 }}>
          {t('assert.add', '断言')}
        </Button>
      </Dropdown>
      {rows.length === 0 ? (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('assert.emptyHint', '无断言(执行只看传输是否可达)')} />
      ) : (
        <div style={{ display: 'flex', gap: 12, border: '1px solid var(--border-soft)', borderRadius: 6 }}>
          {/* 左列:断言类别列表 */}
          <div style={{ width: 160, borderRight: '1px solid var(--border-soft)', padding: 8 }}>
            <Space direction="vertical" style={{ width: '100%' }} size={4}>
              {rows.map((r, i) => (
                <div
                  key={i}
                  onClick={() => setSel(i)}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 6,
                    padding: '6px 8px',
                    borderRadius: 4,
                    cursor: 'pointer',
                    background: i === sel ? 'var(--brand-soft)' : 'transparent',
                  }}
                >
                  <span style={{ width: 18, height: 18, borderRadius: '50%', background: 'var(--brand)', color: '#fff', fontSize: 11, display: 'inline-flex', alignItems: 'center', justifyContent: 'center' }}>{i + 1}</span>
                  <span style={{ flex: 1, fontSize: 13 }}>{CAT_LABEL[r.cat]}</span>
                  <Button
                    type="text"
                    size="small"
                    danger
                    icon={<DeleteOutlined />}
                    onClick={(e) => {
                      e.stopPropagation()
                      const next = rows.filter((_, idx) => idx !== i)
                      push(next)
                      setSel(Math.max(0, Math.min(sel, next.length - 1)))
                    }}
                  />
                </div>
              ))}
            </Space>
          </div>
          {/* 右侧:当前断言编辑 */}
          <div style={{ flex: 1, padding: 12 }}>
            {cur && <RowEditor row={cur} onChange={(p) => update(sel, p)} t={t} />}
          </div>
        </div>
      )}
    </div>
  )
}

function RowEditor({ row, onChange, t }: { row: Row; onChange: (p: Partial<Row>) => void; t: (k: string, f?: string) => string }) {
  const condOptions = row.cat === 'status' || row.cat === 'variable' ? NUMERIC_CONDS : TEXT_CONDS
  if (row.cat === 'time') {
    return (
      <Space>
        <span>{t('assert.maxMs', '最大耗时(ms)')}</span>
        <InputNumber min={0} value={Number(row.expected) || 0} onChange={(v) => onChange({ expected: String(v ?? 0) })} style={{ width: 140 }} />
      </Space>
    )
  }
  return (
    <Space direction="vertical" style={{ width: '100%' }} size={10}>
      {row.cat === 'body' && (
        <Segmented
          size="small"
          value={row.bodyMode}
          onChange={(v) => onChange({ bodyMode: v as Row['bodyMode'] })}
          options={[
            { value: 'jsonpath', label: 'JSONPath' },
            { value: 'whole', label: t('assert.wholeBody', '整体') },
          ]}
        />
      )}
      <Space.Compact style={{ width: '100%' }}>
        {row.cat === 'header' && (
          <Input value={row.name} onChange={(e) => onChange({ name: e.target.value })} placeholder="Content-Type" style={{ width: 200 }} className="ms-mono" />
        )}
        {row.cat === 'variable' && (
          <Input value={row.name} onChange={(e) => onChange({ name: e.target.value })} placeholder={t('assert.varName', '变量名')} style={{ width: 200 }} className="ms-mono" />
        )}
        {row.cat === 'body' && row.bodyMode === 'jsonpath' && (
          <Input value={row.path} onChange={(e) => onChange({ path: e.target.value })} placeholder="$.data.id" style={{ width: 240 }} className="ms-mono" />
        )}
        <Select value={row.condition} onChange={(v) => onChange({ condition: v })} options={condOptions.map((o) => ({ value: o.value, label: t(o.labelKey, o.fallback) }))} style={{ width: 130 }} />
        {row.condition !== 'UNCHECKED' && (
          <Input value={row.expected} onChange={(e) => onChange({ expected: e.target.value })} placeholder={t('assert.matchValue', '匹配值')} />
        )}
      </Space.Compact>
      {row.cat === 'variable' && (
        <Typography.Text type="secondary" style={{ fontSize: 12 }}>{t('assert.varHint', '变量来自前置/后置「提取参数」或环境变量')}</Typography.Text>
      )}
    </Space>
  )
}
