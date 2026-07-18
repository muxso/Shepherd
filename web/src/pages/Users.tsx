import { useEffect, useState } from 'react'
import { Button, Dropdown, Form, Input, Space, Switch, Table, Tag, message } from 'antd'
import { MoreOutlined } from '@ant-design/icons'
import { api, ApiError, type User } from '../api'
import { modal } from '../feedback'
import { useApp } from '../context'
import { useI18n } from '../i18n'
import { useListView, type ListColumn } from '../components/ListView'
import EditDrawer from '../components/EditDrawer'

// System / Users. Email invite and user import have no backend yet (placeholder toast);
// org/user-group columns show system defaults.
export default function Users() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const [users, setUsers] = useState<User[]>([])
  const [loading, setLoading] = useState(false)
  const [modalOpen, setModalOpen] = useState(false)
  const [editing, setEditing] = useState<User | null>(null)
  const openCreate = () => { setEditing(null); setModalOpen(true) }
  const openEdit = (u: User) => { setEditing(u); setModalOpen(true) }

  const load = () => {
    setLoading(true)
    api.users().then((p) => setUsers(p.items ?? [])).catch(() => setUsers([])).finally(() => setLoading(false))
  }
  useEffect(load, [])

  const soon = () => message.info(t('common.comingSoon', '即将接入'))

  // Enable/disable: optimistic update, persist via PUT, roll back on failure.
  const toggleEnable = async (u: User, enable: boolean) => {
    setUsers((prev) => prev.map((x) => (x.id === u.id ? { ...x, enable } : x)))
    try {
      await api.updateUser(u.id, { name: u.name, email: u.email, enable })
    } catch (e) {
      setUsers((prev) => prev.map((x) => (x.id === u.id ? { ...x, enable: !enable } : x)))
      message.error(e instanceof ApiError ? e.message : t('user.updateFailed', '更新失败'))
    }
  }
  const resetPassword = (u: User) => {
    modal.confirm({
      title: t('user.resetConfirm', '重置该用户密码?'),
      content: `${u.name} (${u.email})`,
      onOk: async () => {
        try {
          const r = await api.resetUserPassword(u.id)
          modal.success({
            title: t('user.resetOk', '密码已重置'),
            content: (
              <div>
                {t('user.newPwHint', '新密码(请转交用户):')}
                <div style={{ marginTop: 8, fontFamily: 'monospace', fontWeight: 700, fontSize: 16 }}>{r.password}</div>
              </div>
            ),
          })
        } catch (e) {
          message.error(e instanceof ApiError ? e.message : t('user.resetFailed', '重置失败'))
        }
      },
    })
  }
  const removeUser = (u: User) => {
    modal.confirm({
      title: t('user.delConfirm', '确认删除用户?'),
      content: `${u.name} (${u.email})`,
      okButtonProps: { danger: true },
      onOk: async () => {
        try {
          await api.deleteUser(u.id)
          message.success(t('user.deleted', '已删除'))
          load()
        } catch (e) {
          message.error(e instanceof ApiError ? e.message : t('user.delFailed', '删除失败'))
        }
      },
    })
  }

  const cols: ListColumn<User>[] = [
    { key: 'username', label: t('user.username', '用户名'), title: t('user.username', '用户名'), dataIndex: 'email', width: 240, ellipsis: true },
    { key: 'name', label: t('user.name', '姓名'), title: t('user.name', '姓名'), dataIndex: 'name', width: 160 },
    { key: 'email', label: t('user.email', '邮箱'), title: t('user.email', '邮箱'), dataIndex: 'email', ellipsis: true },
    { key: 'phone', label: t('user.phone', '手机'), title: t('user.phone', '手机'), width: 110, render: () => <span style={{ color: 'var(--text-3)' }}>—</span> },
    { key: 'org', label: t('user.org', '组织'), title: t('user.org', '组织'), width: 140, render: () => <Tag>{t('user.defaultOrg', '默认组织')}</Tag> },
    { key: 'userGroup', label: t('user.userGroup', '用户组'), title: t('user.userGroup', '用户组'), width: 200, render: (_v, u) => (u.userGroups && u.userGroups.length ? u.userGroups.map((g) => <Tag key={g} color="green" style={{ marginBottom: 2 }}>{g}</Tag>) : <Tag color="green">{t('user.sysMember', '系统成员')}</Tag>) },
    {
      key: 'status',
      label: t('user.status', '状态'),
      title: t('user.status', '状态'),
      width: 90,
      render: (_v, u) => <Switch size="small" checked={u.enable !== false} onChange={(c) => toggleEnable(u, c)} />,
    },
    {
      key: 'action',
      label: t('apidef.colAction', '操作'),
      title: t('apidef.colAction', '操作'),
      width: 110,
      fixed: 'right',
      render: (_v, u) => (
        <Space size={4} onClick={(e) => e.stopPropagation()}>
          <Button type="link" size="small" onClick={() => openEdit(u)}>{t('a.edit', '编辑')}</Button>
          <Dropdown
            menu={{
              items: [
                { key: 'reset', label: t('user.resetPwd', '重置密码') },
                { key: 'del', label: t('a.delete', '删除'), danger: true },
              ],
              onClick: ({ key }) => (key === 'del' ? removeUser(u) : key === 'reset' ? resetPassword(u) : soon()),
            }}
          >
            <Button type="link" size="small" icon={<MoreOutlined />} />
          </Dropdown>
        </Space>
      ),
    },
  ]

  // List view/filter/column-settings trio: system pages have no PageHeader, so the toolbar sits right-aligned above the table.
  const lv = useListView<User>({
    kind: 'user',
    projectId,
    searchOf: (u) => `${u.name} ${u.email}`,
    searchLabel: t('user.searchPh', '搜索姓名/邮箱'),
    fields: [
      {
        key: 'status',
        label: t('user.status', '状态'),
        type: 'enum',
        options: [
          { value: 'enabled', label: t('user.stEnabled', '启用') },
          { value: 'disabled', label: t('user.stDisabled', '禁用') },
        ],
        get: (u) => (u.enable !== false ? 'enabled' : 'disabled'),
      },
    ],
    columns: cols,
    rows: users,
  })

  return (
    <div style={{ padding: 12, height: '100%', overflow: 'auto', background: 'var(--bg)' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
        <Button type="primary" onClick={openCreate}>{t('user.create', '创建用户')}</Button>
        <Button onClick={soon}>{t('user.invite', '邮箱邀请')}</Button>
        <Button onClick={soon}>{t('user.import', '导入用户')}</Button>
        <div style={{ flex: 1 }} />
        {lv.toolbar}
      </div>
      <Table<User>
        rowKey="id"
        size="middle"
        loading={loading}
        dataSource={lv.rows}
        columns={lv.columns}
        scroll={{ x: 'max-content' }}
        rowSelection={{ type: 'checkbox' }}
        pagination={{ ...lv.pagination, showTotal: (n) => `${t('apidef.totalPrefix', '共')} ${n} ${t('proj.unit', '条')}` }}
      />
      <UserModal open={modalOpen} editing={editing} onClose={() => setModalOpen(false)} onDone={() => { setModalOpen(false); load() }} t={t} />
    </div>
  )
}

type TFn = (k: string, d?: string) => string

// Non-null editing means edit (PUT, preserves the enable flag), otherwise create (POST).
function UserModal({ open, editing, onClose, onDone, t }: { open: boolean; editing: User | null; onClose: () => void; onDone: () => void; t: TFn }) {
  const [form] = Form.useForm()
  const [busy, setBusy] = useState(false)
  useEffect(() => {
    if (open) form.setFieldsValue({ name: editing?.name ?? '', email: editing?.email ?? '' })
  }, [open, editing, form])
  const submit = async () => {
    const v = await form.validateFields().catch(() => null)
    if (!v) return
    setBusy(true)
    try {
      if (editing) {
        await api.updateUser(editing.id, { name: v.name.trim(), email: v.email.trim(), enable: editing.enable !== false })
        message.success(t('user.updated', '已保存'))
      } else {
        await api.createUser({ name: v.name.trim(), email: v.email.trim() })
        message.success(t('user.created', '用户已创建'))
      }
      form.resetFields()
      onDone()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('user.createFailed', '保存失败'))
    } finally {
      setBusy(false)
    }
  }
  return (
    <EditDrawer open={open} onCancel={onClose} onOk={submit} confirmLoading={busy} title={editing ? t('user.edit', '编辑用户') : t('user.create', '创建用户')}>
      <Form form={form} layout="vertical" requiredMark={false}>
        <Form.Item name="name" label={t('user.name', '姓名')} rules={[{ required: true, message: t('user.nameRequired', '请输入姓名') }]}>
          <Input placeholder={t('user.namePh', '请输入姓名')} />
        </Form.Item>
        <Form.Item name="email" label={t('user.email', '邮箱')} rules={[{ required: true, type: 'email', message: t('user.emailRequired', '请输入有效邮箱') }]}>
          <Input placeholder={t('user.emailPh', '请输入邮箱')} />
        </Form.Item>
      </Form>
    </EditDrawer>
  )
}
