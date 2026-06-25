// Shepherd 设计基线:独立的视觉身份。
// 关键决策:品牌色 = 靛蓝(indigo);绿色仅用作「成功」语义色(brand ≠ success,消歧)。
// AntD 主题 token 一处改、全局生效;非 AntD 的自定义样式走 index.css 里的 CSS 变量(同一套值)。
import { theme as antdTheme, type ThemeConfig } from 'antd'

export type ThemeMode = 'light' | 'dark'

// —— 品牌与语义色(浅色)——
// 青绿(teal-green):配深色 slate 通透清爽(用户偏好);浅色下取稍深值保对比。
export const BRAND = '#13a06c'
const BRAND_HOVER = '#0e8a5c'
const BRAND_ACTIVE = '#0b7550'

const fontFamily =
  "'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', 'PingFang SC', 'Hiragino Sans GB', 'Microsoft YaHei', system-ui, sans-serif"
const fontFamilyCode = "'JetBrains Mono', ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, monospace"

const shared: ThemeConfig['token'] = {
  colorPrimary: BRAND,
  colorInfo: BRAND,
  colorSuccess: '#3aa675',
  colorWarning: '#e0a64b',
  colorError: '#e5616a',
  colorLink: BRAND,
  borderRadius: 8,
  borderRadiusSM: 6,
  borderRadiusLG: 10,
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
    colorBorder: '#e6ebf2',
    colorBorderSecondary: '#eef2f7',
    colorText: '#2b313b',
    colorTextSecondary: '#5b6573',
    colorTextTertiary: '#9aa3b2',
  },
  components: {
    Button: { colorPrimaryHover: BRAND_HOVER, colorPrimaryActive: BRAND_ACTIVE, primaryShadow: 'none' },
    Layout: { headerBg: 'transparent', bodyBg: 'transparent', siderBg: 'transparent' },
    Menu: { itemSelectedColor: BRAND, itemSelectedBg: 'transparent' },
    Table: { headerBg: '#f5f8fc', headerColor: '#5b6573', rowHoverBg: '#f5f8fc' },
    Tabs: { inkBarColor: BRAND, itemSelectedColor: BRAND },
    Card: { borderRadiusLG: 12 },
    Segmented: { itemSelectedBg: '#ffffff', trackBg: '#eef2f7' },
  },
}

export const darkTheme: ThemeConfig = {
  algorithm: antdTheme.darkAlgorithm,
  token: {
    ...shared,
    colorPrimary: '#2bb673', // 青绿,暗色 slate 上更亮
    colorInfo: '#2bb673',
    colorLink: '#3cc487',
    colorSuccess: '#2bb673',
    colorBgLayout: 'transparent',
    colorBgContainer: '#1e252e',
    colorBgElevated: '#252e38',
    colorBorder: '#2e3742',
    colorBorderSecondary: '#283139',
    colorText: '#cdd6e0',
    colorTextSecondary: '#9aa6b2',
    colorTextTertiary: '#6b7682',
  },
  components: {
    Layout: { headerBg: 'transparent', bodyBg: 'transparent', siderBg: 'transparent' },
    Menu: { itemSelectedColor: '#3cc487', itemSelectedBg: 'transparent' },
    Table: { headerBg: '#252e38', headerColor: '#9aa6b2', rowHoverBg: '#252e38' },
    Tabs: { inkBarColor: '#2bb673', itemSelectedColor: '#3cc487' },
    Segmented: { itemSelectedBg: '#23262e', trackBg: '#1b1e24' },
  },
}

export const themeFor = (mode: ThemeMode) => (mode === 'dark' ? darkTheme : lightTheme)
