import { useCallback, useEffect, useState } from 'react'
import { Button, Card, Input, Select, Switch, Table, Tag } from 'antd'
import ResizableDrawer from '../components/ResizableDrawer'
import { PlusOutlined, RobotOutlined, SendOutlined } from '@ant-design/icons'
import type { ColumnsType } from 'antd/es/table'
import { api, type NoticeChannel, type NoticeRobot, type NoticeRobotPlatform, type NoticeRule } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'
import { message, modal } from '../feedback'
import { SelectProjectEmpty } from '../components/Page'

// Message settings ("Project → Message Settings"). Robots (Feishu / DingTalk /
// WeCom webhooks) and notification rules are stored server-side: rules decide
// per event whether it lands in the inbox and/or gets pushed to robots.

type NavKey = 'robot' | 'rules'

const PLATFORMS: NoticeRobotPlatform[] = ['FEISHU', 'DINGTALK', 'WECOM']
const CHANNELS: NoticeChannel[] = ['IN_APP', 'ROBOT']
// Producer event types (matches the backend Notifier producers) plus the wildcard.
const EVENTS = ['BUG_ASSIGNED', 'BUG_STATUS_CHANGED', 'REVIEW_CREATED', 'MENTIONED', 'PLAN_SCHEDULE_FAILED', '*']

type TFn = (k: string, d?: string) => string

const platformLabel = (t: TFn, p: NoticeRobotPlatform) => t(`msgset.platform.${p}`, p)
const channelLabel = (t: TFn, c: NoticeChannel) => t(`msgset.ch.${c}`, c)
const eventLabel = (t: TFn, e: string) => (e === '*' ? t('msgset.evtAll', '全部事件 (*)') : t(`msg.evt.${e}`, e))

interface RobotDraft {
  id?: string
  name: string
  platform: NoticeRobotPlatform
  webhookUrl: string
  secret: string
  enabled: boolean
}

interface RuleDraft {
  id?: string
  eventType: string
  channels: NoticeChannel[]
  robotIds: string[]
  template: string
  enabled: boolean
}

export default function MessageSettings() {
  const { t } = useI18n()
  const { projectId, projects } = useApp()
  const [nav, setNav] = useState<NavKey>('rules')
  const [robots, setRobots] = useState<NoticeRobot[]>([])
  const [rules, setRules] = useState<NoticeRule[]>([])
  const [loading, setLoading] = useState(false)

  const reload = useCallback(() => {
    if (!projectId) return
    setLoading(true)
    Promise.all([api.noticeRobots(projectId), api.noticeRules(projectId)])
      .then(([rb, rl]) => {
        setRobots(rb ?? [])
        setRules(rl ?? [])
      })
      .catch(() => {
        setRobots([])
        setRules([])
      })
      .finally(() => setLoading(false))
  }, [projectId])

  useEffect(() => reload(), [reload])

  if (!projectId || !projects.find((p) => p.id === projectId)) return <SelectProjectEmpty />

  const navItems: { key: NavKey; label: string }[] = [
    { key: 'rules', label: t('msgset.rules', '通知规则') },
    { key: 'robot', label: t('msgset.robot', '机器人') },
  ]

  return (
    <div style={{ display: 'flex', height: '100%' }}>
      {/* Left secondary nav */}
      <div
        style={{
          width: 200,
          flexShrink: 0,
          borderRight: '1px solid var(--border-soft)',
          padding: '12px 8px',
          overflow: 'auto',
          background: 'var(--panel)',
        }}
      >
        <div style={{ fontWeight: 600, fontSize: 13, padding: '4px 10px 8px' }}>{t('msgset.title', '消息设置')}</div>
        {navItems.map((it) => (
          <div
            key={it.key}
            onClick={() => setNav(it.key)}
            style={{
              padding: '8px 12px',
              borderRadius: 6,
              cursor: 'pointer',
              fontSize: 13,
              margin: '2px 0',
              background: nav === it.key ? 'var(--brand-soft)' : 'transparent',
              color: nav === it.key ? 'var(--brand)' : 'var(--text)',
            }}
          >
            {it.label}
          </div>
        ))}
      </div>
      {/* Right panel */}
      <div style={{ flex: 1, minWidth: 0, overflow: 'auto', padding: 16, background: 'var(--bg)' }}>
        {nav === 'robot' ? (
          <RobotPanel projectId={projectId} robots={robots} loading={loading} reload={reload} t={t} />
        ) : (
          <RulePanel projectId={projectId} rules={rules} robots={robots} loading={loading} reload={reload} t={t} />
        )}
      </div>
    </div>
  )
}

