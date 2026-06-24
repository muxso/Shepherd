// 主题模式(浅/暗)上下文:localStorage 持久化 + 在 <html data-theme> 上标注(供 CSS 变量切换),
// 并把对应 AntD ThemeConfig 喂给 ConfigProvider。
import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from 'react'
import type { ThemeMode } from './theme'

const KEY = 'shepherd.theme'

type Ctx = { mode: ThemeMode; toggle: () => void; setMode: (m: ThemeMode) => void }
const ThemeModeContext = createContext<Ctx>({ mode: 'light', toggle: () => {}, setMode: () => {} })

export const useThemeMode = () => useContext(ThemeModeContext)

export function ThemeModeProvider({ children }: { children: (mode: ThemeMode) => ReactNode }) {
  const [mode, setMode] = useState<ThemeMode>(() => (localStorage.getItem(KEY) as ThemeMode) || 'dark')
  useEffect(() => {
    document.documentElement.setAttribute('data-theme', mode)
    localStorage.setItem(KEY, mode)
  }, [mode])
  const toggle = useCallback(() => setMode((m) => (m === 'dark' ? 'light' : 'dark')), [])
  const value = useMemo(() => ({ mode, toggle, setMode }), [mode, toggle])
  return <ThemeModeContext.Provider value={value}>{children(mode)}</ThemeModeContext.Provider>
}
