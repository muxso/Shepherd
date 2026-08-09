import { useCallback, useEffect, useMemo, useState } from 'react'
import { Button, Descriptions, Empty, Form, Input, Select, Space, Tag, Tooltip } from 'antd'
import ResizableDrawer from '../components/ResizableDrawer'
import { message, modal } from '../feedback'
import { LinkOutlined } from '@ant-design/icons'
import { MarkdownEditor } from '../components/MarkdownEditor'
import { MarkdownRenderer } from '../components/MarkdownRenderer'
import { api, ApiError, userIdStore, type Bug, type BugRelation } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'
import { useScopedNavigate } from '../scope'
import { SelectProjectEmpty } from '../components/Page'
import { Workspace, WorkList, useWorkTabs } from '../components/Workspace'
import { useListView, type ListColumn } from '../components/ListView'
import { priorityColor } from '../components/tags'
import { CF_GROUP, CustomFieldItems, collectCustomValues, useFieldTemplate } from '../components/TemplateFields'

const STATUSES = ['NEW', 'RESOLVED', 'CLOSED', 'REOPENED', 'REJECTED']
const SEVERITIES = ['P0', 'P1', 'P2', 'P3']

/** Handler candidates: project members first, falling back to (and merged with) the global
 *  user directory so the assignee list is never empty even for projects with no members. */
function useMemberOptions(projectId: string) {
  const [options, setOptions] = useState<{ value: string; label: string }[]>([])
  useEffect(() => {
    let alive = true
    Promise.all([
      api.projectMembers(projectId).catch(() => [] as { userId?: string }[]),
      api.users().catch(() => ({ items: [] as { id?: string }[] })),
    ])
      .then(async ([ms, us]) => {
        const ids = [
          ...ms.map((m) => m.userId).filter(Boolean),
          ...us.items.map((u) => u.id).filter(Boolean),
        ] as string[]
        const uniq = [...new Set(ids)]
        const names = uniq.length ? await api.userNames(uniq).catch(() => ({}) as Record<string, string>) : {}
        if (alive) setOptions(uniq.map((id) => ({ value: id, label: names[id] || id })))
      })
      .catch(() => {})
    return () => { alive = false }
  }, [projectId])
  return options
}

const severityDot = (s?: string | null) =>
  s ? <span style={{ color: priorityColor(s) }}>● {s}</span> : '-'

/** Timestamptz text → "YYYY-MM-DD HH:mm:ss". */
const fmtTs = (v?: string | null) => (v ? v.slice(0, 19).replace('T', ' ') : '-')

// Key of the persistent "new bug" workspace tab (bugs have no detail tab; the tab pool is list + new only).
const NEW_KEY = '__new_bug__'

const bugColor = (s: string) => {
  const v = s.toUpperCase()
  if (v === 'RESOLVED' || v === 'CLOSED') return 'green'
  if (v === 'REJECTED') return 'red'
  if (v === 'NEW' || v === 'REOPENED') return 'orange'
  return 'blue'
}

// Status code → display label (i18n key with Chinese fallback); unregistered custom statuses render as-is.
const statusLabel = (t: (k: string, d?: string) => string, s: string) => {
  const zh: Record<string, string> = {
    NEW: '新建', RESOLVED: '已解决', CLOSED: '已关闭', REOPENED: '重新打开', REJECTED: '已拒绝',
  }
  return zh[s] ? t(`bug.st.${s}`, zh[s]) : s
}

