import { useState, type ReactNode } from 'react'
import { Layout, Menu, Select, Button, Space, Tooltip, Drawer, Avatar, Descriptions } from 'antd'
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
  BellOutlined,
  SettingOutlined,
} from '@ant-design/icons'
import { useLocation, useNavigate } from 'react-router-dom'
import { useApp } from '../context'
import { useI18n } from '../i18n'
import NewProjectModal from './NewProjectModal'

const { Content } = Layout

// 一级模块 = 左侧全局图标导航项(对齐参考图:图标在上、文字在下)+ 它专属的二级菜单。
// 全局栏只切模块,二级栏随选中模块收敛成该模块子项。系统项底部固定。
// label 用 [i18n key, 中文兜底];未登记字典也能正常显示。
interface ModuleDef {
  key: string
  label: [string, string]
  icon: ReactNode
  match: string[]
  bottom?: boolean // 底部固定(系统)
  children: { key: string; icon: ReactNode; label: [string, string] }[]
}

const MODULES: ModuleDef[] = [
  {
    key: '/home',
    label: ['nav.home', '首页'],
    icon: <DashboardOutlined />,
    match: ['/home'],
    children: [{ key: '/home', icon: <DashboardOutlined />, label: ['m.home', '工作台'] }],
  },
  {
    key: '/project',
    label: ['nav.project', '项目'],
    icon: <ProjectOutlined />,
    match: ['/project'],
    children: [{ key: '/project', icon: <ProjectOutlined />, label: ['m.proj', '项目'] }],
  },
  {
    key: '/test-plan',
    label: ['nav.plan', '计划'],
    icon: <ScheduleOutlined />,
    match: ['/test-plan', '/perf', '/environment', '/resource-pool'],
    children: [
      { key: '/test-plan', icon: <ScheduleOutlined />, label: ['m.plan', '测试计划'] },
      { key: '/perf', icon: <ThunderboltOutlined />, label: ['m.perf', '性能测试'] },
      { key: '/environment', icon: <CloudServerOutlined />, label: ['m.environment', '环境'] },
      { key: '/resource-pool', icon: <DatabaseOutlined />, label: ['m.pool', '资源池'] },
    ],
  },
  {
    key: '/functional-case',
    label: ['nav.case', '用例'],
    icon: <ProfileOutlined />,
    match: ['/functional-case'],
    children: [{ key: '/functional-case', icon: <ProfileOutlined />, label: ['m.functional', '功能用例'] }],
  },
  {
    key: '/api/definition',
    label: ['nav.api', '接口'],
    icon: <ApiOutlined />,
    match: ['/api/'],
    children: [
      { key: '/api/definition', icon: <ApiOutlined />, label: ['m.definition', '接口定义'] },
      { key: '/api/scenario', icon: <PartitionOutlined />, label: ['m.scenario', '场景用例'] },
    ],
  },
  {
    key: '/bug',
    label: ['nav.bug', '缺陷'],
    icon: <BugOutlined />,
    match: ['/bug', '/requirement', '/skill'],
    children: [
      { key: '/bug', icon: <BugOutlined />, label: ['m.bug', '缺陷管理'] },
      { key: '/requirement', icon: <FileTextOutlined />, label: ['m.requirement', '需求'] },
      { key: '/skill', icon: <BulbOutlined />, label: ['m.skill', '技能'] },
    ],
  },
  {
    key: '/organization',
    label: ['nav.sys', '系统'],
    icon: <SettingOutlined />,
    match: ['/organization', '/role', '/user', '/mcp'],
    bottom: true,
    children: [
      { key: '/organization', icon: <ClusterOutlined />, label: ['m.org', '组织'] },
      { key: '/role', icon: <SafetyOutlined />, label: ['m.role', '角色'] },
      { key: '/user', icon: <UserOutlined />, label: ['m.user', '用户'] },
      { key: '/mcp', icon: <TeamOutlined />, label: ['m.mcp', 'MCP'] },
    ],
  },
]

