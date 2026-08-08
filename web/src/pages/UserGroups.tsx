import { useEffect, useMemo, useState } from 'react'
import { Button, Empty, Form, Input, Segmented, Select, Space, Table, Tooltip } from 'antd'
import { message, modal } from '../feedback'
import { DeleteOutlined, EditOutlined, PlusOutlined, SearchOutlined } from '@ant-design/icons'
import { api, ApiError, type Role, type User } from '../api'
import { useI18n } from '../i18n'
import EditDrawer from '../components/EditDrawer'
import PermissionMatrix, { parsePermissions, serializePermissions } from '../components/PermissionMatrix'

// User groups: left tree grouped by scope, right pane permissions/members.
// Both the read view and the editor use the shared PermissionMatrix (checkbox module/feature/action).

const SCOPE_ORDER = ['SYSTEM', 'ORGANIZATION', 'PROJECT']
const SCOPE_LABEL: Record<string, string> = { SYSTEM: '系统用户组', ORGANIZATION: '组织用户组', PROJECT: '项目用户组' }

export default function UserGroups() {
  const { t } = useI18n()
  const [roles, setRoles] = useState<Role[]>([])
  const [users, setUsers] = useState<User[]>([])
  const [loading, setLoading] = useState(false)
  const [q, setQ] = useState('')
  const [selId, setSelId] = useState<string>('')
  const [tab, setTab] = useState('perm')
  const [createScope, setCreateScope] = useState<string | null>(null)
  const [editRole, setEditRole] = useState<Role | null>(null)

  const load = async () => {
    setLoading(true)
    try {
      const [rp, up] = await Promise.all([api.roles(), api.users().catch(() => ({ items: [] as User[] } as any))])
      const list = rp.items ?? []
      setRoles(list)
      setUsers(up.items ?? [])
      setSelId((cur) => cur || (list[0]?.id ?? ''))
    } catch {
      setRoles([])
    } finally {
      setLoading(false)
    }
  }
  useEffect(() => { load() }, []) // eslint-disable-line react-hooks/exhaustive-deps

  const grouped = useMemo(() => {
    const f = roles.filter((r) => !q || r.name.toLowerCase().includes(q.toLowerCase()))
    const by: Record<string, Role[]> = {}
    f.forEach((r) => { const s = (r.scope || 'SYSTEM').toUpperCase(); (by[s] ??= []).push(r) })
    return SCOPE_ORDER.filter((s) => by[s]?.length).map((s) => ({ scope: s, label: t(`ug.scope.${s}`, SCOPE_LABEL[s] || s), roles: by[s] }))
  }, [roles, q, t])

  const sel = roles.find((r) => r.id === selId)
  // Read-only view uses the same matrix as the editor, so what you see maps 1:1 to what you edit.
  const selParsed = useMemo(() => parsePermissions(sel?.permissions), [sel])

  const del = (r: Role) => modal.confirm({
    title: `${t('ug.deleteConfirm', '删除用户组')}「${r.name}」?`,
    okButtonProps: { danger: true },
    onOk: async () => {
      try {
        await api.deleteRole(r.id)
        message.success(t('ug.deleted', '已删除'))
        if (selId === r.id) setSelId('')
        load()
      } catch (e) {
        message.error(e instanceof ApiError ? e.message : t('ug.deleteFailed', '删除失败'))
      }
    },
  })

  // Members = users whose userGroups (role names) include this group's name.
  const members = useMemo(() => (sel ? users.filter((u) => (u.userGroups ?? []).includes(sel.name)) : []), [users, sel])
  const nonMembers = useMemo(() => (sel ? users.filter((u) => !(u.userGroups ?? []).includes(sel.name)) : []), [users, sel])

  const grant = async (userId: string) => {
    if (!sel) return
    try {
      await api.grantUserRole(userId, sel.id)
      message.success(t('ug.granted', '已添加成员'))
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('ug.grantFailed', '添加失败'))
    }
  }
  const revoke = async (userId: string) => {
    if (!sel) return
    try {
      await api.revokeUserRole(userId, sel.id)
      message.success(t('ug.revoked', '已移除成员'))
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('ug.revokeFailed', '移除失败'))
    }
  }

  return (
    <div style={{ display: 'flex', height: '100%' }}>
      {/* Left: group tree grouped by scope */}
      <div style={{ width: 240, flexShrink: 0, borderRight: '1px solid var(--border-soft)', background: 'var(--panel)', display: 'flex', flexDirection: 'column' }}>
        <div style={{ padding: 10 }}>
          <Input allowClear prefix={<SearchOutlined style={{ color: 'var(--text-3)' }} />} placeholder={t('ug.search', '请输入用户组名称')} value={q} onChange={(e) => setQ(e.target.value)} />
        </div>
        <div style={{ flex: 1, overflow: 'auto', padding: '0 6px 10px' }}>
          {/* Allow creating a system group even when no groups exist yet */}
          {!loading && grouped.length === 0 && (
            <div style={{ padding: '8px 8px' }}>
              <Button block icon={<PlusOutlined />} onClick={() => setCreateScope('SYSTEM')}>{t('ug.newGroup', '新建用户组')}</Button>
            </div>
          )}
          {loading ? null : grouped.map((g) => (
            <div key={g.scope} style={{ marginBottom: 8 }}>
              <div style={{ display: 'flex', alignItems: 'center', padding: '6px 8px', fontSize: 13, color: 'var(--text-2)' }}>
                <span style={{ flex: 1, fontWeight: 600 }}>{g.label}</span>
                <Tooltip title={t('ug.newGroupIn', '新建用户组')}>
                  <Button type="text" size="small" icon={<PlusOutlined style={{ color: 'var(--brand)' }} />} onClick={() => setCreateScope(g.scope)} />
                </Tooltip>
              </div>
              {g.roles.map((r) => (
                <div
                  key={r.id}
                  onClick={() => setSelId(r.id)}
                  style={{
                    padding: '7px 14px', margin: '2px 0', borderRadius: 6, cursor: 'pointer', fontSize: 13,
                    background: selId === r.id ? 'var(--brand-soft)' : 'transparent',
                    color: selId === r.id ? 'var(--brand)' : 'var(--text)',
                  }}
                >
                  {r.name}
                </div>
              ))}
            </div>
          ))}
        </div>
      </div>
      {/* Right: permissions / members */}
      <div style={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', background: 'var(--panel)' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '10px 16px', borderBottom: '1px solid var(--border-soft)' }}>
          <Segmented value={tab} onChange={(v) => setTab(v as string)} options={[{ label: t('ug.perm', '权限'), value: 'perm' }, { label: t('ug.members', '成员'), value: 'members' }]} />
          <div style={{ flex: 1 }} />
          {sel && (
            <Space>
              <Button size="small" icon={<EditOutlined />} onClick={() => setEditRole(sel)}>{t('a.edit', '编辑')}</Button>
              <Button size="small" danger icon={<DeleteOutlined />} onClick={() => del(sel)}>{t('a.delete', '删除')}</Button>
            </Space>
          )}
        </div>
        <div style={{ flex: 1, overflow: 'auto', padding: 16 }}>
          {!sel ? (
            <Empty description={t('ug.selectGroup', '请选择用户组')} />
          ) : tab === 'perm' ? (
            !sel.permissions?.length ? <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('ug.noPerm', '该用户组暂无权限')} /> : (
              <>
                <PermissionMatrix checked={selParsed.checked} onChange={() => {}} disabled />
                {selParsed.extras.length > 0 && (
                  <div className="ms-mono" style={{ marginTop: 12, fontSize: 12, color: 'var(--text-3)' }}>
                    {t('ug.permsExtra', '其他权限(不在此表内,已保留)')}: {selParsed.extras.join('  ·  ')}
                  </div>
                )}
              </>
            )
          ) : (
            <MembersPanel members={members} nonMembers={nonMembers} onGrant={grant} onRevoke={revoke} />
          )}
        </div>
      </div>

      <RoleCreateModal scope={createScope} onClose={() => setCreateScope(null)} onDone={(id) => { setCreateScope(null); setSelId(id); load() }} />
      <RoleEditModal role={editRole} onClose={() => setEditRole(null)} onDone={() => { setEditRole(null); load() }} />
    </div>
  )
}

