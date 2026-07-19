import { useEffect, useState } from 'react'
import { Button, Form, Input, Select, Table, Tag, Tooltip } from 'antd'
import { LinkOutlined, SearchOutlined } from '@ant-design/icons'
import { message } from '../../feedback'
import EditDrawer from '../EditDrawer'
import { api, ApiError, type Bug } from '../../api'
import { bugStatusColor, bugStatusLabel, priorityColor } from '../tags'
import { useI18n } from '../../i18n'

const SEVERITIES = ['P0', 'P1', 'P2', 'P3']

/** Handler candidates: project members with display names resolved (id fallback). */
function useMemberOptions(projectId: string) {
  const [options, setOptions] = useState<{ value: string; label: string }[]>([])
  useEffect(() => {
    let alive = true
    api
      .projectMembers(projectId)
      .then(async (ms) => {
        const ids = [...new Set(ms.map((m) => m.userId).filter(Boolean))]
        const names = ids.length ? await api.userNames(ids).catch(() => ({}) as Record<string, string>) : {}
        if (alive) setOptions(ids.map((id) => ({ value: id, label: names[id] || id })))
      })
      .catch(() => {})
    return () => { alive = false }
  }, [projectId])
  return options
}

// 缺陷列表 tab of plan detail: bugs linked to the plan (relation kind = PLAN,
// reverse endpoint /bug/by-plan) + link picker / create-and-link / unlink.
export default function PlanBugsPanel({ planId, projectId, bugs, loading, reload }: {
  planId: string
  projectId: string
  bugs: Bug[]
  loading: boolean
  reload: () => void
}) {
  const { t } = useI18n()
  const [search, setSearch] = useState('')
  const [linkOpen, setLinkOpen] = useState(false)
  const [newOpen, setNewOpen] = useState(false)
  // Resolve handler ids in the current rows to display names.
  const [names, setNames] = useState<Record<string, string>>({})
  useEffect(() => {
    const ids = [...new Set(bugs.map((b) => b.handler).filter((x): x is string => !!x))]
    if (!ids.length) { setNames({}); return }
    api.userNames(ids).then(setNames).catch(() => setNames({}))
  }, [bugs])

  const unlink = async (b: Bug) => {
    try {
      await api.unlinkBugRelation(b.id, 'PLAN', planId)
      message.success(t('plan.bugUnlinked', '已取消关联'))
      reload()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('plan.bugUnlinkFail', '取消关联失败'))
    }
  }

  const visible = bugs.filter((b) => !search || (b.title || '').includes(search) || b.id.includes(search))
  return (
    <div style={{ padding: '12px 16px', height: '100%', overflow: 'auto' }}>
      <div style={{ display: 'flex', gap: 12, marginBottom: 12, alignItems: 'center' }}>
        <Input
          style={{ width: 260 }}
          placeholder={t('plan.bugSearchPh', '通过 ID/名称搜索')}
          suffix={<SearchOutlined style={{ color: 'var(--text-3)' }} />}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          allowClear
        />
        <span style={{ flex: 1 }} />
        <Button type="primary" icon={<LinkOutlined />} onClick={() => setLinkOpen(true)}>{t('plan.bugLink', '关联缺陷')}</Button>
        <Button onClick={() => setNewOpen(true)}>{t('plan.bugNew', '新建缺陷')}</Button>
      </div>
      <Table<Bug>
        rowKey="id"
        size="middle"
        loading={loading}
        pagination={false}
        dataSource={visible}
        locale={{
          emptyText: (
            <span style={{ color: 'var(--text-2)' }}>
              {t('plan.noBugs', '暂无关联缺陷')}，<a onClick={() => setLinkOpen(true)}>{t('plan.bugLink', '关联缺陷')}</a>{' '}
              {t('funcd.or', '或')} <a onClick={() => setNewOpen(true)}>{t('plan.bugNew', '新建缺陷')}</a>
            </span>
          ),
        }}
        columns={[
          {
            title: 'ID', dataIndex: 'id', width: 110,
            render: (v: string) => <Tooltip title={v}><span className="ms-mono" style={{ fontSize: 12, color: 'var(--text-3)' }}>{v.slice(0, 8)}</span></Tooltip>,
          },
          { title: t('plan.bugName', '名称'), dataIndex: 'title' },
          {
            title: t('plan.bugSeverity', '严重程度'), dataIndex: 'severity', width: 110,
            render: (s?: string | null) => (s ? <Tag color={priorityColor(s)}>{s}</Tag> : '—'),
          },
          {
            title: t('plan.bugStatus', '状态'), dataIndex: 'status', width: 120,
            render: (s: string) => <Tag color={bugStatusColor(s || 'NEW')}>{bugStatusLabel(s, t)}</Tag>,
          },
          {
            title: t('plan.bugHandler', '处理人'), dataIndex: 'handler', width: 130,
            render: (u?: string | null) => (u ? names[u] || u : '—'),
          },
          {
            title: t('plan.bugCreatedAt', '创建时间'), dataIndex: 'createdAt', width: 170,
            render: (v?: number) => (v ? <span className="ms-mono" style={{ fontSize: 12 }}>{new Date(v).toLocaleString()}</span> : '—'),
          },
          {
            title: t('req.action', '操作'), width: 110,
            render: (_v, r) => (
              <Button type="link" size="small" danger onClick={() => unlink(r)}>{t('plan.bugUnlink', '取消关联')}</Button>
            ),
          },
        ]}
      />
      <LinkBugsDrawer
        open={linkOpen}
        planId={planId}
        projectId={projectId}
        linked={bugs}
        onClose={() => setLinkOpen(false)}
        onLinked={() => { setLinkOpen(false); reload() }}
      />
      <NewBugDrawer
        open={newOpen}
        planId={planId}
        projectId={projectId}
        onClose={() => setNewOpen(false)}
        onCreated={() => { setNewOpen(false); reload() }}
      />
    </div>
  )
}

