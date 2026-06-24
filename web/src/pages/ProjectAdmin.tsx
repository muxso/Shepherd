import { useEffect, useMemo, useState } from 'react'
import { Button, Card, Drawer, Empty, Input, Switch, Table, Tag } from 'antd'
import { FolderOpenOutlined, SearchOutlined } from '@ant-design/icons'
import type { ColumnsType } from 'antd/es/table'
import { api, userStore, type Organization, type Role, type User } from '../api'
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
                  background: nav === it.key ? '#e6f7ef' : 'transparent',
                  color: nav === it.key ? 'var(--brand)' : '#1f2329',
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
        {nav === 'members' && <Members t={t} />}
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

function Members({ t }: { t: TFn }) {
  const [users, setUsers] = useState<User[]>([])
  const [q, setQ] = useState('')
  const [loading, setLoading] = useState(false)
  useEffect(() => {
    setLoading(true)
    api.users().then((p) => setUsers(p.items ?? [])).catch(() => setUsers([])).finally(() => setLoading(false))
  }, [])
  const rows = users.filter((u) => !q || u.name.toLowerCase().includes(q.toLowerCase()) || u.email.toLowerCase().includes(q.toLowerCase()))
  const cols: ColumnsType<User> = [
    { title: t('proj.username', '用户名'), dataIndex: 'email', width: 220 },
    { title: t('proj.realname', '姓名'), dataIndex: 'name', width: 160 },
    { title: t('proj.email', '邮箱'), dataIndex: 'email' },
    { title: t('proj.phone', '手机'), width: 120, render: () => <span style={{ color: 'var(--text-3)' }}>—</span> },
    { title: t('proj.userGroup', '用户组'), width: 120, render: () => <Tag color="green">{t('proj.projMember', '项目成员')}</Tag> },
    { title: t('proj.status', '状态'), width: 90, render: (_v, u) => <Tag color={u.enable === false ? 'default' : 'green'}>{u.enable === false ? t('proj.disabled', '禁用') : t('proj.normal', '正常')}</Tag> },
    { title: t('apidef.colAction', '操作'), width: 90, render: () => <Button type="link" size="small" danger>{t('proj.remove', '移除')}</Button> },
  ]
  return (
    <Card size="small" styles={{ body: { padding: 12 } }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
        <Button type="primary">{t('proj.addMember', '添加成员')}</Button>
        <Button>{t('proj.inviteEmail', '邮箱邀请')}</Button>
        <div style={{ flex: 1 }} />
        <Input allowClear prefix={<SearchOutlined style={{ color: 'var(--text-3)' }} />} placeholder={t('proj.searchMember', '通过姓名/邮箱/手机搜索')} style={{ width: 260 }} value={q} onChange={(e) => setQ(e.target.value)} />
      </div>
      <Table<User> rowKey="id" size="middle" loading={loading} dataSource={rows} columns={cols} pagination={{ pageSize: 20, size: 'small', showTotal: (n) => `${t('apidef.totalPrefix', '共')} ${n} ${t('proj.unit', '条')}` }} />
    </Card>
  )
}

function UserGroups({ t }: { t: TFn }) {
  const [roles, setRoles] = useState<Role[]>([])
  const [loading, setLoading] = useState(false)
  const [sel, setSel] = useState<Role | null>(null)
  useEffect(() => {
    setLoading(true)
    api.roles().then((p) => setRoles(p.items ?? [])).catch(() => setRoles([])).finally(() => setLoading(false))
  }, [])
  const cols: ColumnsType<Role> = [
    { title: t('proj.groupName', '用户组名称'), dataIndex: 'name' },
    { title: t('proj.memberCount', '成员数'), width: 120, render: () => <span style={{ color: 'var(--brand)' }}>—</span> },
    { title: t('apidef.colAction', '操作'), width: 120, render: (_v, r) => <Button type="link" size="small" onClick={() => setSel(r)}>{t('proj.viewPerm', '查看权限')}</Button> },
  ]
  return (
    <Card size="small" styles={{ body: { padding: 12 } }}>
      <div style={{ marginBottom: 12 }}><Button type="primary">{t('proj.addGroup', '添加用户组')}</Button></div>
      <Table<Role> rowKey="id" size="middle" loading={loading} dataSource={roles} columns={cols} pagination={false} />
      <PermissionDrawer role={sel} onClose={() => setSel(null)} t={t} />
    </Card>
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