const fieldRow = (label: string, control: React.ReactNode, hint?: string) => (
  <div style={{ marginBottom: 16 }} key={label}>
    <div style={{ fontSize: 13, color: 'var(--text-2)', marginBottom: 6 }}>{label}</div>
    {control}
    {hint && <div style={{ fontSize: 12, color: 'var(--text-3)', marginTop: 4 }}>{hint}</div>}
  </div>
)

// ---------- Robots ----------

function RobotPanel({
  projectId,
  robots,
  loading,
  reload,
  t,
}: {
  projectId: string
  robots: NoticeRobot[]
  loading: boolean
  reload: () => void
  t: TFn
}) {
  const [editing, setEditing] = useState<RobotDraft | null>(null)
  const [testing, setTesting] = useState<string | null>(null)

  const test = (r: NoticeRobot) => {
    setTesting(r.id)
    api
      .testNoticeRobot(r.id, projectId)
      .then((res) => {
        const ok = res.status >= 200 && res.status < 300
        const text = `${t(ok ? 'msgset.testOk' : 'msgset.testFail', ok ? '测试发送成功' : '测试发送失败').replace('{status}', String(res.status))}${ok ? '' : ` (${res.status})`}`
        const detail = res.body ? ` ${res.body.slice(0, 120)}` : ''
        if (ok) message.success(text + detail)
        else message.warning(text + detail)
      })
      .catch((e: Error) => message.error(`${t('msgset.testFail', '测试发送失败')}: ${e.message}`))
      .finally(() => setTesting(null))
  }

  const remove = (r: NoticeRobot) =>
    modal.confirm({
      title: t('msgset.delRobotConfirm', '确认删除该机器人?'),
      okType: 'danger',
      okText: t('a.delete', '删除'),
      cancelText: t('common.cancel', '取消'),
      onOk: () =>
        api
          .deleteNoticeRobot(r.id, projectId)
          .then(reload)
          .catch((e: Error) => message.error(e.message)),
    })

  const toggle = (r: NoticeRobot, enabled: boolean) =>
    api
      .updateNoticeRobot(r.id, { projectId, name: r.name, platform: r.platform, webhookUrl: r.webhookUrl, secret: r.secret, enabled })
      .then(reload)
      .catch((e: Error) => message.error(e.message))

  const cols: ColumnsType<NoticeRobot> = [
    {
      title: t('msgset.robotName', '名称'),
      dataIndex: 'name',
      width: 200,
      render: (v: string) => (
        <span>
          <RobotOutlined style={{ color: 'var(--brand)', marginRight: 6 }} />
          {v}
        </span>
      ),
    },
    {
      title: t('msgset.robotPlatform', '平台'),
      dataIndex: 'platform',
      width: 120,
      render: (v: NoticeRobotPlatform) => <Tag color="blue">{platformLabel(t, v)}</Tag>,
    },
    {
      title: t('msgset.robotWebhook', 'Webhook 地址'),
      dataIndex: 'webhookUrl',
      ellipsis: true,
      render: (v: string) => (
        <span className="ms-mono" style={{ fontSize: 12 }}>
          {v}
        </span>
      ),
    },
    {
      title: t('msgset.colEnabled', '是否启用'),
      dataIndex: 'enabled',
      width: 90,
      render: (v: boolean, r) => <Switch size="small" checked={v} onChange={(c) => toggle(r, c)} />,
    },
    {
      title: t('apidef.colAction', '操作'),
      width: 220,
      render: (_v, r) => (
        <>
          <Button type="link" size="small" icon={<SendOutlined />} loading={testing === r.id} onClick={() => test(r)}>
            {t('msgset.test', '测试发送')}
          </Button>
          <Button
            type="link"
            size="small"
            onClick={() =>
              setEditing({ id: r.id, name: r.name, platform: r.platform, webhookUrl: r.webhookUrl, secret: r.secret, enabled: r.enabled })
            }
          >
            {t('a.edit', '编辑')}
          </Button>
          <Button type="link" size="small" danger onClick={() => remove(r)}>
            {t('a.delete', '删除')}
          </Button>
        </>
      ),
    },
  ]

  return (
    <Card
      size="small"
      title={t('msgset.robot', '机器人')}
      extra={
        <Button
          type="primary"
          size="small"
          icon={<PlusOutlined />}
          onClick={() => setEditing({ name: '', platform: 'FEISHU', webhookUrl: '', secret: '', enabled: true })}
        >
          {t('msgset.addRobot', '添加机器人')}
        </Button>
      }
      styles={{ body: { padding: 12 } }}
    >
      <Table<NoticeRobot> rowKey="id" size="middle" loading={loading} dataSource={robots} columns={cols} pagination={false} />
      <RobotDrawer draft={editing} projectId={projectId} onClose={() => setEditing(null)} onSaved={() => { setEditing(null); reload() }} t={t} />
    </Card>
  )
}

