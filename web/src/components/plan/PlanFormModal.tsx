import { useState } from 'react'
import { Button, Collapse, DatePicker, Form, Input, InputNumber, Radio, Select, Space, Switch, Tooltip } from 'antd'
import { QuestionCircleOutlined } from '@ant-design/icons'
import type { Dayjs } from 'dayjs'
import EditDrawer from '../EditDrawer'
import { message } from '../../feedback'
import { api, ApiError, userStore } from '../../api'
import { useI18n } from '../../i18n'
import { regAdd, type RegItem } from '../../registry'
import { groupIdOf, isGroup, joinTags, moduleOf, planRegUpdate, tagsOf, type PlanModule } from './planLocal'

type PlanFormValues = {
  name: string
  description?: string
  createTo?: 'module' | 'group'
  module?: string
  groupId?: string
  range?: [Dayjs | null, Dayjs | null]
  tags?: string[]
  allowDuplicateCases?: boolean
  autoUpdateStatus?: boolean
  passThreshold?: number
}

// Test plan / plan group form.
// Create (drawer): full field set — name / description / create-to (module|group) /
// dates / tags / more-settings (dup cases, auto status, pass threshold); persists the
// extras via PUT /test-plan/{id} after the create call. Edit: local registry only
// (label/module/group/tags), keeping list order.
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
  /** list = updated registry; created = newly created plan (not set for groups); stay = keep the form open (save-and-continue). */
  onSaved: (list: RegItem[], created?: { id: string; name: string }, stay?: boolean) => void
  /** Cancel handler for the drawer/tab footer. */
  onCancel?: () => void
}) {
  const { t } = useI18n()
  const [form] = Form.useForm<PlanFormValues>()
  const [saving, setSaving] = useState(false)
  const createTo = Form.useWatch('createTo', form) || 'module'

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

  const doSubmit = async (stay: boolean) => {
    const v = await form.validateFields()
    setSaving(true)
    const meta: Record<string, string> = {
      module: (v.createTo !== 'group' ? v.module : '') || '',
      tags: joinTags(v.tags || []),
      ...(mode === 'plan' ? { groupId: (v.createTo === 'group' ? v.groupId : '') || '' } : {}),
    }
    try {
      if (editing) {
        onSaved(planRegUpdate(projectId, editing.id, { label: v.name, meta }))
        message.success(t('plan.saved', '已保存'))
      } else {
        const p = await api.createPlan({ projectId, name: v.name, ...(mode === 'group' ? { type: 'GROUP' } : {}) })
        if (mode === 'plan') {
          // Persist the extended fields; the plan row already exists if this fails.
          try {
            await api.updatePlan(p.id, {
              description: v.description || '',
              tags: v.tags || [],
              moduleId: meta.module,
              groupId: meta.groupId || '',
              startAt: v.range?.[0] ? v.range[0].valueOf() : 0,
              endAt: v.range?.[1] ? v.range[1].valueOf() : 0,
              allowDuplicateCases: !!v.allowDuplicateCases,
              autoUpdateStatus: !!v.autoUpdateStatus,
              passThreshold: v.passThreshold ?? 100,
            })
          } catch (e) {
            if (e instanceof ApiError && e.status === 403) message.warning(t('plan.updatePermHint', '扩展设置未保存(权限不足),重新登录后可在「编辑」中补充'))
          }
        }
        const list = regAdd('plan', projectId, {
          id: p.id,
          label: v.name,
          createdAt: Date.now(),
          meta: { createdBy: userStore.get(), ...(mode === 'group' ? { kind: 'group' } : {}), ...meta },
        })
        message.success(mode === 'group' ? t('plan.groupCreated', '计划组已创建') : t('plan.created', '计划已创建'))
        if (stay) form.resetFields()
        onSaved(list, mode === 'plan' && !stay ? { id: p.id, name: v.name } : undefined, stay)
      }
    } catch (e) {
      if (e instanceof Error && 'errorFields' in e) return
      message.error(e instanceof ApiError ? e.message : t('plan.createFail', '创建失败'))
    } finally {
      setSaving(false)
    }
  }

  const isCreatePlan = mode === 'plan' && !editing

  return (
    <Form
      form={form}
      layout="vertical"
      initialValues={
        editing
          ? { name: editing.label, module: moduleOf(editing) || undefined, groupId: groupIdOf(editing) || undefined, tags: tagsOf(editing) }
          : { tags: [], createTo: 'module', passThreshold: 100 }
      }
    >
      <Form.Item name="name" label={isCreatePlan ? t('plan.nameLabel', '计划名称') : t('plan.colName', '测试计划名称')} rules={[{ required: true }]}>
        <Input placeholder={isCreatePlan ? t('plan.nameInputPh', '请输入测试计划名称') : t('plan.namePlaceholder', '如:回归冒烟')} autoFocus />
      </Form.Item>
      {isCreatePlan && (
        <Form.Item name="description" label={t('plan.desc', '描述')}>
          <Input.TextArea rows={3} placeholder={t('plan.inputPh', '请输入')} />
        </Form.Item>
      )}
      {isCreatePlan && (
        <Form.Item name="createTo" label={t('plan.createTo', '创建到')}>
          <Radio.Group
            options={[
              { value: 'module', label: t('plan.radioModule', '模块') },
              { value: 'group', label: t('plan.radioGroup', '计划组') },
            ]}
          />
        </Form.Item>
      )}
      {(mode === 'group' || !isCreatePlan || createTo === 'module') && (
        <Form.Item name="module" label={t('plan.belongModule', '所属模块')}>
          <Select allowClear placeholder={t('plan.moduleUnfiled', '未规划')} options={moduleOptions} />
        </Form.Item>
      )}
      {mode === 'plan' && (!isCreatePlan || createTo === 'group') && (
        <Form.Item name="groupId" label={t('plan.belongGroup', '所属计划组')}>
          <Select
            allowClear
            placeholder={t('plan.noGroup', '不归属计划组')}
            options={groups.filter(isGroup).map((g) => ({ value: g.id, label: g.label }))}
          />
        </Form.Item>
      )}
      {isCreatePlan && (
        <Form.Item name="range" label={t('plan.dateRange', '计划起止时间')}>
          <DatePicker.RangePicker style={{ width: '100%' }} allowEmpty={[true, true]} />
        </Form.Item>
      )}
      <Form.Item name="tags" label={t('plan.tags', '标签')}>
        <Select mode="tags" placeholder={t('plan.tagsPh', '输入后回车添加标签')} open={false} suffixIcon={null} />
      </Form.Item>
      {isCreatePlan && (
        <Collapse
          ghost
          defaultActiveKey={[]}
          items={[
            {
              key: 'more',
              label: <span style={{ color: 'var(--brand)' }}>{t('plan.moreSettings', '更多设置')}</span>,
              children: (
                <>
                  <Form.Item style={{ marginBottom: 12 }}>
                    <Space>
                      <Form.Item name="allowDuplicateCases" valuePropName="checked" noStyle>
                        <Switch size="small" />
                      </Form.Item>
                      <span>{t('plan.allowDup', '允许关联重复用例')}</span>
                      <Tooltip title={t('plan.allowDupTip', '开启后,同一用例可重复关联到本计划')}>
                        <QuestionCircleOutlined style={{ color: 'var(--text-3)' }} />
                      </Tooltip>
                    </Space>
                  </Form.Item>
                  <Form.Item style={{ marginBottom: 12 }}>
                    <Space>
                      <Form.Item name="autoUpdateStatus" valuePropName="checked" noStyle>
                        <Switch size="small" />
                      </Form.Item>
                      <span>{t('plan.autoStatus', '允许自动更新状态')}</span>
                      <Tooltip title={t('plan.autoStatusTip', '开启后,执行结果会自动更新计划状态')}>
                        <QuestionCircleOutlined style={{ color: 'var(--text-3)' }} />
                      </Tooltip>
                    </Space>
                  </Form.Item>
                  <Form.Item
                    name="passThreshold"
                    label={
                      <Space size={4}>
                        {t('plan.threshold', '通过阈值')}
                        <Tooltip title={t('plan.thresholdTip', '通过率达到该百分比时计划判定为通过')}>
                          <QuestionCircleOutlined style={{ color: 'var(--text-3)' }} />
                        </Tooltip>
                      </Space>
                    }
                    rules={[{ required: true }]}
                  >
                    <InputNumber min={0} max={100} precision={2} style={{ width: 140 }} />
                  </Form.Item>
                </>
              ),
            },
          ]}
        />
      )}
      <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8, marginTop: 16 }}>
        {onCancel && <Button onClick={onCancel}>{t('a.cancel', '取消')}</Button>}
        {isCreatePlan && (
          <Button loading={saving} onClick={() => doSubmit(true)}>{t('plan.saveAndContinue', '保存并继续创建')}</Button>
        )}
        <Button type="primary" loading={saving} onClick={() => doSubmit(false)}>
          {editing ? t('lv.save', '保存') : t('a.create', '创建')}
        </Button>
      </div>
    </Form>
  )
}

// Right-drawer wrapper for create + edit (per the reference: creation lives in a drawer, not a tab).
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
  onSaved: (list: RegItem[], created?: { id: string; name: string }, stay?: boolean) => void
}) {
  const { t } = useI18n()
  const title = editing
    ? mode === 'group' ? t('plan.editGroup', '编辑计划组') : t('plan.editPlan', '编辑测试计划')
    : mode === 'group' ? t('plan.newGroup', '新建计划组') : t('plan.newPlan', '新建测试计划')

  return (
    <EditDrawer title={title} open={open} onCancel={onClose} footer={null} width={620}>
      <PlanForm mode={mode} editing={editing} projectId={projectId} modules={modules} groups={groups} onSaved={onSaved} onCancel={onClose} />
    </EditDrawer>
  )
}
