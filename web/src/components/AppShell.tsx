import { useEffect, useRef, useState, type ReactNode } from 'react'
import { Layout, Menu, Select, Button, Space, Tooltip, Drawer, Avatar, Segmented, Empty, Dropdown } from 'antd'
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
  CheckOutlined,
  DashboardOutlined,
  BellOutlined,
  SettingOutlined,
  FileDoneOutlined,
  AuditOutlined,
  RobotOutlined,
  ExperimentOutlined,
  FullscreenOutlined,
  FullscreenExitOutlined,
  KeyOutlined,
  CarryOutOutlined,
  StarOutlined,
} from '@ant-design/icons'
import { useLocation, useNavigate } from 'react-router-dom'
import { userStore } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'
import { useThemeMode } from '../themeMode'
import NewProjectModal from './NewProjectModal'
import PersonalCenter from './PersonalCenter'

const { Content } = Layout

// Single source for language options, shared by the top bar dropdown and the personal-center segmented control so the labels can't drift apart.
const LANG_OPTIONS: { value: 'zh' | 'en'; label: string }[] = [
  { value: 'zh', label: '中文' },
  { value: 'en', label: 'English' },
]

// Top-level module = an icon item in the left global rail (icon above text, per reference shots)
// plus its own secondary menu. The rail only switches modules; the secondary bar shows the
// selected module's children. System module is pinned to the bottom.
// label is [i18n key, fallback], so entries missing from the dictionary still render.
interface ModuleDef {
  key: string
  label: [string, string]
  icon: ReactNode
  match: string[]
  bottom?: boolean // pinned to the rail bottom (system module)
  children: { key: string; icon: ReactNode; label: [string, string] }[]
}