function RobotDrawer({
  draft,
  projectId,
  onClose,
  onSaved,
  t,
}: {
  draft: RobotDraft | null
  projectId: string
  onClose: () => void
  onSaved: () => void
  t: TFn
}) {
  const [d, setD] = useState<RobotDraft | null>(draft)
  const [saving, setSaving] = useState(false)
  useEffect(() => setD(draft), [draft])
  if (!d) return null
  const set = (patch: Partial<RobotDraft>) => setD({ ...d, ...patch })

  const save = () => {
    if (!d.name.trim()) {
      message.warning(t('msgset.needName', '请填写机器人名称'))
      return
    }
    if (!d.webhookUrl.trim()) {
      message.warning(t('msgset.needWebhook', '请填写 Webhook 地址'))
      return
    }
    const body = { projectId, name: d.name, platform: d.platform, webhookUrl: d.webhookUrl, secret: d.secret, enabled: d.enabled }
    setSaving(true)
    ;(d.id ? api.updateNoticeRobot(d.id, body) : api.createNoticeRobot(body))
      .then(() => {
        message.success(t('msgset.saved', '已保存'))
        onSaved()
      })
      .catch((e: Error) => message.error(e.message))
      .finally(() => setSaving(false))
  }

  return (
    <ResizableDrawer
      open={!!draft}
      onClose={onClose}
      width={520}
      title={t(d.id ? 'msgset.editRobot' : 'msgset.addRobot', d.id ? '编辑机器人' : '添加机器人')}
      extra={
        <Button type="primary" loading={saving} onClick={save}>
          {t('msgset.save', '保存')}
        </Button>
      }
    >
      {fieldRow(t('msgset.robotName', '名称'), <Input value={d.name} onChange={(e) => set({ name: e.target.value })} />)}
      {fieldRow(
        t('msgset.robotPlatform', '平台'),
        <Select
          style={{ width: '100%' }}
          value={d.platform}
          onChange={(v) => set({ platform: v as NoticeRobotPlatform })}
          options={PLATFORMS.map((p) => ({ label: platformLabel(t, p), value: p }))}
        />,
      )}
      {fieldRow(
        t('msgset.robotWebhook', 'Webhook 地址'),
        <Input value={d.webhookUrl} onChange={(e) => set({ webhookUrl: e.target.value })} placeholder="https://..." />,
      )}
      {d.platform === 'DINGTALK' &&
        fieldRow(
          t('msgset.robotSecret', '加签密钥'),
          <Input.Password value={d.secret} onChange={(e) => set({ secret: e.target.value })} placeholder={t('msgset.optional', '可选')} />,
          t('msgset.secretHint', '钉钉安全设置「加签」的密钥,留空则不签名'),
        )}
      {fieldRow(t('msgset.colEnabled', '是否启用'), <Switch checked={d.enabled} onChange={(c) => set({ enabled: c })} />)}
    </ResizableDrawer>
  )
}

