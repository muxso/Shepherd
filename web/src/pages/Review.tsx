import { useEffect, useMemo, useState, type Key } from 'react'
import { Button, Card, Checkbox, Col, Dropdown, Empty, Input, Popover, Row, Segmented, Select, Space, Statistic, Table, Tabs, Tag, Tooltip } from 'antd'
import { FilterOutlined, MoreOutlined, ReloadOutlined, SettingOutlined, StarFilled, StarOutlined } from '@ant-design/icons'
import ResizableDrawer from '../components/ResizableDrawer'
import { useScopedNavigate } from '../scope'
import { message, modal } from '../feedback'
import type { ColumnsType } from 'antd/es/table'
import dayjs from 'dayjs'
import { api, ApiError, userIdStore, userStore, type CaseReviewDetail, type CaseReviewSummary, type FunctionalCase, type Requirement } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'
import { ResizableSider } from '../components/Workspace'
import ReviewModuleTree from '../components/review/ReviewModuleTree'
import ReviewFormDrawer from '../components/review/ReviewFormDrawer'
import { inReviewModule, modulePathOf, reviewModules, type ReviewModule } from '../components/review/reviewLocal'

// Review center: requirement review (review = set baseline) + case review list
// (MeterSphere layout: left module tree + toolbar + full-width table; detail/edit in right drawers).
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
    <Tabs
      className="ms-detail-tabs ms-fill-tabs"
      tabBarStyle={{ margin: 0, paddingInline: 16 }}
      items={[
        { key: 'case', label: t('review.caseTab', '用例评审'), children: <CaseReviewList /> },
        { key: 'req', label: t('review.reqTab', '需求评审'), children: <div style={{ height: '100%', overflow: 'auto', padding: 16 }}><RequirementReview /></div> },
      ]}
    />
  )
}

function RequirementReview() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const nav = useScopedNavigate()
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
        {card('BASELINED', t('review.s.BASELINED', '已基线(评审通过)'), 'var(--success)')}
        {card('DELIVERED', t('review.s.DELIVERED', '已交付'), '#1677ff')}
        {card('ARCHIVED', t('review.s.ARCHIVED', '已归档'))}
      </Row>
      <Card size="small" title={t('review.queue', '需求评审队列')} extra={<Segmented size="small" value={filter} onChange={(v) => setFilter(v as string)} options={[{ label: t('review.all', '全部'), value: 'ALL' }, { label: t('review.s.DRAFT', '待评审'), value: 'DRAFT' }, { label: t('review.s.BASELINED', '已通过'), value: 'BASELINED' }]} />}>
        <Table<Requirement> rowKey="id" size="middle" loading={loading} dataSource={filtered} columns={cols} pagination={{ pageSize: 15, size: 'small' }} locale={{ emptyText: <Empty description={t('review.empty', '暂无需求')} /> }} />
      </Card>
    </>
  )
}

// Column-visibility persistence for the case review table (gear in the action column header).
const COLS_KEY = 'shepherd.reviewCols'
const loadHiddenCols = (): string[] => {
  try { return JSON.parse(localStorage.getItem(COLS_KEY) || '[]') as string[] } catch { return [] }
}

const fmtDay = (v?: string | null) => (v ? dayjs(v).format('YYYY-MM-DD') : '')

