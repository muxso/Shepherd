import { useEffect, useMemo, useState } from 'react'
import { Button, Dropdown, Form, Input, Segmented, Space, Switch, Table, Tag, message } from 'antd'
import { MoreOutlined, SearchOutlined } from '@ant-design/icons'
import type { ColumnsType } from 'antd/es/table'
import { api, ApiError, userStore, type Organization, type Project } from '../api'
import { modal } from '../feedback'
import { useI18n } from '../i18n'
import EditDrawer from '../components/EditDrawer'

// System / organizations & projects (ref #53): org/project tabs + create org.
export default function OrgProjects() {
  const { t } = useI18n()
  const [orgs, setOrgs] = useState<Organization[]>([])
  const [projByOrg, setProjByOrg] = useState<Record<string, Project[]>>({})
  const [userCount, setUserCount] = useState(0)
  const [loading, setLoading] = useState(false)
  const [tab, setTab] = useState('org')
  const [q, setQ] = useState('')
  const [createOpen, setCreateOpen] = useState(false)
  const [editing, setEditing] = useState<Organization | null>(null)

  const load = async () => {
    setLoading(true)
    try {
      const [op, up] = await Promise.all([api.organizations(), api.users().catch(() => ({ total: 0, items: [] }))])
      const list = op.items ?? []
      setOrgs(list)
      setUserCount(up.total ?? 0)
      const entries = await Promise.all(list.map(async (o) => [o.id, await api.projects(o.id).then((p) => p.items ?? []).catch(() => [])] as const))
      setProjByOrg(Object.fromEntries(entries))
    } finally {
      setLoading(false)
    }
  }
  useEffect(() => { load() }, [])

  const allProjects = useMemo(() => Object.entries(projByOrg).flatMap(([oid, ps]) => ps.map((p) => ({ ...p, orgName: orgs.find((o) => o.id === oid)?.name || oid }))), [projByOrg, orgs])
  const orgRows = orgs.filter((o) => !q || o.name.toLowerCase().includes(q.toLowerCase()) || o.id.toLowerCase().includes(q.toLowerCase()))
  const projRows = allProjects.filter((p) => !q || p.name.toLowerCase().includes(q.toLowerCase()) || p.id.toLowerCase().includes(q.toLowerCase()))

  const toggleOrg = async (o: Organization, enable: boolean) => {
    setOrgs((prev) => prev.map((x) => (x.id === o.id ? { ...x, enable } : x)))
    try { await api.updateOrganization(o.id, { name: o.name, enable }) } catch (e) {
      setOrgs((prev) => prev.map((x) => (x.id === o.id ? { ...x, enable: !enable } : x)))
      message.error(e instanceof ApiError ? e.message : t('op.updateFailed', '更新失败'))
    }
  }
  const removeOrg = (o: Organization) => modal.confirm({
    title: t('op.delConfirm', '确认删除组织?'), content: o.name, okButtonProps: { danger: true },
    onOk: async () => { try { await api.deleteOrganization(o.id); message.success(t('op.deleted', '已删除')); load() } catch (e) { message.error(e instanceof ApiError ? e.message : t('op.delFailed', '删除失败')) } },
  })
  const soon = () => message.info(t('common.comingSoon', '即将接入'))

  const orgCols: ColumnsType<Organization> = [
    { title: 'ID', dataIndex: 'id', width: 130, ellipsis: true, render: (v: string) => <span className="ms-mono" style={{ fontSize: 12 }}>{v.slice(0, 12)}</span> },
    { title: t('op.name', '名称'), dataIndex: 'name', width: 160, render: (v: string) => <span style={{ fontWeight: 600 }}>{v}</span> },
    { title: t('op.members', '成员'), width: 90, render: () => <span style={{ color: 'var(--brand)' }}>{userCount}</span> },
    { title: t('op.projects', '项目'), width: 90, render: (_v, o) => <span style={{ color: '#1677ff' }}>{projByOrg[o.id]?.length ?? 0}</span> },
    { title: t('op.status', '状态'), width: 80, render: (_v, o) => <Switch size="small" checked={o.enable !== false} onChange={(c) => toggleOrg(o, c)} /> },
    { title: t('op.desc', '描述'), render: () => <span style={{ color: 'var(--text-3)' }}>{t('op.sysDefault', '系统默认创建的组织')}</span> },
    { title: t('op.creator', '创建人'), width: 120, render: () => userStore.get() },
    {
      title: t('apidef.colAction', '操作'), width: 180, fixed: 'right',
      render: (_v, o) => (
        <Space size={4} onClick={(e) => e.stopPropagation()}>
          <Button type="link" size="small" onClick={() => setEditing(o)}>{t('a.edit', '编辑')}</Button>
          <Button type="link" size="small" onClick={soon}>{t('op.addMember', '添加成员')}</Button>
          <Dropdown menu={{ items: [{ key: 'del', label: t('a.delete', '删除'), danger: true }], onClick: () => removeOrg(o) }}>
            <Button type="link" size="small" icon={<MoreOutlined />} />
          </Dropdown>
        </Space>
      ),
    },
  ]
  const projCols: ColumnsType<Project & { orgName: string }> = [
    { title: 'ID', dataIndex: 'id', width: 130, ellipsis: true, render: (v: string) => <span className="ms-mono" style={{ fontSize: 12 }}>{v.slice(0, 12)}</span> },
    { title: t('op.name', '名称'), dataIndex: 'name', render: (v: string) => <span style={{ fontWeight: 600 }}>{v}</span> },
    { title: t('op.status', '状态'), width: 90, render: (_v, p) => <Tag color={p.enable ? 'green' : 'default'}>{p.enable ? t('proj.enabled', '启用') : t('proj.disabled', '禁用')}</Tag> },
    { title: t('op.belongOrg', '所属组织'), width: 160, render: (_v, p) => <Tag>{p.orgName}</Tag> },
  ]

  return (
    <div style={{ padding: 12, height: '100%', overflow: 'auto', background: 'var(--bg)' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
        <Segmented value={tab} onChange={(v) => setTab(v as string)} options={[{ label: `${t('op.orgs', '组织')}(${orgs.length})`, value: 'org' }, { label: `${t('op.projTab', '项目')}(${allProjects.length})`, value: 'proj' }]} />
        <Button type="primary" onClick={() => setCreateOpen(true)}>{t('op.createOrg', '创建组织')}</Button>
        <div style={{ flex: 1 }} />
        <Input allowClear prefix={<SearchOutlined style={{ color: 'var(--text-3)' }} />} placeholder={t('op.search', '通过 ID/名称搜索')} style={{ width: 260 }} value={q} onChange={(e) => setQ(e.target.value)} />
      </div>
      {tab === 'org' ? (
        <Table<Organization> rowKey="id" size="middle" loading={loading} dataSource={orgRows} columns={orgCols} scroll={{ x: 'max-content' }} pagination={{ pageSize: 50, size: 'small', showTotal: (n) => `${t('apidef.totalPrefix', '共')} ${n} ${t('proj.unit', '条')}` }} />
      ) : (
        <Table<Project & { orgName: string }> rowKey="id" size="middle" loading={loading} dataSource={projRows} columns={projCols} pagination={{ pageSize: 50, size: 'small', showTotal: (n) => `${t('apidef.totalPrefix', '共')} ${n} ${t('proj.unit', '条')}` }} />
      )}
      <OrgModal open={createOpen || !!editing} editing={editing} onClose={() => { setCreateOpen(false); setEditing(null) }} onDone={() => { setCreateOpen(false); setEditing(null); load() }} t={t} />
    </div>
  )
}

type TFn = (k: string, d?: string) => string

function OrgModal({ open, editing, onClose, onDone, t }: { open: boolean; editing: Organization | null; onClose: () => void; onDone: () => void; t: TFn }) {
  const [form] = Form.useForm()
  const [busy, setBusy] = useState(false)
  useEffect(() => { if (open) form.setFieldsValue({ name: editing?.name ?? '' }) }, [open, editing, form])
  const submit = async () => {
    const v = await form.validateFields().catch(() => null)
    if (!v) return
    setBusy(true)
    try {
      if (editing) await api.updateOrganization(editing.id, { name: v.name.trim(), enable: editing.enable !== false })
      else await api.createOrganization(v.name.trim())
      message.success(t('op.saved', '已保存'))
      form.resetFields()
      onDone()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('op.saveFailed', '保存失败'))
    } finally { setBusy(false) }
  }
  return (
    <EditDrawer open={open} onCancel={onClose} onOk={submit} confirmLoading={busy} title={editing ? t('op.editOrg', '编辑组织') : t('op.createOrg', '创建组织')}>
      <Form form={form} layout="vertical" requiredMark={false}>
        <Form.Item name="name" label={t('op.name', '名称')} rules={[{ required: true, message: t('op.nameRequired', '请输入名称') }]}>
          <Input placeholder={t('op.namePh', '请输入组织名称')} />
        </Form.Item>
      </Form>
    </EditDrawer>
  )
}