// Bug list/create/status transitions all go through the backend (GET /bug, POST /bug, POST /bug/{id}/status).
export default function Bugs() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const [items, setItems] = useState<Bug[]>([])
  const [loading, setLoading] = useState(false)
  const [relBug, setRelBug] = useState<Bug | null>(null)
  const tabs = useWorkTabs()

  const refresh = useCallback(() => {
    if (!projectId) return
    setLoading(true)
    api
      .bugs(projectId)
      .then(setItems)
      .catch(() => message.error(t('bug.loadFailed', '加载缺陷失败')))
      .finally(() => setLoading(false))
  }, [projectId, t])

  useEffect(refresh, [refresh])
  useEffect(() => {
    tabs.reset()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId])

  if (!projectId) return <SelectProjectEmpty />

  const changeStatus = (item: Bug) => {
    let status = 'RESOLVED'
    modal.confirm({
      title: `${t('bug.changeStatus', '变更缺陷状态')} · ${item.title || item.id}`,
      content: (
        <Select
          defaultValue={status}
          style={{ width: '100%', marginTop: 8 }}
          onChange={(v) => (status = v)}
          options={STATUSES.filter((s) => s !== item.status).map((s) => ({ value: s, label: statusLabel(t, s) }))}
        />
      ),
      onOk: async () => {
        try {
          const b = await api.setBugStatus(item.id, status)
          message.success(`${t('bug.changedTo', '已变更为')} ${statusLabel(t, b.status)}`)
          // Merge the full response so the stamped updatedBy/updatedAt show immediately.
          setItems((prev) => prev.map((x) => (x.id === item.id ? { ...x, ...b } : x)))
        } catch (e) {
          message.error(e instanceof ApiError ? `${t('bug.changeFailedStatus', '变更失败')}:${e.status}${t('bug.illegalTransition', '(非法流转?)')}` : t('bug.changeFailed', '变更失败'))
        }
      },
    })
  }

  return <BugsList items={items} loading={loading} projectId={projectId} refresh={refresh} setItems={setItems} relBug={relBug} setRelBug={setRelBug} changeStatus={changeStatus} tabs={tabs} t={t} />
}

