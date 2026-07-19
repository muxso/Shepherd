import { useEffect, useMemo, useState } from 'react'
import { Alert, Button, Card, Input, Select, Switch, Table, Tag } from 'antd'
import ResizableDrawer from '../components/ResizableDrawer'
import { PlusOutlined, RobotOutlined } from '@ant-design/icons'
import type { ColumnsType } from 'antd/es/table'
import { api, type User } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'
import { message, modal } from '../feedback'
import { SelectProjectEmpty } from '../components/Page'

// Message settings ("Project → Message Settings"). Left: secondary nav (robots + business event categories);
// right: notification rules for the selected category (event → recipients → channels → template → enabled).
// Persisted per project to localStorage (like App Settings: local state until the backend notification service lands).

type ChannelKey = 'inapp' | 'email' | 'wecom' | 'dingtalk' | 'lark' | 'webhook'
type CatKey = 'plan' | 'bug' | 'case' | 'api' | 'schedule' | 'git'
type NavKey = 'robot' | CatKey

interface Rule {
  id: string
  cat: CatKey
  event: string // event key (see EVENTS)
  recipients: string[]
  channels: ChannelKey[]
  template: string
  enabled: boolean
}

interface Robot {
  id: string
  type: ChannelKey
  name: string
  webhook: string
  secret: string
  enabled: boolean
}

interface Settings {
  robots: Robot[]
  rules: Rule[]
}

const CATS: CatKey[] = ['plan', 'bug', 'case', 'api', 'schedule', 'git']

const CHANNELS: ChannelKey[] = ['inapp', 'email', 'wecom', 'dingtalk', 'lark', 'webhook']

// Built-in notification events per category. value is a stable key; display goes through i18n.
const EVENTS: Record<CatKey, { key: string; zh: string; en: string }[]> = {
  plan: [
    { key: 'planExec', zh: '测试计划执行完成', en: 'Test plan executed' },
    { key: 'planReview', zh: '测试计划评审完成', en: 'Test plan reviewed' },
    { key: 'planStart', zh: '测试计划开始', en: 'Test plan started' },
    { key: 'planEnd', zh: '测试计划归档', en: 'Test plan archived' },
  ],
  bug: [
    { key: 'bugCreate', zh: '创建缺陷', en: 'Bug created' },
    { key: 'bugStatus', zh: '缺陷状态流转', en: 'Bug status changed' },
    { key: 'bugComment', zh: '缺陷评论', en: 'Bug commented' },
  ],
  case: [
    { key: 'caseReviewPass', zh: '用例评审通过', en: 'Case review passed' },
    { key: 'caseReviewFail', zh: '用例评审不通过', en: 'Case review rejected' },
    { key: 'caseComment', zh: '用例评论', en: 'Case commented' },
  ],
  api: [
    { key: 'apiCaseExec', zh: '接口用例执行完成', en: 'API case executed' },
    { key: 'apiScenarioExec', zh: '接口场景执行完成', en: 'API scenario executed' },
    { key: 'apiDefChange', zh: '接口定义变更', en: 'API definition changed' },
  ],
  schedule: [
    { key: 'schedSuccess', zh: '定时任务执行成功', en: 'Scheduled job succeeded' },
    { key: 'schedFail', zh: '定时任务执行失败', en: 'Scheduled job failed' },
  ],
  git: [
    { key: 'gitPush', zh: '代码推送', en: 'Code pushed' },
    { key: 'gitMr', zh: '合并请求', en: 'Merge request' },
  ],
}

// Fixed recipient roles (semantic recipients beyond project members).
const ROLE_RCPTS = ['creator', 'follower', 'operator', 'assignee', 'projectAdmin'] as const

const storageKey = (projectId: string) => `shepherd.msgSettings.${projectId}`

function seed(): Settings {
  const rules: Rule[] = []
  for (const cat of CATS) {
    for (const ev of EVENTS[cat]) {
      rules.push({
        id: `${cat}-${ev.key}`,
        cat,
        event: ev.key,
        recipients: ['creator', 'follower'],
        channels: ['inapp'],
        template: '',
        enabled: cat === 'plan' || cat === 'bug',
      })
    }
  }
  return {
    robots: [
      { id: 'builtin-inapp', type: 'inapp', name: '站内信', webhook: '', secret: '', enabled: true },
      { id: 'builtin-email', type: 'email', name: '邮件', webhook: '', secret: '', enabled: true },
    ],
    rules,
  }
}

