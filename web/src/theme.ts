// Shepherd design baseline: clean, light style (after Volcengine console / Arco design language).
// Key decision: brand = standard blue; green is reserved for the "success" semantic (brand ≠ success).
// Light = white panels + neutral gray text + faint borders; dark = neutral dark gray.
// AntD theme tokens change here, apply globally; non-AntD custom styles use the CSS vars in index.css (same values).
import { theme as antdTheme, type ThemeConfig } from 'antd'

export type ThemeMode = 'light' | 'dark'

// Brand & semantic colors (light)
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
  // Flat radii: 4px for buttons/inputs, 2px for small controls; large containers (cards/modals) configured separately.
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
    colorBgLayout: 'transparent', // transparent so the body ambient glow shows through (frosted effect)
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
    colorPrimary: '#4086ff', // standard blue, brightened for the dark gray canvas
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