// List + view/filter/column toolbar: useListView is a hook, so this lives in a child component to avoid calling hooks after the parent's conditional return.
function BugsList({ items, loading, projectId, refresh, setItems, relBug, setRelBug, changeStatus, tabs, t }: {
  items: Bug[]
  loading: boolean
  projectId: string
  refresh: () => void
  setItems: React.Dispatch<React.SetStateAction<Bug[]>>
  relBug: Bug | null
  setRelBug: (b: Bug | null) => void
  changeStatus: (b: Bug) => void
  tabs: ReturnType<typeof useWorkTabs>
  t: (k: string, d?: string) => string
}) {
  const [editBug, setEditBug] = useState<Bug | null>(null)
  const [detailBug, setDetailBug] = useState<Bug | null>(null)
  // Jump to the global user directory so a bug's handler can be inspected in /user.
  const nav = useScopedNavigate()
  // Resolve handler/updater ids in the current rows to display names.
  const [names, setNames] = useState<Record<string, string>>({})
  useEffect(() => {
    const ids = [...new Set(items.flatMap((b) => [b.createdBy, b.handler, b.updatedBy]).filter((x): x is string => !!x))]
    if (!ids.length) { setNames({}); return }
    api.userNames(ids).then(setNames).catch(() => setNames({}))
  }, [items])
  const nameOf = (id?: string | null) => (id ? names[id] || id : '-')

  const allColumns: ListColumn<Bug>[] = [
    { key: 'title', label: t('bug.title', '标题'), title: t('bug.title', '标题'), dataIndex: 'title' },
    {
      key: 'severity',
      label: t('bug.severity', '严重程度'),
      title: t('bug.severity', '严重程度'),
      width: 100,
      render: (_, r) => severityDot(r.severity),
    },
    {
      key: 'status',
      label: t('bug.status', '状态'),
      title: t('bug.status', '状态'),
      width: 130,
      render: (_, r) => <Tag color={bugColor(r.status || 'NEW')}>{statusLabel(t, r.status || 'NEW')}</Tag>,
    },
    {
      key: 'handler',
      label: t('bug.handler', '处理人'),
      title: t('bug.handler', '处理人'),
      width: 120,
      render: (_, r) => (r.handler
        ? <a style={{ cursor: 'pointer' }} onClick={() => nav('/user')} title={t('bug.openUser', '在用户管理中查看')}>{nameOf(r.handler)}</a>
        : '-'),
    },
    {
      key: 'updatedBy',
      label: t('bug.updatedBy', '更新人'),
      title: t('bug.updatedBy', '更新人'),
      width: 120,
      render: (_, r) => nameOf(r.updatedBy),
    },
    {
      key: 'updatedAt',
      label: t('bug.updatedAt', '更新时间'),
      title: t('bug.updatedAt', '更新时间'),
      width: 170,
      sorter: (a, b) => (a.updatedAt || '').localeCompare(b.updatedAt || ''),
      render: (_, r) => <span className="ms-mono" style={{ fontSize: 12 }}>{fmtTs(r.updatedAt)}</span>,
    },
    { key: 'id', label: 'ID', title: 'ID', dataIndex: 'id', width: 110, render: (v: string) => <Tooltip title={v}><span className="ms-mono" style={{ fontSize: 12, color: 'var(--text-3)' }}>{v?.slice(0, 8)}</span></Tooltip> },
    {
      key: 'action',
      label: t('bug.action', '操作'),
      title: t('bug.action', '操作'),
      width: 230,
      render: (_, r) => (
        <>
          <Button type="link" size="small" onClick={() => setEditBug(r)}>
            {t('bug.edit', '编辑')}
          </Button>
          <Button type="link" size="small" onClick={() => changeStatus(r)}>
            {t('bug.changeStatusBtn', '变更状态')}
          </Button>
          <Button type="link" size="small" icon={<LinkOutlined />} onClick={() => setRelBug(r)}>
            {t('bug.relations', '关联')}
          </Button>
        </>
      ),
    },
  ]
  const lv = useListView<Bug>({
    kind: 'bug',
    projectId,
    searchOf: (r) => r.title || r.id,
    searchLabel: t('bug.searchPh', '搜索标题'),
    systemViews: [
      { key: 'mine', label: t('lv.mine', '我创建的'), pred: (r) => !!r.createdBy && r.createdBy === userIdStore.get() },
    ],
    fields: [
      {
        key: 'status',
        label: t('bug.status', '状态'),
        type: 'enum',
        options: STATUSES.map((s) => ({ value: s, label: statusLabel(t, s) })),
        get: (r) => (r.status || 'NEW').toUpperCase(),
      },
      {
        key: 'severity',
        label: t('bug.severity', '严重程度'),
        type: 'enum',
        options: SEVERITIES.map((s) => ({ value: s, label: s })),
        get: (r) => (r.severity || '').toUpperCase(),
      },
      // Advanced-condition only (duplicates search box / columns; not rendered in the declarative filter area).
      { key: 'id', label: 'ID', type: 'text', advOnly: true, get: (r) => r.id },
      { key: 'title', label: t('bug.colTitle', '标题'), type: 'text', advOnly: true, get: (r) => r.title || '' },
      { key: 'createdBy', label: t('lv.createdBy', '创建人'), type: 'text', advOnly: true, get: (r) => r.createdBy || '' },
      { key: 'handler', label: t('bug.handler', '处理人'), type: 'text', advOnly: true, get: (r) => r.handler || '' },
      { key: 'updatedBy', label: t('bug.updatedBy', '更新人'), type: 'text', advOnly: true, get: (r) => r.updatedBy || '' },
    ],
    columns: allColumns,
    rows: items,
  })

  const detailTabs = tabs.openIds.flatMap((id) =>
    id === NEW_KEY
      ? [{
          key: NEW_KEY,
          label: t('bug.new', '新建缺陷'),
          children: (
            <NewBugTab
              projectId={projectId}
              onCancel={() => tabs.close(NEW_KEY)}
              onCreated={(b) => {
                tabs.close(NEW_KEY)
                setItems((prev) => [b, ...prev.filter((x) => x.id !== b.id)])
              }}
            />
          ),
        }]
      : [],
  )

  return (
    <>
      <Workspace
        listLabel={t('bug.allBugs', '全部缺陷')}
        activeKey={tabs.activeKey}
        onChange={tabs.setActiveKey}
        onClose={tabs.close}
        tabs={detailTabs}
        listContent={
          <WorkList<Bug>
            onNew={() => tabs.open(NEW_KEY)}
            newLabel={t('bug.new', '新建缺陷')}
            extraActions={lv.toolbar}
            onRefresh={refresh}
            onRowClick={(r) => setDetailBug(r)}
            columns={lv.columns}
            data={lv.rows}
            pagination={lv.pagination}
            loading={loading}
            emptyText={t('bug.empty', '暂无缺陷')}
          />
        }
      />
      <RelationsDrawer bug={relBug} projectId={projectId} onClose={() => setRelBug(null)} />
      <EditBugDrawer
        bug={editBug}
        projectId={projectId}
        onClose={() => setEditBug(null)}
        onSaved={(b) => {
          setEditBug(null)
          setItems((prev) => prev.map((x) => (x.id === b.id ? { ...x, ...b } : x)))
        }}
      />
      <DetailBugDrawer bug={detailBug} onClose={() => setDetailBug(null)} />
    </>
  )
}

