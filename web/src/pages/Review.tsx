import { useEffect, useMemo, useState } from 'react'
import { Button, Card, Col, Empty, Form, Input, InputNumber, Row, Segmented, Select, Space, Statistic, Table, Tabs, Tag } from 'antd'
import ResizableDrawer from '../components/ResizableDrawer'
import EditDrawer from '../components/EditDrawer'
import { useNavigate } from 'react-router-dom'
import { message, modal } from '../feedback'
import type { ColumnsType } from 'antd/es/table'
import { api, ApiError, type CaseReviewDetail, type CaseReviewSummary, type FunctionalCase, type Requirement } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'

// AI review: 1) requirement review (review = set baseline) 2) case review queue (any-one / all-approve verdicts).
const REVIEW: Record<string, { label: string; color: string }> = {
  DRAFT: { label: '待评审', color: 'orange' },
  BASELINED: { label: '已基线(评审通过)', color: 'green' },
  DELIVERED: { label: '已交付', color: 'blue' },
  ARCHIVED: { label: '已归档', color: 'default' },
}
const CASE_STATUS: Record<string, { label: string; color: string }> = {
  UN_REVIEWED: { label: '未评审', color: 'default' },
  UNDER_REVIEWED: { label: '评审中', color: 'blue' },
  PASS: { label: '通过', color: 'green' },
  UN_PASS: { label: '未通过', color: 'red' },
  RE_REVIEWED: { label: '重新评审', color: 'orange' },
}
const ruleLabel = (r: string) => (r === 'MULTIPLE' ? '会签(全部通过)' : '或签(任一通过)')

export default function Review() {
  const { t } = useI18n()
  return (
    <div style={{ height: '100%', overflow: 'auto', padding: 16 }}>
      <Tabs
        items={[
          { key: 'req', label: t('review.reqTab', '需求评审'), children: <RequirementReview /> },
          { key: 'case', label: t('review.caseTab', '用例评审'), children: <CaseReviewQueue /> },
        ]}
      />
    </div>
  )
}

function RequirementReview() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const nav = useNavigate()
  const [items, setItems] = useState<Requirement[]>([])
  const [loading, setLoading] = useState(false)
  const [filter, setFilter] = useState<string>('ALL')

  const load = async () => {
    if (!projectId) { setItems([]); return }
    setLoading(true)
    try {
      const page = await api.requirements(projectId)
      setItems(page.items ?? [])
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('review.loadFailed', '加载需求失败'))
    } finally {
      setLoading(false)
    }
  }
  useEffect(() => { load() }, [projectId]) // eslint-disable-line react-hooks/exhaustive-deps

  const counts = useMemo(() => {
    const c: Record<string, number> = { DRAFT: 0, BASELINED: 0, DELIVERED: 0, ARCHIVED: 0 }
    items.forEach((r) => { c[r.status] = (c[r.status] ?? 0) + 1 })
    return c
  }, [items])
  const filtered = useMemo(() => (filter === 'ALL' ? items : items.filter((r) => r.status === filter)), [items, filter])

  if (!projectId) return <Empty description={t('common.selectProject', '请先在顶部选择项目')} style={{ marginTop: 64 }} />

  const cols: ColumnsType<Requirement> = [
    { title: t('review.title', '需求标题'), dataIndex: 'title', ellipsis: true, render: (v: string) => <span style={{ fontWeight: 500 }}>{v}</span> },
    { title: t('review.baseline', '基线版本'), dataIndex: 'baselineVersion', width: 110, render: (v?: number) => (v ? `v${v}` : '—') },
    { title: t('review.status', '评审状态'), dataIndex: 'status', width: 160, render: (s: string) => { const m = REVIEW[s] || { label: s, color: 'default' }; return <Tag color={m.color}>{t(`review.s.${s}`, m.label)}</Tag> } },
    { title: t('req.action', '操作'), width: 120, render: (_v, r) => <Button type="link" size="small" onClick={() => nav('/requirement')}>{r.status === 'DRAFT' ? t('review.goReview', '去评审') : t('review.view', '查看')}</Button> },
  ]
  const card = (key: string, title: string, color?: string) => (
    <Col span={6}>
      <Card size="small" hoverable onClick={() => setFilter(key)} style={{ borderColor: filter === key ? 'var(--brand)' : undefined }}>
        <Statistic title={title} value={counts[key] ?? 0} valueStyle={color ? { color } : undefined} />
      </Card>
    </Col>
  )

  return (
    <>
      <Row gutter={12} style={{ marginBottom: 12 }}>
        {card('DRAFT', t('review.s.DRAFT', '待评审'), '#fa8c16')}
        {card('BASELINED', t('review.s.BASELINED', '已基线(评审通过)'), '#2e7d32')}
        {card('DELIVERED', t('review.s.DELIVERED', '已交付'), '#1677ff')}
        {card('ARCHIVED', t('review.s.ARCHIVED', '已归档'))}
      </Row>
      <Card size="small" title={t('review.queue', '需求评审队列')} extra={<Segmented size="small" value={filter} onChange={(v) => setFilter(v as string)} options={[{ label: t('review.all', '全部'), value: 'ALL' }, { label: t('review.s.DRAFT', '待评审'), value: 'DRAFT' }, { label: t('review.s.BASELINED', '已通过'), value: 'BASELINED' }]} />}>
        <Table<Requirement> rowKey="id" size="middle" loading={loading} dataSource={filtered} columns={cols} pagination={{ pageSize: 15, size: 'small' }} locale={{ emptyText: <Empty description={t('review.empty', '暂无需求')} /> }} />
      </Card>
    </>
  )
}

