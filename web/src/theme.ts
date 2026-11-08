// Primary colors follow the user palette; semantic status colors remain independent.
import { theme as antdTheme, type ThemeConfig } from 'antd'
import { DEFAULT_PRIMARY_COLOR, themePalette } from './themeColor'

export type ThemeMode = 'light' | 'dark'

// Brand & semantic colors (light)
export const BRAND = DEFAULT_PRIMARY_COLOR
const BRAND_HOVER = '#599F94'
const BRAND_ACTIVE = '#397F74'

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

const lightBase: ThemeConfig = {
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
    // Lighter overlay mask: the stock 45% black reads too heavy behind drawers/modals.
    colorBgMask: 'rgba(15, 23, 42, 0.18)',
    // Flat elevation: shallow, low-alpha shadows across popovers/modals/drawers.
    boxShadow: '0 2px 8px rgba(15, 23, 42, 0.06), 0 0 1px rgba(15, 23, 42, 0.08)',
    boxShadowSecondary: '0 4px 16px rgba(15, 23, 42, 0.08), 0 0 1px rgba(15, 23, 42, 0.08)',
    boxShadowTertiary: '0 1px 4px rgba(15, 23, 42, 0.05)',
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

const darkBase: ThemeConfig = {
  algorithm: antdTheme.darkAlgorithm,
  token: {
    ...shared,
    colorPrimary: BRAND, // Adjusted by the selected user palette.
    colorInfo: BRAND,
    colorLink: BRAND,
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
    // Lighter overlay mask (see lightTheme note).
    colorBgMask: 'rgba(0, 0, 0, 0.35)',
    // Flat elevation (see lightTheme note).
    boxShadow: '0 2px 8px rgba(0, 0, 0, 0.22), 0 0 1px rgba(0, 0, 0, 0.3)',
    boxShadowSecondary: '0 4px 16px rgba(0, 0, 0, 0.26), 0 0 1px rgba(0, 0, 0, 0.3)',
    boxShadowTertiary: '0 1px 4px rgba(0, 0, 0, 0.2)',
  },
  components: {
    Layout: { headerBg: 'transparent', bodyBg: 'transparent', siderBg: 'transparent' },
    Menu: { itemSelectedColor: BRAND, itemSelectedBg: 'transparent' },
    Table: { headerBg: '#24262e', headerColor: '#a3aab6', rowHoverBg: '#24262e' },
    Tabs: { inkBarColor: BRAND, itemSelectedColor: BRAND },
    Card: { borderRadiusLG: 8 },
    Segmented: { itemSelectedBg: '#24262e', trackBg: '#161719' },
  },
}

export const themeFor = (mode: ThemeMode, color = DEFAULT_PRIMARY_COLOR): ThemeConfig => {
  const base = mode === 'dark' ? darkBase : lightBase, palette = themePalette(color, mode)
  return { ...base, token: { ...base.token, colorPrimary: palette.primary, colorInfo: palette.primary, colorLink: palette.primary, colorTextLightSolid: '#FFFFFF' }, components: { ...base.components, Tooltip: { colorBgSpotlight: '#20252B', colorTextLightSolid: '#FFFFFF' }, Button: { ...base.components?.Button, colorPrimaryHover: palette.hover, colorPrimaryActive: palette.active, primaryColor: palette.onPrimary, borderRadius: 6 }, Menu: { ...base.components?.Menu, itemSelectedColor: palette.primary }, Tabs: { ...base.components?.Tabs, inkBarColor: palette.primary, itemSelectedColor: palette.primary } } }
}
export const lightTheme = themeFor('light')
export const darkTheme = themeFor('dark')
