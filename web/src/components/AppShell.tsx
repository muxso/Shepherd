import { useEffect, useState, type ReactNode } from 'react'
import { Layout, Menu, Select, Button, Space, Tooltip, Drawer, Avatar, Descriptions, Segmented, Empty, Dropdown } from 'antd'
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
} from '@ant-design/icons'
import { useLocation, useNavigate } from 'react-router-dom'
import { userStore } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'
import { useThemeMode } from '../themeMode'
import NewProjectModal from './NewProjectModal'

const { Content } = Layout

// 语言选项单一来源:顶栏(图标 + 下拉)与个人中心(分段切换)共用,避免两处文案漂移。
const LANG_OPTIONS: { value: 'zh' | 'en'; label: string }[] = [
  { value: 'zh', label: '中文' },
  { value: 'en', label: 'English' },
]

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
    label: ['nav.req', '需求'],
    icon: <FileDoneOutlined />,
    match: ['/requirement'],
    children: [{ key: '/requirement', icon: <FileDoneOutlined />, label: ['m.requirement', '需求'] }],
  },
  {
    key: '/review',
    label: ['nav.review', '评审'],
    icon: <AuditOutlined />,
    match: ['/review'],
    children: [{ key: '/review', icon: <AuditOutlined />, label: ['m.review', '评审'] }],
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
  // 全屏:整窗进入浏览器全屏(隐藏地址栏/系统边框),沉浸式查看大表格/场景画布。
  // 跟随原生 fullscreenchange 同步图标,兼容用户按 Esc 退出。
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
  const currentProject = projects.find((p) => p.id === projectId)
  const username = userStore.get()

  // 当前所在模块由路由推断;二级栏与面包屑都跟着它走。
  const activeModule = MODULES.find((m) => m.match.some((x) => loc.pathname.startsWith(x))) || MODULES[0]
  // 取「key 是当前路径前缀」中最长的子项,这样嵌套路由(如 /resource-pool/new、
  // /project/files)也能正确高亮二级菜单并落到对的面包屑,而非回退到第一个子项。
  const currentChild =
    activeModule.children
      .filter((c) => loc.pathname === c.key || loc.pathname.startsWith(c.key + '/'))
      .sort((a, b) => b.key.length - a.key.length)[0] || activeModule.children[0]
  const topModules = MODULES.filter((m) => !m.bottom)
  const sysModule = MODULES.find((m) => m.bottom)

  // 浏览器标签标题随「当前页面 + 语言」切换:页面名取自当前二级菜单项的 i18n 标签。
  useEffect(() => {
    document.title = `${t(...currentChild.label)} · Shepherd`
  }, [currentChild.key, lang, t])

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
            color: active ? 'var(--brand)' : 'var(--text-2)',
            background: active ? 'var(--brand-soft)' : 'transparent',
          }}
        >
          <span style={{ fontSize: 18 }}>{m.icon}</span>
          {/* 英文标签可能超过窄栏宽度:超出省略,完整名在 Tooltip 里 */}
          <span style={{ maxWidth: '100%', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{t(...m.label)}</span>
        </div>
      </Tooltip>
    )
  }

  return (
    <Layout style={{ height: '100vh' }}>
      {/* hasSider:左侧为自定义 div(非 antd Sider),需显式声明横向布局,否则默认竖排。 */}
      <Layout hasSider>
        {/* 全局图标导航栏:品牌标 / 导航项 / 底部系统 + 头像。磨砂玻璃,环境光晕透出。 */}
        <div style={{ width: 72, background: 'var(--glass)', backdropFilter: 'var(--glass-blur)', WebkitBackdropFilter: 'var(--glass-blur)', borderRight: '1px solid var(--border-soft)', display: 'flex', flexDirection: 'column', flexShrink: 0 }}>
          {/* 品牌:几何标记 + 字标。点击回工作台(顶栏不再放重复的工作台图标)。 */}
          <Tooltip title={t('m.home', '工作台')} placement="right">
            <div
              onClick={() => nav('/home')}
              style={{ height: 56, display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', gap: 2, cursor: 'pointer' }}
            >
              <DeploymentUnitOutlined style={{ color: 'var(--brand)', fontSize: 22 }} />
              <span style={{ fontSize: 10, fontWeight: 700, letterSpacing: 0.3, color: 'var(--brand)' }}>Shepherd</span>
            </div>
          </Tooltip>
          <div style={{ flex: 1, overflowY: 'auto', paddingTop: 4 }}>
            {topModules.map((m) => <RailItem key={m.key} m={m} />)}
          </div>
          <div style={{ paddingBottom: 6 }}>
            {sysModule && <RailItem m={sysModule} />}
            <Tooltip title={t('pc.title', '个人中心')} placement="right">
              <div style={{ display: 'flex', justifyContent: 'center', padding: '10px 0', cursor: 'pointer' }} onClick={() => setPcOpen(true)}>
                <Avatar size={30} style={{ background: 'var(--brand)' }}>{username.slice(0, 1).toUpperCase()}</Avatar>
              </div>
            </Tooltip>
          </div>
        </div>

        <Layout style={{ background: 'var(--bg)' }}>
          {/* 顶栏:当前模块的二级菜单(左,横向)+ 右上角图标簇。磨砂玻璃。 */}
          <div
            style={{
              height: 48,
              display: 'flex',
              alignItems: 'center',
              paddingInline: 16,
              gap: 8,
              background: 'var(--glass)',
              backdropFilter: 'var(--glass-blur)',
              WebkitBackdropFilter: 'var(--glass-blur)',
              borderBottom: '1px solid var(--border-soft)',
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
              // 项目多时:下拉列表限高内部滚动,避免溢出视口
              listHeight={320}
              // 名称长时:弹层可比选择框更宽(限上限,避免顶到屏幕边),完整可读
              popupMatchSelectWidth={false}
              styles={{ popup: { root: { minWidth: 200, maxWidth: 360 } } }}
              optionRender={(opt) => (
                <span style={{ display: 'block', overflow: 'hidden', textOverflow: 'ellipsis' }} title={String(opt.label)}>
                  {opt.label}
                </span>
              )}
            />
            <Tooltip title={t('top.newProject')}>
              <Button type="text" size="small" icon={<PlusOutlined />} onClick={() => setNewProjOpen(true)} />
            </Tooltip>
            <Tooltip title={t('top.notifications', '通知')}>
              <Button type="text" size="small" icon={<BellOutlined />} onClick={() => setMsgOpen(true)} />
            </Tooltip>
            <Tooltip title={mode === 'dark' ? t('top.lightMode', '浅色模式') : t('top.darkMode', '暗色模式')}>
              <Button type="text" size="small" icon={<BulbOutlined style={{ color: mode === 'dark' ? 'var(--brand)' : undefined }} />} onClick={toggle} />
            </Tooltip>
            <Tooltip title={isFullscreen ? t('top.exitFullscreen', '退出全屏') : t('top.fullscreen', '全屏')}>
              <Button type="text" size="small" icon={isFullscreen ? <FullscreenExitOutlined style={{ color: 'var(--brand)' }} /> : <FullscreenOutlined />} onClick={toggleFullscreen} />
            </Tooltip>
            {/* 语言切换:与相邻图标按钮风格一致的紧凑触发器(地球图标 + 当前语言短码),
                下拉项对选中语言显示品牌色对勾,切换后所见即所得。 */}
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
                <Button type="text" size="small">
                  <Space size={4}>
                    <GlobalOutlined />
                    {lang === 'zh' ? '中' : 'EN'}
                  </Space>
                </Button>
              </Tooltip>
            </Dropdown>
          </div>
          {/* 不放面包屑:层级已由左栏模块高亮 + 顶栏选中标签完整表达,再来一行是纯重复。 */}
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
            <span style={{ fontSize: 13, fontWeight: 400, color: 'var(--text-3)' }}>
              {t('msg.subtitle', '(仅展示近 3 个月内站内消息)')}
            </span>
          </span>
        }
      >
        <div style={{ display: 'flex', flex: 1, minHeight: 0 }}>
          {/* 左:消息分类(计数徽标右对齐)+ 底部消息设置 */}
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

      {/* 个人中心(后端暂无 /me,展示登录态可得信息) */}
      <Drawer title={t('pc.title', '个人中心')} open={pcOpen} onClose={() => setPcOpen(false)} width={420}>
        <Space direction="vertical" size={16} style={{ width: '100%' }}>
          <Space align="center" size={12}>
            <Avatar size={48} style={{ background: 'var(--brand)' }}>{username.slice(0, 1).toUpperCase()}</Avatar>
            <span style={{ fontSize: 16, fontWeight: 600 }}>{username}</span>
          </Space>
          <Descriptions column={1} size="small" bordered>
            <Descriptions.Item label={t('pc.username', '用户名')}>{username}</Descriptions.Item>
            <Descriptions.Item label={t('pc.project', '当前项目')}>{currentProject?.name || '—'}</Descriptions.Item>
            <Descriptions.Item label={t('pc.projectCount', '可见项目数')}>{projects.length}</Descriptions.Item>
            <Descriptions.Item label={t('pc.lang', '语言')}>
              {/* 设置面板内有富余空间:用分段切换一眼可见全部选项与当前态,无需展开下拉。 */}
              <Segmented
                size="small"
                value={lang}
                onChange={(v) => setLang(v as 'zh' | 'en')}
                options={LANG_OPTIONS}
              />
            </Descriptions.Item>
          </Descriptions>
          <Button danger icon={<LogoutOutlined />} onClick={logout}>{t('top.logout', '退出登录')}</Button>
        </Space>
      </Drawer>
    </Layout>
  )
}