// Members: current members (removable) + picker to grant non-member users.
function MembersPanel({ members, nonMembers, onGrant, onRevoke }: { members: User[]; nonMembers: User[]; onGrant: (userId: string) => void; onRevoke: (userId: string) => void }) {
  const { t } = useI18n()
  const [pick, setPick] = useState<string>('')
  return (
    <Space direction="vertical" style={{ width: '100%' }} size={12}>
      <Space.Compact style={{ width: 420 }}>
        <Select
          style={{ flex: 1 }}
          showSearch
          optionFilterProp="label"
          placeholder={t('ug.addMemberPlaceholder', '选择用户加入本用户组')}
          value={pick || undefined}
          onChange={setPick}
          options={nonMembers.map((u) => ({ value: u.id, label: u.name ? `${u.name} (${u.email})` : u.email }))}
        />
        <Button type="primary" disabled={!pick} onClick={() => { onGrant(pick); setPick('') }}>{t('ug.addMember', '添加成员')}</Button>
      </Space.Compact>
      <Table<User>
        rowKey="id"
        size="small"
        dataSource={members}
        pagination={false}
        locale={{ emptyText: <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('ug.noMembers', '该用户组暂无成员')} /> }}
        columns={[
          { title: t('user.name', '名称'), dataIndex: 'name', render: (v?: string) => v || '—' },
          { title: t('user.email', '邮箱'), dataIndex: 'email' },
          { title: t('req.action', '操作'), width: 90, render: (_v, u) => <Button type="link" size="small" danger onClick={() => onRevoke(u.id)}>{t('ug.remove', '移除')}</Button> },
        ]}
      />
    </Space>
  )
}

