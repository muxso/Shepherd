import { useCallback, useEffect, useMemo, useState } from 'react'
import { Alert, Button, Card, Form, Input, Popconfirm, Select, Space, Switch, Table, Tag } from 'antd'
import ResizableDrawer from '../components/ResizableDrawer'
import EditDrawer from '../components/EditDrawer'
import { FolderOpenOutlined, SearchOutlined } from '@ant-design/icons'
import type { ColumnsType } from 'antd/es/table'
import { message } from '../feedback'
import { api, ApiError, userStore, type Organization, type Project, type ProjectMember, type Role, type User } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'
import { SelectProjectEmpty } from '../components/Page'
import PermissionMatrix, { parsePermissions, serializePermissions } from '../components/PermissionMatrix'

// Project & permissions: narrow grouped left nav + right content (ref #44-#48).
type NavKey = 'projects' | 'basic' | 'appSettings' | 'members' | 'userGroups'

export default function ProjectAdmin() {
  const { t } = useI18n()
  const { projects, projectId } = useApp()
  const [nav, setNav] = useState<NavKey>('projects')
  const project = projects.find((p) => p.id === projectId)

  if (!projectId || !project) return <SelectProjectEmpty />

  const groups: { title: string; items: { key: NavKey; label: string }[] }[] = [
    { title: t('pa.grpProjectPerm', '项目与权限'), items: [
      { key: 'projects', label: t('pa.navProjects', '项目') },
      { key: 'basic', label: t('proj.basic', '基本信息') },
      { key: 'appSettings', label: t('proj.appSettings', '应用设置') },
    ] },
    { title: t('proj.grpMember', '成员权限'), items: [
      { key: 'members', label: t('proj.members', '成员') },
      { key: 'userGroups', label: t('proj.userGroups', '用户组') },
    ] },
  ]

  return (
    <div style={{ display: 'flex', height: '100%' }}>
      {/* Left: grouped nav */}
      <div style={{ width: 180, flexShrink: 0, borderRight: '1px solid var(--border-soft)', padding: '12px 8px', overflow: 'auto', background: 'var(--panel)' }}>
        {groups.map((g, gi) => (
          <div key={g.title} style={gi > 0 ? { borderTop: '1px solid var(--border-soft)', marginTop: 8, paddingTop: 8 } : undefined}>
            <div style={{ fontSize: 12, color: 'var(--text-3)', padding: '6px 10px 2px' }}>{g.title}</div>
            {g.items.map((it) => (
              <div
                key={it.key}
                onClick={() => setNav(it.key)}
                style={{
                  padding: '8px 12px', borderRadius: 6, cursor: 'pointer', fontSize: 13, margin: '2px 0',
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
      {/* Right: content */}
      <div style={{ flex: 1, minWidth: 0, overflow: 'auto', padding: 16, background: 'var(--bg)' }}>
        {nav === 'projects' && <ProjectsPanel t={t} projects={projects} currentId={projectId} />}
        {nav === 'basic' && <BasicInfo project={project} t={t} />}
        {nav === 'appSettings' && <AppSettings t={t} />}
        {nav === 'members' && <Members t={t} projectId={projectId} />}
        {nav === 'userGroups' && <UserGroups t={t} />}
      </div>
    </div>
  )
}

type TFn = (k: string, d?: string) => string

// Projects: org/project list (org names resolved via /organization).
function ProjectsPanel({ t, projects, currentId }: { t: TFn; projects: Project[]; currentId: string }) {
  const [orgs, setOrgs] = useState<Organization[]>([])
  useEffect(() => {
    api.organizations().then((p) => setOrgs(p.items ?? [])).catch(() => setOrgs([]))
  }, [])
  const orgName = useMemo(() => new Map(orgs.map((o) => [o.id, o.name])), [orgs])
  const cols: ColumnsType<Project> = [
    {
      title: t('pa.colProjectName', '项目名称'),
      render: (_v, p) => (
        <Space size={8}>
          <FolderOpenOutlined style={{ color: 'var(--brand)' }} />
          <span style={{ fontWeight: 600 }}>{p.name}</span>
          {p.id === currentId && <Tag color="blue">{t('pa.current', '当前')}</Tag>}
        </Space>
      ),
    },
    { title: t('proj.org', '所属组织'), width: 220, render: (_v, p) => <Tag>{orgName.get(p.organizationId) || p.organizationId}</Tag> },
    { title: t('proj.status', '状态'), width: 100, render: (_v, p) => <Tag color={p.enable ? 'green' : 'default'}>{p.enable ? t('proj.enabled', '启用') : t('proj.disabled', '禁用')}</Tag> },
    { title: 'ID', width: 300, render: (_v, p) => <span className="ms-mono" style={{ fontSize: 12, color: 'var(--text-3)' }}>{p.id}</span> },
  ]
  return (
    <Card size="small" styles={{ body: { padding: 12 } }}>
      <Table<Project> rowKey="id" size="middle" dataSource={projects} columns={cols} pagination={false} />
    </Card>
  )
}

function BasicInfo({ project, t }: { project: { id: string; name: string; enable: boolean; organizationId: string }; t: TFn }) {
  const [orgName, setOrgName] = useState('')
  useEffect(() => {
    api.organizations().then((p: { items: Organization[] }) => {
      setOrgName(p.items.find((o) => o.id === project.organizationId)?.name || project.organizationId)
    }).catch(() => setOrgName(project.organizationId))
  }, [project.organizationId])
  const row = (label: string, value: React.ReactNode) => (
    <div style={{ display: 'flex', padding: '8px 0', fontSize: 13 }}>
      <span style={{ width: 90, color: 'var(--text-3)' }}>{label}</span>
      <span style={{ color: 'var(--text)' }}>{value}</span>
    </div>
  )
  return (
    <Card title={t('proj.basic', '基本信息')} size="small" extra={<Button size="small">{t('a.edit', '编辑')}</Button>}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 16 }}>
        <FolderOpenOutlined style={{ fontSize: 28, color: 'var(--brand)' }} />
        <div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <span style={{ fontWeight: 600, fontSize: 15 }}>{project.name}</span>
            <Tag color={project.enable ? 'green' : 'default'}>{project.enable ? t('proj.enabled', '启用') : t('proj.disabled', '禁用')}</Tag>
          </div>
          <div style={{ color: 'var(--text-3)', fontSize: 12 }}>{t('proj.systemDefault', '系统默认创建的项目')}</div>
        </div>
      </div>
      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', columnGap: 48, maxWidth: 800 }}>
        {row(t('proj.creator', '创建人'), userStore.get())}
        {row(t('proj.org', '所属组织'), <Tag>{orgName || '—'}</Tag>)}
        {row(t('proj.resourcePool', '资源池'), <span style={{ color: 'var(--text-3)' }}>—</span>)}
        {row('ID', <span className="ms-mono" style={{ fontSize: 12 }}>{project.id}</span>)}
      </div>
    </Card>
  )
}

// App settings: menu toggles (ref screenshots). Local state only; persistence waits on a backend project-config API.
function AppSettings({ t }: { t: TFn }) {
  const [reuse, setReuse] = useState(false)
  const [linkReq, setLinkReq] = useState(false)
  const [retention, setRetention] = useState('1')
  const [linkTtl, setLinkTtl] = useState('1')
  const section = (title: string, body: React.ReactNode) => (
    <div style={{ borderBottom: '1px solid var(--border-soft)', padding: '12px 0' }}>
      <div style={{ fontWeight: 600, fontSize: 13, marginBottom: 8 }}>{title}</div>
      {body}
    </div>
  )
  const item = (label: string, desc: string, control: React.ReactNode) => (
    <div style={{ display: 'flex', alignItems: 'center', padding: '8px 0', fontSize: 13 }}>
      <span style={{ width: 130, color: 'var(--text)' }}>{label}</span>
      <span style={{ flex: 1, color: 'var(--text-3)' }}>{desc}</span>
      {control}
    </div>
  )
  return (
    <Card title={t('proj.appSettings', '应用设置')} size="small">
      {section(t('proj.testCase', '测试用例'), (
        <>
          {item(t('proj.reReview', '重新提审'), t('proj.reReviewDesc', '评审活动中用例发生变更,用例状态自动切换为重新提审'), <Switch size="small" checked={reuse} onChange={setReuse} />)}
          {item(t('proj.linkReq', '关联需求'), t('proj.linkReqDesc', '可将用例与第三方项目管理平台进行关联'), <Switch size="small" checked={linkReq} onChange={setLinkReq} />)}
        </>
      ))}
      {section(t('proj.apiTest', '接口测试'), (
        <>
          {item(t('proj.reportRetention', '报告保留时间'), '', <Input style={{ width: 120 }} size="small" value={retention} onChange={(e) => setRetention(e.target.value)} suffix={t('proj.month', '月')} />)}
          {item(t('proj.linkTtl', '报告链接有效期'), '', <Input style={{ width: 120 }} size="small" value={linkTtl} onChange={(e) => setLinkTtl(e.target.value)} suffix={t('proj.day', '天')} />)}
        </>
      ))}
    </Card>
  )
}

// Project members: real member list (/project/{id}/member), joined with the platform user table for name/email.
type MemberRow = ProjectMember & { user?: User }

function Members({ t, projectId }: { t: TFn; projectId: string }) {
  const [members, setMembers] = useState<ProjectMember[]>([])
  const [users, setUsers] = useState<User[]>([])
  const [q, setQ] = useState('')
  const [loading, setLoading] = useState(false)
  const [addOpen, setAddOpen] = useState(false)
  const load = useCallback(() => {
    setLoading(true)
    Promise.all([api.projectMembers(projectId), api.users()])
      .then(([ms, up]) => { setMembers(ms ?? []); setUsers(up.items ?? []) })
      .catch(() => { setMembers([]); setUsers([]) })
      .finally(() => setLoading(false))
  }, [projectId])
  useEffect(load, [load])
  const byId = useMemo(() => new Map(users.map((u) => [u.id, u])), [users])
  const rows: MemberRow[] = members
    .map((m) => ({ ...m, user: byId.get(m.userId) }))
    .filter((m) => {
      if (!q) return true
      const s = q.toLowerCase()
      return (m.user?.name ?? '').toLowerCase().includes(s) || (m.user?.email ?? m.userId).toLowerCase().includes(s)
    })
  const remove = async (m: MemberRow) => {
    try {
      await api.removeProjectMember(projectId, m.userId)
      message.success(t('proj.removed', '已移除'))
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('proj.removeFailed', '移除失败'))
    }
  }
  const cols: ColumnsType<MemberRow> = [
    { title: t('proj.username', '用户名'), width: 220, render: (_v, m) => m.user?.email ?? <span className="ms-mono" style={{ fontSize: 12 }}>{m.userId}</span> },
    { title: t('proj.realname', '姓名'), width: 160, render: (_v, m) => m.user?.name ?? '—' },
    { title: t('proj.email', '邮箱'), render: (_v, m) => m.user?.email ?? '—' },
    { title: t('proj.role', '角色'), width: 120, render: (_v, m) => m.role === 'OWNER' ? <Tag color="gold">{t('proj.roleOwner', '拥有者')}</Tag> : <Tag color="green">{t('proj.projMember', '项目成员')}</Tag> },
    { title: t('proj.status', '状态'), width: 90, render: (_v, m) => <Tag color={m.user?.enable === false ? 'default' : 'green'}>{m.user?.enable === false ? t('proj.disabled', '禁用') : t('proj.normal', '正常')}</Tag> },
    {
      title: t('apidef.colAction', '操作'), width: 90,
      render: (_v, m) => (
        <Popconfirm title={t('proj.removeConfirm', '确认将该成员移出项目?')} onConfirm={() => remove(m)}>
          <Button type="link" size="small" danger>{t('proj.remove', '移除')}</Button>
        </Popconfirm>
      ),
    },
  ]
  return (
    <Card size="small" styles={{ body: { padding: 12 } }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
        <Button type="primary" onClick={() => setAddOpen(true)}>{t('proj.addMember', '添加成员')}</Button>
        <div style={{ flex: 1 }} />
        <Input allowClear prefix={<SearchOutlined style={{ color: 'var(--text-3)' }} />} placeholder={t('proj.searchMember', '通过姓名/邮箱搜索')} style={{ width: 260 }} value={q} onChange={(e) => setQ(e.target.value)} />
      </div>
      <Table<MemberRow> rowKey="userId" size="middle" loading={loading} dataSource={rows} columns={cols} pagination={{ pageSize: 20, size: 'small', showTotal: (n) => `${t('apidef.totalPrefix', '共')} ${n} ${t('proj.unit', '条')}` }} />
      <AddMemberModal
        open={addOpen}
        projectId={projectId}
        candidates={users.filter((u) => u.enable !== false && !members.some((m) => m.userId === u.id))}
        onClose={() => setAddOpen(false)}
        onDone={() => { setAddOpen(false); load() }}
        t={t}
      />
    </Card>
  )
}

function AddMemberModal({ open, projectId, candidates, onClose, onDone, t }: {
  open: boolean
  projectId: string
  candidates: User[]
  onClose: () => void
  onDone: () => void
  t: TFn
}) {
  const [form] = Form.useForm<{ userId: string; role: string }>()
  const [busy, setBusy] = useState(false)
  useEffect(() => { if (open) form.resetFields() }, [open, form])
  return (
    <EditDrawer title={t('proj.addMember', '添加成员')} open={open} onCancel={onClose} footer={null}>
      <Form
        form={form}
        layout="vertical"
        initialValues={{ role: 'MEMBER' }}
        onFinish={async (v) => {
          setBusy(true)
          try {
            await api.addProjectMember(projectId, { userId: v.userId, role: v.role })
            message.success(t('proj.added', '已添加'))
            onDone()
          } catch (e) {
            message.error(e instanceof ApiError ? e.message : t('proj.addFailed', '添加失败'))
          } finally {
            setBusy(false)
          }
        }}
      >
        <Form.Item name="userId" label={t('proj.selectUser', '选择用户')} rules={[{ required: true }]}>
          <Select
            showSearch
            optionFilterProp="label"
            placeholder={t('proj.searchMember', '通过姓名/邮箱搜索')}
            options={candidates.map((u) => ({ value: u.id, label: `${u.name} (${u.email})` }))}
            notFoundContent={t('common.empty', '暂无数据')}
          />
        </Form.Item>
        <Form.Item name="role" label={t('proj.role', '角色')}>
          <Select
            options={[
              { value: 'MEMBER', label: t('proj.projMember', '项目成员') },
              { value: 'OWNER', label: t('proj.roleOwner', '拥有者') },
            ]}
          />
        </Form.Item>
        <Button type="primary" htmlType="submit" block loading={busy}>{t('proj.addMember', '添加成员')}</Button>
      </Form>
    </EditDrawer>
  )
}

// User groups: table (name + member count) → row click opens the permission matrix drawer. Member count is
// computed from the user table's userGroups (matches either group name or group id).
function UserGroups({ t }: { t: TFn }) {
  const [roles, setRoles] = useState<Role[]>([])
  const [users, setUsers] = useState<User[]>([])
  const [loading, setLoading] = useState(false)
  const [sel, setSel] = useState<Role | null>(null)
  const [createOpen, setCreateOpen] = useState(false)
  const load = useCallback(() => {
    setLoading(true)
    Promise.all([api.roles(), api.users().catch(() => ({ items: [] as User[] }))])
      .then(([rp, up]) => { setRoles(rp.items ?? []); setUsers(up.items ?? []) })
      .catch(() => { setRoles([]); setUsers([]) })
      .finally(() => setLoading(false))
  }, [])
  useEffect(load, [load])
  const memberCount = useCallback(
    (r: Role) => users.filter((u) => (u.userGroups ?? []).some((g) => g === r.name || g === r.id)).length,
    [users],
  )
  const cols: ColumnsType<Role> = [
    {
      title: t('proj.groupName', '用户组名称'), dataIndex: 'name',
      render: (v: string, r) => (
        <span>
          {v}
          {r.scope === 'SYSTEM' && <span style={{ color: 'var(--text-3)', fontSize: 12, marginLeft: 6 }}>{t('pa.sysBuiltin', '(系统内置)')}</span>}
        </span>
      ),
    },
    {
      title: t('proj.memberCount', '成员数'), width: 120,
      render: (_v, r) => <span style={{ color: 'var(--brand)', cursor: 'pointer' }}>{memberCount(r)}</span>,
    },
  ]
  return (
    <Card size="small" styles={{ body: { padding: 12 } }}>
      <div style={{ marginBottom: 12 }}><Button type="primary" onClick={() => setCreateOpen(true)}>{t('proj.addGroup', '添加用户组')}</Button></div>
      <Table<Role>
        rowKey="id"
        size="middle"
        loading={loading}
        dataSource={roles}
        columns={cols}
        pagination={false}
        onRow={(r) => ({ onClick: () => setSel(r), style: { cursor: 'pointer' } })}
      />
      <PermissionDrawer role={sel} onClose={() => setSel(null)} onSaved={() => { setSel(null); load() }} t={t} />
      <AddGroupModal open={createOpen} onClose={() => setCreateOpen(false)} onDone={() => { setCreateOpen(false); load() }} t={t} />
    </Card>
  )
}

// New user group within a project: PROJECT scope is fixed; permissions are edited in the row-click matrix drawer.
function AddGroupModal({ open, onClose, onDone, t }: { open: boolean; onClose: () => void; onDone: () => void; t: TFn }) {
  const [form] = Form.useForm<{ name: string }>()
  const [busy, setBusy] = useState(false)
  useEffect(() => { if (open) form.resetFields() }, [open, form])
  return (
    <EditDrawer title={t('proj.addGroup', '添加用户组')} open={open} onCancel={onClose} footer={null}>
      <Form
        form={form}
        layout="vertical"
        onFinish={async (v) => {
          setBusy(true)
          try {
            await api.createRole({ name: v.name.trim(), scope: 'PROJECT', permissions: [] })
            message.success(t('ug.created', '用户组已创建'))
            onDone()
          } catch (e) {
            message.error(e instanceof ApiError ? e.message : t('ug.createFailed', '创建失败'))
          } finally {
            setBusy(false)
          }
        }}
      >
        <Form.Item name="name" label={t('proj.groupName', '用户组名称')} rules={[{ required: true }]}>
          <Input placeholder={t('ug.groupNamePlaceholder', '如:接口测试工程师')} autoFocus />
        </Form.Item>
        <Button type="primary" htmlType="submit" block loading={busy}>{t('a.create', '创建')}</Button>
      </Form>
    </EditDrawer>
  )
}

// Permission drawer: Role.permissions ("RES:ACT+ACT") ⇄ checkbox matrix; system built-in groups are read-only; saving goes through updateRole.
function PermissionDrawer({ role, onClose, onSaved, t }: { role: Role | null; onClose: () => void; onSaved: () => void; t: TFn }) {
  const readOnly = role?.scope === 'SYSTEM'
  const [checked, setChecked] = useState<Set<string>>(new Set())
  const [extras, setExtras] = useState<string[]>([])
  const [busy, setBusy] = useState(false)
  useEffect(() => {
    if (role) {
      const p = parsePermissions(role.permissions)
      setChecked(p.checked)
      setExtras(p.extras)
    }
  }, [role])
  const save = async () => {
    if (!role) return
    setBusy(true)
    try {
      await api.updateRole(role.id, { name: role.name, permissions: serializePermissions(checked, extras) })
      message.success(t('ug.saved', '已保存'))
      onSaved()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('ug.saveFailed', '保存失败'))
    } finally {
      setBusy(false)
    }
  }
  return (
    <ResizableDrawer
      open={!!role}
      onClose={onClose}
      width="55%"
      title={role?.name}
      destroyOnHidden
      footer={readOnly ? null : (
        <Space>
          <Button type="primary" loading={busy} onClick={save}>{t('a.save', '保存')}</Button>
          <Button onClick={onClose}>{t('a.cancel', '取消')}</Button>
        </Space>
      )}
    >
      <Alert
        type="info"
        closable
        showIcon
        style={{ marginBottom: 12 }}
        message={t('pa.reloginTip', '修改权限后,已登录的会话需要重新登录才会生效')}
      />
      <PermissionMatrix checked={checked} onChange={setChecked} disabled={readOnly} />
    </ResizableDrawer>
  )
}