// ---------- Rules ----------

function RulePanel({
  projectId,
  rules,
  robots,
  loading,
  reload,
  t,
}: {
  projectId: string
  rules: NoticeRule[]
  robots: NoticeRobot[]
  loading: boolean
  reload: () => void
  t: TFn
}) {
  const [editing, setEditing] = useState<RuleDraft | null>(null)
  const robotName = (id: string) => robots.find((r) => r.id === id)?.name ?? id

  const remove = (r: NoticeRule) =>
    modal.confirm({
      title: t('msgset.delConfirm', '确认删除该规则?'),
      okType: 'danger',
      okText: t('a.delete', '删除'),
      cancelText: t('common.cancel', '取消'),
      onOk: () =>
        api
          .deleteNoticeRule(r.id, projectId)
          .then(reload)
          .catch((e: Error) => message.error(e.message)),
    })

  const toggle = (r: NoticeRule, enabled: boolean) =>
    api
      .updateNoticeRule(r.id, { projectId, eventType: r.eventType, channels: r.channels, robotIds: r.robotIds, template: r.template, enabled })
      .then(reload)
      .catch((e: Error) => message.error(e.message))

  const cols: ColumnsType<NoticeRule> = [
    {
      title: t('msgset.colEvent', '事件'),
      dataIndex: 'eventType',
      width: 200,
      render: (v: string) => eventLabel(t, v),
    },
    {
      title: t('msgset.colChannels', '通道'),
      dataIndex: 'channels',
      width: 180,
      render: (cs: NoticeChannel[]) =>
        cs.length ? (
          cs.map((c) => (
            <Tag key={c} color="green" style={{ marginBottom: 4 }}>
              {channelLabel(t, c)}
            </Tag>
          ))
        ) : (
          <span style={{ color: 'var(--text-3)' }}>—</span>
        ),
    },
    {
      title: t('msgset.colRobots', '机器人'),
      dataIndex: 'robotIds',
      render: (ids: string[]) =>
        ids.length ? (
          ids.map((id) => (
            <Tag key={id} style={{ marginBottom: 4 }}>
              {robotName(id)}
            </Tag>
          ))
        ) : (
          <span style={{ color: 'var(--text-3)' }}>—</span>
        ),
    },
    {
      title: t('msgset.colTemplate', '模板'),
      dataIndex: 'template',
      ellipsis: true,
      render: (v: string) => (v ? <span className="ms-mono" style={{ fontSize: 12 }}>{v}</span> : <span style={{ color: 'var(--text-3)' }}>—</span>),
    },
    {
      title: t('msgset.colEnabled', '是否启用'),
      dataIndex: 'enabled',
      width: 90,
      render: (v: boolean, r) => <Switch size="small" checked={v} onChange={(c) => toggle(r, c)} />,
    },
    {
      title: t('apidef.colAction', '操作'),
      width: 120,
      render: (_v, r) => (
        <>
          <Button
            type="link"
            size="small"
            onClick={() => setEditing({ id: r.id, eventType: r.eventType, channels: r.channels, robotIds: r.robotIds, template: r.template, enabled: r.enabled })}
          >
            {t('a.edit', '编辑')}
          </Button>
          <Button type="link" size="small" danger onClick={() => remove(r)}>
            {t('a.delete', '删除')}
          </Button>
        </>
      ),
    },
  ]

  return (
    <Card
      size="small"
      title={t('msgset.rules', '通知规则')}
      extra={
        <Button
          type="primary"
          size="small"
          icon={<PlusOutlined />}
          onClick={() => setEditing({ eventType: EVENTS[0], channels: ['IN_APP'], robotIds: [], template: '', enabled: true })}
        >
          {t('msgset.addRule', '添加规则')}
        </Button>
      }
      styles={{ body: { padding: 12 } }}
    >
      <Table<NoticeRule>
        rowKey="id"
        size="middle"
        loading={loading}
        dataSource={rules}
        columns={cols}
        pagination={false}
        locale={{ emptyText: t('msgset.noRules', '暂无规则:所有事件默认发送站内信') }}
      />
      <RuleDrawer
        draft={editing}
        projectId={projectId}
        robots={robots}
        onClose={() => setEditing(null)}
        onSaved={() => { setEditing(null); reload() }}
        t={t}
      />
    </Card>
  )
}

