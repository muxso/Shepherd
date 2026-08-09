import { useEffect, useState } from 'react'
import { Alert, Button, Form, Input, Modal, Popconfirm, Popover, Segmented, Table, Tag, Typography } from 'antd'
import EditDrawer from '../components/EditDrawer'
import { PlusOutlined, ReloadOutlined } from '@ant-design/icons'
import { api, ApiError, type ApiKey } from '../api'
import { message } from '../feedback'
import { useApp } from '../context'
import { useI18n } from '../i18n'
import { useListView, type ListColumn } from '../components/ListView'
import PermissionMatrix, { serializePermissions } from '../components/PermissionMatrix'

// Parse packed permission strings like "SYSTEM_USER:ADD+DELETE+READ+UPDATE" into
// { resource, actions[] }. Unknown/custom strings fall back to resource-only.
function parsePermissions(perms: string[] | undefined): { resource: string; actions: string[] }[] {
  if (!perms?.length) return []
  return perms.map((p) => {
    const [resource, rest] = p.split(':')
    if (!resource || rest === undefined) return { resource: p, actions: [] }
    return { resource, actions: rest.split('+').filter(Boolean) }
  })
}

const RESOURCE_COLORS: Record<string, string> = {
  SYSTEM_USER: 'red',
  PROJECT: 'blue',
  ORGANIZATION: 'purple',
  USER_ROLE: 'orange',
  USER_GROUP: 'volcano',
  ROLE: 'geekblue',
  BUG: 'magenta',
  REQUIREMENT: 'geekblue',
  DECOMPOSITION: 'cyan',
  DELIVERY: 'lime',
  VERIFICATION: 'gold',
  TEST_PLAN: 'cyan',
  FUNCTIONAL_CASE: 'gold',
  CASE_REVIEW: 'gold',
  API_DEFINITION: 'green',
  API_SCENARIO: 'green',
  API_MOCK: 'green',
  PERF: 'purple',
  RUNNER: 'blue',
  RUNNER_AGENT: 'blue',
  AGENT: 'cyan',
  FLEET: 'cyan',
  MCP: 'purple',
  SKILL: 'orange',
  SYSTEM_SETTING: 'default',
  SYSTEM_APIKEY: 'default',
}

const resourceColor = (r: string) => RESOURCE_COLORS[r.toUpperCase()] || 'default'

