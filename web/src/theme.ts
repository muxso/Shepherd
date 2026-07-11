// Shepherd 设计基线:简约小清新(参考火山引擎控制台/Arco 设计语言)。
// 关键决策:品牌色 = 标准蓝;绿色仅用作「成功」语义色(brand ≠ success,消歧)。
// 浅色 = 纯白面板 + 中性灰文字 + 极淡边框,克制留白;暗色 = 中性深灰。
// AntD 主题 token 一处改、全局生效;非 AntD 的自定义样式走 index.css 里的 CSS 变量(同一套值)。
import { theme as antdTheme, type ThemeConfig } from 'antd'

export type ThemeMode = 'light' | 'dark'

// —— 品牌与语义色(浅色)——
export const BRAND = '#1664ff'
const BRAND_HOVER = '#4086ff'
const BRAND_ACTIVE = '#0e42d2'

const fontFamily =
  "'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', 'PingFang SC', 'Hiragino Sans GB', 'Microsoft YaHei', system-ui, sans-serif"
const fontFamilyCode = "'JetBrains Mono', ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, monospace"

const shared: ThemeConfig['token'] = {
  colorPrimary: BRAND,
  colorInfo: BRAND,
  colorSuccess: '#00b42a',
  colorWarning: '#ff7d00',
  colorError: '#f53f3f',
  colorLink: BRAND,
  // 扁平圆角:按钮/输入等控件 4px,小控件 2px;大容器(卡片/弹窗)另配。
  borderRadius: 4,
  borderRadiusSM: 2,
  borderRadiusLG: 6,
  fontFamily,
  fontFamilyCode,
  fontSize: 14,
  wireframe: false,
}

export const lightTheme: ThemeConfig = {
  algorithm: antdTheme.defaultAlgorithm,
  token: {
    ...shared,
    colorBgLayout: 'transparent', // 透明:让 body 的环境光晕透出(磨砂效果)
    colorBgContainer: '#ffffff',
    colorBorder: '#e5e6eb',
    colorBorderSecondary: '#f2f3f5',
    colorText: '#1d2129',
    colorTextSecondary: '#4e5969',
    colorTextTertiary: '#86909c',
  },
  components: {
    Button: { colorPrimaryHover: BRAND_HOVER, colorPrimaryActive: BRAND_ACTIVE, primaryShadow: 'none' },
    Layout: { headerBg: 'transparent', bodyBg: 'transparent', siderBg: 'transparent' },
    Menu: { itemSelectedColor: BRAND, itemSelectedBg: 'transparent' },
    Table: { headerBg: '#f7f8fa', headerColor: '#4e5969', rowHoverBg: '#f7f8fa' },
    Tabs: { inkBarColor: BRAND, itemSelectedColor: BRAND },
    Card: { borderRadiusLG: 8 },
    Segmented: { itemSelectedBg: '#ffffff', trackBg: '#f2f3f5' },
  },
}

export const darkTheme: ThemeConfig = {
  algorithm: antdTheme.darkAlgorithm,
  token: {
    ...shared,
    colorPrimary: '#4086ff', // 标准蓝,深灰底上取亮值
    colorInfo: '#4086ff',
    colorLink: '#5b9aff',
    colorSuccess: '#27c346',
    colorWarning: '#ff9a2e',
    colorError: '#f76965',
    colorBgLayout: 'transparent',
    colorBgContainer: '#1d1f25',
    colorBgElevated: '#24262e',
    colorBorder: '#2f333b',
    colorBorderSecondary: '#272b32',
    colorText: '#e2e5ec',
    colorTextSecondary: '#a3aab6',
    colorTextTertiary: '#6e7681',
  },
  components: {
    Layout: { headerBg: 'transparent', bodyBg: 'transparent', siderBg: 'transparent' },
    Menu: { itemSelectedColor: '#5b9aff', itemSelectedBg: 'transparent' },
    Table: { headerBg: '#24262e', headerColor: '#a3aab6', rowHoverBg: '#24262e' },
    Tabs: { inkBarColor: '#4086ff', itemSelectedColor: '#5b9aff' },
    Card: { borderRadiusLG: 8 },
    Segmented: { itemSelectedBg: '#24262e', trackBg: '#161719' },
  },
}

export const themeFor = (mode: ThemeMode) => (mode === 'dark' ? darkTheme : lightTheme)