function RuleDrawer({
  draft,
  projectId,
  robots,
  onClose,
  onSaved,
  t,
}: {
  draft: RuleDraft | null
  projectId: string
  robots: NoticeRobot[]
  onClose: () => void
  onSaved: () => void
  t: TFn
}) {
  const [d, setD] = useState<RuleDraft | null>(draft)
  const [saving, setSaving] = useState(false)
  useEffect(() => setD(draft), [draft])
  if (!d) return null
  const set = (patch: Partial<RuleDraft>) => setD({ ...d, ...patch })

  const save = () => {
    if (d.channels.includes('ROBOT') && !d.robotIds.length) {
      message.warning(t('msgset.needRobots', '机器人通道需要至少选择一个机器人'))
      return
    }
    const body = { projectId, eventType: d.eventType, channels: d.channels, robotIds: d.robotIds, template: d.template, enabled: d.enabled }
    setSaving(true)
    ;(d.id ? api.updateNoticeRule(d.id, body) : api.createNoticeRule(body))
      .then(() => {
        message.success(t('msgset.saved', '已保存'))
        onSaved()
      })
      .catch((e: Error) => message.error(e.message))
      .finally(() => setSaving(false))
  }

  return (
    <ResizableDrawer
      open={!!draft}
      onClose={onClose}
      width={520}
      title={t(d.id ? 'msgset.editRule' : 'msgset.addRule', d.id ? '编辑规则' : '添加规则')}
      extra={
        <Button type="primary" loading={saving} onClick={save}>
          {t('msgset.save', '保存')}
        </Button>
      }
    >
      {fieldRow(
        t('msgset.colEvent', '事件'),
        <Select
          style={{ width: '100%' }}
          value={d.eventType}
          onChange={(v) => set({ eventType: v })}
          options={EVENTS.map((e) => ({ label: eventLabel(t, e), value: e }))}
        />,
      )}
      {fieldRow(
        t('msgset.colChannels', '通道'),
        <Select
          mode="multiple"
          style={{ width: '100%' }}
          value={d.channels}
          onChange={(v) => set({ channels: v as NoticeChannel[] })}
          options={CHANNELS.map((c) => ({ label: channelLabel(t, c), value: c }))}
          placeholder={t('msgset.pickChannels', '选择通道,留空则不通知')}
        />,
      )}
      {d.channels.includes('ROBOT') &&
        fieldRow(
          t('msgset.colRobots', '机器人'),
          <Select
            mode="multiple"
            style={{ width: '100%' }}
            value={d.robotIds}
            onChange={(v) => set({ robotIds: v })}
            options={robots.map((r) => ({ label: `${r.name} (${platformLabel(t, r.platform)})`, value: r.id }))}
            placeholder={t('msgset.pickRobots', '选择机器人')}
          />,
        )}
      {fieldRow(
        t('msgset.colTemplate', '模板'),
        <Input.TextArea
          rows={4}
          value={d.template}
          onChange={(e) => set({ template: e.target.value })}
          placeholder={t('msgset.tmplPlaceholder', '可使用 ${title} ${operator} ${time} 变量,留空则用默认模板')}
        />,
      )}
      {fieldRow(t('msgset.colEnabled', '是否启用'), <Switch checked={d.enabled} onChange={(c) => set({ enabled: c })} />)}
    </ResizableDrawer>
  )
}
