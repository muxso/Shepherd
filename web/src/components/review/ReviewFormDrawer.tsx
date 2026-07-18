import { useState } from 'react'
import { Button, DatePicker, Form, Input, InputNumber, Select, Space } from 'antd'
import dayjs, { type Dayjs } from 'dayjs'
import EditDrawer from '../EditDrawer'
import { message } from '../../feedback'
import { api, ApiError, type CaseReviewSummary, type FunctionalCase } from '../../api'
import { useI18n } from '../../i18n'
import { modulePathOf, type ReviewModule } from './reviewLocal'

interface FormValues {
  name: string
  description?: string
  moduleId: string
  tags?: string[]
  period?: [Dayjs | null, Dayjs | null] | null
  reviewMode: 'single' | 'multi'
  reviewerCount?: number
  passRule: string
  caseIds?: string[]
}

// Create/edit right drawer for a case review. Create posts setting + case set + meta;
// edit PUTs meta + pass setting only (the case set is fixed after creation).
export default function ReviewFormDrawer({
  open,
  editing,
  projectId,
  modules,
  cases,
  onClose,
  onSaved,
}: {
  open: boolean
  /** Present = edit mode (PUT), absent = create. */
  editing?: CaseReviewSummary
  projectId: string
  modules: ReviewModule[]
  cases: FunctionalCase[]
  onClose: () => void
  onSaved: () => void
}) {
  const { t } = useI18n()
  const [form] = Form.useForm<FormValues>()
  const [busy, setBusy] = useState(false)
  const isEdit = !!editing

  const initial: FormValues = editing
    ? {
        name: editing.name,
        description: editing.description || undefined,
        moduleId: editing.moduleId && modules.some((m) => m.id === editing.moduleId) ? editing.moduleId : '',
        tags: editing.tags?.length ? editing.tags : undefined,
        period: editing.startAt || editing.endAt
          ? [editing.startAt ? dayjs(editing.startAt) : null, editing.endAt ? dayjs(editing.endAt) : null]
          : undefined,
        reviewMode: editing.reviewerCount > 1 ? 'multi' : 'single',
        reviewerCount: Math.max(editing.reviewerCount, 2),
        passRule: editing.passRule || 'SINGLE',
      }
    : { name: '', moduleId: '', reviewMode: 'single', reviewerCount: 2, passRule: 'SINGLE' }

  const submit = async (v: FormValues) => {
    if (!isEdit && !v.caseIds?.length) return void message.warning(t('review.pickCases', '请选择要评审的用例'))
    const [start, end] = v.period ?? []
    const body = {
      name: v.name.trim(),
      description: v.description?.trim() || '',
      tags: v.tags ?? [],
      moduleId: v.moduleId || null,
      startAt: start ? start.toISOString() : null,
      endAt: end ? end.toISOString() : null,
      passRule: v.passRule,
      reviewerCount: v.reviewMode === 'multi' ? Math.max(v.reviewerCount ?? 2, 2) : 1,
    }
    setBusy(true)
    try {
      if (isEdit) {
        await api.updateCaseReview(editing.id, body)
        message.success(t('review.updated', '评审已更新'))
      } else {
        await api.createCaseReview({ projectId, caseIds: v.caseIds ?? [], ...body })
        message.success(t('review.created', '评审已创建'))
      }
      onSaved()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t(isEdit ? 'review.updateFailed' : 'review.createFailed', isEdit ? '更新失败' : '创建失败'))
    } finally {
      setBusy(false)
    }
  }

  return (
    <EditDrawer
      title={isEdit ? t('review.editReview', '编辑评审') : t('review.newReview2', '新建评审')}
      open={open}
      onCancel={onClose}
      width={560}
      footer={
        <Space style={{ display: 'flex', justifyContent: 'flex-end' }}>
          <Button onClick={onClose}>{t('a.cancel', '取消')}</Button>
          <Button type="primary" loading={busy} onClick={() => form.submit()}>
            {isEdit ? t('a.save', '保存') : t('a.create', '创建')}
          </Button>
        </Space>
      }
    >
      <Form form={form} layout="vertical" initialValues={initial} onFinish={submit}>
        <Form.Item name="name" label={t('review.colName', '评审名称')} rules={[{ required: true, whitespace: true, message: t('review.namePh', '请输入评审名称') }]}>
          <Input maxLength={255} placeholder={t('review.namePh', '请输入评审名称')} />
        </Form.Item>
        <Form.Item name="description" label={t('review.colDesc', '描述')}>
          <Input.TextArea rows={3} maxLength={1000} placeholder={t('review.descPh', '请输入描述')} />
        </Form.Item>
        <Form.Item name="moduleId" label={t('review.colModule', '所属模块')}>
          <Select
            showSearch
            optionFilterProp="label"
            options={[
              { value: '', label: t('review.moduleUnfiled', '未规划评审') },
              ...modules.map((m) => ({ value: m.id, label: modulePathOf(modules, m.id) || m.name })),
            ]}
          />
        </Form.Item>
        <Form.Item name="tags" label={t('review.colTags', '标签')}>
          <Select mode="tags" open={false} suffixIcon={null} placeholder={t('review.tagsPh', '输入后回车添加标签')} tokenSeparators={[',']} />
        </Form.Item>
        <Form.Item name="period" label={t('review.colPeriod', '评审周期')}>
          <DatePicker.RangePicker style={{ width: '100%' }} allowEmpty={[true, true]} />
        </Form.Item>
        <Form.Item name="reviewMode" label={t('review.colMode', '评审模式')}>
          <Select
            options={[
              { value: 'single', label: t('review.modeSingle', '单人评审') },
              { value: 'multi', label: t('review.modeMulti', '多人评审') },
            ]}
          />
        </Form.Item>
        <Form.Item noStyle shouldUpdate={(a, b) => a.reviewMode !== b.reviewMode}>
          {({ getFieldValue }) =>
            getFieldValue('reviewMode') === 'multi' && (
              <Form.Item name="reviewerCount" label={t('review.reviewerCount', '评审人数')}>
                <InputNumber min={2} max={20} style={{ width: 120 }} />
              </Form.Item>
            )
          }
        </Form.Item>
        <Form.Item name="passRule" label={t('review.rule', '通过规则')}>
          <Select
            options={[
              { value: 'SINGLE', label: t('review.rule.SINGLE', '或签(任一通过)') },
              { value: 'MULTIPLE', label: t('review.rule.MULTIPLE', '会签(全部通过)') },
            ]}
          />
        </Form.Item>
        {!isEdit && (
          <Form.Item name="caseIds" label={t('review.cases', '评审用例')} rules={[{ required: true, message: t('review.pickCases', '请选择要评审的用例') }]}>
            <Select
              mode="multiple"
              showSearch
              optionFilterProp="label"
              placeholder={t('review.pickCases', '请选择要评审的用例')}
              options={cases.map((c) => ({ value: c.id, label: c.name }))}
            />
          </Form.Item>
        )}
      </Form>
    </EditDrawer>
  )
}
