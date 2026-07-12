import { useEffect, useState } from 'react'
import { DatePicker, Form, Input, InputNumber, Select } from 'antd'
import dayjs, { type Dayjs } from 'dayjs'
import { api, type TemplateField } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'
import { FIELDS_TEMPLATE_NAME, defaultTemplateFields, fieldLabel, normalizeFields, type TemplateKind } from '../fieldTemplates'

// Field-template consumer side: loads a kind's field config (with built-in default fallback)
// plus dynamic rendering/collection of custom fields. Shared by the requirement/case/bug create forms.

/** Form group name for custom fields: values.cf[key]. */
export const CF_GROUP = 'cf'

/** Load the current project's field config for a kind; falls back to built-in defaults when unconfigured or on load failure. */
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

/** One custom field → form item (text/textarea/select/multiselect/date/number). */
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

/** Custom fields in array order, skipping system/hidden fields. */
export function CustomFieldItems({ kind, fields }: { kind: TemplateKind; fields: TemplateField[] }) {
  return (
    <>
      {fields.filter((f) => !f.system && f.enabled).map((f) => (
        <CustomFieldItem key={f.key} kind={kind} field={f} />
      ))}
    </>
  )
}

/** Collect custom field values on submit → map<key,string> (multiselect comma-joined, date YYYY-MM-DD, number stringified; empties skipped). */
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

/** Edit backfill: saved map<key,string> → form values, parsed per field type. */
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
