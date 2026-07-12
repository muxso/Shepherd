import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Button, Form, Input, Modal, Popconfirm, Select, Space, Switch, Table, Tabs, Tag } from 'antd'
import { DeleteOutlined, HolderOutlined, PlusOutlined } from '@ant-design/icons'
import type { ColumnsType } from 'antd/es/table'
import { message } from '../feedback'
import { api, ApiError, type TemplateField, type TemplateFieldType } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'
import { PageBody, PageContainer, PageHeader, SelectProjectEmpty } from '../components/Page'
import {
  FIELDS_TEMPLATE_NAME,
  defaultTemplateFields,
  fieldLabel,
  isLockedField,
  newCustomFieldKey,
  normalizeFields,
  type TemplateKind,
} from '../fieldTemplates'

// Template management (field templates): one field config per module (requirement/test case/bug)
// controlling which fields the create form shows, their order and required flags; custom fields can be added/removed.

const FIELD_TYPES: TemplateFieldType[] = ['text', 'textarea', 'select', 'multiselect', 'date', 'number']

export default function ProjectTemplates() {
  const { t } = useI18n()
  const { projectId } = useApp()
  if (!projectId) return <SelectProjectEmpty />
  return (
    <PageContainer>
      <PageHeader title={t('tmpl.title', '模板管理')} />
      <PageBody>
        <Tabs
          items={[
            { key: 'requirement', label: t('tmpl.kindRequirement', '需求'), children: <FieldTemplateEditor kind="requirement" projectId={projectId} /> },
            { key: 'functional-case', label: t('tmpl.kindCase', '测试用例'), children: <FieldTemplateEditor kind="functional-case" projectId={projectId} /> },
            { key: 'bug', label: t('tmpl.kindBug', '缺陷'), children: <FieldTemplateEditor kind="bug" projectId={projectId} /> },
          ]}
        />
      </PageBody>
    </PageContainer>
  )
}

function FieldTemplateEditor({ kind, projectId }: { kind: TemplateKind; projectId: string }) {
  const { t } = useI18n()
  const [fields, setFields] = useState<TemplateField[]>(() => defaultTemplateFields(kind))
  // Saved snapshot (JSON) for dirty checking; empty templateId = first save goes through POST.
  const [snapshot, setSnapshot] = useState(() => JSON.stringify(defaultTemplateFields(kind)))
  const [templateId, setTemplateId] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [saving, setSaving] = useState(false)
  const [addOpen, setAddOpen] = useState(false)

  const load = useCallback(() => {
    setLoading(true)
    api.projectTemplates(projectId, kind)
      .then((p) => {
        const row = (p.items ?? []).find((x) => x.name === FIELDS_TEMPLATE_NAME)
        const fs = normalizeFields(kind, row?.config)
        setTemplateId(row?.id ?? null)
        setFields(fs)
        setSnapshot(JSON.stringify(fs))
      })
      .catch(() => {
        const fs = defaultTemplateFields(kind)
        setTemplateId(null)
        setFields(fs)
        setSnapshot(JSON.stringify(fs))
      })
      .finally(() => setLoading(false))
  }, [projectId, kind])
  useEffect(load, [load])

  const dirty = useMemo(() => JSON.stringify(fields) !== snapshot, [fields, snapshot])

  // Row drag reordering (native HTML5 DnD): drag starts from the handle; the hovered row shows an insert indicator on its upper/lower half.
  const dragFrom = useRef<number | null>(null)
  const [dragOver, setDragOver] = useState<{ idx: number; after: boolean } | null>(null)
  const reorder = (from: number, to: number) =>
    setFields((fs) => {
      const next = [...fs]
      const [x] = next.splice(from, 1)
      next.splice(to, 0, x)
      return next
    })
  const patch = (key: string, p: Partial<TemplateField>) =>
    setFields((fs) => fs.map((f) => (f.key === key ? { ...f, ...p } : f)))
  const removeField = (key: string) => setFields((fs) => fs.filter((f) => f.key !== key))

  const save = async () => {
    setSaving(true)
    try {
      const config = { fields }
      if (templateId) {
        await api.updateProjectTemplate(templateId, { config })
      } else {
        const created = await api.createProjectTemplate(projectId, { kind, name: FIELDS_TEMPLATE_NAME, config })
        setTemplateId(created.id)
      }
      setSnapshot(JSON.stringify(fields))
      message.success(t('tmpl.saved', '已保存'))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('tmpl.saveFailed', '保存失败'))
    } finally {
      setSaving(false)
    }
  }

  const typeLabel = (ty: TemplateFieldType) => t(`tmpl.type.${ty}`, ty)

  const cols: ColumnsType<TemplateField> = [
    {
      title: t('tmpl.colOrder', '顺序'), width: 56,
      render: () => <HolderOutlined style={{ cursor: 'grab', color: 'var(--text-3)' }} />,
    },
    {
      title: t('tmpl.colField', '字段名'), ellipsis: true,
      render: (_v, f) => (
        <Space size={6}>
          <span>{fieldLabel(t, kind, f)}</span>
          {f.system && <Tag style={{ marginRight: 0 }}>{t('tmpl.systemTag', '系统')}</Tag>}
        </Space>
      ),
    },
    {
      title: t('tmpl.colType', '类型'), width: 120,
      render: (_v, f) => typeLabel(f.type),
    },
    {
      title: t('tmpl.colRequired', '必填'), width: 90,
      render: (_v, f) => (
        <Switch size="small" checked={f.required} disabled={isLockedField(kind, f)} onChange={(v) => patch(f.key, { required: v })} />
      ),
    },
    {
      title: t('tmpl.colEnabled', '显示'), width: 90,
      render: (_v, f) => (
        <Switch size="small" checked={f.enabled} disabled={isLockedField(kind, f)} onChange={(v) => patch(f.key, { enabled: v })} />
      ),
    },
    {
      title: t('req.action', '操作'), width: 90,
      render: (_v, f) =>
        f.system ? null : (
          <Popconfirm title={t('tmpl.deleteFieldConfirm', '删除该字段?')} onConfirm={() => removeField(f.key)}>
            <Button type="link" size="small" danger icon={<DeleteOutlined />}>{t('a.delete', '删除')}</Button>
          </Popconfirm>
        ),
    },
  ]

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'flex-end', alignItems: 'center', marginBottom: 12 }}>
        <Space>
          <Button icon={<PlusOutlined />} onClick={() => setAddOpen(true)}>{t('tmpl.addField', '添加字段')}</Button>
          <Button type="primary" loading={saving} disabled={!dirty} onClick={save}>{t('a.save', '保存')}</Button>
        </Space>
      </div>
      <Table<TemplateField>
        rowKey="key"
        size="middle"
        loading={loading}
        dataSource={fields}
        columns={cols}
        pagination={false}
        onRow={(_r, index) => ({
          draggable: true,
          onDragStart: (e) => {
            dragFrom.current = index ?? null
            e.dataTransfer.effectAllowed = 'move'
          },
          onDragOver: (e) => {
            e.preventDefault()
            const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
            setDragOver({ idx: index ?? 0, after: e.clientY > rect.top + rect.height / 2 })
          },
          onDrop: (e) => {
            e.preventDefault()
            const from = dragFrom.current
            if (from == null || dragOver == null) return
            let to = dragOver.idx + (dragOver.after ? 1 : 0)
            if (from < to) to--
            if (to !== from) reorder(from, to)
            dragFrom.current = null
            setDragOver(null)
          },
          onDragEnd: () => {
            dragFrom.current = null
            setDragOver(null)
          },
          style:
            dragOver != null && dragOver.idx === index
              ? { boxShadow: dragOver.after ? 'inset 0 -2px 0 var(--brand)' : 'inset 0 2px 0 var(--brand)' }
              : undefined,
        })}
      />
      <AddFieldModal
        open={addOpen}
        existing={fields}
        kind={kind}
        onClose={() => setAddOpen(false)}
        onAdd={(f) => { setFields((fs) => [...fs, f]); setAddOpen(false) }}
      />
    </div>
  )
}

