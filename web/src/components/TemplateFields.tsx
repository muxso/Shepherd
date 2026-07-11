import { useEffect, useState } from 'react'
import { DatePicker, Form, Input, InputNumber, Select } from 'antd'
import dayjs, { type Dayjs } from 'dayjs'
import { api, type TemplateField } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'
import { FIELDS_TEMPLATE_NAME, defaultTemplateFields, fieldLabel, normalizeFields, type TemplateKind } from '../fieldTemplates'

// 字段模板消费端:加载某 kind 的字段配置(带内置默认回退)+ 自定义字段的动态渲染/取值。
// 三个创建表单(需求/功能用例/缺陷)复用。

/** 自定义字段在 Form 里的分组名:values.cf[key]。 */
export const CF_GROUP = 'cf'

/** 加载当前项目某 kind 的字段配置;未配置或加载失败回落内置默认。 */
export function useFieldTemplate(kind: TemplateKind): { fields: TemplateField[]; loading: boolean } {
  const { projectId } = useApp()
  const [fields, setFields] = useState<TemplateField[]>(() => defaultTemplateFields(kind))
  const [loading, setLoading] = useState(false)
  useEffect(() => {
    if (!projectId) {
      setFields(defaultTemplateFields(kind))
      return
    }
    let alive = true
    setLoading(true)
    api.projectTemplates(projectId, kind)
      .then((p) => {
        if (!alive) return
        const row = (p.items ?? []).find((x) => x.name === FIELDS_TEMPLATE_NAME)
        setFields(normalizeFields(kind, row?.config))
      })
      .catch(() => alive && setFields(defaultTemplateFields(kind)))
      .finally(() => alive && setLoading(false))
    return () => { alive = false }
  }, [projectId, kind])
  return { fields, loading }
}

/** 单个自定义字段 → 表单项(text/textarea/select/multiselect/date/number)。 */
export function CustomFieldItem({ kind, field }: { kind: TemplateKind; field: TemplateField }) {
  const { t } = useI18n()
  if (field.system || !field.enabled) return null
  const label = fieldLabel(t, kind, field)
  const opts = (field.options ?? []).map((o) => ({ value: o, label: o }))
  const control =
    field.type === 'textarea' ? <Input.TextArea rows={3} /> :
    field.type === 'select' ? <Select allowClear options={opts} /> :
    field.type === 'multiselect' ? <Select mode="multiple" allowClear options={opts} /> :
    field.type === 'date' ? <DatePicker format="YYYY-MM-DD" style={{ width: '100%' }} /> :
    field.type === 'number' ? <InputNumber style={{ width: '100%' }} /> :
    <Input />
  return (
    <Form.Item name={[CF_GROUP, field.key]} label={label} rules={field.required ? [{ required: true, message: t('tmpl.requiredMsg', '请填写') + ' ' + label }] : undefined}>
      {control}
    </Form.Item>
  )
}

/** 一组自定义字段(按数组序渲染,跳过系统/隐藏字段)。 */
export function CustomFieldItems({ kind, fields }: { kind: TemplateKind; fields: TemplateField[] }) {
  return (
    <>
      {fields.filter((f) => !f.system && f.enabled).map((f) => (
        <CustomFieldItem key={f.key} kind={kind} field={f} />
      ))}
    </>
  )
}

/** 提交时收集自定义字段值 → map<key,string>(多选逗号拼接,date YYYY-MM-DD,number String;空值跳过)。 */
export function collectCustomValues(fields: TemplateField[], cf?: Record<string, unknown>): Record<string, string> {
  const out: Record<string, string> = {}
  if (!cf) return out
  for (const f of fields) {
    if (f.system || !f.enabled) continue
    const v = cf[f.key]
    if (v === undefined || v === null) continue
    if (f.type === 'multiselect') {
      const arr = (Array.isArray(v) ? v : [v]).map((x) => String(x).trim()).filter(Boolean)
      if (arr.length) out[f.key] = arr.join(',')
    } else if (f.type === 'date') {
      const d = v as Dayjs
      if (dayjs.isDayjs(d)) out[f.key] = d.format('YYYY-MM-DD')
    } else if (f.type === 'number') {
      if (typeof v === 'number' && !Number.isNaN(v)) out[f.key] = String(v)
    } else {
      const s = String(v).trim()
      if (s) out[f.key] = s
    }
  }
  return out
}

/** 编辑回填:已存的 map<key,string> → 表单值(按字段类型解析)。 */
export function customFormValues(fields: TemplateField[], saved?: Record<string, string>): Record<string, unknown> {
  const out: Record<string, unknown> = {}
  if (!saved) return out
  for (const f of fields) {
    if (f.system) continue
    const v = saved[f.key]
    if (v === undefined || v === '') continue
    if (f.type === 'multiselect') out[f.key] = v.split(',').map((x) => x.trim()).filter(Boolean)
    else if (f.type === 'date') out[f.key] = dayjs(v).isValid() ? dayjs(v) : undefined
    else if (f.type === 'number') out[f.key] = Number.isNaN(Number(v)) ? undefined : Number(v)
    else out[f.key] = v
  }
  return out
}
