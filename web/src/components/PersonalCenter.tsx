import { useEffect, useMemo, useState, type ReactNode } from 'react'
import {
  Alert,
  Button,
  Descriptions,
  Drawer,
  Empty,
  Form,
  Input,
  InputNumber,
  Modal,
  Popconfirm,
  Radio,
  Switch,
  Table,
  Tooltip,
  Typography,
} from 'antd'
import { EyeInvisibleOutlined, PlusOutlined, QuestionCircleOutlined } from '@ant-design/icons'
import { api, ApiError, userIdStore, userStore, type ApiKey, type AuthMe, type LlmModel } from '../api'
import { message } from '../feedback'
import { useI18n } from '../i18n'

// 个人中心(对齐 MeterSphere):全屏抽屉,左侧分组导航(个人信息 / 个人设置),
// 右侧内容随导航切换。数据契约见 api.ts 的「个人中心」段;后端并行开发中,
// 接口未就绪时各面板降级为空态/本地回退,不阻塞打开。
type TabKey = 'basic' | 'password' | 'apikey' | 'models'

export default function PersonalCenter({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { t } = useI18n()
  const [tab, setTab] = useState<TabKey>('basic')

  const groups: { title: string; items: { key: TabKey; label: string }[] }[] = [
    {
      title: t('pc.groupInfo', '个人信息'),
      items: [
        { key: 'basic', label: t('pc.basic', '基本信息') },
        { key: 'password', label: t('pc.password', '密码设置') },
      ],
    },
    {
      title: t('pc.groupSettings', '个人设置'),
      items: [
        { key: 'apikey', label: 'API KEY' },
        { key: 'models', label: t('pc.models', '模型设置') },
      ],
    },
  ]

  return (
    <Drawer
      open={open}
      onClose={onClose}
      title={t('pc.title', '个人中心')}
      width="min(1100px, calc(100vw - 72px))"
      destroyOnHidden
      styles={{ body: { padding: 0, display: 'flex', overflow: 'hidden' } }}
    >
      {/* 左:窄栏分组导航,选中项品牌色高亮 */}
      <div style={{ width: 208, flexShrink: 0, borderRight: '1px solid var(--border-soft)', overflowY: 'auto', padding: '12px 8px' }}>
        {groups.map((g, gi) => (
          <div key={g.title}>
            {gi > 0 && <div style={{ height: 1, background: 'var(--border-soft)', margin: '8px 4px' }} />}
            <div style={{ padding: '8px 12px 4px', fontSize: 12, color: 'var(--text-3)' }}>{g.title}</div>
            {g.items.map((it) => {
              const active = it.key === tab
              return (
                <div
                  key={it.key}
                  onClick={() => setTab(it.key)}
                  style={{
                    padding: '9px 12px',
                    marginBottom: 2,
                    borderRadius: 6,
                    cursor: 'pointer',
                    fontSize: 14,
                    color: active ? 'var(--brand)' : 'var(--text)',
                    background: active ? 'var(--brand-soft)' : 'transparent',
                  }}
                >
                  {it.label}
                </div>
              )
            })}
          </div>
        ))}
      </div>
      {/* 右:内容区 */}
      <div style={{ flex: 1, minWidth: 0, overflowY: 'auto', padding: 24 }}>
        {tab === 'basic' && <BasicPanel />}
        {tab === 'password' && <PasswordPanel />}
        {tab === 'apikey' && <ApiKeyPanel />}
        {tab === 'models' && <ModelPanel />}
      </div>
    </Drawer>
  )
}

// YYYY-MM-DD HH:mm:ss(对齐参考样式的时间口径)。
function fmtTime(v?: string | null): string {
  if (!v) return '-'
  const d = new Date(v)
  if (Number.isNaN(d.getTime())) return v
  const p = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`
}

// 面板标题:名称 + 可选的问号提示。
function PanelTitle({ text, tip }: { text: string; tip?: string }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 16, fontWeight: 600, marginBottom: 16 }}>
      {text}
      {tip && (
        <Tooltip title={tip}>
          <QuestionCircleOutlined style={{ color: 'var(--text-3)', fontSize: 14 }} />
        </Tooltip>
      )}
    </div>
  )
}

// ---------- 基本信息 ----------

function BasicPanel() {
  const { t } = useI18n()
  const [me, setMe] = useState<AuthMe | null>(null)
  useEffect(() => {
    api.me().then(setMe).catch(() => setMe(null)) // 接口未就绪 → 回退本地 store
  }, [])
  return (
    <div style={{ maxWidth: 560 }}>
      <PanelTitle text={t('pc.basic', '基本信息')} />
      <Descriptions column={1} size="small" bordered>
        <Descriptions.Item label={t('pc.username', '用户名')}>{userStore.get()}</Descriptions.Item>
        <Descriptions.Item label={t('pc.userId', '用户 ID')}>
          <span className="ms-mono">{me?.userId || userIdStore.get()}</span>
        </Descriptions.Item>
        <Descriptions.Item label={t('pc.permCount', '权限条数')}>{me ? me.permissions.length : '-'}</Descriptions.Item>
      </Descriptions>
    </div>
  )
}

// ---------- 密码设置 ----------

function PasswordPanel() {
  const { t } = useI18n()
  const [form] = Form.useForm<{ oldPassword: string; newPassword: string; confirm: string }>()
  const [busy, setBusy] = useState(false)

  const submit = async () => {
    const v = await form.validateFields().catch(() => null)
    if (!v) return
    setBusy(true)
    try {
      await api.changePassword({ oldPassword: v.oldPassword, newPassword: v.newPassword })
      message.success(t('pc.pwdOk', '密码已修改'))
      form.resetFields()
    } catch (e) {
      if (e instanceof ApiError && e.status === 401) message.error(t('pc.pwdWrong', '当前密码不正确'))
      else if (e instanceof ApiError && e.status === 409) message.error(t('pc.pwdManaged', '该账号密码由部署环境管理,不可在此修改'))
      else message.error(e instanceof ApiError ? e.message : t('pc.pwdFailed', '修改失败'))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div style={{ maxWidth: 400 }}>
      <PanelTitle text={t('pc.password', '密码设置')} />
      <Form form={form} layout="vertical" requiredMark={false}>
        <Form.Item
          name="oldPassword"
          label={t('pc.pwdOld', '当前密码')}
          rules={[{ required: true, message: t('pc.pwdOldRequired', '请输入当前密码') }]}
        >
          <Input.Password autoComplete="current-password" />
        </Form.Item>
        <Form.Item
          name="newPassword"
          label={t('pc.pwdNew', '新密码')}
          rules={[
            { required: true, message: t('pc.pwdNewRequired', '请输入新密码') },
            { min: 8, message: t('pc.pwdMin', '新密码至少 8 位') },
          ]}
        >
          <Input.Password autoComplete="new-password" />
        </Form.Item>
        <Form.Item
          name="confirm"
          label={t('pc.pwdConfirm', '确认新密码')}
          dependencies={['newPassword']}
          rules={[
            { required: true, message: t('pc.pwdNewRequired', '请输入新密码') },
            ({ getFieldValue }) => ({
              validator: (_r, v) =>
                !v || v === getFieldValue('newPassword')
                  ? Promise.resolve()
                  : Promise.reject(new Error(t('pc.pwdMismatch', '两次输入的密码不一致'))),
            }),
          ]}
        >
          <Input.Password autoComplete="new-password" />
        </Form.Item>
        <Button type="primary" loading={busy} onClick={submit}>
          {t('pc.pwdSubmit', '修改密码')}
        </Button>
      </Form>
    </div>
  )
}

// ---------- API KEY ----------

function ApiKeyPanel() {
  const { t } = useI18n()
  const [items, setItems] = useState<ApiKey[]>([])
  const [loading, setLoading] = useState(false)
  const [createOpen, setCreateOpen] = useState(false)
  // 创建成功待展示的一次性明文 key(后端不再返回明文,只此一次)。
  const [createdKey, setCreatedKey] = useState<string | null>(null)

  const load = () => {
    setLoading(true)
    api.myApiKeys()
      .then((p) => setItems(p.items ?? []))
      .catch(() => setItems([])) // 接口未就绪 → 空态
      .finally(() => setLoading(false))
  }
  useEffect(load, [])

  const remove = async (k: ApiKey) => {
    try {
      await api.revokeApiKey(k.id)
      message.success(t('pc.akDeleted', '已删除'))
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('pc.opFailed', '操作失败'))
    }
  }

  const toggle = async (k: ApiKey, enabled: boolean) => {
    try {
      await api.setApiKeyEnabled(k.id, enabled)
      message.success(enabled ? t('pc.akEnabled', '已启用') : t('pc.akDisabled', '已停用'))
      setItems((prev) => prev.map((x) => (x.id === k.id ? { ...x, revoked: !enabled } : x)))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('pc.opFailed', '操作失败'))
    }
  }

  return (
    <div>
      <PanelTitle text="API KEY" tip={t('pc.akTip', '用于以你的身份调用开放接口')} />
      <Button icon={<PlusOutlined />} onClick={() => setCreateOpen(true)} style={{ marginBottom: 16 }}>
        {t('pc.akAdd', '新增')}
      </Button>
      {!loading && items.length === 0 ? (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('pc.akEmpty', '暂无 API KEY')} style={{ marginTop: 48 }} />
      ) : (
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 16 }}>
          {items.map((k) => (
            <KeyCard key={k.id} k={k} onDelete={() => remove(k)} onToggle={(en) => toggle(k, en)} />
          ))}
        </div>
      )}
      <CreateKeyModal
        open={createOpen}
        onClose={() => setCreateOpen(false)}
        onDone={(key) => { setCreateOpen(false); setCreatedKey(key); load() }}
      />
      <KeyOnceModal keyText={createdKey} onClose={() => setCreatedKey(null)} />
    </div>
  )
}

function KeyCard({ k, onDelete, onToggle }: { k: ApiKey; onDelete: () => void; onToggle: (enabled: boolean) => void }) {
  const { t } = useI18n()
  const expired = !!k.expiresAt && new Date(k.expiresAt).getTime() < Date.now()
  const accessKey = k.id.startsWith('sak_') ? k.id : `sak_${k.id}`
  const row = (label: string, value: ReactNode) => (
    <div style={{ display: 'flex', gap: 12, padding: '5px 0', fontSize: 13 }}>
      <span style={{ width: 88, flexShrink: 0, color: 'var(--text-3)' }}>{label}</span>
      <span style={{ minWidth: 0, wordBreak: 'break-all', color: 'var(--text)' }}>{value}</span>
    </div>
  )
  return (
    <div
      style={{
        width: 450,
        maxWidth: '100%',
        border: '1px solid var(--border)',
        borderRadius: 8,
        background: 'var(--panel)',
        overflow: 'hidden',
      }}
    >
      <div style={{ padding: '12px 16px' }}>
        {row(t('pc.akAccessKey', 'Access Key'), <span className="ms-mono">{accessKey}</span>)}
        {row(
          t('pc.akSecretKey', 'Secret Key'),
          <span style={{ display: 'inline-flex', alignItems: 'center', gap: 8 }}>
            <span className="ms-mono">****************</span>
            {/* 只存哈希,无法回显:眼睛不做切换,点它只给说明 */}
            <Tooltip title={t('pc.akSecretHint', '仅创建时可见')}>
              <EyeInvisibleOutlined style={{ color: 'var(--text-3)', cursor: 'pointer' }} />
            </Tooltip>
          </span>,
        )}
        {row(t('pc.akDesc', '描述'), k.name || '-')}
        {row(t('pc.akCreatedAt', '创建时间'), fmtTime(k.createdAt))}
        {row(
          t('pc.akTtl', '有效时间'),
          k.expiresAt
            ? expired
              ? <span style={{ color: 'var(--error)' }}>{t('pc.akExpired', '已过期')}</span>
              : fmtTime(k.expiresAt)
            : t('pc.akForever', '永久有效'),
        )}
      </div>
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          padding: '8px 16px',
          borderTop: '1px solid var(--border-soft)',
          background: 'var(--panel-2)',
        }}
      >
        <Popconfirm
          title={t('pc.akDeleteConfirm', '删除该 API KEY?使用它的调用方将立即失效。')}
          okText={t('pc.akDelete', '删除')}
          okButtonProps={{ danger: true }}
          onConfirm={onDelete}
        >
          <Button size="small" danger>{t('pc.akDelete', '删除')}</Button>
        </Popconfirm>
        <Switch size="small" checked={!k.revoked} onChange={onToggle} />
      </div>
    </div>
  )
}

function CreateKeyModal({ open, onClose, onDone }: { open: boolean; onClose: () => void; onDone: (key: string) => void }) {
  const { t } = useI18n()
  const [form] = Form.useForm<{ name?: string; ttl: 'forever' | 'custom'; days?: number }>()
  const [busy, setBusy] = useState(false)
  useEffect(() => {
    if (open) form.setFieldsValue({ name: '', ttl: 'forever', days: 30 })
  }, [open, form])

  const submit = async () => {
    const v = await form.validateFields().catch(() => null)
    if (!v) return
    setBusy(true)
    try {
      const r = await api.createMyApiKey({
        name: v.name?.trim() || undefined,
        ttlSecs: v.ttl === 'custom' && v.days ? v.days * 86400 : undefined,
      })
      form.resetFields()
      onDone(r.key ?? '')
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('pc.opFailed', '操作失败'))
    } finally {
      setBusy(false)
    }
  }

  return (
    <Modal open={open} onCancel={onClose} onOk={submit} confirmLoading={busy} title={t('pc.akAdd', '新增')} destroyOnHidden>
      <Form form={form} layout="vertical" requiredMark={false}>
        <Form.Item name="name" label={t('pc.akName', '名称')}>
          <Input placeholder={t('pc.akNamePh', '可选')} autoFocus maxLength={64} />
        </Form.Item>
        <Form.Item name="ttl" label={t('pc.akTtl', '有效时间')}>
          <Radio.Group
            options={[
              { value: 'forever', label: t('pc.akForever', '永久有效') },
              { value: 'custom', label: t('pc.akCustom', '自定义') },
            ]}
          />
        </Form.Item>
        <Form.Item noStyle shouldUpdate={(a, b) => a.ttl !== b.ttl}>
          {({ getFieldValue }) =>
            getFieldValue('ttl') === 'custom' && (
              <Form.Item name="days" label={null} rules={[{ required: true, message: t('pc.akDaysRequired', '请输入有效天数') }]}>
                <InputNumber min={1} max={3650} precision={0} suffix={t('pc.akDays', '天')} style={{ width: 160 }} />
              </Form.Item>
            )
          }
        </Form.Item>
      </Form>
    </Modal>
  )
}

// 一次性明文 key:遮罩/ESC 关不掉,只能点「我已保存」,防手滑丢 key。
function KeyOnceModal({ keyText, onClose }: { keyText: string | null; onClose: () => void }) {
  const { t } = useI18n()
  return (
    <Modal
      open={keyText !== null}
      title={t('pc.akCreatedTitle', 'API KEY 已创建')}
      closable={false}
      maskClosable={false}
      keyboard={false}
      okText={t('pc.akISaved', '我已保存')}
      cancelButtonProps={{ style: { display: 'none' } }}
      onOk={onClose}
      onCancel={onClose}
      destroyOnHidden
    >
      <Alert type="warning" showIcon message={t('pc.akKeyOnce', '密钥只显示这一次,请复制保存')} style={{ marginBottom: 12 }} />
      <Typography.Paragraph
        className="ms-mono"
        copyable={{ text: keyText ?? '', onCopy: () => message.success(t('pc.akCopied', '已复制')) }}
        style={{
          background: 'var(--panel)',
          border: '1px solid var(--border-soft)',
          borderRadius: 6,
          padding: '10px 12px',
          wordBreak: 'break-all',
          marginBottom: 0,
        }}
      >
        {keyText}
      </Typography.Paragraph>
    </Modal>
  )
}

// ---------- 模型设置 ----------

// 供应商固定四档(对齐参考样式);图标用首字母圆形占位,不引外部图片。
const PROVIDERS: { key: string; label: [string, string]; letter: string }[] = [
  { key: 'deepseek', label: ['', 'DeepSeek'], letter: 'D' },
  { key: 'openai', label: ['', 'OpenAI'], letter: 'O' },
  { key: 'zhipu', label: ['pc.providerZhipu', '智谱 AI'], letter: 'Z' },
  { key: 'custom', label: ['pc.providerCustom', '自定义'], letter: 'C' },
]

function ModelPanel() {
  const { t } = useI18n()
  const [items, setItems] = useState<LlmModel[]>([])
  const [loading, setLoading] = useState(false)
  const [provider, setProvider] = useState('deepseek')
  const [search, setSearch] = useState('')
  const [editorOpen, setEditorOpen] = useState(false)
  const [editing, setEditing] = useState<LlmModel | null>(null)

  const load = () => {
    setLoading(true)
    api.llmModels()
      .then((p) => setItems(p.items ?? []))
      .catch(() => setItems([])) // 接口未就绪 → 空态
      .finally(() => setLoading(false))
  }
  useEffect(load, [])

  const rows = useMemo(
    () => items.filter((m) => m.provider === provider && (!search || m.name.toLowerCase().includes(search.toLowerCase()))),
    [items, provider, search],
  )

  const remove = async (m: LlmModel) => {
    try {
      await api.deleteLlmModel(m.id)
      message.success(t('pc.deleted', '已删除'))
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('pc.opFailed', '操作失败'))
    }
  }

  const toggle = async (m: LlmModel, enabled: boolean) => {
    try {
      const r = await api.updateLlmModel(m.id, { enabled })
      setItems((prev) => prev.map((x) => (x.id === m.id ? { ...x, ...r } : x)))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('pc.opFailed', '操作失败'))
    }
  }

  const providerLabel = (p: { label: [string, string] }) => (p.label[0] ? t(p.label[0], p.label[1]) : p.label[1])

  return (
    <div>
      <PanelTitle text={t('pc.models', '模型设置')} />
      <div style={{ display: 'flex', gap: 16, alignItems: 'flex-start' }}>
        {/* 左:供应商卡片列表 */}
        <div style={{ width: 216, flexShrink: 0, display: 'flex', flexDirection: 'column', gap: 8 }}>
          <div style={{ fontSize: 12, color: 'var(--text-3)', padding: '0 2px' }}>{t('pc.provider', '供应商')}</div>
          {PROVIDERS.map((p) => {
            const active = p.key === provider
            const count = items.filter((m) => m.provider === p.key).length
            return (
              <div
                key={p.key}
                onClick={() => setProvider(p.key)}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 10,
                  padding: '10px 12px',
                  border: `1px solid ${active ? 'var(--brand)' : 'var(--border)'}`,
                  borderRadius: 8,
                  background: active ? 'var(--brand-soft)' : 'var(--panel)',
                  cursor: 'pointer',
                }}
              >
                <span
                  style={{
                    width: 26,
                    height: 26,
                    borderRadius: '50%',
                    background: active ? 'var(--brand-soft)' : 'var(--panel-2)',
                    color: active ? 'var(--brand)' : 'var(--text-2)',
                    display: 'inline-flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    fontSize: 13,
                    fontWeight: 600,
                    flexShrink: 0,
                  }}
                >
                  {p.letter}
                </span>
                <span style={{ flex: 1, fontSize: 14, color: active ? 'var(--brand)' : 'var(--text)' }}>{providerLabel(p)}</span>
                <span style={{ fontSize: 12, color: 'var(--text-3)' }}>{count}</span>
              </div>
            )
          })}
        </div>
        {/* 右:工具条 + 模型表格 */}
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8, marginBottom: 12 }}>
            <Button type="primary" icon={<PlusOutlined />} onClick={() => { setEditing(null); setEditorOpen(true) }}>
              {t('pc.addModel', '添加模型')}
            </Button>
            <Input.Search
              allowClear
              placeholder={t('pc.searchModel', '通过模型名称搜索')}
              style={{ width: 240 }}
              onSearch={setSearch}
              onChange={(e) => { if (!e.target.value) setSearch('') }}
            />
          </div>
          <Table<LlmModel>
            rowKey="id"
            size="middle"
            loading={loading}
            dataSource={rows}
            pagination={false}
            locale={{ emptyText: <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('pc.modelEmpty', '暂无模型数据')} /> }}
            columns={[
              { title: t('pc.modelName', '名称'), dataIndex: 'name', ellipsis: true },
              {
                title: t('pc.baseUrl', 'Base URL'),
                dataIndex: 'baseUrl',
                ellipsis: true,
                render: (v?: string) => (v ? <span className="ms-mono">{v}</span> : <span style={{ color: 'var(--text-3)' }}>-</span>),
              },
              {
                title: t('pc.apiKeyCol', 'API Key'),
                dataIndex: 'apiKeyMasked',
                width: 160,
                render: (v?: string) => (v ? <span className="ms-mono">{v}</span> : <span style={{ color: 'var(--text-3)' }}>-</span>),
              },
              {
                title: t('pc.enabled', '启用'),
                dataIndex: 'enabled',
                width: 80,
                render: (v: boolean, m) => <Switch size="small" checked={v} onChange={(en) => toggle(m, en)} />,
              },
              {
                title: t('pc.actions', '操作'),
                width: 130,
                render: (_v, m) => (
                  <>
                    <Button type="link" size="small" onClick={() => { setEditing(m); setEditorOpen(true) }}>
                      {t('pc.edit', '编辑')}
                    </Button>
                    <Popconfirm
                      title={t('pc.deleteModelConfirm', '删除该模型?')}
                      okText={t('pc.delete', '删除')}
                      okButtonProps={{ danger: true }}
                      onConfirm={() => remove(m)}
                    >
                      <Button type="link" size="small" danger>{t('pc.delete', '删除')}</Button>
                    </Popconfirm>
                  </>
                ),
              },
            ]}
          />
        </div>
      </div>
      <ModelEditorModal
        open={editorOpen}
        provider={provider}
        editing={editing}
        onClose={() => setEditorOpen(false)}
        onDone={() => { setEditorOpen(false); load() }}
      />
    </div>
  )
}

function ModelEditorModal({
  open,
  provider,
  editing,
  onClose,
  onDone,
}: {
  open: boolean
  provider: string
  editing: LlmModel | null
  onClose: () => void
  onDone: () => void
}) {
  const { t } = useI18n()
  const [form] = Form.useForm<{ name: string; baseUrl?: string; apiKey?: string; enabled: boolean }>()
  const [busy, setBusy] = useState(false)
  useEffect(() => {
    if (open) {
      form.setFieldsValue({
        name: editing?.name ?? '',
        baseUrl: editing?.baseUrl ?? '',
        apiKey: '', // 编辑时留空 = 不改;只存服务端,不回显
        enabled: editing?.enabled ?? true,
      })
    }
  }, [open, editing, form])

  const submit = async () => {
    const v = await form.validateFields().catch(() => null)
    if (!v) return
    setBusy(true)
    try {
      if (editing) {
        await api.updateLlmModel(editing.id, {
          name: v.name.trim(),
          baseUrl: v.baseUrl?.trim() || undefined,
          apiKey: v.apiKey || undefined,
          enabled: v.enabled,
        })
      } else {
        await api.createLlmModel({
          provider,
          name: v.name.trim(),
          baseUrl: v.baseUrl?.trim() || undefined,
          apiKey: v.apiKey || undefined,
        })
      }
      message.success(t('pc.saved', '已保存'))
      form.resetFields()
      onDone()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('pc.opFailed', '操作失败'))
    } finally {
      setBusy(false)
    }
  }

  return (
    <Modal
      open={open}
      onCancel={onClose}
      onOk={submit}
      confirmLoading={busy}
      title={editing ? t('pc.editModel', '编辑模型') : t('pc.addModel', '添加模型')}
      destroyOnHidden
    >
      <Form form={form} layout="vertical" requiredMark={false}>
        <Form.Item
          name="name"
          label={t('pc.modelNameLabel', '模型名称')}
          rules={[{ required: true, message: t('pc.modelNameRequired', '请输入模型名称') }]}
        >
          <Input autoFocus maxLength={128} />
        </Form.Item>
        <Form.Item name="baseUrl" label={t('pc.baseUrl', 'Base URL')}>
          <Input placeholder="https://" />
        </Form.Item>
        <Form.Item
          name="apiKey"
          label={t('pc.apiKeyCol', 'API Key')}
          extra={editing ? t('pc.apiKeyKeepHint', '留空表示不修改') : undefined}
        >
          <Input.Password autoComplete="new-password" />
        </Form.Item>
        {editing && (
          <Form.Item name="enabled" label={t('pc.enabled', '启用')} valuePropName="checked">
            <Switch size="small" />
          </Form.Item>
        )}
      </Form>
    </Modal>
  )
}
