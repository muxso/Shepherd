import { useEffect, useState } from 'react'
import { Button, Collapse, DatePicker, Drawer, Form, Input, InputNumber, Radio, Select, Space, Switch } from 'antd'
import dayjs, { type Dayjs } from 'dayjs'
import { message } from '../../feedback'
import { api, ApiError, type PlanDetailInfo } from '../../api'
import { useI18n } from '../../i18n'
import type { RegItem } from '../../registry'
import { fetchPlanItems, isGroup, planModules } from './planLocal'

interface FormValues {
  name: string
  description?: string
  moveKind: 'module' | 'group'
  moduleId?: string
  groupId?: string
  range?: [Dayjs | null, Dayjs | null]
  tags?: string[]
  allowDuplicateCases: boolean
  autoUpdateStatus: boolean
  passThreshold: number
}

/** Right drawer: update a test plan (name/description/module|group/dates/tags + more settings). */
export default function PlanEditDrawer({
  open,
  planId,
  projectId,
  onClose,
  onSaved,
}: {
  open: boolean
  planId: string
  projectId: string
  onClose: () => void
  onSaved?: (detail: PlanDetailInfo) => void
}) {
  const { t } = useI18n()
  const [form] = Form.useForm<FormValues>()
  const [saving, setSaving] = useState(false)
  const [groups, setGroups] = useState<RegItem[]>([])
  const modules = planModules(projectId)

  // Module dropdown: tree flattened with depth indentation (same as PlanFormModal).
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

  useEffect(() => {
    if (!open) return
    fetchPlanItems(projectId)
      .then((list) => setGroups(list.filter(isGroup)))
      .catch(() => setGroups([]))
    api.planDetail(planId)
      .then((d) => {
        form.setFieldsValue({
          name: d.name,
          description: d.description,
          moveKind: d.groupId && d.groupId !== 'NONE' ? 'group' : 'module',
          moduleId: d.moduleId || undefined,
          groupId: d.groupId && d.groupId !== 'NONE' ? d.groupId : undefined,
          range: d.startAt || d.endAt ? [d.startAt ? dayjs(d.startAt) : null, d.endAt ? dayjs(d.endAt) : null] : undefined,
          tags: d.tags,
          allowDuplicateCases: d.allowDuplicateCases,
          autoUpdateStatus: d.autoUpdateStatus,
          passThreshold: Math.round(d.passThreshold),
        })
      })
      .catch((e) => message.error(e instanceof ApiError ? e.message : t('plan.loadFail', '加载计划失败')))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, planId])

  const submit = async (v: FormValues) => {
    setSaving(true)
    try {
      const detail = await api.updatePlan(planId, {
        name: v.name,
        description: v.description || '',
        tags: v.tags || [],
        moduleId: v.moveKind === 'module' ? v.moduleId || '' : '',
        groupId: v.moveKind === 'group' ? v.groupId || '' : '',
        startAt: v.range?.[0] ? v.range[0].valueOf() : 0,
        endAt: v.range?.[1] ? v.range[1].valueOf() : 0,
        allowDuplicateCases: v.allowDuplicateCases,
        autoUpdateStatus: v.autoUpdateStatus,
        passThreshold: v.passThreshold,
      })
      message.success(t('plan.updated', '已更新测试计划'))
      onSaved?.(detail)
      onClose()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('plan.updateFail', '更新失败'))
    } finally {
      setSaving(false)
    }
  }

  return (
    <Drawer
      title={t('plan.editTitle', '更新测试计划')}
      open={open}
      onClose={onClose}
      width={480}
      destroyOnHidden
      footer={
        <Space style={{ float: 'right' }}>
          <Button onClick={onClose}>{t('a.cancel', '取消')}</Button>
          <Button type="primary" loading={saving} onClick={() => form.submit()}>{t('lv.save', '保存')}</Button>
        </Space>
      }
    >
      <Form<FormValues>
        form={form}
        layout="vertical"
        onFinish={submit}
        initialValues={{ moveKind: 'module', allowDuplicateCases: true, autoUpdateStatus: true, passThreshold: 100 }}
      >
        <Form.Item name="name" label={t('plan.colName', '测试计划名称')} rules={[{ required: true }]}>
          <Input placeholder={t('plan.namePlaceholder', '如:回归冒烟')} maxLength={255} />
        </Form.Item>
        <Form.Item name="description" label={t('plan.desc', '描述')}>
          <Input.TextArea rows={3} placeholder={t('plan.descPh', '请输入描述')} maxLength={1000} showCount />
        </Form.Item>
        <Form.Item name="moveKind" label={t('plan.moveTo', '移动到')}>
          <Radio.Group
            options={[
              { value: 'module', label: t('plan.radioModule', '模块') },
              { value: 'group', label: t('plan.radioGroup', '计划组') },
            ]}
          />
        </Form.Item>
        <Form.Item noStyle shouldUpdate={(a, b) => a.moveKind !== b.moveKind}>
          {({ getFieldValue }) =>
            getFieldValue('moveKind') === 'group' ? (
              <Form.Item name="groupId" label={t('plan.belongGroup', '所属计划组')}>
                <Select allowClear placeholder={t('plan.noGroup', '不归属计划组')} options={groups.map((g) => ({ value: g.id, label: g.label }))} />
              </Form.Item>
            ) : (
              <Form.Item name="moduleId" label={t('plan.belongModule', '所属模块')}>
                <Select allowClear placeholder={t('plan.moduleUnfiled', '未规划')} options={moduleOptions} />
              </Form.Item>
            )
          }
        </Form.Item>
        <Form.Item name="range" label={t('plan.dateRange', '计划起止时间')}>
          <DatePicker.RangePicker style={{ width: '100%' }} showTime allowEmpty={[true, true]} />
        </Form.Item>
        <Form.Item name="tags" label={t('plan.tags', '标签')}>
          <Select mode="tags" placeholder={t('plan.tagsPh', '输入后回车添加标签')} open={false} suffixIcon={null} />
        </Form.Item>
        <Collapse
          ghost
          items={[
            {
              key: 'more',
              label: t('plan.moreSettings', '更多设置'),
              children: (
                <>
                  <Form.Item name="allowDuplicateCases" label={t('plan.allowDup', '允许关联重复用例')} valuePropName="checked">
                    <Switch size="small" />
                  </Form.Item>
                  <Form.Item name="autoUpdateStatus" label={t('plan.autoStatus', '允许自动更新状态')} valuePropName="checked">
                    <Switch size="small" />
                  </Form.Item>
                  <Form.Item name="passThreshold" label={`${t('plan.threshold', '通过阈值')} (%)`}>
                    <InputNumber min={0} max={100} precision={0} style={{ width: 120 }} />
                  </Form.Item>
                </>
              ),
            },
          ]}
        />
      </Form>
    </Drawer>
  )
}
