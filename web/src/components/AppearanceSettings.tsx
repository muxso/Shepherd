import { useEffect, useState, type CSSProperties } from 'react'
import { Alert, Button, ColorPicker, ConfigProvider, Input, Segmented, Space } from 'antd'
import { CheckOutlined } from '@ant-design/icons'
import { useThemeMode } from '../themeMode'
import { DEFAULT_PRIMARY_COLOR, normalizeThemeColor, themePalette, themePresets } from '../themeColor'
import { themeFor, type ThemeMode } from '../theme'
import { message } from '../feedback'
import './AppearanceSettings.css'
export default function AppearanceSettings() {
  const current = useThemeMode(), [mode, setMode] = useState<ThemeMode>(current.mode), [color, setColor] = useState(current.primaryColor), [busy, setBusy] = useState(false), [error, setError] = useState(''), [dirty, setDirty] = useState(false)
  useEffect(() => { if (!dirty) { setMode(current.mode); setColor(current.primaryColor) } }, [current.mode, current.primaryColor, dirty])
  const normalized = normalizeThemeColor(color), palette = themePalette(normalized || DEFAULT_PRIMARY_COLOR, mode)
  const save = async () => { if (!normalized) return; setBusy(true); setError(''); try { await current.saveTheme({ mode, primaryColor: normalized }); setDirty(false); message.success('主题已保存并应用') } catch (e) { setError(e instanceof Error ? e.message : String(e)) } finally { setBusy(false) } }
  const previewStyle = { '--brand': palette.primary, '--brand-hover': palette.hover, '--brand-active': palette.active, '--brand-soft': palette.soft, '--brand-solid': palette.solid, '--brand-solid-hover': palette.solidHover, '--brand-solid-active': palette.solidActive, '--preview-accent': palette.primary, '--preview-soft': palette.soft, '--preview-text': mode === 'dark' ? '#E2E5EC' : '#1D2129', '--preview-muted': mode === 'dark' ? '#A3AAB6' : '#6B7280', '--preview-border': mode === 'dark' ? '#353A40' : '#E5E6EB', background: palette.panel } as CSSProperties
  return <section aria-label="个人主题设置" className="appearance-settings">
    <h3>主题与外观</h3><p className="appearance-description">为当前账号选择喜欢的颜色，保存后在所有页面生效。</p>
    {error && <Alert type="error" showIcon title="主题保存失败" description={error} style={{ marginBottom: 16 }} />}
    <h4>显示模式</h4><Segmented aria-label="个人显示模式" value={mode} onChange={value => { setDirty(true); setMode(value as ThemeMode) }} options={[{ value: 'light', label: '浅色' }, { value: 'dark', label: '深色' }]} />
    <h4>预设主题</h4><div className="appearance-presets">{themePresets.map(preset => {const p=themePalette(preset.color,current.mode),selected=normalized===preset.color;return <button type="button" key={preset.color} aria-label={preset.name} aria-pressed={selected} onClick={() => { setDirty(true); setColor(preset.color) }} style={{borderColor:selected?p.primary:undefined,background:selected?p.soft:undefined}}><i style={{background:preset.color}}/>{preset.name}{selected&&<CheckOutlined className="appearance-preset-check" style={{color:p.primary}}/>}</button>})}</div>
    <h4>自定义主色</h4><Space wrap><ColorPicker aria-label="选择自定义主题色" value={normalized || DEFAULT_PRIMARY_COLOR} disabledAlpha onChangeComplete={value => { setDirty(true); setColor(value.toHexString().toUpperCase()) }} /><Input aria-label="自定义主题色" value={color} onChange={e => { setDirty(true); setColor(e.target.value) }} placeholder="#4D9F92" maxLength={7} status={normalized ? undefined : 'error'} style={{ width: 150, fontFamily: 'monospace' }} /><Button type="link" onClick={() => { setDirty(true); setColor(DEFAULT_PRIMARY_COLOR) }}>恢复默认绿色</Button></Space>
    {!normalized && <p style={{ color: 'var(--error)' }}>请输入 # 开头的六位十六进制颜色。</p>}
    <ConfigProvider theme={themeFor(mode,normalized||DEFAULT_PRIMARY_COLOR)}><div aria-label="主题效果预览" className="appearance-preview" style={previewStyle}>
      <header><span>Shepherd</span><small>实时预览 · {mode==='light'?'浅色':'深色'}</small></header>
      <div className="appearance-preview-layout"><aside><span>工作台</span><span className="appearance-preview-selected">需求与研发</span></aside><div className="appearance-preview-content"><strong>专注工作，保持清晰</strong><p>从导航到操作按钮，始终使用统一的主题风格。</p><Space size={16}><Button type="primary">主要操作</Button><Button type="link">查看详情</Button></Space></div></div>
    </div></ConfigProvider>
    <footer className="appearance-actions"><Button type="primary" icon={<CheckOutlined aria-hidden="true"/>} disabled={!normalized} loading={busy} onClick={save}>保存并应用主题</Button><span>仅影响当前账号</span></footer>
  </section>
}
