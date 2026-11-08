export const DEFAULT_PRIMARY_COLOR = '#4D9F92'
export const themePresets = [
  { name: '青松绿', color: DEFAULT_PRIMARY_COLOR },
  { name: '海盐蓝', color: '#467CB2' },
  { name: '湖水青', color: '#328C96' },
  { name: '鸢尾紫', color: '#8064AD' },
  { name: '暖杏橙', color: '#B67842' },
]
export function normalizeThemeColor(value: string) { return /^#[0-9a-f]{6}$/i.test(value) ? value.toUpperCase() : null }
const channels = (hex: string) => [1, 3, 5].map(i => parseInt(hex.slice(i, i + 2), 16))
const hex = (rgb: number[]) => `#${rgb.map(v => Math.round(v).toString(16).padStart(2, '0')).join('')}`.toUpperCase()
const mix = (a: string, b: string, weight: number) => hex(channels(a).map((v, i) => v * (1 - weight) + channels(b)[i] * weight))
function luminance(color: string) { return channels(color).map(v => { const s = v / 255; return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4 }).reduce((sum, value, i) => sum + value * [0.2126, 0.7152, 0.0722][i], 0) }
export function contrastRatio(a: string, b: string) { const values = [luminance(a), luminance(b)].sort((x, y) => y - x); return (values[0] + 0.05) / (values[1] + 0.05) }
export function themePalette(seed: string, mode: 'light' | 'dark') {
  const color = normalizeThemeColor(seed) || DEFAULT_PRIMARY_COLOR, panel = mode === 'dark' ? '#1D1F25' : '#FFFFFF', target = mode === 'dark' ? '#FFFFFF' : '#000000'
  const readable = (value: string) => { for (let i = 0; i <= 100; i++) { const next = mix(value, target, i / 100); if (contrastRatio(next, panel) >= 4.5) return next } return target }
  const primary = readable(color), hover = readable(mix(color, '#FFFFFF', 0.12)), active = readable(mix(color, '#000000', 0.12))
  const solidColor = (value: string, minimum = 4.6) => {
    let result=value
    for(let i=0;i<=100;i++){const next=mix(value,'#000000',i/100);if(contrastRatio(next,'#FFFFFF')>=minimum){result=next;break}}
    if(mode==='dark'&&contrastRatio(result,panel)<3){for(let i=0;i<=100;i++){const next=mix(result,'#FFFFFF',i/100);if(contrastRatio(next,panel)>=3&&contrastRatio(next,'#FFFFFF')>=4.5){return next}}}
    return result
  }
  const solid=solidColor(color,5),solidHover=solidColor(mix(solid,'#FFFFFF',.08)),solidActive=solidColor(mix(solid,'#000000',.12))
  return { primary, hover, active, solid, solidHover, solidActive, soft: mix(panel, primary, 0.10), rgb: channels(primary).join(', '), onPrimary: '#FFFFFF', panel }
}
export function applyThemePalette(color: string, mode: 'light' | 'dark') {
  const p = themePalette(color, mode), root = document.documentElement
  for (const [name, value] of Object.entries({ '--brand': p.primary, '--brand-hover': p.hover, '--brand-active': p.active, '--brand-soft': p.soft, '--brand-rgb': p.rgb, '--brand-solid': p.solid, '--brand-solid-hover': p.solidHover, '--brand-solid-active': p.solidActive, '--brand-mindmap-root': p.active, '--accent': p.hover })) root.style.setProperty(name, value)
  root.dataset.primaryColor = normalizeThemeColor(color) || DEFAULT_PRIMARY_COLOR
}