function CaseReviewQueue() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const [items, setItems] = useState<CaseReviewSummary[]>([])
  const [loading, setLoading] = useState(false)
  const [createOpen, setCreateOpen] = useState(false)
  const [detailId, setDetailId] = useState<string | null>(null)
  const [cases, setCases] = useState<FunctionalCase[]>([])
  const caseName = (id: string) => cases.find((c) => c.id === id)?.name || id.slice(0, 8)

  const load = async () => {
    if (!projectId) { setItems([]); return }
    setLoading(true)
    try {
      const [rs, cs] = await Promise.all([api.caseReviews(projectId), api.functionalCases(projectId).catch(() => [])])
      setItems(Array.isArray(rs) ? rs : [])
      setCases(Array.isArray(cs) ? cs : [])
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('review.caseLoadFailed', '加载评审失败'))
    } finally {
      setLoading(false)
    }
  }
  useEffect(() => { load() }, [projectId]) // eslint-disable-line react-hooks/exhaustive-deps

  if (!projectId) return <Empty description={t('common.selectProject', '请先在顶部选择项目')} style={{ marginTop: 64 }} />

  const cols: ColumnsType<CaseReviewSummary> = [
    { title: t('review.rule', '通过规则'), dataIndex: 'passRule', width: 180, render: (r: string) => <Tag color={r === 'MULTIPLE' ? 'purple' : 'blue'}>{t(`review.rule.${r}`, ruleLabel(r))}</Tag> },
    { title: t('review.caseCount', '用例数'), dataIndex: 'total', width: 90 },
    { title: t('review.passed', '已通过'), dataIndex: 'passed', width: 100, render: (p: number, r) => <span style={{ color: p >= r.total && r.total > 0 ? '#2e7d32' : undefined }}>{p} / {r.total}</span> },
    { title: t('review.createdAt', '创建时间'), dataIndex: 'createdAt', width: 180, render: (v: string) => <span style={{ color: 'var(--text-3)', fontSize: 12 }}>{v?.slice(0, 19) || '—'}</span> },
    { title: t('req.action', '操作'), width: 90, render: (_v, r) => <Button type="link" size="small" onClick={() => setDetailId(r.id)}>{t('review.view', '查看')}</Button> },
  ]

  return (
    <Card size="small" title={t('review.caseQueue', '用例评审队列')} extra={<Button type="primary" onClick={() => setCreateOpen(true)} disabled={!cases.length}>{t('review.newReview', '发起评审')}</Button>}>
      <Table<CaseReviewSummary> rowKey="id" size="middle" loading={loading} dataSource={items} columns={cols} pagination={{ pageSize: 15, size: 'small' }} locale={{ emptyText: <Empty description={t('review.caseEmpty', '暂无评审,点「发起评审」选用例创建')} /> }} />
      <CreateReviewModal open={createOpen} cases={cases} projectId={projectId} onClose={() => setCreateOpen(false)} onDone={() => { setCreateOpen(false); load() }} />
      <ReviewDetailDrawer reviewId={detailId} caseName={caseName} onClose={() => setDetailId(null)} onChanged={load} />
    </Card>
  )
}

