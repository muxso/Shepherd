import { useState } from 'react'
import { Button, Form, Input, Modal, Select, Space } from 'antd'
import { message } from '../../feedback'
import { api, ApiError, userStore } from '../../api'
import { useI18n } from '../../i18n'
import { regAdd, type RegItem } from '../../registry'
import { groupIdOf, isGroup, joinTags, moduleOf, planRegUpdate, tagsOf, type PlanModule } from './planLocal'

// Test plan / plan group form (name / module / owning group / tags).
// Create: api.createPlan (groups pass type='GROUP') + regAdd into the local registry.
// Edit: backend has no update endpoint — only local label/meta (module/group/tags) change, keeping list order.
// Create renders inline in a workspace tab (pass onCancel to show a cancel button); edit still uses PlanFormModal below.
export function PlanForm({
  mode,
  editing,
  projectId,
  modules,
  groups,
  onSaved,
  onCancel,
}: {
  /** plan = test plan; group = plan group (no owning-group field). */
  mode: 'plan' | 'group'
  /** Present = edit, absent = create. */
  editing?: RegItem | null
  projectId: string
  modules: PlanModule[]
  groups: RegItem[]
  /** list = updated registry; created = newly created plan (not set for groups), used to open its detail. */
  onSaved: (list: RegItem[], created?: { id: string; name: string }) => void
  /** Present = show a cancel button next to submit (inline-tab mode); omit in modal mode to keep the full-width button. */
  onCancel?: () => void
}) {
  const { t } = useI18n()
  const [saving, setSaving] = useState(false)

  // Module dropdown: tree flattened with depth indentation.
  const moduleOptions: { value: string; label: string }[] = []
  const walk = (pid: string | null, depth: number) => {
    modules
      .filter((m) => (m.parentId || null) === pid)
      .forEach((m) => {
        moduleOptions.push({ value: m.id, label: `${'　'.repeat(depth)}${m.name}` })
        walk(m.id, depth + 1)
      })
  }
  walk(null, 0)

  const submit = async (v: { name: string; module?: string; groupId?: string; tags?: string[] }) => {
    setSaving(true)
    const meta: Record<string, string> = {
      module: v.module || '',
      tags: joinTags(v.tags || []),
      ...(mode === 'plan' ? { groupId: v.groupId || '' } : {}),
    }
    try {
      if (editing) {
        onSaved(planRegUpdate(projectId, editing.id, { label: v.name, meta }))
        message.success(t('plan.saved', '已保存'))
      } else {
        const p = await api.createPlan({ projectId, name: v.name, ...(mode === 'group' ? { type: 'GROUP' } : {}) })
        const list = regAdd('plan', projectId, {
          id: p.id,
          label: v.name,
          createdAt: Date.now(),
          meta: { createdBy: userStore.get(), ...(mode === 'group' ? { kind: 'group' } : {}), ...meta },
        })
        message.success(mode === 'group' ? t('plan.groupCreated', '计划组已创建') : t('plan.created', '计划已创建'))
        onSaved(list, mode === 'plan' ? { id: p.id, name: v.name } : undefined)
      }
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('plan.createFail', '创建失败'))
    } finally {
      setSaving(false)
    }
  }

  const submitLabel = editing ? t('lv.save', '保存') : t('a.create', '创建')

  return (
    <Form
      layout="vertical"
      onFinish={submit}
      initialValues={
        editing
          ? { name: editing.label, module: moduleOf(editing) || undefined, groupId: groupIdOf(editing) || undefined, tags: tagsOf(editing) }
          : { tags: [] }
      }
    >
      <Form.Item name="name" label={t('plan.colName', '测试计划名称')} rules={[{ required: true }]}>
        <Input placeholder={t('plan.namePlaceholder', '如:回归冒烟')} autoFocus />
      </Form.Item>
      <Form.Item name="module" label={t('plan.belongModule', '所属模块')}>
        <Select allowClear placeholder={t('plan.moduleUnfiled', '未规划')} options={moduleOptions} />
      </Form.Item>
      {mode === 'plan' && (
        <Form.Item name="groupId" label={t('plan.belongGroup', '所属计划组')}>
          <Select
            allowClear
            placeholder={t('plan.noGroup', '不归属计划组')}
            options={groups.filter(isGroup).map((g) => ({ value: g.id, label: g.label }))}
          />
        </Form.Item>
      )}
      <Form.Item name="tags" label={t('plan.tags', '标签')}>
        <Select mode="tags" placeholder={t('plan.tagsPh', '输入后回车添加标签')} open={false} suffixIcon={null} />
      </Form.Item>
      {onCancel ? (
        <Space>
          <Button type="primary" htmlType="submit" loading={saving}>{submitLabel}</Button>
          <Button onClick={onCancel}>{t('a.cancel', '取消')}</Button>
        </Space>
      ) : (
        <Button type="primary" htmlType="submit" loading={saving} block>{submitLabel}</Button>
      )}
    </Form>
  )
}

// Edit modal for test plan / plan group (create moved to a workspace tab — see PlanForm usage in TestPlans).
export default function PlanFormModal({
  open,
  mode,
  editing,
  projectId,
  modules,
  groups,
  onClose,
  onSaved,
}: {
  open: boolean
  mode: 'plan' | 'group'
  editing?: RegItem | null
  projectId: string
  modules: PlanModule[]
  groups: RegItem[]
  onClose: () => void
  onSaved: (list: RegItem[], created?: { id: string; name: string }) => void
}) {
  const { t } = useI18n()
  const title = editing
    ? mode === 'group' ? t('plan.editGroup', '编辑计划组') : t('plan.editPlan', '编辑测试计划')
    : mode === 'group' ? t('plan.newGroup', '新建计划组') : t('plan.newPlan', '新建测试计划')

  return (
    <Modal title={title} open={open} onCancel={onClose} footer={null} destroyOnHidden>
      <PlanForm mode={mode} editing={editing} projectId={projectId} modules={modules} groups={groups} onSaved={onSaved} />
    </Modal>
  )
}
