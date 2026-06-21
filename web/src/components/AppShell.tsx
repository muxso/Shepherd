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
  GlobalOutlined,
  DashboardOutlined,
} from '@ant-design/icons'
import { useLocation, useNavigate } from 'react-router-dom'
import { useApp } from '../context'
import { useI18n } from '../i18n'
import NewProjectModal from './NewProjectModal'

const { Sider, Content } = Layout

// 一级模块 = 顶栏导航项 + 它专属的左侧二级菜单(单一数据源)。
// 顶栏只切模块,左栏随选中模块收敛成该模块的子项,避免顶/侧信息重复。
// label 全走 i18n 字典 key,未登记的回落原文。
interface ModuleDef {
  key: string
  labelKey: string
  match: string[]
  groupKey: string
  children: { key: string; icon: ReactNode; labelKey: string }[]
}

const MODULES: ModuleDef[] = [
  {
    key: '/home',
    labelKey: 'top.home',
    match: ['/home'],
    groupKey: 'g.home',
    children: [{ key: '/home', icon: <DashboardOutlined />, labelKey: 'm.home' }],
  },
  {
    key: '/api/definition',
    labelKey: 'top.api',
    match: ['/api/', '/functional-case'],
    groupKey: 'g.asset',
    children: [
      { key: '/api/definition', icon: <ApiOutlined />, labelKey: 'm.definition' },
      { key: '/api/scenario', icon: <PartitionOutlined />, labelKey: 'm.scenario' },
      { key: '/functional-case', icon: <ProfileOutlined />, labelKey: 'm.functional' },
    ],
  },
  {
    key: '/test-plan',
    labelKey: 'top.exec',
    match: ['/test-plan', '/perf', '/environment', '/resource-pool'],
    groupKey: 'g.exec',
    children: [
      { key: '/test-plan', icon: <ScheduleOutlined />, labelKey: 'm.plan' },
      { key: '/perf', icon: <ThunderboltOutlined />, labelKey: 'm.perf' },
      { key: '/environment', icon: <CloudServerOutlined />, labelKey: 'm.environment' },
      { key: '/resource-pool', icon: <DatabaseOutlined />, labelKey: 'm.pool' },
    ],
  },
  {
    key: '/requirement',
    labelKey: 'top.orch',
    match: ['/requirement', '/bug', '/skill'],
    groupKey: 'g.orch',
    children: [
      { key: '/requirement', icon: <FileTextOutlined />, labelKey: 'm.requirement' },
      { key: '/bug', icon: <BugOutlined />, labelKey: 'm.bug' },
      { key: '/skill', icon: <BulbOutlined />, labelKey: 'm.skill' },
    ],
  },
  {
    key: '/organization',
    labelKey: 'top.sys',
    match: ['/organization', '/role', '/user', '/project', '/mcp'],
    groupKey: 'g.sys',
    children: [
      { key: '/organization', icon: <ClusterOutlined />, labelKey: 'm.org' },
      { key: '/role', icon: <SafetyOutlined />, labelKey: 'm.role' },
      { key: '/user', icon: <UserOutlined />, labelKey: 'm.user' },
      { key: '/project', icon: <ProjectOutlined />, labelKey: 'm.proj' },
      { key: '/mcp', icon: <TeamOutlined />, labelKey: 'm.mcp' },
    ],
  },
]

export default function AppShell({ children }: { children: ReactNode }) {
  const { projects, projectId, setProjectId, logout } = useApp()
  const { t, lang, setLang } = useI18n()
  const nav = useNavigate()
  const loc = useLocation()
  const [newProjOpen, setNewProjOpen] = useState(false)
  const currentProject = projects.find((p) => p.id === projectId)

  // 当前所在模块由路由推断;左栏与面包屑都跟着它走。
  const activeModule = MODULES.find((m) => m.match.some((x) => loc.pathname.startsWith(x))) || MODULES[0]
  const currentChild = activeModule.children.find((c) => c.key === loc.pathname) || activeModule.children[0]

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
          selectedKeys={[activeModule.key]}
          items={MODULES.map((m) => ({ key: m.key, label: t(m.labelKey) }))}
          onClick={(e) => nav(e.key)}
          style={{ flex: 1, background: 'transparent', borderBottom: 'none', minWidth: 0 }}
        />
        <Space size={8}>
          <Select
            size="small"
            value={lang}
            onChange={setLang}
            style={{ width: 92 }}
            suffixIcon={<GlobalOutlined style={{ color: '#fff' }} />}
            options={[
              { value: 'zh', label: '中文' },
              { value: 'en', label: 'English' },
            ]}
          />
          <Select
            size="small"
            style={{ width: 200 }}
            value={projectId || undefined}
            placeholder={t('top.project')}
            onChange={setProjectId}
            showSearch
            optionFilterProp="label"
            options={projects.map((p) => ({ value: p.id, label: p.name }))}
            notFoundContent={t('common.empty')}
          />
          <Tooltip title={t('top.newProject')}>
            <Button type="text" size="small" icon={<PlusOutlined />} style={{ color: '#fff' }} onClick={() => setNewProjOpen(true)} />
          </Tooltip>
          <Tooltip title={t('top.logout')}>
            <Button type="text" size="small" icon={<LogoutOutlined />} style={{ color: '#fff' }} onClick={logout} />
          </Tooltip>
        </Space>
      </div>

      <Layout>
        <Sider width={184} theme="light" style={{ borderRight: '1px solid #f0f0f0', overflow: 'auto' }}>
          <Menu
            mode="inline"
            selectedKeys={[loc.pathname]}
            items={[
              {
                key: activeModule.groupKey,
                label: t(activeModule.groupKey),
                type: 'group' as const,
                children: activeModule.children.map((c) => ({ key: c.key, icon: c.icon, label: t(c.labelKey) })),
              },
            ]}
            onClick={(e) => nav(e.key)}
            style={{ height: '100%', borderInlineEnd: 'none', paddingTop: 4 }}
          />
        </Sider>
        <Layout style={{ background: '#f5f6f8' }}>
          {/* 面包屑:模块 / 当前页 / 项目,均随路由真实变化 */}
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
                { title: <Space size={4}><AppstoreOutlined />{t(activeModule.labelKey)}</Space> },
                { title: t(currentChild.labelKey) },
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