function CreateReviewModal({ open, cases, projectId, onClose, onDone }: { open: boolean; cases: FunctionalCase[]; projectId: string; onClose: () => void; onDone: () => void }) {
  const { t } = useI18n()
  const [form] = Form.useForm<{ passRule: string; reviewerCount: number; caseIds: string[] }>()
  const [busy, setBusy] = useState(false)
  useEffect(() => { if (open) form.setFieldsValue({ passRule: 'SINGLE', reviewerCount: 1, caseIds: [] }) }, [open, form])
  return (
    <EditDrawer title={t('review.newReview', '发起用例评审')} open={open} onCancel={onClose} footer={null}>
      <Form form={form} layout="vertical" onFinish={async (v) => {
        if (!v.caseIds?.length) return message.warning(t('review.pickCases', '请选择要评审的用例'))
        setBusy(true)
        try {
          await api.createCaseReview({ projectId, passRule: v.passRule, reviewerCount: v.reviewerCount, caseIds: v.caseIds })
          message.success(t('review.created', '评审已创建'))
          onDone()
        } catch (e) {
          message.error(e instanceof ApiError ? e.message : t('review.createFailed', '创建失败'))
        } finally { setBusy(false) }
      }}>
        <Form.Item name="caseIds" label={t('review.cases', '评审用例')} rules={[{ required: true }]}>
          <Select mode="multiple" showSearch optionFilterProp="label" placeholder={t('review.pickCases', '请选择要评审的用例')} options={cases.map((c) => ({ value: c.id, label: c.name }))} />
        </Form.Item>
        <Form.Item name="passRule" label={t('review.rule', '通过规则')}>
          <Select options={[{ value: 'SINGLE', label: t('review.rule.SINGLE', '或签(任一通过)') }, { value: 'MULTIPLE', label: t('review.rule.MULTIPLE', '会签(全部通过)') }]} />
        </Form.Item>
        <Form.Item name="reviewerCount" label={t('review.reviewerCount', '评审人数')}>
          <InputNumber min={1} max={20} style={{ width: 120 }} />
        </Form.Item>
        <Button type="primary" htmlType="submit" block loading={busy}>{t('a.create', '创建')}</Button>
      </Form>
    </EditDrawer>
  )
}