// Compact permission summary for a table cell: resource tags (each with its action count), capped with
// a "+N" overflow, and the full resource→actions breakdown on hover — instead of an unreadable tag wall.
function PermCell({ perms }: { perms?: string[] }) {
  const groups = parsePermissions(perms)
  if (!groups.length) return <span style={{ color: 'var(--text-3)' }}>—</span>
  const CAP = 4
  const shown = groups.slice(0, CAP)
  const extra = groups.length - CAP
  const detail = (
    <div style={{ maxWidth: 460, display: 'flex', flexDirection: 'column', gap: 6 }}>
      {groups.map((g) => (
        <div key={g.resource} style={{ display: 'flex', flexWrap: 'wrap', gap: 4, alignItems: 'center' }}>
          <Tag color={resourceColor(g.resource)} style={{ margin: 0 }}>{g.resource}</Tag>
          {g.actions.map((a) => (
            <Tag key={a} bordered={false} style={{ margin: 0, fontSize: 11, padding: '0 5px', lineHeight: '18px' }}>{a}</Tag>
          ))}
        </div>
      ))}
    </div>
  )
  return (
    <Popover content={detail} trigger="hover" placement="topLeft">
      <div style={{ display: 'inline-flex', flexWrap: 'wrap', gap: 4, alignItems: 'center', cursor: 'default' }}>
        {shown.map((g) => (
          <Tag key={g.resource} color={resourceColor(g.resource)} style={{ margin: 0 }}>
            {g.resource}
            {g.actions.length > 0 && <span style={{ opacity: 0.65, marginLeft: 5 }}>{g.actions.length}</span>}
          </Tag>
        ))}
        {extra > 0 && <Tag style={{ margin: 0 }}>+{extra}</Tag>}
      </div>
    </Popover>
  )
}
// The plaintext key (sak_…) appears exactly once, in the create response — an un-dismissable
// modal forces the user to save it. The list shows metadata only; revoking (DELETE) keeps the
// row and marks it revoked.
export default function ApiKeys() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const [items, setItems] = useState<ApiKey[]>([])
  const [loading, setLoading] = useState(false)
  const [createOpen, setCreateOpen] = useState(false)
  // One-time plaintext key from a successful create; non-null opens the save-now modal.
  const [createdKey, setCreatedKey] = useState<string | null>(null)

  const load = () => {
    setLoading(true)
    api.apiKeys()
      .then((p) => setItems(p.items ?? []))
      .catch((e) => {
        setItems([])
        message.error(e instanceof ApiError ? e.message : t('ak.loadFailed', '加载失败'))
      })
      .finally(() => setLoading(false))
  }
  useEffect(load, []) // eslint-disable-line react-hooks/exhaustive-deps

  const revoke = async (k: ApiKey) => {
    try {
      await api.revokeApiKey(k.id)
      message.success(t('ak.revokeOk', '已吊销'))
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('ak.revokeFailed', '吊销失败'))
    }
  }

  const cols: ListColumn<ApiKey>[] = [
    { key: 'name', label: t('ak.colName', '名称'), title: t('ak.colName', '名称'), dataIndex: 'name', width: 220, ellipsis: true },
    {
      key: 'perms',
      label: t('ak.colPerms', '权限'),
      title: t('ak.colPerms', '权限'),
      dataIndex: 'permissions',
      width: 380,
      render: (perms: string[]) => <PermCell perms={perms} />,
    },
    {
      key: 'created',
      label: t('ak.colCreated', '创建时间'),
      title: t('ak.colCreated', '创建时间'),
      dataIndex: 'createdAt',
      width: 180,
      render: (v: string) => <span style={{ color: 'var(--text-2)' }}>{v ? new Date(v).toLocaleString() : '—'}</span>,
    },
    {
      key: 'status',
      label: t('ak.colStatus', '状态'),
      title: t('ak.colStatus', '状态'),
      dataIndex: 'revoked',
      width: 100,
      render: (revoked?: boolean) =>
        revoked ? <Tag>{t('ak.revoked', '已吊销')}</Tag> : <Tag color="green">{t('ak.active', '正常')}</Tag>,
    },
    {
      key: 'action',
      label: t('ak.colAction', '操作'),
      title: t('ak.colAction', '操作'),
      width: 90,
      fixed: 'right',
      render: (_v, k) =>
        k.revoked ? (
          <Button type="link" size="small" danger disabled>{t('ak.revoke', '吊销')}</Button>
        ) : (
          <Popconfirm
            title={t('ak.revokeConfirm', '吊销该密钥?使用它的调用方将立即失效。')}
            okText={t('ak.revoke', '吊销')}
            okButtonProps={{ danger: true }}
            onConfirm={() => revoke(k)}
          >
            <Button type="link" size="small" danger>{t('ak.revoke', '吊销')}</Button>
          </Popconfirm>
        ),
    },
  ]

  // List toolbar (view/filter/columns): system pages have no PageHeader, so it sits right-aligned above the table.
  const lv = useListView<ApiKey>({
    kind: 'apikey',
    projectId,
    searchOf: (k) => k.name,
    searchLabel: t('ak.searchPh', '搜索名称'),
    fields: [
      {
        key: 'status',
        label: t('ak.colStatus', '状态'),
        type: 'enum',
        options: [
          { value: 'active', label: t('ak.active', '正常') },
          { value: 'revoked', label: t('ak.revoked', '已吊销') },
        ],
        get: (k) => (k.revoked ? 'revoked' : 'active'),
      },
    ],
    columns: cols,
    rows: items,
  })

  return (
    <div style={{ padding: 12, height: '100%', overflow: 'auto', background: 'var(--bg)' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
        <Button type="primary" icon={<PlusOutlined />} onClick={() => setCreateOpen(true)}>{t('ak.create', '新建密钥')}</Button>
        <div style={{ flex: 1 }} />
        {lv.toolbar}
        <Button icon={<ReloadOutlined />} onClick={load}>{t('a.refresh', '刷新')}</Button>
      </div>
      <Table<ApiKey>
        rowKey="id"
        size="middle"
        loading={loading}
        dataSource={lv.rows}
        columns={lv.columns}
        scroll={{ x: 'max-content' }}
        pagination={{ ...lv.pagination, showTotal: (n) => `${t('apidef.totalPrefix', '共')} ${n} ${t('proj.unit', '条')}` }}
      />
      <CreateKeyModal
        open={createOpen}
        onClose={() => setCreateOpen(false)}
        onDone={(key) => { setCreateOpen(false); setCreatedKey(key); load() }}
      />
      <KeyOnceModal keyText={createdKey} onClose={() => setCreatedKey(null)} />
    </div>
  )
}

// Permission presets: executor = minimal set for the dispatch/write-back loop; custom = free-form.
// Verified minimal executor set: register/heartbeat/claim/callbacks = DELIVERY:UPDATE; design-doc write-back = REQUIREMENT:UPDATE.
const PRESET_EXECUTOR = ['DELIVERY:UPDATE', 'REQUIREMENT:UPDATE']

function CreateKeyModal({ open, onClose, onDone }: { open: boolean; onClose: () => void; onDone: (key: string) => void }) {
  const { t } = useI18n()
  const [form] = Form.useForm<{ name: string }>()
  const [preset, setPreset] = useState<'executor' | 'custom'>('executor')
  const [checked, setChecked] = useState<Set<string>>(new Set(PRESET_EXECUTOR))
  const [extras, setExtras] = useState<string[]>([])
  const [busy, setBusy] = useState(false)
  useEffect(() => {
    if (open) {
      setPreset('executor')
      setChecked(new Set(PRESET_EXECUTOR))
      setExtras([])
      form.setFieldsValue({ name: '' })
    }
  }, [open, form])

  const applyPreset = (p: 'executor' | 'custom') => {
    setPreset(p)
    if (p === 'executor') { setChecked(new Set(PRESET_EXECUTOR)); setExtras([]) }
  }

  // User edits in the matrix imply a custom permission set.
  const onMatrix = (next: Set<string>) => { setChecked(next); setPreset('custom') }

  const submit = async () => {
    const v = await form.validateFields().catch(() => null)
    if (!v) return
    const permissions = serializePermissions(checked, extras)
    if (!permissions.length) {
      message.warning(t('ak.permsRequired', '请至少选择一条权限'))
      return
    }
    setBusy(true)
    try {
      const r = await api.createApiKey({ name: v.name.trim(), permissions })
      message.success(t('ak.created', '密钥已创建'))
      form.resetFields()
      onDone(r.key ?? '')
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('ak.createFailed', '创建失败'))
    } finally {
      setBusy(false)
    }
  }

  return (
    <EditDrawer open={open} onCancel={onClose} onOk={submit} confirmLoading={busy} title={t('ak.create', '新建密钥')} width={860}>
      <Form form={form} layout="vertical" requiredMark={false}>
        <Form.Item name="name" label={t('ak.colName', '名称')} rules={[{ required: true, message: t('ak.nameRequired', '请输入名称') }]}>
          <Input placeholder={t('ak.namePh', '如:执行机-01')} autoFocus />
        </Form.Item>
        <Form.Item label={t('ak.preset', '权限预设')} extra={t('ak.presetHint', '执行机 = 派发回写所需的最小权限;自定义则在下方勾选')}>
          <Segmented
            value={preset}
            onChange={(v) => applyPreset(v as 'executor' | 'custom')}
            options={[
              { label: t('ak.presetExecutor', '执行机(推荐)'), value: 'executor' },
              { label: t('ak.presetCustom', '自定义'), value: 'custom' },
            ]}
          />
        </Form.Item>
        <Form.Item label={t('ak.colPerms', '权限')}>
          <PermissionMatrix checked={checked} onChange={onMatrix} />
        </Form.Item>
        {extras.length > 0 && (
          <Form.Item label={t('ak.permsExtra', '其他权限(不在此表内,已保留)')}>
            <div className="ms-mono" style={{ fontSize: 12, color: 'var(--text-3)' }}>{extras.join('  ·  ')}</div>
          </Form.Item>
        )}
      </Form>
    </EditDrawer>
  )
}

// One-time plaintext key display: mask/ESC/close button are all disabled — only "I saved it"
// dismisses, so a stray click can't lose the key.
function KeyOnceModal({ keyText, onClose }: { keyText: string | null; onClose: () => void }) {
  const { t } = useI18n()
  return (
    <Modal
      open={keyText !== null}
      title={t('ak.keyModalTitle', '密钥已创建')}
      closable={false}
      maskClosable={false}
      keyboard={false}
      okText={t('ak.iSaved', '我已保存')}
      cancelButtonProps={{ style: { display: 'none' } }}
      onOk={onClose}
      onCancel={onClose}
      destroyOnHidden
    >
      <Alert type="warning" showIcon message={t('ak.keyOnce', '密钥只显示这一次,请立即保存')} style={{ marginBottom: 12 }} />
      <Typography.Paragraph
        className="ms-mono"
        copyable={{ text: keyText ?? '', onCopy: () => message.success(t('ak.copied', '已复制')) }}
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