// Meta edit drawer: title / severity / handler via PUT /bug/{id}; the backend stamps updated_by/updated_at.
function EditBugDrawer({ bug, projectId, onClose, onSaved }: { bug: Bug | null; projectId: string; onClose: () => void; onSaved: (b: Bug) => void }) {
  const { t } = useI18n()
  const members = useMemberOptions(projectId)
  const [form] = Form.useForm<{ title: string; severity?: string; handler?: string; description?: string }>()
  const [saving, setSaving] = useState(false)
  useEffect(() => {
    if (bug) form.setFieldsValue({ title: bug.title || '', severity: bug.severity || undefined, handler: bug.handler || undefined, description: bug.description || undefined })
  }, [bug, form])
  const save = async () => {
    const v = await form.validateFields()
    if (!bug) return
    setSaving(true)
    try {
      const b = await api.updateBug(bug.id, { title: v.title, severity: v.severity, handler: v.handler, description: v.description })
      message.success(t('bug.updated', '缺陷已更新'))
      onSaved(b)
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('bug.updateFailed', '更新失败'))
    } finally {
      setSaving(false)
    }
  }
  return (
    <ResizableDrawer
      open={!!bug}
      onClose={onClose}
      width={480}
      title={`${t('bug.editTitle', '编辑缺陷')} · ${bug?.title || bug?.id || ''}`}
      footer={
        <Space>
          <Button type="primary" loading={saving} onClick={save}>{t('a.save', '保存')}</Button>
          <Button onClick={onClose}>{t('a.cancel', '取消')}</Button>
        </Space>
      }
    >
      <Form form={form} layout="vertical">
        <Form.Item name="title" label={t('bug.title', '标题')} rules={[{ required: true }]}>
          <Input />
        </Form.Item>
        <Form.Item name="severity" label={t('bug.severity', '严重程度')}>
          <Select
            allowClear
            placeholder={t('bug.severityPh', '选择严重程度')}
            options={SEVERITIES.map((s) => ({ value: s, label: severityDot(s) }))}
          />
        </Form.Item>
        <Form.Item name="handler" label={t('bug.handler', '处理人')}>
          <Select allowClear showSearch optionFilterProp="label" placeholder={t('bug.handlerPh', '选择处理人')} options={members} />
        </Form.Item>
        <Form.Item label={t('bug.description', '描述')}>
          <Form.Item name="description" noStyle>
            <MarkdownEditor projectId={projectId} placeholder={t('bug.descriptionPh', '复现步骤 / 期望结果 / 影响范围')} />
          </Form.Item>
        </Form.Item>
      </Form>
    </ResizableDrawer>
  )
}

// Read-only detail drawer opened by row click: shows metadata + the markdown description rendered.
function DetailBugDrawer({ bug, onClose }: { bug: Bug | null; onClose: () => void }) {
  const { t } = useI18n()
  const nav = useScopedNavigate()
  const [names, setNames] = useState<Record<string, string>>({})
  useEffect(() => {
    if (!bug) return
    const ids = [bug.createdBy, bug.handler, bug.updatedBy].filter((x): x is string => !!x)
    if (!ids.length) { setNames({}); return }
    api.userNames(ids).then(setNames).catch(() => setNames({}))
  }, [bug])
  const nameOf = (id?: string | null) => (id ? names[id] || id : '-')
  return (
    <ResizableDrawer
      open={!!bug}
      onClose={onClose}
      width={560}
      title={`${t('bug.detail', '缺陷详情')} · ${bug?.title || bug?.id || ''}`}
    >
      {bug && (
        <>
          <Descriptions bordered column={1} size="small">
            <Descriptions.Item label={t('bug.severity', '严重程度')}>{severityDot(bug.severity)}</Descriptions.Item>
            <Descriptions.Item label={t('bug.status', '状态')}>
              <Tag color={bugColor(bug.status || 'NEW')}>{statusLabel(t, bug.status || 'NEW')}</Tag>
            </Descriptions.Item>
            <Descriptions.Item label={t('bug.handler', '处理人')}>
              {bug.handler
                ? <a style={{ cursor: 'pointer' }} onClick={() => nav('/user')} title={t('bug.openUser', '在用户管理中查看')}>{nameOf(bug.handler)}</a>
                : '-'}
            </Descriptions.Item>
            <Descriptions.Item label={t('bug.createdBy', '创建人')}>{nameOf(bug.createdBy)}</Descriptions.Item>
            <Descriptions.Item label={t('bug.updatedBy', '更新人')}>{nameOf(bug.updatedBy)}</Descriptions.Item>
            <Descriptions.Item label={t('bug.createdAt', '创建时间')}>
              {bug.createdAt ? new Date(bug.createdAt).toLocaleString() : '-'}
            </Descriptions.Item>
            <Descriptions.Item label={t('bug.updatedAt', '更新时间')}>{fmtTs(bug.updatedAt)}</Descriptions.Item>
          </Descriptions>
          <div style={{ marginTop: 16 }}>
            <div style={{ fontWeight: 600, marginBottom: 8 }}>{t('bug.description', '描述')}</div>
            {bug.description
              ? <MarkdownRenderer value={bug.description} />
              : <Empty description={t('bug.noDescription', '暂无描述')} />}
          </div>
        </>
      )}
    </ResizableDrawer>
  )
}