type AddFieldValues = { label: string; type: TemplateFieldType; options?: string[]; required?: boolean }

function AddFieldModal({ open, existing, kind, onClose, onAdd }: {
  open: boolean
  existing: TemplateField[]
  kind: TemplateKind
  onClose: () => void
  onAdd: (f: TemplateField) => void
}) {
  const { t } = useI18n()
  const [form] = Form.useForm<AddFieldValues>()
  const type = Form.useWatch('type', form)
  useEffect(() => {
    if (open) form.resetFields()
  }, [open, form])

  const submit = (v: AddFieldValues) => {
    const label = v.label.trim()
    if (existing.some((f) => fieldLabel(t, kind, f) === label)) {
      message.warning(t('tmpl.dupField', '已有同名字段'))
      return
    }
    const withOptions = v.type === 'select' || v.type === 'multiselect'
    onAdd({
      key: newCustomFieldKey(),
      label,
      type: v.type,
      required: !!v.required,
      enabled: true,
      system: false,
      options: withOptions && v.options?.length ? v.options.map((o) => o.trim()).filter(Boolean) : undefined,
    })
  }

  return (
    <Modal title={t('tmpl.addField', '添加字段')} open={open} onCancel={onClose} onOk={() => form.submit()} okText={t('a.confirm', '确定')} cancelText={t('a.cancel', '取消')} destroyOnHidden>
      <Form form={form} layout="vertical" initialValues={{ type: 'text', required: false }} onFinish={submit}>
        <Form.Item name="label" label={t('tmpl.colField', '字段名')} rules={[{ required: true, whitespace: true }]}>
          <Input placeholder={t('tmpl.fieldNamePh', '如:所属版本')} autoFocus maxLength={40} />
        </Form.Item>
        <Form.Item name="type" label={t('tmpl.colType', '类型')}>
          <Select options={FIELD_TYPES.map((ty) => ({ value: ty, label: t(`tmpl.type.${ty}`, ty) }))} />
        </Form.Item>
        {(type === 'select' || type === 'multiselect') && (
          <Form.Item name="options" label={t('tmpl.options', '选项')} rules={[{ required: true, message: t('tmpl.optionsRequired', '至少一个选项') }]}>
            <Select mode="tags" open={false} suffixIcon={null} tokenSeparators={[',']} placeholder={t('tmpl.optionsPh', '输入后回车添加')} />
          </Form.Item>
        )}
        <Form.Item name="required" label={t('tmpl.colRequired', '必填')} valuePropName="checked">
          <Switch size="small" />
        </Form.Item>
      </Form>
    </Modal>
  )
}
