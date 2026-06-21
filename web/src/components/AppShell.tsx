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
import { useI18n } from '../i18n'
import NewProjectModal from './NewProjectModal'

const { Sider, Content } = Layout

// 顶部一级导航:点击跳到该域的着陆页;高亮由当前路由推断。
const TOP_NAV: { key: string; labelKey: string; match: string[] }[] = [
  { key: '/api/definition', labelKey: 'top.api', match: ['/api/', '/functional-case'] },
  { key: '/test-plan', labelKey: 'top.exec', match: ['/test-plan', '/perf', '/environment', '/resource-pool'] },
  { key: '/requirement', labelKey: 'top.orch', match: ['/requirement', '/bug', '/skill'] },
  { key: '/organization', labelKey: 'top.sys', match: ['/organization', '/role', '/user', '/project', '/mcp'] },
]

const SIDE_NAV = [
  {
    gkey: 'g.asset',
    children: [
      { key: '/api/definition', icon: <ApiOutlined />, lk: 'm.definition' },
      { key: '/api/scenario', icon: <PartitionOutlined />, lk: 'm.scenario' },
      { key: '/functional-case', icon: <ProfileOutlined />, lk: 'm.functional' },
    ],
  },
  {
    gkey: 'g.exec',
    children: [
      { key: '/test-plan', icon: <ScheduleOutlined />, lk: 'm.plan' },
      { key: '/perf', icon: <ThunderboltOutlined />, lk: 'm.perf' },
      { key: '/environment', icon: <CloudServerOutlined />, lk: 'm.environment' },
      { key: '/resource-pool', icon: <DatabaseOutlined />, lk: 'm.pool' },
    ],
  },
  {
    gkey: 'g.orch',
    children: [
      { key: '/requirement', icon: <FileTextOutlined />, lk: 'm.requirement' },
      { key: '/bug', icon: <BugOutlined />, lk: 'm.bug' },
      { key: '/skill', icon: <BulbOutlined />, lk: 'm.skill' },
    ],
  },
  {
    gkey: 'g.sys',
    children: [
      { key: '/organization', icon: <ClusterOutlined />, lk: 'm.org' },
      { key: '/role', icon: <SafetyOutlined />, lk: 'm.role' },
      { key: '/user', icon: <UserOutlined />, lk: 'm.user' },
      { key: '/project', icon: <ProjectOutlined />, lk: 'm.proj' },
      { key: '/mcp', icon: <TeamOutlined />, lk: 'm.mcp' },
    ],
  },
]

const SIDE_LABEL_KEY: Record<string, string> = {
  '/api/definition': 'm.definition',
  '/api/scenario': 'm.scenario',
  '/functional-case': 'm.functional',
  '/test-plan': 'm.plan',
  '/perf': 'm.perf',
  '/environment': 'm.environment',
  '/resource-pool': 'm.pool',
  '/requirement': 'm.requirement',
  '/bug': 'm.bug',
  '/skill': 'm.skill',
  '/organization': 'm.org',
  '/role': 'm.role',
  '/user': 'm.user',
  '/project': 'm.proj',
  '/mcp': 'm.mcp',
}

export default function AppShell({ children }: { children: ReactNode }) {
  const { projects, projectId, setProjectId, logout } = useApp()
  const { t, lang, setLang } = useI18n()
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
          selectedKeys={[TOP_NAV.find((tn) => tn.match.some((m) => loc.pathname.startsWith(m)))?.key || '/api/definition']}
          items={TOP_NAV.map((tn) => ({ key: tn.key, label: t(tn.labelKey) }))}
          onClick={(e) => nav(e.key)}
          style={{ flex: 1, background: 'transparent', borderBottom: 'none', minWidth: 0 }}
        />
        <Space size={8}>
          <Button size="small" type="text" style={{ color: '#fff' }} onClick={() => setLang(lang === 'zh' ? 'en' : 'zh')}>
            {lang === 'zh' ? 'EN' : '中'}
          </Button>
          <Select
            size="small"
            style={{ width: 200 }}
            value={projectId || undefined}
            placeholder={t('top.project')}
            onChange={setProjectId}
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
            items={SIDE_NAV.map((g) => ({
              key: g.gkey,
              label: t(g.gkey),
              type: 'group' as const,
              children: g.children.map((c) => ({ key: c.key, icon: c.icon, label: t(c.lk) })),
            }))}
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
                { title: <Space size={4}><AppstoreOutlined />{t('top.api')}</Space> },
                { title: t(SIDE_LABEL_KEY[loc.pathname] || 'm.definition') },
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