// IA follows the AI delivery lifecycle: home → requirements → review → dev → test,
// then project (supporting) and system (bottom). Requirements is the product mainline
// (promoted out of the old "bugs" catch-all); skills/agents/MCP live under dev;
// test assets (cases/API/scenarios/plans/perf/bugs) are all under test.
const MODULES: ModuleDef[] = [
  {
    // Home = personal entry points: workbench (overview) + todos (awaiting me) + follows (assets I watch).
    key: '/home',
    label: ['nav.home', '首页'],
    icon: <DashboardOutlined />,
    match: ['/home', '/todo', '/follow'],
    children: [
      { key: '/home', icon: <DashboardOutlined />, label: ['m.home', '工作台'] },
      { key: '/todo', icon: <CarryOutOutlined />, label: ['home.todo.nav', '待办'] },
      { key: '/follow', icon: <StarOutlined />, label: ['home.follow.nav', '关注'] },
    ],
  },
  {
    // Requirements and review share one domain: single rail entry, switched via the secondary menu (same pattern as the test module).
    key: '/requirement',
    label: ['nav.req', '需求'],
    icon: <FileDoneOutlined />,
    match: ['/requirement', '/review'],
    children: [
      { key: '/requirement', icon: <FileDoneOutlined />, label: ['m.requirement', '需求'] },
      { key: '/review', icon: <AuditOutlined />, label: ['m.review', '评审'] },
    ],
  },
  {
    key: '/skill',
    label: ['nav.dev', '研发'],
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
    label: ['nav.test', '测试'],
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
      { key: '/system/apikeys', icon: <KeyOutlined />, label: ['sys.apikeys', 'API 密钥'] },
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
  const { mode, toggle } = useThemeMode()
  const nav = useNavigate()
  const loc = useLocation()
  const [newProjOpen, setNewProjOpen] = useState(false)
  const [pcOpen, setPcOpen] = useState(false)
  const [msgOpen, setMsgOpen] = useState(false)
  const [msgCat, setMsgCat] = useState('all')
  const [msgTab, setMsgTab] = useState('all')
  // Fullscreen: whole window enters browser fullscreen for large tables / scenario canvas.
  // Icon syncs via the native fullscreenchange event so Esc-exit is also reflected.
  const [isFullscreen, setIsFullscreen] = useState(false)
  useEffect(() => {
    const onChange = () => setIsFullscreen(!!document.fullscreenElement)
    document.addEventListener('fullscreenchange', onChange)
    return () => document.removeEventListener('fullscreenchange', onChange)
  }, [])
  const toggleFullscreen = () => {
    if (document.fullscreenElement) {
      document.exitFullscreen().catch(() => {})
    } else {
      document.documentElement.requestFullscreen().catch(() => {})
    }
  }
  // Top bar turns solid once any scroll container in the content area leaves the top.
  // scroll doesn't bubble, so listen on document in the capture phase and restrict targets to the content area.
  const contentRef = useRef<HTMLDivElement>(null)
  const [scrolled, setScrolled] = useState(false)
  useEffect(() => {
    const onScroll = (e: Event) => {
      const el = e.target
      if (!(el instanceof Element) || !contentRef.current?.contains(el)) return
      setScrolled(el.scrollTop > 8)
    }
    document.addEventListener('scroll', onScroll, true)
    return () => document.removeEventListener('scroll', onScroll, true)
  }, [])

  const username = userStore.get()

  // Current module is inferred from the route; the secondary bar follows it.
  const activeModule = MODULES.find((m) => m.match.some((x) => loc.pathname.startsWith(x))) || MODULES[0]
  // Pick the longest child whose key prefixes the current path, so nested routes
  // (/resource-pool/new, /project/files) highlight the right secondary item instead of
  // falling back to the first child.
  const currentChild =
    activeModule.children
      .filter((c) => loc.pathname === c.key || loc.pathname.startsWith(c.key + '/'))
      .sort((a, b) => b.key.length - a.key.length)[0] || activeModule.children[0]
  const topModules = MODULES.filter((m) => !m.bottom)
  const sysModule = MODULES.find((m) => m.bottom)

  // Browser tab title tracks current page + language, using the active secondary item's i18n label.
  useEffect(() => {
    document.title = `${t(...currentChild.label)} · Shepherd`
  }, [currentChild.key, lang, t])

  // Rail nav item: icon above text; selected state uses brand color (per reference shot #40).
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
            color: active ? 'var(--brand)' : 'var(--text-2)',
            background: active ? 'var(--brand-soft)' : 'transparent',
          }}
        >
          <span style={{ fontSize: 18 }}>{m.icon}</span>
          {/* English labels can exceed the narrow rail: ellipsize, full name in the Tooltip */}
          <span style={{ maxWidth: '100%', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{t(...m.label)}</span>
        </div>
      </Tooltip>
    )
  }

  return (
    <Layout style={{ height: '100vh' }}>
      {/* hasSider: the left rail is a custom div (not antd Sider), so horizontal layout must be declared explicitly or antd stacks vertically. */}
      <Layout hasSider>
        {/* Global icon rail: brand mark / nav items / bottom system + avatar. Frosted glass over the ambient glow. */}
        <div style={{ width: 72, background: 'var(--glass)', backdropFilter: 'var(--glass-blur)', WebkitBackdropFilter: 'var(--glass-blur)', borderRight: '1px solid var(--border-soft)', display: 'flex', flexDirection: 'column', flexShrink: 0 }}>
          {/* Brand: mark + wordmark. Click returns to the workbench (so the top bar carries no duplicate home icon). */}
          <Tooltip title={t('m.home', '工作台')} placement="right">
            <div
              onClick={() => nav('/home')}
              style={{ height: 56, display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', gap: 2, cursor: 'pointer' }}
            >
              <img src="/logo.svg" alt="Shepherd" width={24} height={24} style={{ display: 'block' }} />
              <span style={{ fontSize: 10, fontWeight: 700, letterSpacing: 0.3, color: 'var(--brand)' }}>Shepherd</span>
            </div>
          </Tooltip>
          <div style={{ flex: 1, overflowY: 'auto', paddingTop: 4 }}>
            {topModules.map((m) => <RailItem key={m.key} m={m} />)}
          </div>
          <div style={{ paddingBottom: 6 }}>
            {sysModule && <RailItem m={sysModule} />}
            {/* Avatar: dropdown (personal center / logout); personal center is a fullscreen drawer */}
            <Dropdown
              trigger={['click']}
              placement="topRight"
              menu={{
                items: [
                  { key: 'pc', icon: <UserOutlined />, label: t('pc.title', '个人中心'), onClick: () => setPcOpen(true) },
                  { type: 'divider' },
                  { key: 'logout', icon: <LogoutOutlined />, danger: true, label: t('top.logout', '退出登录'), onClick: logout },
                ],
              }}
            >
              <div style={{ display: 'flex', justifyContent: 'center', padding: '10px 0', cursor: 'pointer' }}>
                <Avatar size={30} style={{ background: 'var(--brand)' }}>{username.slice(0, 1).toUpperCase()}</Avatar>
              </div>
            </Dropdown>
          </div>
        </div>

        <Layout style={{ background: 'var(--bg)' }}>
          {/* Top bar: active module's secondary menu (left, horizontal) + icon cluster on the right. Frosted glass. */}
          <div
            className={`ms-topbar${scrolled ? ' ms-scrolled' : ''}`}
            style={{
              height: 48,
              display: 'flex',
              alignItems: 'center',
              paddingInline: 16,
              gap: 8,
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
              style={{ width: 200 }}
              value={projectId || undefined}
              placeholder={t('top.project')}
              onChange={setProjectId}
              showSearch
              optionFilterProp="label"
              options={projects.map((p) => ({ value: p.id, label: p.name }))}
              notFoundContent={t('common.empty')}
              // Cap dropdown height with internal scroll so many projects don't overflow the viewport
              listHeight={320}
              // Popup may grow wider than the select (bounded max) so long names stay readable
              popupMatchSelectWidth={false}
              styles={{ popup: { root: { minWidth: 200, maxWidth: 360 } } }}
              optionRender={(opt) => (
                <span style={{ display: 'block', overflow: 'hidden', textOverflow: 'ellipsis' }} title={String(opt.label)}>
                  {opt.label}
                </span>
              )}
            />
            <Tooltip title={t('top.newProject')}>
              <Button type="text" icon={<PlusOutlined />} onClick={() => setNewProjOpen(true)} />
            </Tooltip>
            <Tooltip title={t('top.notifications', '通知')}>
              <Button type="text" icon={<BellOutlined />} onClick={() => setMsgOpen(true)} />
            </Tooltip>
            <Tooltip title={mode === 'dark' ? t('top.lightMode', '浅色模式') : t('top.darkMode', '暗色模式')}>
              <Button type="text" icon={<BulbOutlined style={{ color: mode === 'dark' ? 'var(--brand)' : undefined }} />} onClick={(e) => toggle(e)} />
            </Tooltip>
            <Tooltip title={isFullscreen ? t('top.exitFullscreen', '退出全屏') : t('top.fullscreen', '全屏')}>
              <Button type="text" icon={isFullscreen ? <FullscreenExitOutlined style={{ color: 'var(--brand)' }} /> : <FullscreenOutlined />} onClick={toggleFullscreen} />
            </Tooltip>
            {/* Language switch: compact trigger matching the neighboring icon buttons (globe + current
                language code); dropdown marks the selected language with a brand-color check. */}
            <Dropdown
              trigger={['click']}
              placement="bottomRight"
              menu={{
                selectable: true,
                selectedKeys: [lang],
                onClick: ({ key }) => setLang(key as 'zh' | 'en'),
                items: LANG_OPTIONS.map((o) => ({
                  key: o.value,
                  label: (
                    <span style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 24, minWidth: 116 }}>
                      <span>{o.label}</span>
                      {lang === o.value && <CheckOutlined style={{ color: 'var(--brand)' }} />}
                    </span>
                  ),
                })),
              }}
            >
              <Tooltip title={t('top.language', '语言')}>
                <Button type="text">
                  <Space size={4}>
                    <GlobalOutlined />
                    {lang === 'zh' ? '中' : 'EN'}
                  </Space>
                </Button>
              </Tooltip>
            </Dropdown>
          </div>
          {/* No breadcrumb: rail highlight + selected top-bar tab already express the hierarchy; another row would be pure repetition. */}
          <Content style={{ overflow: 'hidden' }}>
            <div ref={contentRef} style={{ height: '100%' }}>{children}</div>
          </Content>
        </Layout>
      </Layout>
      <NewProjectModal open={newProjOpen} onClose={() => setNewProjOpen(false)} />

      {/* Message center (per reference shots): right drawer with category list + all/@me/unread/read tabs. Backend has no in-app message API yet, so counts are 0 and content is empty state. */}
      <Drawer
        open={msgOpen}
        onClose={() => setMsgOpen(false)}
        width={960}
        styles={{ body: { padding: 0, display: 'flex', flexDirection: 'column' } }}
        title={
          <span>
            {t('msg.title', '消息管理')}
            <span style={{ fontSize: 13, fontWeight: 400, color: 'var(--text-3)' }}>
              {t('msg.subtitle', '(仅展示近 3 个月内站内消息)')}
            </span>
          </span>
        }
      >
        <div style={{ display: 'flex', flex: 1, minHeight: 0 }}>
          {/* Left: message categories (count badge right-aligned) + message settings at the bottom */}
          <div style={{ width: 220, borderRight: '1px solid var(--border-soft)', display: 'flex', flexDirection: 'column' }}>
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
                      color: active ? 'var(--brand)' : 'var(--text)',
                      background: active ? 'var(--brand-soft)' : 'transparent',
                    }}
                  >
                    <span>{label}</span>
                    <span
                      style={{
                        minWidth: 22,
                        height: 20,
                        padding: '0 6px',
                        borderRadius: 10,
                        background: active ? 'var(--brand-soft)' : 'var(--border-soft)',
                        color: active ? 'var(--brand)' : 'var(--text-3)',
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
              style={{ borderTop: '1px solid var(--border-soft)', padding: '12px 16px', cursor: 'pointer', color: 'var(--text)' }}
              onClick={() => { setMsgOpen(false); nav('/project/messages') }}
            >
              <SettingOutlined style={{ marginRight: 8 }} />
              {t('msg.settings', '消息设置')}
            </div>
          </div>
          {/* Right: tab filter + mark-all-read + empty state */}
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
              <Button type="link" size="small" icon={<FileDoneOutlined />} style={{ color: 'var(--brand)' }}>
                {t('msg.markAllRead', '全部标为已读')}
              </Button>
            </div>
            <div style={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
              <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('common.empty', '暂无数据')} />
            </div>
          </div>
        </div>
      </Drawer>

      {/* Personal center: fullscreen drawer (profile / password / API keys / model settings) */}
      <PersonalCenter open={pcOpen} onClose={() => setPcOpen(false)} />
    </Layout>
  )
}
