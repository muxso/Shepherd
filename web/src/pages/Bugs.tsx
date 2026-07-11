import { useCallback, useEffect, useMemo, useState } from 'react'
import { Button, Drawer, Empty, Form, Input, Modal, Select, Space, Table, Tag, Tooltip } from 'antd'
import { message, modal } from '../feedback'
import { LinkOutlined, PlusOutlined, ReloadOutlined } from '@ant-design/icons'
import { api, ApiError, userIdStore, type Bug, type BugRelation } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'
import { PageBody, PageContainer, PageHeader, SelectProjectEmpty } from '../components/Page'
import { useListView, type ListColumn } from '../components/ListView'
import { CF_GROUP, CustomFieldItems, collectCustomValues, useFieldTemplate } from '../components/TemplateFields'

const STATUSES = ['NEW', 'RESOLVED', 'CLOSED', 'REOPENED', 'REJECTED']

const bugColor = (s: string) => {
  const v = s.toUpperCase()
  if (v === 'RESOLVED' || v === 'CLOSED') return 'green'
  if (v === 'REJECTED') return 'red'
  if (v === 'NEW' || v === 'REOPENED') return 'orange'
  return 'blue'
}

// 状态码 → 展示文案(i18n key 兜底中文);未登记的自定义状态原样显示。
const statusLabel = (t: (k: string, d?: string) => string, s: string) => {
  const zh: Record<string, string> = {
    NEW: '新建', RESOLVED: '已解决', CLOSED: '已关闭', REOPENED: '重新打开', REJECTED: '已拒绝',
  }
  return zh[s] ? t(`bug.st.${s}`, zh[s]) : s
}

// 缺陷列表/创建/状态流转全走后端(GET /bug、POST /bug、POST /bug/{id}/status)。
export default function Bugs() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const [items, setItems] = useState<Bug[]>([])
  const [loading, setLoading] = useState(false)
  const [createOpen, setCreateOpen] = useState(false)
  const [relBug, setRelBug] = useState<Bug | null>(null)

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
          setItems((prev) => prev.map((x) => (x.id === item.id ? { ...x, status: b.status } : x)))
        } catch (e) {
          message.error(e instanceof ApiError ? `${t('bug.changeFailedStatus', '变更失败')}:${e.status}${t('bug.illegalTransition', '(非法流转?)')}` : t('bug.changeFailed', '变更失败'))
        }
      },
    })
  }

  return <BugsList items={items} loading={loading} projectId={projectId} refresh={refresh} createOpen={createOpen} setCreateOpen={setCreateOpen} setItems={setItems} relBug={relBug} setRelBug={setRelBug} changeStatus={changeStatus} t={t} />
}

