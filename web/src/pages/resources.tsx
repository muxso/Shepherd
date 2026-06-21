import { Empty, Space, Table, Tag, Typography } from 'antd'
import { ToolOutlined } from '@ant-design/icons'
import CrudResource, { type CrudConfig } from '../components/CrudResource'
import { api, type Environment, type FunctionalCase, type Organization, type ResourcePool, type Role, type User } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'

type TFn = ReturnType<typeof useI18n>['t']

const mono = (v: string) => <span className="ms-mono" style={{ fontSize: 12, color: '#8a9099' }}>{v}</span>

// —— 系统管理 ——
const makeOrgCfg = (t: TFn): CrudConfig<Organization> => ({
  title: t('res.org', '组织'),
  list: () => api.organizations().then((p) => p.items),
  create: { fields: [{ name: 'name', label: t('res.orgName', '组织名'), required: true }], submit: (v) => api.createOrganization(String(v.name)) },
  columns: [
    { title: t('res.colName', '名称'), dataIndex: 'name' },
    { title: 'ID', dataIndex: 'id', render: mono },
  ],
})

const makeRoleCfg = (t: TFn): CrudConfig<Role> => ({
  title: t('res.role', '角色'),
  list: () => api.roles().then((p) => p.items),
  create: {
    fields: [
      { name: 'name', label: t('res.roleName', '角色名'), required: true },
      { name: 'permissions', label: t('res.permissionsComma', '权限(逗号分隔)'), placeholder: 'PROJECT:READ,PROJECT:ADD' },
    ],
    submit: (v) =>
      api.createRole({
        name: String(v.name),
        permissions: String(v.permissions || '')
          .split(',')
          .map((s) => s.trim())
          .filter(Boolean),
      }),
  },
  columns: [
    { title: t('res.colRole', '角色'), dataIndex: 'name' },
    { title: t('res.colPermissions', '权限'), dataIndex: 'permissions', render: (p?: string[]) => (p?.length ? p.map((x) => <Tag key={x}>{x}</Tag>) : '—') },
  ],
})

const makeUserCfg = (t: TFn): CrudConfig<User> => ({
  title: t('res.user', '用户'),
  list: () => api.users().then((p) => p.items),
  create: {
    fields: [
      { name: 'name', label: t('res.userName', '用户名'), required: true },
      { name: 'email', label: t('res.email', '邮箱'), required: true },
    ],
    submit: (v) => api.createUser({ name: String(v.name), email: String(v.email) }),
  },
  columns: [
    { title: t('res.colUserName', '用户名'), dataIndex: 'name' },
    { title: t('res.email', '邮箱'), dataIndex: 'email', render: mono },
    { title: t('res.colEnabled', '启用'), dataIndex: 'enable', render: (e?: boolean) => (e === false ? <Tag>{t('res.disabled', '停用')}</Tag> : <Tag color="green">{t('res.enabled', '启用')}</Tag>) },
  ],
})

// —— 测试资产 / 执行 ——
const makeFuncCaseCfg = (t: TFn): CrudConfig<FunctionalCase> => ({
  title: t('res.funcCase', '功能用例'),
  needsProject: true,
  list: ({ projectId }) => api.functionalCases(projectId),
  create: {
    fields: [
      { name: 'name', label: t('res.caseName', '用例名'), required: true },
      { name: 'module', label: t('res.colModule', '模块'), placeholder: t('res.optional', '可选') },
      {
        name: 'priority',
        label: t('res.colPriority', '优先级'),
        type: 'select',
        initial: 'P1',
        options: ['P0', 'P1', 'P2', 'P3'].map((p) => ({ value: p, label: p })),
      },
    ],
    submit: (v, { projectId }) =>
      api.createFunctionalCase({ projectId, name: String(v.name), priority: v.priority as string, module: v.module as string }),
  },
  columns: [
    { title: t('res.colName', '名称'), dataIndex: 'name' },
    { title: t('res.colModule', '模块'), dataIndex: 'module', render: (m?: string) => m || '—' },
    { title: t('res.colPriority', '优先级'), dataIndex: 'priority', width: 90, render: (p?: string) => <Tag color="blue">{p || 'P1'}</Tag> },
    { title: t('res.colStatus', '状态'), dataIndex: 'status', width: 120, render: (s?: string) => <Tag>{s || 'PREPARED'}</Tag> },
  ],
})