export default function AppShell({ children }: { children: ReactNode }) {
  const { projects, projectId, setProjectId, logout } = useApp()
  const { t, lang, setLang } = useI18n()
  const nav = useNavigate()
  const loc = useLocation()
  const [newProjOpen, setNewProjOpen] = useState(false)
  const [pcOpen, setPcOpen] = useState(false)
  const currentProject = projects.find((p) => p.id === projectId)
  const username = localStorage.getItem('shepherd.user') || 'admin'

  // 当前所在模块由路由推断;二级栏与面包屑都跟着它走。
  const activeModule = MODULES.find((m) => m.match.some((x) => loc.pathname.startsWith(x))) || MODULES[0]
  const currentChild = activeModule.children.find((c) => c.key === loc.pathname) || activeModule.children[0]
  const topModules = MODULES.filter((m) => !m.bottom)
  const sysModule = MODULES.find((m) => m.bottom)

  // 全局栏导航项:图标在上、文字在下;选中态绿色(对齐参考图 #40)。
  const RailItem = ({ m }: { m: ModuleDef }) => {
    const active = m.key === activeModule.key
    return (
      <Tooltip title={t(...m.label)} placement="right">
        <div
          onClick={() => nav(m.key)}
          style={{
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'center',
            gap: 3,
            margin: '2px 8px',
            padding: '8px 2px',
            borderRadius: 8,
            cursor: 'pointer',
            fontSize: 12,
            lineHeight: 1.1,
            color: active ? '#06a561' : '#5b6470',
            background: active ? '#e6f7ef' : 'transparent',
          }}
        >
          <span style={{ fontSize: 18 }}>{m.icon}</span>
          <span>{t(...m.label)}</span>
        </div>
      </Tooltip>
    )
  }

  return (
    <Layout style={{ height: '100vh' }}>
      {/* hasSider:左侧为自定义 div(非 antd Sider),需显式声明横向布局,否则默认竖排。 */}
      <Layout hasSider>
        {/* 全局图标导航栏(对齐参考图 #40):logo / 导航项 / 底部系统 + 头像 */}
        <div style={{ width: 72, background: '#fff', borderRight: '1px solid #f0f0f0', display: 'flex', flexDirection: 'column', flexShrink: 0 }}>
          <div style={{ height: 48, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
            <DeploymentUnitOutlined style={{ color: '#7c3aed', fontSize: 22 }} />
          </div>
          <div style={{ flex: 1, overflowY: 'auto', paddingTop: 4 }}>
            {topModules.map((m) => <RailItem key={m.key} m={m} />)}
          </div>
          <div style={{ paddingBottom: 6 }}>
            {sysModule && <RailItem m={sysModule} />}
            <Tooltip title={t('pc.title', '个人中心')} placement="right">
              <div style={{ display: 'flex', justifyContent: 'center', padding: '10px 0', cursor: 'pointer' }} onClick={() => setPcOpen(true)}>
                <Avatar size={30} style={{ background: '#7c3aed' }}>{username.slice(0, 1).toUpperCase()}</Avatar>
              </div>
            </Tooltip>
          </div>
        </div>

        <Layout style={{ background: '#f5f6f8' }}>
          {/* 顶栏:当前模块的二级菜单(左,横向)+ 右上角图标簇(对齐参考图 #39)。 */}
          <div
            style={{
              height: 48,
              display: 'flex',
              alignItems: 'center',
              paddingInline: 16,
              gap: 8,
              background: '#fff',
              borderBottom: '1px solid #f0f0f0',
              flexShrink: 0,
            }}
          >
            <Menu
              mode="horizontal"
              selectedKeys={[currentChild.key]}
              items={activeModule.children.map((c) => ({ key: c.key, icon: c.icon, label: t(...c.label) }))}
              onClick={(e) => nav(e.key)}
              style={{ flex: 1, minWidth: 0, borderBottom: 'none', background: 'transparent' }}
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
              <Button type="text" size="small" icon={<PlusOutlined />} onClick={() => setNewProjOpen(true)} />
            </Tooltip>
            <Tooltip title={t('top.notifications', '通知')}>
              <Button type="text" size="small" icon={<BellOutlined />} />
            </Tooltip>
            <Tooltip title={t('m.home', '工作台')}>
              <Button type="text" size="small" icon={<AppstoreOutlined />} onClick={() => nav('/home')} />
            </Tooltip>
            <Select
              size="small"
              value={lang}
              onChange={setLang}
              style={{ width: 92 }}
              suffixIcon={<GlobalOutlined />}
              options={[
                { value: 'zh', label: '中文' },
                { value: 'en', label: 'English' },
              ]}
            />
            <Tooltip title={t('top.logout')}>
              <Button type="text" size="small" icon={<LogoutOutlined />} onClick={logout} />
            </Tooltip>
          </div>
          <Content style={{ overflow: 'hidden' }}>{children}</Content>
        </Layout>
      </Layout>
      <NewProjectModal open={newProjOpen} onClose={() => setNewProjOpen(false)} />

      {/* 个人中心(后端暂无 /me,展示登录态可得信息) */}
      <Drawer title={t('pc.title', '个人中心')} open={pcOpen} onClose={() => setPcOpen(false)} width={420}>
        <Space direction="vertical" size={16} style={{ width: '100%' }}>
          <Space align="center" size={12}>
            <Avatar size={48} style={{ background: '#7c3aed' }}>{username.slice(0, 1).toUpperCase()}</Avatar>
            <span style={{ fontSize: 16, fontWeight: 600 }}>{username}</span>
          </Space>
          <Descriptions column={1} size="small" bordered>
            <Descriptions.Item label={t('pc.username', '用户名')}>{username}</Descriptions.Item>
            <Descriptions.Item label={t('pc.project', '当前项目')}>{currentProject?.name || '—'}</Descriptions.Item>
            <Descriptions.Item label={t('pc.projectCount', '可见项目数')}>{projects.length}</Descriptions.Item>
            <Descriptions.Item label={t('pc.lang', '语言')}>
              <Select
                size="small"
                value={lang}
                onChange={setLang}
                style={{ width: 120 }}
                options={[
                  { value: 'zh', label: '中文' },
                  { value: 'en', label: 'English' },
                ]}
              />
            </Descriptions.Item>
          </Descriptions>
          <Button danger icon={<LogoutOutlined />} onClick={logout}>{t('top.logout', '退出登录')}</Button>
        </Space>
      </Drawer>
    </Layout>
  )
}
