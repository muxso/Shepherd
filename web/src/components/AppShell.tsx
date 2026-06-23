import { useState, type ReactNode } from 'react'
import { Layout, Menu, Select, Button, Space, Tooltip, Drawer, Avatar, Descriptions, Segmented, Empty } from 'antd'
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
  SafetyOutlined,
  UserOutlined,
  ProjectOutlined,
  FileTextOutlined,
  GlobalOutlined,
  DashboardOutlined,
  BellOutlined,
  SettingOutlined,
  FileDoneOutlined,
  AuditOutlined,
  RobotOutlined,
  ExperimentOutlined,
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

// IA 主轴 = AI 交付生命周期:工作台 → AI需求 → AI评审 → AI研发 → AI测试,再 项目(支撑)/ 系统(底部)。
// 需求是产品主线(从原「缺陷」杂物抽屉里提出来);技能/协同/MCP 归入「AI 研发」;
// 测试资产(用例/接口/场景/计划/性能/缺陷)统一收进「AI 测试」。
const MODULES: ModuleDef[] = [
  {
    key: '/home',
    label: ['nav.home', '首页'],
    icon: <DashboardOutlined />,
    match: ['/home'],
    children: [{ key: '/home', icon: <DashboardOutlined />, label: ['m.home', '工作台'] }],
  },
  {
    key: '/requirement',
    label: ['nav.req', 'AI 需求'],
    icon: <FileDoneOutlined />,
    match: ['/requirement'],
    children: [{ key: '/requirement', icon: <FileDoneOutlined />, label: ['m.requirement', '需求'] }],
  },
  {
    key: '/review',
    label: ['nav.review', 'AI 评审'],
    icon: <AuditOutlined />,
    match: ['/review'],
    children: [{ key: '/review', icon: <AuditOutlined />, label: ['m.review', '评审'] }],
  },
  {
    key: '/skill',
    label: ['nav.dev', 'AI 研发'],
    icon: <RobotOutlined />,
    match: ['/skill', '/agents', '/mcp'],
    children: [
      { key: '/agents', icon: <DeploymentUnitOutlined />, label: ['m.agents', '人机协同'] },
      { key: '/skill', icon: <BulbOutlined />, label: ['m.skill', '技能库'] },
      { key: '/mcp', icon: <ApiOutlined />, label: ['m.mcp', 'MCP 工具'] },
    ],
  },
  {
    key: '/functional-case',
    label: ['nav.test', 'AI 测试'],
    icon: <ExperimentOutlined />,
    match: ['/functional-case', '/api/', '/test-plan', '/perf', '/bug'],
    children: [
      { key: '/functional-case', icon: <ProfileOutlined />, label: ['m.functional', '功能用例'] },
      { key: '/api/definition', icon: <ApiOutlined />, label: ['m.definition', '接口定义'] },
      { key: '/api/scenario', icon: <PartitionOutlined />, label: ['m.scenario', '场景用例'] },
      { key: '/test-plan', icon: <ScheduleOutlined />, label: ['m.plan', '测试计划'] },
      { key: '/perf', icon: <ThunderboltOutlined />, label: ['m.perf', '性能测试'] },
      { key: '/bug', icon: <BugOutlined />, label: ['m.bug', '缺陷管理'] },
    ],
  },
  {
    key: '/project',
    label: ['nav.project', '项目'],
    icon: <ProjectOutlined />,
    match: ['/project', '/environment'],
    children: [
      { key: '/project', icon: <SafetyOutlined />, label: ['proj.permTab', '项目与权限'] },
      { key: '/project/templates', icon: <ProfileOutlined />, label: ['proj.tmplTab', '模板管理'] },
      { key: '/project/files', icon: <FileTextOutlined />, label: ['proj.fileTab', '文件管理'] },
      { key: '/project/messages', icon: <BellOutlined />, label: ['proj.msgTab', '消息管理'] },
      { key: '/project/scripts', icon: <ApiOutlined />, label: ['proj.scriptTab', '公共脚本'] },
      { key: '/environment', icon: <CloudServerOutlined />, label: ['m.environment', '环境管理'] },
      { key: '/project/logs', icon: <FileTextOutlined />, label: ['proj.logTab', '日志'] },
    ],
  },
  {
    key: '/user',
    label: ['nav.sys', '系统'],
    icon: <SettingOutlined />,
    match: ['/user', '/role', '/organization', '/resource-pool', '/system'],
    bottom: true,
    children: [
      { key: '/user', icon: <UserOutlined />, label: ['sys.users', '用户'] },
      { key: '/role', icon: <SafetyOutlined />, label: ['sys.userGroups', '用户组'] },
      { key: '/organization', icon: <ClusterOutlined />, label: ['sys.orgProj', '组织与项目'] },
      { key: '/system/params', icon: <SettingOutlined />, label: ['sys.params', '系统参数'] },
      { key: '/resource-pool', icon: <DatabaseOutlined />, label: ['m.pool', '资源池'] },
      { key: '/system/tasks', icon: <ScheduleOutlined />, label: ['sys.tasks', '任务中心'] },
      { key: '/system/plugins', icon: <AppstoreOutlined />, label: ['sys.plugins', '插件'] },
      { key: '/system/logs', icon: <FileTextOutlined />, label: ['sys.logs', '日志'] },
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
  const [msgOpen, setMsgOpen] = useState(false)
  const [msgCat, setMsgCat] = useState('all')
  const [msgTab, setMsgTab] = useState('all')
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
            <DeploymentUnitOutlined style={{ color: '#06a561', fontSize: 22 }} />
          </div>
          <div style={{ flex: 1, overflowY: 'auto', paddingTop: 4 }}>
            {topModules.map((m) => <RailItem key={m.key} m={m} />)}
          </div>
          <div style={{ paddingBottom: 6 }}>
            {sysModule && <RailItem m={sysModule} />}
            <Tooltip title={t('pc.title', '个人中心')} placement="right">
              <div style={{ display: 'flex', justifyContent: 'center', padding: '10px 0', cursor: 'pointer' }} onClick={() => setPcOpen(true)}>
                <Avatar size={30} style={{ background: '#06a561' }}>{username.slice(0, 1).toUpperCase()}</Avatar>
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
              <Button type="text" size="small" icon={<BellOutlined />} onClick={() => setMsgOpen(true)} />
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

      {/* 消息管理(对齐参考图):右侧抽屉,左侧分类列表 + 右侧 全部/@我的/未读/已读 标签。后端暂无站内消息接口,计数为 0、内容空态。 */}
      <Drawer
        open={msgOpen}
        onClose={() => setMsgOpen(false)}
        width={960}
        styles={{ body: { padding: 0, display: 'flex', flexDirection: 'column' } }}
        title={
          <span>
            {t('msg.title', '消息管理')}
            <span style={{ fontSize: 13, fontWeight: 400, color: '#8c8c8c' }}>
              {t('msg.subtitle', '(仅展示近 3 个月内站内消息)')}
            </span>
          </span>
        }
      >
        <div style={{ display: 'flex', flex: 1, minHeight: 0 }}>
          {/* 左:消息分类(计数徽标右对齐)+ 底部消息设置 */}
          <div style={{ width: 220, borderRight: '1px solid #f0f0f0', display: 'flex', flexDirection: 'column' }}>
            <div style={{ flex: 1, overflowY: 'auto', padding: 8 }}>
              {[
                ['all', t('msg.cat.all', '全部消息')],
                ['plan', t('msg.cat.plan', '测试计划')],
                ['bug', t('msg.cat.bug', '缺陷管理')],
                ['case', t('msg.cat.case', '测试用例')],
                ['api', t('msg.cat.api', '接口测试')],
                ['schedule', t('msg.cat.schedule', '定时任务')],
                ['git', t('msg.cat.git', 'Git')],
              ].map(([key, label]) => {
                const active = key === msgCat
                return (
                  <div
                    key={key}
                    onClick={() => setMsgCat(key)}
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'space-between',
                      padding: '10px 12px',
                      borderRadius: 6,
                      cursor: 'pointer',
                      fontSize: 14,
                      color: active ? '#06a561' : '#1f2329',
                      background: active ? '#e6f7ef' : 'transparent',
                    }}
                  >
                    <span>{label}</span>
                    <span
                      style={{
                        minWidth: 22,
                        height: 20,
                        padding: '0 6px',
                        borderRadius: 10,
                        background: active ? '#c6ecda' : '#f0f0f0',
                        color: active ? '#06a561' : '#8c8c8c',
                        fontSize: 12,
                        display: 'inline-flex',
                        alignItems: 'center',
                        justifyContent: 'center',
                      }}
                    >
                      0
                    </span>
                  </div>
                )
              })}
            </div>
            <div
              style={{ borderTop: '1px solid #f0f0f0', padding: '12px 16px', cursor: 'pointer', color: '#1f2329' }}
              onClick={() => { setMsgOpen(false); nav('/project/messages') }}
            >
              <SettingOutlined style={{ marginRight: 8 }} />
              {t('msg.settings', '消息设置')}
            </div>
          </div>
          {/* 右:标签筛选 + 全部标为已读 + 内容空态 */}
          <div style={{ flex: 1, display: 'flex', flexDirection: 'column', minWidth: 0 }}>
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '12px 16px' }}>
              <Segmented
                value={msgTab}
                onChange={(v) => setMsgTab(v as string)}
                options={[
                  { value: 'all', label: t('msg.tab.all', '全部') },
                  { value: 'mine', label: t('msg.tab.mine', '@我的') },
                  { value: 'unread', label: t('msg.tab.unread', '未读') },
                  { value: 'read', label: t('msg.tab.read', '已读') },
                ]}
              />
              <Button type="link" size="small" icon={<FileDoneOutlined />} style={{ color: '#06a561' }}>
                {t('msg.markAllRead', '全部标为已读')}
              </Button>
            </div>
            <div style={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
              <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('common.empty', '暂无数据')} />
            </div>
          </div>
        </div>
      </Drawer>

      {/* 个人中心(后端暂无 /me,展示登录态可得信息) */}
      <Drawer title={t('pc.title', '个人中心')} open={pcOpen} onClose={() => setPcOpen(false)} width={420}>
        <Space direction="vertical" size={16} style={{ width: '100%' }}>
          <Space align="center" size={12}>
            <Avatar size={48} style={{ background: '#06a561' }}>{username.slice(0, 1).toUpperCase()}</Avatar>
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