function load(projectId: string): Settings {
  try {
    const raw = localStorage.getItem(storageKey(projectId))
    if (!raw) return seed()
    const parsed = JSON.parse(raw) as Settings
    if (!parsed.rules || !parsed.robots) return seed()
    return parsed
  } catch {
    return seed()
  }
}

const newId = () => `r${Date.now().toString(36)}${Math.floor(Math.random() * 1e4).toString(36)}`

export default function MessageSettings() {
  const { t, lang } = useI18n()
  const { projectId, projects } = useApp()
  const [nav, setNav] = useState<NavKey>('plan')
  const [settings, setSettings] = useState<Settings | null>(null)
  const [users, setUsers] = useState<User[]>([])

  useEffect(() => {
    if (projectId) setSettings(load(projectId))
  }, [projectId])

  useEffect(() => {
    api.users().then((p) => setUsers(p.items ?? [])).catch(() => setUsers([]))
  }, [])

  // Persist per project on every change (auto-save).
  const persist = (next: Settings) => {
    setSettings(next)
    if (projectId) localStorage.setItem(storageKey(projectId), JSON.stringify(next))
  }

  if (!projectId || !projects.find((p) => p.id === projectId)) return <SelectProjectEmpty />
  if (!settings) return null

  const groups: { title: string; items: { key: NavKey; label: string }[] }[] = [
    {
      title: t('msgset.grpRobot', '机器人'),
      items: [{ key: 'robot', label: t('msgset.robot', '机器人') }],
    },
    {
      title: t('msgset.grpEvent', '消息设置'),
      items: CATS.map((c) => ({ key: c, label: t(`msg.cat.${c}`, EVENTS[c][0]?.zh ?? c) })),
    },
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
        {groups.map((g) => (
          <div key={g.title} style={{ marginBottom: 8 }}>
            <div style={{ fontSize: 12, color: 'var(--text-3)', padding: '6px 10px 2px' }}>{g.title}</div>
            {g.items.map((it) => (
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
        ))}
      </div>
      {/* Right panel */}
      <div style={{ flex: 1, minWidth: 0, overflow: 'auto', padding: 16, background: 'var(--bg)' }}>
        {/* In-app delivery works server-side already; these rules stay local until the rule engine moves server-side. */}
        <Alert
          type="info"
          showIcon
          style={{ marginBottom: 12 }}
          message={t('msgset.liveBanner', '站内信已生效;接收规则与 webhook 通道后续接入服务端')}
        />
        {nav === 'robot' ? (
          <RobotPanel settings={settings} persist={persist} t={t} lang={lang} />
        ) : (
          <RulePanel cat={nav} settings={settings} persist={persist} users={users} t={t} lang={lang} />
        )}
      </div>
    </div>
  )
}

type TFn = (k: string, d?: string) => string
type Lang = 'zh' | 'en'

function channelLabel(t: TFn, c: ChannelKey): string {
  const def: Record<ChannelKey, string> = {
    inapp: '站内信',
    email: '邮件',
    wecom: '企业微信',
    dingtalk: '钉钉',
    lark: '飞书',
    webhook: '自定义 Webhook',
  }
  return t(`msgset.ch.${c}`, def[c])
}

function roleLabel(t: TFn, r: string): string {
  const def: Record<string, string> = {
    creator: '创建人',
    follower: '关注人',
    operator: '操作人',
    assignee: '处理人',
    projectAdmin: '项目管理员',
  }
  return t(`msgset.rcpt.${r}`, def[r] ?? r)
}

function eventLabel(cat: CatKey, key: string, lang: Lang): string {
  const ev = EVENTS[cat].find((e) => e.key === key)
  if (!ev) return key
  return lang === 'en' ? ev.en : ev.zh
}

// ---------- Notification rules panel ----------

function RulePanel({
  cat,
  settings,
  persist,
  users,
  t,
  lang,
}: {
  cat: CatKey
  settings: Settings
  persist: (s: Settings) => void
  users: User[]
  t: TFn
  lang: Lang
}) {
  const [editing, setEditing] = useState<Rule | null>(null)
  const rows = settings.rules.filter((r) => r.cat === cat)

  const rcptOptions = useMemo(
    () => [
      ...ROLE_RCPTS.map((r) => ({ label: roleLabel(t, r), value: r })),
      ...users.map((u) => ({ label: u.name, value: `u:${u.name}` })),
    ],
    [users, t],
  )
  const rcptLabel = (v: string) => (v.startsWith('u:') ? v.slice(2) : roleLabel(t, v))

  const channelOptions = CHANNELS.map((c) => ({ label: channelLabel(t, c), value: c }))

  const upsert = (rule: Rule) => {
    const exists = settings.rules.some((r) => r.id === rule.id)
    persist({
      ...settings,
      rules: exists ? settings.rules.map((r) => (r.id === rule.id ? rule : r)) : [...settings.rules, rule],
    })
    setEditing(null)
  }
  const toggle = (id: string, enabled: boolean) =>
    persist({ ...settings, rules: settings.rules.map((r) => (r.id === id ? { ...r, enabled } : r)) })
  const remove = (id: string) =>
    modal.confirm({
      title: t('msgset.delConfirm', '确认删除该通知?'),
      okType: 'danger',
      okText: t('a.delete', '删除'),
      cancelText: t('common.cancel', '取消'),
      onOk: () => persist({ ...settings, rules: settings.rules.filter((r) => r.id !== id) }),
    })

  const cols: ColumnsType<Rule> = [
    {
      title: t('msgset.colEvent', '通知场景'),
      dataIndex: 'event',
      width: 200,
      render: (v: string) => eventLabel(cat, v, lang),
    },
    {
      title: t('msgset.colRecipients', '接收人'),
      dataIndex: 'recipients',
      render: (rs: string[]) =>
        rs.length ? (
          rs.map((r) => (
            <Tag key={r} style={{ marginBottom: 4 }}>
              {rcptLabel(r)}
            </Tag>
          ))
        ) : (
          <span style={{ color: 'var(--text-3)' }}>—</span>
        ),
    },
    {
      title: t('msgset.colChannels', '接收方式'),
      dataIndex: 'channels',
      width: 240,
      render: (cs: ChannelKey[]) =>
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
      title: t('msgset.colEnabled', '是否启用'),
      dataIndex: 'enabled',
      width: 90,
      render: (v: boolean, r) => <Switch size="small" checked={v} onChange={(c) => toggle(r.id, c)} />,
    },
    {
      title: t('apidef.colAction', '操作'),
      width: 120,
      render: (_v, r) => (
        <>
          <Button type="link" size="small" onClick={() => setEditing(r)}>
            {t('a.edit', '编辑')}
          </Button>
          <Button type="link" size="small" danger onClick={() => remove(r.id)}>
            {t('a.delete', '删除')}
          </Button>
        </>
      ),
    },
  ]

  const addRule = () =>
    setEditing({
      id: newId(),
      cat,
      event: EVENTS[cat][0].key,
      recipients: ['creator'],
      channels: ['inapp'],
      template: '',
      enabled: true,
    })

  return (
    <Card
      size="small"
      title={t(`msg.cat.${cat}`, EVENTS[cat][0]?.zh ?? cat)}
      extra={
        <Button type="primary" size="small" icon={<PlusOutlined />} onClick={addRule}>
          {t('msgset.addRule', '添加通知')}
        </Button>
      }
      styles={{ body: { padding: 12 } }}
    >
      <Table<Rule> rowKey="id" size="middle" dataSource={rows} columns={cols} pagination={false} />
      <RuleDrawer
        rule={editing}
        cat={cat}
        rcptOptions={rcptOptions}
        channelOptions={channelOptions}
        onClose={() => setEditing(null)}
        onSave={upsert}
        t={t}
        lang={lang}
      />
    </Card>
  )
}

function RuleDrawer({
  rule,
  cat,
  rcptOptions,
  channelOptions,
  onClose,
  onSave,
  t,
  lang,
}: {
  rule: Rule | null
  cat: CatKey
  rcptOptions: { label: string; value: string }[]
  channelOptions: { label: string; value: ChannelKey }[]
  onClose: () => void
  onSave: (r: Rule) => void
  t: TFn
  lang: Lang
}) {
  const [draft, setDraft] = useState<Rule | null>(rule)
  useEffect(() => setDraft(rule), [rule])
  if (!draft) return null
  const set = (patch: Partial<Rule>) => setDraft({ ...draft, ...patch })
  const field = (label: string, control: React.ReactNode) => (
    <div style={{ marginBottom: 16 }}>
      <div style={{ fontSize: 13, color: 'var(--text-2)', marginBottom: 6 }}>{label}</div>
      {control}
    </div>
  )
  return (
    <ResizableDrawer
      open={!!rule}
      onClose={onClose}
      width={520}
      title={t('msgset.editRule', '编辑通知')}
      extra={
        <Button
          type="primary"
          onClick={() => {
            if (!draft.channels.length) {
              message.warning(t('msgset.needChannel', '请至少选择一种接收方式'))
              return
            }
            onSave(draft)
            message.success(t('msgset.saved', '已保存'))
          }}
        >
          {t('msgset.save', '保存')}
        </Button>
      }
    >
      {field(
        t('msgset.colEvent', '通知场景'),
        <Select
          style={{ width: '100%' }}
          value={draft.event}
          onChange={(v) => set({ event: v })}
          options={EVENTS[cat].map((e) => ({ label: lang === 'en' ? e.en : e.zh, value: e.key }))}
        />,
      )}
      {field(
        t('msgset.colRecipients', '接收人'),
        <Select
          mode="multiple"
          style={{ width: '100%' }}
          value={draft.recipients}
          onChange={(v) => set({ recipients: v })}
          options={rcptOptions}
          placeholder={t('msgset.pickRecipients', '选择接收人')}
        />,
      )}
      {field(
        t('msgset.colChannels', '接收方式'),
        <Select
          mode="multiple"
          style={{ width: '100%' }}
          value={draft.channels}
          onChange={(v) => set({ channels: v as ChannelKey[] })}
          options={channelOptions}
          placeholder={t('msgset.pickChannels', '选择接收方式')}
        />,
      )}
      {field(
        t('msgset.colTemplate', '模板'),
        <Input.TextArea
          rows={4}
          value={draft.template}
          onChange={(e) => set({ template: e.target.value })}
          placeholder={t('msgset.tmplPlaceholder', '可使用 ${title} ${operator} ${time} 等变量,留空则用默认模板')}
        />,
      )}
      {field(
        t('msgset.colEnabled', '是否启用'),
        <Switch checked={draft.enabled} onChange={(c) => set({ enabled: c })} />,
      )}
    </ResizableDrawer>
  )
}

// ---------- Robot panel ----------

function RobotPanel({
  settings,
  persist,
  t,
  lang,
}: {
  settings: Settings
  persist: (s: Settings) => void
  t: TFn
  lang: Lang
}) {
  const [editing, setEditing] = useState<Robot | null>(null)
  void lang

  const upsert = (robot: Robot) => {
    const exists = settings.robots.some((r) => r.id === robot.id)
    persist({
      ...settings,
      robots: exists ? settings.robots.map((r) => (r.id === robot.id ? robot : r)) : [...settings.robots, robot],
    })
    setEditing(null)
  }
  const toggle = (id: string, enabled: boolean) =>
    persist({ ...settings, robots: settings.robots.map((r) => (r.id === id ? { ...r, enabled } : r)) })
  const remove = (id: string) =>
    modal.confirm({
      title: t('msgset.delRobotConfirm', '确认删除该机器人?'),
      okType: 'danger',
      okText: t('a.delete', '删除'),
      cancelText: t('common.cancel', '取消'),
      onOk: () => persist({ ...settings, robots: settings.robots.filter((r) => r.id !== id) }),
    })

  const cols: ColumnsType<Robot> = [
    {
      title: t('msgset.robotName', '名称'),
      dataIndex: 'name',
      render: (v: string) => (
        <span>
          <RobotOutlined style={{ color: 'var(--brand)', marginRight: 6 }} />
          {v}
        </span>
      ),
    },
    {
      title: t('msgset.robotType', '类型'),
      dataIndex: 'type',
      width: 140,
      render: (v: ChannelKey) => <Tag color="green">{channelLabel(t, v)}</Tag>,
    },
    {
      title: t('msgset.robotWebhook', 'Webhook 地址'),
      dataIndex: 'webhook',
      render: (v: string) =>
        v ? (
          <span className="ms-mono" style={{ fontSize: 12 }}>
            {v}
          </span>
        ) : (
          <span style={{ color: 'var(--text-3)' }}>—</span>
        ),
    },
    {
      title: t('msgset.colEnabled', '是否启用'),
      dataIndex: 'enabled',
      width: 90,
      render: (v: boolean, r) => <Switch size="small" checked={v} onChange={(c) => toggle(r.id, c)} />,
    },
    {
      title: t('apidef.colAction', '操作'),
      width: 120,
      render: (_v, r) => (
        <>
          <Button type="link" size="small" onClick={() => setEditing(r)}>
            {t('a.edit', '编辑')}
          </Button>
          <Button type="link" size="small" danger onClick={() => remove(r.id)}>
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
          onClick={() => setEditing({ id: newId(), type: 'wecom', name: '', webhook: '', secret: '', enabled: true })}
        >
          {t('msgset.addRobot', '添加机器人')}
        </Button>
      }
      styles={{ body: { padding: 12 } }}
    >
      <Table<Robot> rowKey="id" size="middle" dataSource={settings.robots} columns={cols} pagination={false} />
      <RobotDrawer robot={editing} onClose={() => setEditing(null)} onSave={upsert} t={t} />
    </Card>
  )
}

function RobotDrawer({
  robot,
  onClose,
  onSave,
  t,
}: {
  robot: Robot | null
  onClose: () => void
  onSave: (r: Robot) => void
  t: TFn
}) {
  const [draft, setDraft] = useState<Robot | null>(robot)
  useEffect(() => setDraft(robot), [robot])
  if (!draft) return null
  const set = (patch: Partial<Robot>) => setDraft({ ...draft, ...patch })
  // In-app and email are built-in channels (no webhook); all other robots require one.
  const needsWebhook = draft.type !== 'inapp' && draft.type !== 'email'
  const field = (label: string, control: React.ReactNode) => (
    <div style={{ marginBottom: 16 }}>
      <div style={{ fontSize: 13, color: 'var(--text-2)', marginBottom: 6 }}>{label}</div>
      {control}
    </div>
  )
  return (
    <ResizableDrawer
      open={!!robot}
      onClose={onClose}
      width={520}
      title={t('msgset.editRobot', '编辑机器人')}
      extra={
        <Button
          type="primary"
          onClick={() => {
            if (!draft.name.trim()) {
              message.warning(t('msgset.needName', '请填写机器人名称'))
              return
            }
            if (needsWebhook && !draft.webhook.trim()) {
              message.warning(t('msgset.needWebhook', '请填写 Webhook 地址'))
              return
            }
            onSave(draft)
            message.success(t('msgset.saved', '已保存'))
          }}
        >
          {t('msgset.save', '保存')}
        </Button>
      }
    >
      {field(
        t('msgset.robotName', '名称'),
        <Input value={draft.name} onChange={(e) => set({ name: e.target.value })} />,
      )}
      {field(
        t('msgset.robotType', '类型'),
        <Select
          style={{ width: '100%' }}
          value={draft.type}
          onChange={(v) => set({ type: v as ChannelKey })}
          options={CHANNELS.map((c) => ({ label: channelLabel(t, c), value: c }))}
        />,
      )}
      {needsWebhook &&
        field(
          t('msgset.robotWebhook', 'Webhook 地址'),
          <Input
            value={draft.webhook}
            onChange={(e) => set({ webhook: e.target.value })}
            placeholder="https://..."
          />,
        )}
      {needsWebhook &&
        field(
          t('msgset.robotSecret', '签名密钥'),
          <Input.Password
            value={draft.secret}
            onChange={(e) => set({ secret: e.target.value })}
            placeholder={t('msgset.optional', '可选')}
          />,
        )}
      {field(t('msgset.colEnabled', '是否启用'), <Switch checked={draft.enabled} onChange={(c) => set({ enabled: c })} />)}
    </ResizableDrawer>
  )
}
