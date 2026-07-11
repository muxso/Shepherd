import { useCallback, useEffect, useMemo, useState } from 'react'
import { Button, Card, Drawer, Empty, Form, Input, Modal, Popconfirm, Select, Switch, Table, Tag } from 'antd'
import { FolderOpenOutlined, SearchOutlined } from '@ant-design/icons'
import type { ColumnsType } from 'antd/es/table'
import { message } from '../feedback'
import { api, ApiError, userStore, type Organization, type ProjectMember, type Role, type User } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'
import { SelectProjectEmpty } from '../components/Page'

// 项目与权限:左侧二级导航(项目 / 成员权限)+ 右侧内容。对齐参考图 #44-#48。
type NavKey = 'basic' | 'appSettings' | 'members' | 'userGroups'

export default function ProjectAdmin() {
  const { t } = useI18n()
  const { projects, projectId } = useApp()
  const [nav, setNav] = useState<NavKey>('basic')
  const project = projects.find((p) => p.id === projectId)

  if (!projectId || !project) return <SelectProjectEmpty />

  const groups: { title: string; items: { key: NavKey; label: string }[] }[] = [
    { title: t('proj.grpProject', '项目'), items: [
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
      {/* 左侧二级导航 */}
      <div style={{ width: 200, flexShrink: 0, borderRight: '1px solid var(--border-soft)', padding: '12px 8px', overflow: 'auto', background: 'var(--panel)' }}>
        <div style={{ fontWeight: 600, fontSize: 13, padding: '4px 10px 8px' }}>{t('proj.permTitle', '项目与权限')}</div>
        {groups.map((g) => (
          <div key={g.title} style={{ marginBottom: 8 }}>
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
      {/* 右侧内容 */}
      <div style={{ flex: 1, minWidth: 0, overflow: 'auto', padding: 16, background: 'var(--bg)' }}>
        {nav === 'basic' && <BasicInfo project={project} t={t} />}
        {nav === 'appSettings' && <AppSettings t={t} />}
        {nav === 'members' && <Members t={t} projectId={projectId} />}
        {nav === 'userGroups' && <UserGroups t={t} />}
      </div>
    </div>
  )
}

type TFn = (k: string, d?: string) => string

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

// 应用设置:对齐参考图的菜单开关(当前为本地态,持久化待后端项目配置接口)。
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

// 项目成员:真实成员列表(/project/{id}/member),关联平台用户表补齐姓名/邮箱。
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
    <Modal title={t('proj.addMember', '添加成员')} open={open} onCancel={onClose} footer={null} destroyOnHidden>
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
    </Modal>
  )
}

function UserGroups({ t }: { t: TFn }) {
  const [roles, setRoles] = useState<Role[]>([])
  const [loading, setLoading] = useState(false)
  const [sel, setSel] = useState<Role | null>(null)
  const [createOpen, setCreateOpen] = useState(false)
  const load = useCallback(() => {
    setLoading(true)
    api.roles().then((p) => setRoles(p.items ?? [])).catch(() => setRoles([])).finally(() => setLoading(false))
  }, [])
  useEffect(load, [load])
  const cols: ColumnsType<Role> = [
    { title: t('proj.groupName', '用户组名称'), dataIndex: 'name' },
    { title: t('proj.memberCount', '成员数'), width: 120, render: () => <span style={{ color: 'var(--brand)' }}>—</span> },
    { title: t('apidef.colAction', '操作'), width: 120, render: (_v, r) => <Button type="link" size="small" onClick={() => setSel(r)}>{t('proj.viewPerm', '查看权限')}</Button> },
  ]
  return (
    <Card size="small" styles={{ body: { padding: 12 } }}>
      <div style={{ marginBottom: 12 }}><Button type="primary" onClick={() => setCreateOpen(true)}>{t('proj.addGroup', '添加用户组')}</Button></div>
      <Table<Role> rowKey="id" size="middle" loading={loading} dataSource={roles} columns={cols} pagination={false} />
      <PermissionDrawer role={sel} onClose={() => setSel(null)} t={t} />
      <AddGroupModal open={createOpen} onClose={() => setCreateOpen(false)} onDone={() => { setCreateOpen(false); load() }} t={t} />
    </Card>
  )
}

// 项目内新建用户组:固定 PROJECT 作用域;权限矩阵在「系统/用户组」里编辑。
function AddGroupModal({ open, onClose, onDone, t }: { open: boolean; onClose: () => void; onDone: () => void; t: TFn }) {
  const [form] = Form.useForm<{ name: string }>()
  const [busy, setBusy] = useState(false)
  useEffect(() => { if (open) form.resetFields() }, [open, form])
  return (
    <Modal title={t('proj.addGroup', '添加用户组')} open={open} onCancel={onClose} footer={null} destroyOnHidden>
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
    </Modal>
  )
}

// 权限抽屉:把角色 permissions(如 "API_DEFINITION:READ+ADD+UPDATE")解析为「资源 → 动作」矩阵。
function PermissionDrawer({ role, onClose, t }: { role: Role | null; onClose: () => void; t: TFn }) {
  const parsed = useMemo(() => {
    return (role?.permissions ?? []).map((p) => {
      const [res, actions] = p.split(':')
      return { res, actions: (actions ?? '').split('+').filter(Boolean) }
    })
  }, [role])
  return (
    <Drawer open={!!role} onClose={onClose} width={560} title={role?.name || t('proj.viewPerm', '查看权限')}>
      {parsed.length === 0 ? (
        <Empty description={t('common.empty', '暂无数据')} />
      ) : (
        <Table
          size="small"
          rowKey="res"
          pagination={false}
          dataSource={parsed}
          columns={[
            { title: t('proj.resource', '资源'), dataIndex: 'res', width: 220, render: (v: string) => <span className="ms-mono" style={{ fontSize: 12 }}>{v}</span> },
            { title: t('proj.permission', '权限'), dataIndex: 'actions', render: (a: string[]) => a.map((x) => <Tag key={x} color="green" style={{ marginBottom: 4 }}>{x}</Tag>) },
          ]}
        />
      )}
    </Drawer>
  )
}