const makeEnvCfg = (t: TFn): CrudConfig<Environment> => ({
  title: t('res.env', '环境'),
  needsProject: true,
  list: ({ projectId }) => api.environments(projectId),
  create: {
    fields: [
      { name: 'name', label: t('res.envName', '环境名'), required: true },
      { name: 'baseUrl', label: 'Base URL', required: true, placeholder: 'http://127.0.0.1:9180' },
    ],
    submit: (v, { projectId }) => api.createEnvironment({ projectId, name: String(v.name), baseUrl: String(v.baseUrl) }),
  },
  columns: [
    { title: t('res.colEnv', '环境'), dataIndex: 'name' },
    { title: 'Base URL', dataIndex: 'baseUrl', render: mono },
    { title: t('res.colProtocols', '协议'), dataIndex: 'protocols', render: (p?: string[]) => (p?.length ? p.map((x) => <Tag key={x}>{x}</Tag>) : '—') },
  ],
})

const makePoolCfg = (t: TFn): CrudConfig<ResourcePool> => ({
  title: t('res.pool', '资源池'),
  list: () => api.resourcePools(),
  create: { fields: [{ name: 'name', label: t('res.poolName', '资源池名'), required: true }], submit: (v) => api.createResourcePool(String(v.name)) },
  columns: [
    { title: t('res.colName', '名称'), dataIndex: 'name' },
    { title: t('res.colEnabled', '启用'), dataIndex: 'enabled', width: 90, render: (e: boolean) => (e ? <Tag color="green">{t('res.enabled', '启用')}</Tag> : <Tag>{t('res.disabled', '停用')}</Tag>) },
    { title: 'ID', dataIndex: 'id', render: mono },
  ],
})

export const OrganizationsPage = () => { const { t } = useI18n(); return <CrudResource cfg={makeOrgCfg(t)} /> }
export const RolesPage = () => { const { t } = useI18n(); return <CrudResource cfg={makeRoleCfg(t)} /> }
export const UsersPage = () => { const { t } = useI18n(); return <CrudResource cfg={makeUserCfg(t)} /> }
export const FunctionalCasesPage = () => { const { t } = useI18n(); return <CrudResource cfg={makeFuncCaseCfg(t)} /> }
export const EnvironmentsPage = () => { const { t } = useI18n(); return <CrudResource cfg={makeEnvCfg(t)} /> }
export const ResourcePoolsPage = () => { const { t } = useI18n(); return <CrudResource cfg={makePoolCfg(t)} /> }

// 项目页:直接用 context 已加载的项目(无全局列表端点)。
export function ProjectsPage() {
  const { projects } = useApp()
  const { t } = useI18n()
  return (
    <div style={{ height: '100%', display: 'flex', flexDirection: 'column' }}>
      <div style={{ padding: '14px 16px', background: '#fff', borderBottom: '1px solid #f0f0f0' }}>
        <Typography.Text strong style={{ fontSize: 15 }}>
          {t('res.project', '项目')}
        </Typography.Text>
        <Typography.Text type="secondary" style={{ marginLeft: 8, fontSize: 12 }}>
          {t('res.newProjectHint', '新建项目在右上角「+」')}
        </Typography.Text>
      </div>
      <div style={{ flex: 1, overflow: 'auto', padding: 16 }}>
        <Table
          rowKey="id"
          size="middle"
          dataSource={projects}
          pagination={{ pageSize: 15, size: 'small' }}
          locale={{ emptyText: <Empty description={t('res.emptyProject', '暂无项目')} /> }}
          columns={[
            { title: t('res.projectName', '项目名'), dataIndex: 'name' },
            { title: t('res.org', '组织'), dataIndex: 'organizationId', render: mono },
            { title: t('res.colEnabled', '启用'), dataIndex: 'enable', width: 90, render: (e: boolean) => (e ? <Tag color="green">{t('res.enabled', '启用')}</Tag> : <Tag>{t('res.disabled', '停用')}</Tag>) },
          ]}
        />
      </div>
    </div>
  )
}

// 占位页:尚未接入的模块(下一波)。
export function Placeholder({ name }: { name: string }) {
  const { t } = useI18n()
  return (
    <div style={{ padding: 64, textAlign: 'center' }}>
      <Empty
        image={<ToolOutlined style={{ fontSize: 48, color: '#c9cdd4' }} />}
        description={
          <Space direction="vertical">
            <Typography.Text strong>{name}</Typography.Text>
            <Typography.Text type="secondary">{t('res.planning', '该模块规划中,下一波接入')}</Typography.Text>
          </Space>
        }
      />
    </div>
  )
}