function RoleCreateModal({ scope, onClose, onDone }: { scope: string | null; onClose: () => void; onDone: (id: string) => void }) {
  const { t } = useI18n()
  const [form] = Form.useForm<{ name: string }>()
  const [busy, setBusy] = useState(false)
  useEffect(() => { if (scope) form.resetFields() }, [scope, form])
  return (
    <EditDrawer title={`${t('ug.newGroup', '新建用户组')}${scope ? ` · ${t(`ug.scope.${scope}`, SCOPE_LABEL[scope] || scope)}` : ''}`} open={!!scope} onCancel={onClose} footer={null}>
      <Form
        form={form}
        layout="vertical"
        onFinish={async (v) => {
          setBusy(true)
          try {
            const r = await api.createRole({ name: v.name.trim(), scope: scope || 'SYSTEM', permissions: [] })
            message.success(t('ug.created', '用户组已创建'))
            onDone(r.id)
          } catch (e) {
            message.error(e instanceof ApiError ? e.message : t('ug.createFailed', '创建失败'))
          } finally {
            setBusy(false)
          }
        }}
      >
        <Form.Item name="name" label={t('ug.groupName', '用户组名称')} rules={[{ required: true }]}>
          <Input placeholder={t('ug.groupNamePlaceholder', '如:接口测试工程师')} autoFocus />
        </Form.Item>
        <Button type="primary" htmlType="submit" block loading={busy}>{t('a.create', '创建')}</Button>
      </Form>
    </EditDrawer>
  )
}

// Edit group: rename + a checkbox permission matrix (module / feature / actions). Permissions the
// catalog can't render (legacy/unknown resources) are preserved verbatim via `extras` so nothing is lost.
function RoleEditModal({ role, onClose, onDone }: { role: Role | null; onClose: () => void; onDone: () => void }) {
  const { t } = useI18n()
  const [form] = Form.useForm<{ name: string }>()
  const [busy, setBusy] = useState(false)
  const [checked, setChecked] = useState<Set<string>>(new Set())
  const [extras, setExtras] = useState<string[]>([])
  useEffect(() => {
    if (role) {
      form.setFieldsValue({ name: role.name })
      const { checked: c, extras: e } = parsePermissions(role.permissions)
      setChecked(c); setExtras(e)
    }
  }, [role, form])
  return (
    <EditDrawer title={t('ug.editGroup', '编辑用户组')} open={!!role} onCancel={onClose} footer={null} width={860}>
      <Form
        form={form}
        layout="vertical"
        onFinish={async (v) => {
          if (!role) return
          setBusy(true)
          try {
            const permissions = serializePermissions(checked, extras)
            await api.updateRole(role.id, { name: v.name.trim(), permissions })
            message.success(t('ug.saved', '已保存'))
            onDone()
          } catch (e) {
            message.error(e instanceof ApiError ? e.message : t('ug.saveFailed', '保存失败'))
          } finally {
            setBusy(false)
          }
        }}
      >
        <Form.Item name="name" label={t('ug.groupName', '用户组名称')} rules={[{ required: true }]}>
          <Input autoFocus />
        </Form.Item>
        <Form.Item label={t('ug.permsLabel2', '权限')}>
          <PermissionMatrix checked={checked} onChange={setChecked} />
        </Form.Item>
        {extras.length > 0 && (
          <Form.Item label={t('ug.permsExtra', '其他权限(不在此表内,已保留)')}>
            <div className="ms-mono" style={{ fontSize: 12, color: 'var(--text-3)' }}>{extras.join('  ·  ')}</div>
          </Form.Item>
        )}
        <Button type="primary" htmlType="submit" block loading={busy}>{t('a.save', '保存')}</Button>
      </Form>
    </EditDrawer>
  )
}