// Right-side picker: multi-select project bugs (already-linked ones excluded) → link each to the plan.
function LinkBugsDrawer({ open, planId, projectId, linked, onClose, onLinked }: {
  open: boolean
  planId: string
  projectId: string
  linked: Bug[]
  onClose: () => void
  onLinked: () => void
}) {
  const { t } = useI18n()
  const [candidates, setCandidates] = useState<Bug[]>([])
  const [selected, setSelected] = useState<string[]>([])
  const [saving, setSaving] = useState(false)
  useEffect(() => {
    if (!open) return
    setSelected([])
    api.bugs(projectId).then(setCandidates).catch(() => setCandidates([]))
  }, [open, projectId])
  const linkedIds = new Set(linked.map((b) => b.id))
  const options = candidates
    .filter((b) => !linkedIds.has(b.id))
    .map((b) => ({ value: b.id, label: b.title || b.id }))
  const doLink = async () => {
    if (!selected.length) return
    setSaving(true)
    try {
      for (const id of selected) await api.linkBugRelation(id, { kind: 'PLAN', targetId: planId })
      message.success(t('plan.bugLinked', '已关联'))
      onLinked()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('plan.bugLinkFail', '关联失败'))
    } finally {
      setSaving(false)
    }
  }
  return (
    <EditDrawer
      open={open}
      title={t('plan.bugLink', '关联缺陷')}
      onCancel={onClose}
      onOk={doLink}
      confirmLoading={saving}
      okButtonProps={{ disabled: !selected.length }}
    >
      <Select
        mode="multiple"
        showSearch
        style={{ width: '100%' }}
        placeholder={t('plan.bugPick', '选择要关联的缺陷')}
        optionFilterProp="label"
        value={selected}
        onChange={setSelected}
        options={options}
        notFoundContent={t('plan.bugNoCandidates', '项目暂无可关联缺陷')}
      />
    </EditDrawer>
  )
}

// New-bug shortcut: same fields as the bug workspace create form (title/severity/handler),
// always starts in NEW, then auto-links to the plan.
function NewBugDrawer({ open, planId, projectId, onClose, onCreated }: {
  open: boolean
  planId: string
  projectId: string
  onClose: () => void
  onCreated: () => void
}) {
  const { t } = useI18n()
  const members = useMemberOptions(projectId)
  const [form] = Form.useForm<{ title: string; severity?: string; handler?: string }>()
  const [saving, setSaving] = useState(false)
  useEffect(() => { if (open) form.resetFields() }, [open, form])
  const doCreate = async () => {
    const v = await form.validateFields()
    setSaving(true)
    try {
      const b = await api.createBug({ projectId, title: v.title.trim(), initialStatus: 'NEW', severity: v.severity, handler: v.handler })
      await api.linkBugRelation(b.id, { kind: 'PLAN', targetId: planId })
      message.success(t('plan.bugCreated', '缺陷已创建并关联'))
      onCreated()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('plan.bugCreateFail', '创建失败'))
    } finally {
      setSaving(false)
    }
  }
  return (
    <EditDrawer
      open={open}
      title={t('plan.bugNew', '新建缺陷')}
      onCancel={onClose}
      onOk={doCreate}
      confirmLoading={saving}
      okText={t('a.create', '创建')}
    >
      <Form form={form} layout="vertical">
        <Form.Item name="title" label={t('plan.bugName', '名称')} rules={[{ required: true, message: t('plan.bugTitleRequired', '请输入缺陷名称') }]}>
          <Input placeholder={t('plan.bugTitleRequired', '请输入缺陷名称')} autoFocus />
        </Form.Item>
        <Form.Item name="severity" label={t('plan.bugSeverity', '严重程度')}>
          <Select
            allowClear
            placeholder={t('bug.severityPh', '选择严重程度')}
            options={SEVERITIES.map((s) => ({ value: s, label: <span style={{ color: priorityColor(s) }}>● {s}</span> }))}
          />
        </Form.Item>
        <Form.Item name="handler" label={t('plan.bugHandler', '处理人')}>
          <Select allowClear showSearch optionFilterProp="label" placeholder={t('bug.handlerPh', '选择处理人')} options={members} />
        </Form.Item>
      </Form>
    </EditDrawer>
  )
}