function CaseReviewList() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const [items, setItems] = useState<CaseReviewSummary[]>([])
  const [cases, setCases] = useState<FunctionalCase[]>([])
  const [modules, setModules] = useState<ReviewModule[]>([])
  const [moduleKey, setModuleKey] = useState('ALL')
  const [seg, setSeg] = useState<'all' | 'reviewed' | 'created'>('all')
  const [search, setSearch] = useState('')
  const [statusF, setStatusF] = useState<string[]>([])
  const [reviewerF, setReviewerF] = useState<string[]>([])
  const [loading, setLoading] = useState(false)
  const [selectedKeys, setSelectedKeys] = useState<Key[]>([])
  const [hiddenCols, setHiddenCols] = useState<string[]>(loadHiddenCols)
  const [pageSize, setPageSize] = useState(50)
  const [form, setForm] = useState<{ editing?: CaseReviewSummary } | null>(null)
  const [detailId, setDetailId] = useState<string | null>(null)
  const [followedIds, setFollowedIds] = useState<Set<string>>(new Set())
  const caseName = (id: string) => cases.find((c) => c.id === id)?.name || id.slice(0, 8)

  const load = async () => {
    if (!projectId) { setItems([]); return }
    setLoading(true)
    try {
      const [rs, cs] = await Promise.all([api.caseReviews(projectId), api.functionalCases(projectId).catch(() => [])])
      setItems(Array.isArray(rs) ? rs : [])
      setCases(Array.isArray(cs) ? cs : [])
      // Followed review ids for the star action; failure just leaves stars empty.
      api.myFollows(projectId, 'CASE_REVIEW')
        .then((r) => setFollowedIds(new Set(r.entityIds)))
        .catch(() => {})
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('review.caseLoadFailed', '加载评审失败'))
    } finally {
      setLoading(false)
    }
  }
  useEffect(() => {
    setModules(reviewModules(projectId))
    setModuleKey('ALL')
    setSeg('all')
    setSelectedKeys([])
    load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId])

  const setCols = (next: string[]) => {
    setHiddenCols(next)
    localStorage.setItem(COLS_KEY, JSON.stringify(next))
  }

  // "Reviewed by me": the list carries distinct verdict reviewers; with no verdicts yet fall back to creator match.
  const meId = userIdStore.get()
  const meName = userStore.get()
  const isMine = (r: CaseReviewSummary) =>
    r.reviewers?.length ? r.reviewers.includes(meId) || r.reviewers.includes(meName) : r.createdBy === meId || r.createdBy === meName

  const rows = useMemo(() => {
    const q = search.trim().toLowerCase()
    return items.filter((r) => {
      if (!inReviewModule(modules, moduleKey, r.moduleId)) return false
      if (seg === 'reviewed' && !isMine(r)) return false
      if (seg === 'created' && !(r.createdBy === meId || r.createdBy === meName)) return false
      if (q && !(r.id.toLowerCase().includes(q) || r.name.toLowerCase().includes(q) || r.tags?.some((tg) => tg.toLowerCase().includes(q)))) return false
      return true
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [items, modules, moduleKey, seg, search])

  const statusMeta: Record<string, { label: string; color: string }> = {
    IN_PROGRESS: { label: t('review.statusInProgress', '进行中'), color: 'processing' },
    COMPLETED: { label: t('review.statusCompleted', '已完成'), color: 'success' },
  }
  const allReviewers = useMemo(() => [...new Set(items.flatMap((r) => r.reviewers ?? []))], [items])

  const passRateCell = (r: CaseReviewSummary) => {
    const pr = r.total > 0 ? Math.round((r.passed * 100) / r.total) : 0
    return (
      <Tooltip title={`${t('review.pass', '通过')} ${r.passed} / ${r.total}`}>
        <span style={{ display: 'inline-flex', alignItems: 'center', gap: 8 }}>
          <span style={{ width: 60, height: 6, background: 'var(--panel-2)', border: '1px solid var(--border-soft)', borderRadius: 3, overflow: 'hidden', display: 'inline-flex' }}>
            <span style={{ width: `${pr}%`, background: 'var(--success)' }} />
            {r.total > 0 && <span style={{ flex: 1, background: 'var(--error)', opacity: 0.75 }} />}
          </span>
          <span style={{ color: 'var(--text-2)', fontSize: 12 }}>{pr}%</span>
        </span>
      </Tooltip>
    )
  }

  // Optimistic star toggle; revert and warn on failure (same pattern as Scenarios).
  const toggleFollow = async (r: CaseReviewSummary) => {
    if (!projectId) return
    const was = followedIds.has(r.id)
    const apply = (on: boolean) =>
      setFollowedIds((prev) => {
        const n = new Set(prev)
        if (on) n.add(r.id)
        else n.delete(r.id)
        return n
      })
    apply(!was)
    const b = { projectId, entityType: 'CASE_REVIEW', entityId: r.id }
    try {
      const st = was ? await api.unfollow(b) : await api.follow(b)
      apply(st.following)
      message.success(st.following ? t('follow.followed', '已关注') : t('follow.unfollowed', '已取消关注'))
    } catch {
      apply(was)
      message.error(t('follow.failed', '关注操作失败'))
    }
  }
  const removeReview = (r: CaseReviewSummary) => {
    modal.confirm({
      title: t('review.deleteConfirmTitle', '删除评审?'),
      content: t('review.deleteConfirmBody', '将删除该评审(用例与评审历史不受影响)。'),
      okType: 'danger',
      okText: t('a.delete', '删除'),
      cancelText: t('a.cancel', '取消'),
      onOk: async () => {
        try {
          await api.deleteCaseReview(r.id)
          message.success(t('review.deleted', '已删除'))
          if (detailId === r.id) setDetailId(null)
          load()
        } catch (e) {
          message.error(e instanceof ApiError ? e.message : t('review.deleteFailed', '删除失败'))
        }
      },
    })
  }

  const dash = <span style={{ color: 'var(--text-3)' }}>-</span>
  const periodCell = (r: CaseReviewSummary) => {
    const s = fmtDay(r.startAt)
    const e = fmtDay(r.endAt)
    if (!s && !e) return dash
    return <span style={{ fontSize: 12, color: 'var(--text-2)' }}>{s || '?'} ~ {e || '?'}</span>
  }

  type ReviewCol = ColumnsType<CaseReviewSummary>[number] & { key: string; label: string }
  const dataCols: ReviewCol[] = [
    {
      key: 'id',
      label: 'ID',
      title: 'ID',
      width: 100,
      sorter: (a, b) => a.id.localeCompare(b.id),
      render: (_v, r) => (
        <a className="ms-mono" style={{ color: 'var(--brand)', fontSize: 12 }} onClick={() => setDetailId(r.id)}>{r.id.slice(0, 8)}</a>
      ),
    },
    {
      key: 'name',
      label: t('review.colName', '评审名称'),
      title: t('review.colName', '评审名称'),
      width: 220,
      ellipsis: true,
      sorter: (a, b) => a.name.localeCompare(b.name),
      render: (_v, r) => (
        <a style={{ color: 'var(--text-1)' }} onClick={() => setDetailId(r.id)}>{r.name || t('review.unnamed', '未命名评审')}</a>
      ),
    },
    { key: 'caseCount', label: t('review.colCaseCount', '用例数量'), title: t('review.colCaseCount', '用例数量'), width: 90, render: (_v, r) => r.total },
    {
      key: 'status',
      label: t('review.status', '评审状态'),
      title: t('review.status', '评审状态'),
      width: 110,
      filters: Object.entries(statusMeta).map(([v, m]) => ({ value: v, text: m.label })),
      filteredValue: statusF.length ? statusF : null,
      onFilter: (v, r) => r.status === v,
      render: (_v, r) => { const m = statusMeta[r.status] || { label: r.status, color: 'default' }; return <Tag color={m.color}>{m.label}</Tag> },
    },
    { key: 'passRate', label: t('review.colPassRate', '通过率'), title: t('review.colPassRate', '通过率'), width: 140, render: (_v, r) => passRateCell(r) },
    {
      key: 'mode',
      label: t('review.colMode', '评审模式'),
      title: t('review.colMode', '评审模式'),
      width: 100,
      render: (_v, r) => (
        <Tag style={{ color: 'var(--brand)', borderColor: 'var(--brand)', background: 'transparent' }}>
          {r.reviewerCount > 1 ? t('review.modeMulti', '多人评审') : t('review.modeSingle', '单人评审')}
        </Tag>
      ),
    },
    {
      key: 'reviewers',
      label: t('review.reviewers', '评审人'),
      title: t('review.reviewers', '评审人'),
      width: 130,
      ellipsis: true,
      filters: allReviewers.map((v) => ({ value: v, text: v })),
      filteredValue: reviewerF.length ? reviewerF : null,
      onFilter: (v, r) => (r.reviewers ?? []).includes(String(v)),
      render: (_v, r) => (r.reviewers?.length ? r.reviewers.join(', ') : dash),
    },
    { key: 'createdBy', label: t('review.colCreatedBy', '创建人'), title: t('review.colCreatedBy', '创建人'), width: 110, ellipsis: true, render: (_v, r) => r.createdBy || dash },
    {
      key: 'module',
      label: t('review.colModule', '所属模块'),
      title: t('review.colModule', '所属模块'),
      width: 130,
      ellipsis: true,
      render: (_v, r) => modulePathOf(modules, r.moduleId) || `/${t('review.moduleUnfiled', '未规划评审')}`,
    },
    {
      key: 'tags',
      label: t('review.colTags', '标签'),
      title: t('review.colTags', '标签'),
      width: 140,
      render: (_v, r) => (r.tags?.length ? r.tags.map((tg) => <Tag key={tg}>{tg}</Tag>) : dash),
    },
    { key: 'desc', label: t('review.colDesc', '描述'), title: t('review.colDesc', '描述'), width: 160, ellipsis: true, render: (_v, r) => r.description || dash },
    { key: 'period', label: t('review.colPeriod', '评审周期'), title: t('review.colPeriod', '评审周期'), width: 180, render: (_v, r) => periodCell(r) },
  ]
  const gearPanel = (
    <div style={{ width: 180 }}>
      {dataCols.map((c) => (
        <div key={c.key} style={{ padding: '3px 0' }}>
          <Checkbox
            checked={!hiddenCols.includes(c.key)}
            onChange={(e) => setCols(e.target.checked ? hiddenCols.filter((k) => k !== c.key) : [...hiddenCols, c.key])}
          >
            {c.label}
          </Checkbox>
        </div>
      ))}
    </div>
  )
  const actionCol: ReviewCol = {
    key: 'action',
    label: t('req.action', '操作'),
    title: (
      <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
        {t('req.action', '操作')}
        <Popover trigger="click" placement="bottomRight" content={gearPanel}>
          <SettingOutlined style={{ color: 'var(--text-3)', cursor: 'pointer' }} />
        </Popover>
      </span>
    ),
    width: 130,
    fixed: 'right' as const,
    render: (_v, r) => (
      <Space size={4} onClick={(e) => e.stopPropagation()}>
        <Tooltip title={followedIds.has(r.id) ? t('follow.unfollow', '取消关注') : t('follow.follow', '关注')}>
          <Button
            type="text"
            size="small"
            icon={followedIds.has(r.id) ? <StarFilled style={{ color: 'var(--warning, #ff7d00)' }} /> : <StarOutlined />}
            onClick={() => toggleFollow(r)}
          />
        </Tooltip>
        <Button type="link" size="small" style={{ paddingInline: 0 }} onClick={() => setForm({ editing: r })}>{t('a.edit', '编辑')}</Button>
        <Dropdown menu={{ items: [{ key: 'del', label: t('a.delete', '删除'), danger: true }], onClick: ({ key }) => { if (key === 'del') removeReview(r) } }}>
          <Button type="link" size="small" icon={<MoreOutlined />} />
        </Dropdown>
      </Space>
    ),
  }
  const columns = [...dataCols.filter((c) => !hiddenCols.includes(c.key)), actionCol]

  const filterPanel = (
    <div style={{ width: 260 }}>
      <div style={{ fontSize: 12, color: 'var(--text-2)', marginBottom: 4 }}>{t('review.status', '评审状态')}</div>
      <Select
        mode="multiple"
        allowClear
        size="small"
        style={{ width: '100%', marginBottom: 10 }}
        value={statusF}
        onChange={setStatusF}
        options={Object.entries(statusMeta).map(([v, m]) => ({ value: v, label: m.label }))}
      />
      <div style={{ fontSize: 12, color: 'var(--text-2)', marginBottom: 4 }}>{t('review.reviewers', '评审人')}</div>
      <Select
        mode="multiple"
        allowClear
        size="small"
        style={{ width: '100%', marginBottom: 10 }}
        value={reviewerF}
        onChange={setReviewerF}
        options={allReviewers.map((v) => ({ value: v, label: v }))}
      />
      <Button size="small" block onClick={() => { setStatusF([]); setReviewerF([]) }}>{t('lv.clearFilters', '清空筛选')}</Button>
    </div>
  )
  const filterCount = statusF.length + reviewerF.length

  if (!projectId) return <Empty description={t('common.selectProject', '请先在顶部选择项目')} style={{ marginTop: 64 }} />

  return (
    <div style={{ display: 'flex', height: '100%', minHeight: 0 }}>
      <ResizableSider defaultWidth={240} storageKey="review-sider">
        <div style={{ display: 'flex', flexDirection: 'column', height: '100%', borderRight: '1px solid var(--border-soft)' }}>
          <ReviewModuleTree
            projectId={projectId}
            items={items}
            modules={modules}
            selectedKey={moduleKey}
            onSelect={setModuleKey}
            onNewReview={() => setForm({})}
            onModulesChanged={(mods) => { setModules(mods); }}
          />
        </div>
      </ResizableSider>
      <div style={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '10px 12px', borderBottom: '1px solid var(--border-soft)' }}>
          <Segmented
            value={seg}
            onChange={(v) => setSeg(v as typeof seg)}
            options={[
              { value: 'all', label: t('review.all', '全部') },
              { value: 'reviewed', label: t('review.segReviewed', '我评审的') },
              { value: 'created', label: t('review.segCreated', '我创建的') },
            ]}
          />
          <div style={{ flex: 1 }} />
          <Input.Search
            allowClear
            style={{ width: 230 }}
            placeholder={t('review.searchPh', '通过 ID/名称/标签搜索')}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          <Select
            value="all"
            style={{ width: 150 }}
            prefix={<span style={{ color: 'var(--text-3)' }}>{t('lv.views', '视图')}</span>}
            options={[{ value: 'all', label: t('lv.allData', '全部数据') }]}
          />
          <Popover content={filterPanel} trigger="click" placement="bottomRight">
            <Button icon={<FilterOutlined />}>
              {t('lv.filter', '筛选')}{filterCount > 0 ? ` (${filterCount})` : ''}
            </Button>
          </Popover>
          <Tooltip title={t('a.refresh', '刷新')}>
            <Button icon={<ReloadOutlined />} onClick={load} />
          </Tooltip>
        </div>
        <div style={{ flex: 1, overflow: 'auto', padding: 12 }}>
          <Table<CaseReviewSummary>
            rowKey="id"
            size="middle"
            loading={loading}
            dataSource={rows}
            columns={columns}
            scroll={{ x: 1720 }}
            rowSelection={{ selectedRowKeys: selectedKeys, onChange: setSelectedKeys }}
            onChange={(_p, filters) => {
              setStatusF((filters.status as string[]) ?? [])
              setReviewerF((filters.reviewers as string[]) ?? [])
            }}
            pagination={{
              pageSize,
              size: 'small',
              showSizeChanger: true,
              pageSizeOptions: [10, 20, 50, 100],
              onChange: (_page, size) => { if (size && size !== pageSize) setPageSize(size) },
              showTotal: (n) => t('ws.total', '共 {n} 条').replace('{n}', String(n)),
            }}
            locale={{
              emptyText: (
                <Empty
                  image={Empty.PRESENTED_IMAGE_SIMPLE}
                  description={
                    <span style={{ color: 'var(--text-3)' }}>
                      {t('common.empty', '暂无数据')}{' '}
                      <a style={{ color: 'var(--brand)' }} onClick={() => setForm({})}>{t('review.newReview2', '新建评审')}</a>
                    </span>
                  }
                />
              ),
            }}
          />
        </div>
      </div>
      <ReviewFormDrawer
        key={form ? form.editing?.id || 'new' : 'closed'}
        open={!!form}
        editing={form?.editing}
        projectId={projectId}
        modules={modules}
        cases={cases}
        onClose={() => setForm(null)}
        onSaved={() => { setForm(null); load() }}
      />
      <ReviewDetailDrawer reviewId={detailId} caseName={caseName} onClose={() => setDetailId(null)} onChanged={load} />
    </div>
  )
}

function ReviewDetailDrawer({ reviewId, caseName, onClose, onChanged }: { reviewId: string | null; caseName: (id: string) => string; onClose: () => void; onChanged: () => void }) {
  const { t } = useI18n()
  const [detail, setDetail] = useState<CaseReviewDetail | null>(null)
  const [loading, setLoading] = useState(false)
  const [passingAll, setPassingAll] = useState(false)
  const reviewer = userStore.get()

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
                <Button type="link" size="small" style={{ color: 'var(--success)' }} onClick={() => submit(c.caseId, 'PASS')}>{t('review.pass', '通过')}</Button>
                <Button type="link" size="small" danger onClick={() => reject(c.caseId)}>{t('review.unpass', '不通过')}</Button>
              </Space>
            ),
          },
        ]}
      />
    </ResizableDrawer>
  )
}