function ReviewDetailDrawer({ reviewId, caseName, onClose, onChanged }: { reviewId: string | null; caseName: (id: string) => string; onClose: () => void; onChanged: () => void }) {
  const { t } = useI18n()
  const [detail, setDetail] = useState<CaseReviewDetail | null>(null)
  const [loading, setLoading] = useState(false)
  const [passingAll, setPassingAll] = useState(false)
  const reviewer = localStorage.getItem('shepherd.user') || 'admin'

  const load = async () => {
    if (!reviewId) return
    setLoading(true)
    try { setDetail(await api.caseReview(reviewId)) } catch (e) { message.error(e instanceof ApiError ? e.message : t('review.detailFailed', '加载详情失败')) } finally { setLoading(false) }
  }
  useEffect(() => { setDetail(null); if (reviewId) load() }, [reviewId]) // eslint-disable-line react-hooks/exhaustive-deps

  const submit = async (caseId: string, status: string, content?: string) => {
    if (!reviewId) return
    try {
      await api.submitCaseReview(reviewId, caseId, { reviewerId: reviewer, status, content })
      message.success(t('review.submitted', '已提交裁决'))
      load(); onChanged()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('review.submitFailed', '提交失败'))
    }
  }
  // One-click approve: submit PASS for every not-yet-passed case in this review (the review passes once its rule is met).
  const passAll = () => {
    if (!reviewId || !detail) return
    const pending = detail.cases.filter((c) => c.status !== 'PASS')
    if (!pending.length) { message.info(t('review.allPassed', '所有用例均已通过')); return }
    modal.confirm({
      title: t('review.passAllTitle', '一键评审通过'),
      content: t('review.passAllConfirm', `将对 ${pending.length} 个未通过用例提交「通过」,确认?`),
      okButtonProps: { style: { background: '#2e7d32', borderColor: '#2e7d32' } },
      onOk: async () => {
        setPassingAll(true)
        try {
          for (const c of pending) {
            await api.submitCaseReview(reviewId, c.caseId, { reviewerId: reviewer, status: 'PASS' })
          }
          message.success(t('review.passedAll', '已全部提交通过'))
          load(); onChanged()
        } catch (e) {
          message.error(e instanceof ApiError ? e.message : t('review.submitFailed', '提交失败'))
        } finally { setPassingAll(false) }
      },
    })
  }
  const reject = (caseId: string) => {
    let reason = ''
    modal.confirm({
      title: t('review.rejectTitle', '不通过原因'),
      content: <Input.TextArea rows={3} onChange={(e) => (reason = e.target.value)} placeholder={t('review.reasonPh', '请填写不通过原因(必填)')} style={{ marginTop: 8 }} />,
      okButtonProps: { danger: true },
      onOk: async () => { if (!reason.trim()) { message.warning(t('review.reasonRequired', '请填写原因')); throw new Error('no reason') } await submit(caseId, 'UN_PASS', reason.trim()) },
    })
  }

  return (
    <ResizableDrawer
      title={t('review.reviewDetail', '评审详情')}
      open={!!reviewId}
      onClose={onClose}
      width={680}
      extra={detail && detail.cases.length > 0 && (
        <Button
          type="primary"
          loading={passingAll}
          disabled={loading || detail.cases.every((c) => c.status === 'PASS')}
          style={{ background: '#2e7d32', borderColor: '#2e7d32' }}
          onClick={passAll}
        >
          {t('review.passAll', '一键评审通过')}
        </Button>
      )}
    >
      {detail && <Tag color={detail.passRule === 'MULTIPLE' ? 'purple' : 'blue'} style={{ marginBottom: 12 }}>{t(`review.rule.${detail.passRule}`, ruleLabel(detail.passRule))} · {detail.reviewerCount} {t('review.reviewers', '评审人')}</Tag>}
      <Table<CaseReviewDetail['cases'][number]>
        rowKey="caseId"
        size="small"
        loading={loading}
        dataSource={detail?.cases ?? []}
        pagination={false}
        locale={{ emptyText: <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('review.noCases', '无用例')} /> }}
        columns={[
          { title: t('review.case', '用例'), dataIndex: 'caseId', render: (id: string) => caseName(id) },
          { title: t('review.status', '状态'), dataIndex: 'status', width: 110, render: (s: string) => { const m = CASE_STATUS[s] || { label: s, color: 'default' }; return <Tag color={m.color}>{t(`review.cs.${s}`, m.label)}</Tag> } },
          {
            title: t('review.verdict', '裁决'), width: 150,
            render: (_v, c) => (
              <Space size={4}>
                <Button type="link" size="small" style={{ color: '#2e7d32' }} onClick={() => submit(c.caseId, 'PASS')}>{t('review.pass', '通过')}</Button>
                <Button type="link" size="small" danger onClick={() => reject(c.caseId)}>{t('review.unpass', '不通过')}</Button>
              </Space>
            ),
          },
        ]}
      />
    </ResizableDrawer>
  )
}
