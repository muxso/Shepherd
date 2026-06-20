import { Empty, Space, Table, Tag, Typography } from 'antd'
import { ToolOutlined } from '@ant-design/icons'
import CrudResource, { type CrudConfig } from '../components/CrudResource'
import { api, type Environment, type FunctionalCase, type Organization, type ResourcePool, type Role, type User } from '../api'
import { useApp } from '../context'

const mono = (v: string) => <span className="ms-mono" style={{ fontSize: 12, color: '#8a9099' }}>{v}</span>

// —— 系统管理 ——
const orgCfg: CrudConfig<Organization> = {
  title: '组织',
  list: () => api.organizations().then((p) => p.items),
  create: { fields: [{ name: 'name', label: '组织名', required: true }], submit: (v) => api.createOrganization(String(v.name)) },
  columns: [
    { title: '名称', dataIndex: 'name' },
    { title: 'ID', dataIndex: 'id', render: mono },
  ],
}

const roleCfg: CrudConfig<Role> = {
  title: '角色',
  list: () => api.roles().then((p) => p.items),
  create: {
    fields: [
      { name: 'name', label: '角色名', required: true },
      { name: 'permissions', label: '权限(逗号分隔)', placeholder: 'PROJECT:READ,PROJECT:ADD' },
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
    { title: '角色', dataIndex: 'name' },
    { title: '权限', dataIndex: 'permissions', render: (p?: string[]) => (p?.length ? p.map((x) => <Tag key={x}>{x}</Tag>) : '—') },
  ],
}

const userCfg: CrudConfig<User> = {
  title: '用户',
  list: () => api.users().then((p) => p.items),
  create: {
    fields: [
      { name: 'name', label: '用户名', required: true },
      { name: 'email', label: '邮箱', required: true },
    ],
    submit: (v) => api.createUser({ name: String(v.name), email: String(v.email) }),
  },
  columns: [
    { title: '用户名', dataIndex: 'name' },
    { title: '邮箱', dataIndex: 'email', render: mono },
    { title: '启用', dataIndex: 'enable', render: (e?: boolean) => (e === false ? <Tag>停用</Tag> : <Tag color="green">启用</Tag>) },
  ],
}

// —— 测试资产 / 执行 ——
const funcCaseCfg: CrudConfig<FunctionalCase> = {
  title: '功能用例',
  needsProject: true,
  list: ({ projectId }) => api.functionalCases(projectId),
  create: {
    fields: [
      { name: 'name', label: '用例名', required: true },
      { name: 'module', label: '模块', placeholder: '可选' },
      {
        name: 'priority',
        label: '优先级',
        type: 'select',
        initial: 'P1',
        options: ['P0', 'P1', 'P2', 'P3'].map((p) => ({ value: p, label: p })),
      },
    ],
    submit: (v, { projectId }) =>
      api.createFunctionalCase({ projectId, name: String(v.name), priority: v.priority as string, module: v.module as string }),
  },
  columns: [
    { title: '名称', dataIndex: 'name' },
    { title: '模块', dataIndex: 'module', render: (m?: string) => m || '—' },
    { title: '优先级', dataIndex: 'priority', width: 90, render: (p?: string) => <Tag color="blue">{p || 'P1'}</Tag> },
    { title: '状态', dataIndex: 'status', width: 120, render: (s?: string) => <Tag>{s || 'PREPARED'}</Tag> },
  ],
}

const envCfg: CrudConfig<Environment> = {
  title: '环境',
  needsProject: true,
  list: ({ projectId }) => api.environments(projectId),
  create: {
    fields: [
      { name: 'name', label: '环境名', required: true },
      { name: 'baseUrl', label: 'Base URL', required: true, placeholder: 'http://127.0.0.1:9180' },
    ],
    submit: (v, { projectId }) => api.createEnvironment({ projectId, name: String(v.name), baseUrl: String(v.baseUrl) }),
  },
  columns: [
    { title: '环境', dataIndex: 'name' },
    { title: 'Base URL', dataIndex: 'baseUrl', render: mono },
    { title: '协议', dataIndex: 'protocols', render: (p?: string[]) => (p?.length ? p.map((x) => <Tag key={x}>{x}</Tag>) : '—') },
  ],
}

const poolCfg: CrudConfig<ResourcePool> = {
  title: '资源池',
  list: () => api.resourcePools(),
  create: { fields: [{ name: 'name', label: '资源池名', required: true }], submit: (v) => api.createResourcePool(String(v.name)) },
  columns: [
    { title: '名称', dataIndex: 'name' },
    { title: '启用', dataIndex: 'enabled', width: 90, render: (e: boolean) => (e ? <Tag color="green">启用</Tag> : <Tag>停用</Tag>) },
    { title: 'ID', dataIndex: 'id', render: mono },
  ],
}

export const OrganizationsPage = () => <CrudResource cfg={orgCfg} />
export const RolesPage = () => <CrudResource cfg={roleCfg} />
export const UsersPage = () => <CrudResource cfg={userCfg} />
export const FunctionalCasesPage = () => <CrudResource cfg={funcCaseCfg} />
export const EnvironmentsPage = () => <CrudResource cfg={envCfg} />
export const ResourcePoolsPage = () => <CrudResource cfg={poolCfg} />

// 项目页:直接用 context 已加载的项目(无全局列表端点)。
export function ProjectsPage() {
  const { projects } = useApp()
  return (
    <div style={{ height: '100%', display: 'flex', flexDirection: 'column' }}>
      <div style={{ padding: '14px 16px', background: '#fff', borderBottom: '1px solid #f0f0f0' }}>
        <Typography.Text strong style={{ fontSize: 15 }}>
          项目
        </Typography.Text>
        <Typography.Text type="secondary" style={{ marginLeft: 8, fontSize: 12 }}>
          新建项目在右上角「+」
        </Typography.Text>
      </div>
      <div style={{ flex: 1, overflow: 'auto', padding: 16 }}>
        <Table
          rowKey="id"
          size="middle"
          dataSource={projects}
          pagination={{ pageSize: 15, size: 'small' }}
          locale={{ emptyText: <Empty description="暂无项目" /> }}
          columns={[
            { title: '项目名', dataIndex: 'name' },
            { title: '组织', dataIndex: 'organizationId', render: mono },
            { title: '启用', dataIndex: 'enable', width: 90, render: (e: boolean) => (e ? <Tag color="green">启用</Tag> : <Tag>停用</Tag>) },
          ]}
        />
      </div>
    </div>
  )
}

// 占位页:尚未接入的模块(下一波)。
export function Placeholder({ name }: { name: string }) {
  return (
    <div style={{ padding: 64, textAlign: 'center' }}>
      <Empty
        image={<ToolOutlined style={{ fontSize: 48, color: '#c9cdd4' }} />}
        description={
          <Space direction="vertical">
            <Typography.Text strong>{name}</Typography.Text>
            <Typography.Text type="secondary">该模块规划中,下一波接入</Typography.Text>
          </Space>
        }
      />
    </div>
  )
}
