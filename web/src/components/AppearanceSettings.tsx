import { useEffect, useState } from 'react'
import { Alert, Button, ColorPicker, Input, Segmented, Space, Tag } from 'antd'
import { useThemeMode } from '../themeMode'
import { DEFAULT_PRIMARY_COLOR, normalizeThemeColor, themePalette, themePresets } from '../themeColor'
import type { ThemeMode } from '../theme'
import { message } from '../feedback'
export default function AppearanceSettings() {
  const current = useThemeMode(), [mode, setMode] = useState<ThemeMode>(current.mode), [color, setColor] = useState(current.primaryColor), [busy, setBusy] = useState(false), [error, setError] = useState(''), [dirty, setDirty] = useState(false)
  useEffect(() => { if (!dirty) { setMode(current.mode); setColor(current.primaryColor) } }, [current.mode, current.primaryColor, dirty])
  const normalized = normalizeThemeColor(color), palette = themePalette(normalized || DEFAULT_PRIMARY_COLOR, mode)
  const save = async () => { if (!normalized) return; setBusy(true); setError(''); try { await current.saveTheme({ mode, primaryColor: normalized }); setDirty(false); message.success('主题已保存并应用') } catch (e) { setError(e instanceof Error ? e.message : String(e)) } finally { setBusy(false) } }
  return <section aria-label="个人主题设置" style={{ maxWidth: 720 }}>
    <h3 style={{ marginTop: 0 }}>主题与外观</h3><p style={{ color: 'var(--text-3)' }}>为你的账号设置主色和明暗模式，下次登录自动恢复。系统会适配文字对比度。</p>
    {error && <Alert type="error" showIcon title="主题保存失败" description={error} style={{ marginBottom: 16 }} />}
    <h4>显示模式</h4><Segmented aria-label="个人显示模式" value={mode} onChange={value => { setDirty(true); setMode(value as ThemeMode) }} options={[{ value: 'light', label: '浅色' }, { value: 'dark', label: '深色' }]} />
    <h4 style={{ marginTop: 24 }}>预设主题</h4>
    <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit,minmax(100px,1fr))', gap: 10 }}>{themePresets.map(preset => <button type="button" key={preset.color} aria-label={preset.name} aria-pressed={normalized === preset.color} onClick={() => { setDirty(true); setColor(preset.color) }} style={{ border: `1px solid ${normalized === preset.color ? preset.color : 'var(--border)'}`, borderRadius: 8, background: 'var(--panel)', padding: '16px 10px', color: 'var(--text)', cursor: 'pointer' }}><i style={{ display: 'block', width: 28, height: 28, margin: '0 auto 9px', borderRadius: 8, background: preset.color }} />{preset.name}</button>)}</div>
    <h4 style={{ marginTop: 24 }}>自定义主色</h4><Space wrap><ColorPicker aria-label="选择自定义主题色" value={normalized || DEFAULT_PRIMARY_COLOR} disabledAlpha onChangeComplete={value => { setDirty(true); setColor(value.toHexString().toUpperCase()) }} /><Input aria-label="自定义主题色" value={color} onChange={e => { setDirty(true); setColor(e.target.value) }} placeholder="#4D9F92" maxLength={7} status={normalized ? undefined : 'error'} style={{ width: 150, fontFamily: 'monospace' }} /><Button type="link" onClick={() => { setDirty(true); setColor(DEFAULT_PRIMARY_COLOR) }}>恢复默认绿色</Button></Space>
    {!normalized && <p style={{ color: 'var(--error)' }}>请输入 # 开头的六位十六进制颜色。</p>}
    <div aria-label="主题效果预览" style={{ marginBlock: 24, border: `1px solid ${mode === 'dark' ? '#353A40' : '#E5E6EB'}`, borderRadius: 10, overflow: 'hidden', background: palette.panel, color: mode === 'dark' ? '#E2E5EC' : '#1D2129' }}>
      <div style={{ padding: '12px 16px', borderBottom: `1px solid ${mode === 'dark' ? '#353A40' : '#E5E6EB'}`, color: palette.primary }}>Shepherd · 主题预览</div><div style={{ display: 'grid', gridTemplateColumns: '110px 1fr', minHeight: 135 }}><div style={{ padding: 14, background: palette.soft, color: palette.primary }}>工作台<br /><br />需求与研发</div><div style={{ padding: 20 }}><strong>统一的颜色与清晰的层次</strong><p style={{ fontSize: 12 }}>导航、按钮、链接与图表会使用你的主题色。</p><span style={{ display: 'inline-block', padding: '6px 15px', background: palette.primary, color: palette.onPrimary, borderRadius: 5 }}>主要操作</span><span style={{ marginLeft: 16, color: palette.primary }}>查看详情</span></div></div>
    </div>
    <Space><Button type="primary" disabled={!normalized} loading={busy} onClick={save}>保存并应用主题</Button><Tag>仅影响当前账号</Tag></Space>
  </section>
}