// 列表 + 三件套(视图/筛选/列设置):useListView 是 hook,拆成子组件避免主组件条件返回后调用 hook。
function BugsList({ items, loading, projectId, refresh, createOpen, setCreateOpen, setItems, relBug, setRelBug, changeStatus, t }: {
  items: Bug[]
  loading: boolean
  projectId: string
  refresh: () => void
  createOpen: boolean
  setCreateOpen: (v: boolean) => void
  setItems: React.Dispatch<React.SetStateAction<Bug[]>>
  relBug: Bug | null
  setRelBug: (b: Bug | null) => void
  changeStatus: (b: Bug) => void
  t: (k: string, d?: string) => string
}) {
  // 缺陷字段模板:title 之外的字段全靠自定义,创建弹窗按模板渲染。
  const { fields: tplFields } = useFieldTemplate('bug')
  const allColumns: ListColumn<Bug>[] = [
    { key: 'title', label: t('bug.title', '标题'), title: t('bug.title', '标题'), dataIndex: 'title' },
    {
      key: 'status',
      label: t('bug.status', '状态'),
      title: t('bug.status', '状态'),
      width: 130,
      render: (_, r) => <Tag color={bugColor(r.status || 'NEW')}>{statusLabel(t, r.status || 'NEW')}</Tag>,
    },
    { key: 'id', label: 'ID', title: 'ID', dataIndex: 'id', width: 110, render: (v: string) => <Tooltip title={v}><span className="ms-mono" style={{ fontSize: 12, color: 'var(--text-3)' }}>{v?.slice(0, 8)}</span></Tooltip> },
    {
      key: 'action',
      label: t('bug.action', '操作'),
      title: t('bug.action', '操作'),
      width: 190,
      render: (_, r) => (
        <>
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
      // 以下仅供条件选择(与搜索框/列展示重复,不渲染在声明式筛选区)。
      { key: 'id', label: 'ID', type: 'text', advOnly: true, get: (r) => r.id },
      { key: 'title', label: t('bug.colTitle', '标题'), type: 'text', advOnly: true, get: (r) => r.title || '' },
      { key: 'createdBy', label: t('lv.createdBy', '创建人'), type: 'text', advOnly: true, get: (r) => r.createdBy || '' },
    ],
    columns: allColumns,
    rows: items,
  })

  return (
    <PageContainer>
      <PageHeader
        title={t('m.bug', '缺陷')}
        extra={
          <>
            {lv.toolbar}
            <Button type="primary" icon={<PlusOutlined />} onClick={() => setCreateOpen(true)}>
              {t('bug.new', '新建缺陷')}
            </Button>
            <Button icon={<ReloadOutlined />} onClick={refresh} />
          </>
        }
      />
      <PageBody>
        <Table<Bug>
          rowKey="id"
          size="middle"
          loading={loading}
          dataSource={lv.rows}
          pagination={{ pageSize: 15, size: 'small' }}
          locale={{ emptyText: <Empty description={t('bug.empty', '暂无缺陷')} /> }}
          columns={lv.columns}
        />
      </PageBody>

      <Modal title={t('bug.new', '新建缺陷')} open={createOpen} onCancel={() => setCreateOpen(false)} footer={null} destroyOnHidden>
        {/* 新建缺陷一律从「新建」状态开始;导入历史缺陷带其它状态走 API 的 initialStatus。 */}
        <Form
          layout="vertical"
          onFinish={async (v: { title: string; [CF_GROUP]?: Record<string, unknown> }) => {
            const customFields = collectCustomValues(tplFields, v[CF_GROUP])
            try {
              const b = await api.createBug({
                projectId,
                title: v.title,
                initialStatus: 'NEW',
                customFields: Object.keys(customFields).length ? customFields : undefined,
              })
              message.success(t('bug.created', '缺陷已创建'))
              setItems((prev) => [b, ...prev.filter((x) => x.id !== b.id)])
              setCreateOpen(false)
            } catch (e) {
              message.error(e instanceof ApiError ? e.message : t('bug.createFailed', '创建失败'))
            }
          }}
        >
          <Form.Item name="title" label={t('bug.title', '标题')} rules={[{ required: true }]}>
            <Input placeholder={t('bug.titlePlaceholder', '如:登录按钮无响应')} autoFocus />
          </Form.Item>
          {/* 自定义字段(字段模板):按模板配置动态渲染。 */}
          <CustomFieldItems kind="bug" fields={tplFields} />
          <Button type="primary" htmlType="submit" block>
            {t('a.create', '创建')}
          </Button>
        </Form>
      </Modal>

      <RelationsDrawer bug={relBug} projectId={projectId} onClose={() => setRelBug(null)} />
    </PageContainer>
  )
}

// 缺陷 ↔ 资产关联抽屉:回溯缺陷来自哪条需求/场景用例/功能用例。
// 目标名称从各自列表接口解析;已关联的目标在候选里隐藏。
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
    // 三类资产各拉一次,构建 id → 名称 映射(候选下拉 + 已关联行共用)。
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
    <Drawer open={!!bug} onClose={onClose} width={520} title={`${t('bug.relTitle', '关联资产')} · ${bug?.title || bug?.id || ''}`}>
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
    </Drawer>
  )
}