// New bugs always start in NEW status; importing historical bugs with other statuses goes through the API's initialStatus.
function NewBugTab({ projectId, onCreated, onCancel }: { projectId: string; onCreated: (b: Bug) => void; onCancel: () => void }) {
  const { t } = useI18n()
  // Bug field template: every field except title/severity/handler is custom, rendered from the template.
  const { fields: tplFields } = useFieldTemplate('bug')
  const members = useMemberOptions(projectId)
  const [saving, setSaving] = useState(false)
  return (
    <div style={{ padding: '16px 24px', height: '100%', overflow: 'auto' }}>
      <Form
        layout="vertical"
        style={{ maxWidth: 560 }}
        onFinish={async (v: { title: string; severity?: string; handler?: string; description?: string; [CF_GROUP]?: Record<string, unknown> }) => {
          const customFields = collectCustomValues(tplFields, v[CF_GROUP])
          setSaving(true)
          try {
            const b = await api.createBug({
              projectId,
              title: v.title,
              initialStatus: 'NEW',
              severity: v.severity,
              handler: v.handler,
              description: v.description?.trim() || undefined,
              customFields: Object.keys(customFields).length ? customFields : undefined,
            })
            message.success(t('bug.created', '缺陷已创建'))
            onCreated(b)
          } catch (e) {
            message.error(e instanceof ApiError ? e.message : t('bug.createFailed', '创建失败'))
          } finally {
            setSaving(false)
          }
        }}
      >
        <Form.Item name="title" label={t('bug.title', '标题')} rules={[{ required: true }]}>
          <Input placeholder={t('bug.titlePlaceholder', '如:登录按钮无响应')} autoFocus />
        </Form.Item>
        <Form.Item name="severity" label={t('bug.severity', '严重程度')}>
          <Select
            allowClear
            placeholder={t('bug.severityPh', '选择严重程度')}
            options={SEVERITIES.map((s) => ({ value: s, label: severityDot(s) }))}
          />
        </Form.Item>
        <Form.Item name="handler" label={t('bug.handler', '处理人')}>
          <Select allowClear showSearch optionFilterProp="label" placeholder={t('bug.handlerPh', '选择处理人')} options={members} />
        </Form.Item>
        <Form.Item label={t('bug.description', '描述')}>
          <Form.Item name="description" noStyle>
            <MarkdownEditor projectId={projectId} placeholder={t('bug.descriptionPh', '复现步骤 / 期望结果 / 影响范围')} />
          </Form.Item>
        </Form.Item>
        <CustomFieldItems kind="bug" fields={tplFields} />
        <Space>
          <Button type="primary" htmlType="submit" loading={saving}>{t('a.create', '创建')}</Button>
          <Button onClick={onCancel}>{t('a.cancel', '取消')}</Button>
        </Space>
      </Form>
    </div>
  )
}

// Bug ↔ asset relations drawer: traces which requirement/scenario/functional case a bug came from.
// Target names resolve from each list endpoint; already-linked targets are hidden from candidates.
const REL_KINDS = ['REQUIREMENT', 'SCENARIO', 'FUNCTIONAL_CASE'] as const

