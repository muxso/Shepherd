import { useState, type ReactNode } from 'react'
import { Layout, Menu, Select, Button, Space, Tooltip, Breadcrumb } from 'antd'
import {
  ApiOutlined,
  PartitionOutlined,
  LogoutOutlined,
  DeploymentUnitOutlined,
  PlusOutlined,
  AppstoreOutlined,
  ProfileOutlined,
  BugOutlined,
  BulbOutlined,
  ClusterOutlined,
  ThunderboltOutlined,
  ScheduleOutlined,
  DatabaseOutlined,
  CloudServerOutlined,
  TeamOutlined,
  SafetyOutlined,
  UserOutlined,
  ProjectOutlined,
  FileTextOutlined,
} from '@ant-design/icons'
import { useLocation, useNavigate } from 'react-router-dom'
import { useApp } from '../context'
import NewProjectModal from './NewProjectModal'

const { Sider, Content } = Layout

// 顶部一级导航:点击跳到该域的着陆页;高亮由当前路由推断。
const TOP_NAV: { key: string; label: string; match: string[] }[] = [
  { key: '/api/definition', label: '接口测试', match: ['/api/', '/functional-case'] },
  { key: '/test-plan', label: '计划与执行', match: ['/test-plan', '/perf', '/environment', '/resource-pool'] },
  { key: '/requirement', label: '需求与编排', match: ['/requirement', '/bug', '/skill'] },
  { key: '/organization', label: '系统管理', match: ['/organization', '/role', '/user', '/project', '/mcp'] },
]

const SIDE_NAV = [
  {
    key: 'g-asset',
    label: '测试资产',
    type: 'group' as const,
    children: [
      { key: '/api/definition', icon: <ApiOutlined />, label: '接口定义' },
      { key: '/api/scenario', icon: <PartitionOutlined />, label: '场景用例' },
      { key: '/functional-case', icon: <ProfileOutlined />, label: '功能用例' },
    ],
  },
  {
    key: 'g-exec',
    label: '计划与执行',
    type: 'group' as const,
    children: [
      { key: '/test-plan', icon: <ScheduleOutlined />, label: '测试计划' },
      { key: '/perf', icon: <ThunderboltOutlined />, label: '性能压测' },
      { key: '/environment', icon: <CloudServerOutlined />, label: '环境' },
      { key: '/resource-pool', icon: <DatabaseOutlined />, label: '资源池' },
    ],
  },
  {
    key: 'g-orch',
    label: '需求与编排',
    type: 'group' as const,
    children: [
      { key: '/requirement', icon: <FileTextOutlined />, label: '需求与编排' },
      { key: '/bug', icon: <BugOutlined />, label: '缺陷' },
      { key: '/skill', icon: <BulbOutlined />, label: '技能' },
    ],
  },
  {
    key: 'g-sys',
    label: '系统管理',
    type: 'group' as const,
    children: [
      { key: '/organization', icon: <ClusterOutlined />, label: '组织' },
      { key: '/role', icon: <SafetyOutlined />, label: '角色' },
      { key: '/user', icon: <UserOutlined />, label: '用户' },
      { key: '/project', icon: <ProjectOutlined />, label: '项目' },
      { key: '/mcp', icon: <TeamOutlined />, label: 'MCP 工具' },
    ],
  },
]

const SIDE_LABEL: Record<string, string> = {
  '/api/definition': '接口定义',
  '/api/scenario': '场景用例',
  '/functional-case': '功能用例',
  '/test-plan': '测试计划',
  '/perf': '性能压测',
  '/environment': '环境',
  '/resource-pool': '资源池',
  '/requirement': '需求与编排',
  '/bug': '缺陷',
  '/skill': '技能',
  '/organization': '组织',
  '/role': '角色',
  '/user': '用户',
  '/project': '项目',
  '/mcp': 'MCP 工具',
}

export default function AppShell({ children }: { children: ReactNode }) {
  const { projects, projectId, setProjectId, logout } = useApp()
  const nav = useNavigate()
  const loc = useLocation()
  const [newProjOpen, setNewProjOpen] = useState(false)
  const currentProject = projects.find((p) => p.id === projectId)

  return (
    <Layout style={{ height: '100vh' }}>
      {/* 全局顶栏 */}
      <div
        style={{
          height: 48,
          display: 'flex',
          alignItems: 'center',
          background: '#1d2129',
          paddingInline: 16,
          gap: 28,
          flexShrink: 0,
        }}
      >
        <Space style={{ color: '#fff', fontSize: 16, fontWeight: 700 }} size={6}>
          <DeploymentUnitOutlined style={{ color: '#7c3aed', fontSize: 18 }} />
          Shepherd
        </Space>
        <Menu
          mode="horizontal"
          theme="dark"
          selectedKeys={[TOP_NAV.find((t) => t.match.some((m) => loc.pathname.startsWith(m)))?.key || '/api/definition']}
          items={TOP_NAV.map((t) => ({ key: t.key, label: t.label }))}
          onClick={(e) => nav(e.key)}
          style={{ flex: 1, background: 'transparent', borderBottom: 'none', minWidth: 0 }}
        />
        <Space size={8}>
          <Select
            size="small"
            style={{ width: 200 }}
            value={projectId || undefined}
            placeholder="选择项目"
            onChange={setProjectId}
            options={projects.map((p) => ({ value: p.id, label: p.name }))}
            notFoundContent="暂无项目"
          />
          <Tooltip title="新建项目">
            <Button type="text" size="small" icon={<PlusOutlined />} style={{ color: '#fff' }} onClick={() => setNewProjOpen(true)} />
          </Tooltip>
          <Tooltip title="退出登录">
            <Button type="text" size="small" icon={<LogoutOutlined />} style={{ color: '#fff' }} onClick={logout} />
          </Tooltip>
        </Space>
      </div>

      <Layout>
        <Sider width={184} theme="light" style={{ borderRight: '1px solid #f0f0f0', overflow: 'auto' }}>
          <Menu
            mode="inline"
            selectedKeys={[loc.pathname]}
            items={SIDE_NAV}
            onClick={(e) => nav(e.key)}
            style={{ height: '100%', borderInlineEnd: 'none', paddingTop: 4 }}
          />
        </Sider>
        <Layout style={{ background: '#f5f6f8' }}>
          {/* 面包屑 */}
          <div
            style={{
              height: 38,
              display: 'flex',
              alignItems: 'center',
              paddingInline: 16,
              background: '#fff',
              borderBottom: '1px solid #f0f0f0',
              flexShrink: 0,
            }}
          >
            <Breadcrumb
              items={[
                { title: <Space size={4}><AppstoreOutlined />接口测试</Space> },
                { title: SIDE_LABEL[loc.pathname] || '接口定义' },
                ...(currentProject ? [{ title: <span style={{ color: '#7c3aed' }}>{currentProject.name}</span> }] : []),
              ]}
            />
            <div style={{ flex: 1 }} />
          </div>
          <Content style={{ overflow: 'hidden' }}>{children}</Content>
        </Layout>
      </Layout>
      <NewProjectModal open={newProjOpen} onClose={() => setNewProjOpen(false)} />
    </Layout>
  )
}