function RelationsDrawer({ bug, projectId, onClose }: { bug: Bug | null; projectId: string; onClose: () => void }) {
  const { t } = useI18n()
  const [relations, setRelations] = useState<BugRelation[]>([])
  const [names, setNames] = useState<Record<string, Map<string, string>>>({})
  const [kind, setKind] = useState<string>('REQUIREMENT')
  const [targetId, setTargetId] = useState<string | undefined>()
  const [busy, setBusy] = useState(false)

  const kindLabel: Record<string, string> = {
    REQUIREMENT: t('bug.kindRequirement', '需求'),
    SCENARIO: t('bug.kindScenario', '场景用例'),
    FUNCTIONAL_CASE: t('bug.kindFunctional', '功能用例'),
  }
  const kindColor: Record<string, string> = { REQUIREMENT: 'geekblue', SCENARIO: 'purple', FUNCTIONAL_CASE: 'cyan' }

  useEffect(() => {
    if (!bug) return
    setKind('REQUIREMENT')
    setTargetId(undefined)
    api.bugRelations(bug.id).then((p) => setRelations(p.relations ?? [])).catch(() => setRelations([]))
    // Fetch each asset kind once and build id → name maps (shared by candidate dropdown and linked rows).
    Promise.all([
      api.requirements(projectId).then((p) => p.items ?? []).catch(() => []),
      api.scenarios(projectId).catch(() => []),
      api.functionalCases(projectId).catch(() => []),
    ]).then(([reqs, scns, cases]) => {
      setNames({
        REQUIREMENT: new Map(reqs.map((r) => [r.id, r.title])),
        SCENARIO: new Map(scns.map((s) => [s.id, s.name])),
        FUNCTIONAL_CASE: new Map(cases.map((c) => [c.id, c.name])),
      })
    })
  }, [bug, projectId])

  const linkedKeys = useMemo(() => new Set(relations.map((r) => `${r.kind}:${r.targetId}`)), [relations])
  const candidates = [...(names[kind]?.entries() ?? [])]
    .filter(([id]) => !linkedKeys.has(`${kind}:${id}`))
    .map(([id, name]) => ({ value: id, label: name }))

  const link = async () => {
    if (!bug || !targetId) return
    setBusy(true)
    try {
      const p = await api.linkBugRelation(bug.id, { kind, targetId })
      setRelations(p.relations ?? [])
      setTargetId(undefined)
      message.success(t('bug.linked', '已关联'))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('bug.linkFailed', '关联失败'))
    } finally {
      setBusy(false)
    }
  }

  const unlink = async (r: BugRelation) => {
    if (!bug) return
    try {
      const p = await api.unlinkBugRelation(bug.id, r.kind, r.targetId)
      setRelations(p.relations ?? [])
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('bug.unlinkFailed', '取消关联失败'))
    }
  }

  return (
    <ResizableDrawer open={!!bug} onClose={onClose} width={520} title={`${t('bug.relTitle', '关联资产')} · ${bug?.title || bug?.id || ''}`}>
      <Space.Compact style={{ width: '100%', marginBottom: 16 }}>
        <Select
          value={kind}
          onChange={(v) => { setKind(v); setTargetId(undefined) }}
          style={{ width: 130 }}
          options={REL_KINDS.map((k) => ({ value: k, label: kindLabel[k] }))}
        />
        <Select
          showSearch
          optionFilterProp="label"
          placeholder={t('bug.relTarget', '选择要关联的目标')}
          value={targetId}
          onChange={setTargetId}
          style={{ flex: 1 }}
          options={candidates}
          notFoundContent={t('common.empty', '暂无数据')}
        />
        <Button type="primary" loading={busy} disabled={!targetId} onClick={link}>
          {t('bug.linkBtn', '关联')}
        </Button>
      </Space.Compact>
      {relations.length === 0 ? (
        <Empty description={t('bug.relEmpty', '暂无关联,从上方选择需求/用例建立追溯')} />
      ) : (
        relations.map((r) => (
          <div
            key={`${r.kind}:${r.targetId}`}
            style={{
              display: 'flex', alignItems: 'center', gap: 8, padding: '8px 10px', marginBottom: 6,
              background: 'var(--panel-2)', border: '1px solid var(--border-soft)', borderRadius: 8,
            }}
          >
            <Tag color={kindColor[r.kind] || 'default'} style={{ marginRight: 0 }}>{kindLabel[r.kind] || r.kind}</Tag>
            <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
              {names[r.kind]?.get(r.targetId) || <span className="ms-mono" style={{ fontSize: 12, color: 'var(--text-3)' }}>{r.targetId}</span>}
            </span>
            <Button type="link" size="small" danger onClick={() => unlink(r)}>{t('bug.unlink', '取消关联')}</Button>
          </div>
        ))
      )}
    </ResizableDrawer>
  )
}
