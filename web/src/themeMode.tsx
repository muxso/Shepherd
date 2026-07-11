// 主题模式(浅/暗)上下文:localStorage 持久化 + 在 <html data-theme> 上标注(供 CSS 变量切换),
// 并把对应 AntD ThemeConfig 喂给 ConfigProvider。
import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from 'react'
import { flushSync } from 'react-dom'
import type { ThemeMode } from './theme'

const KEY = 'shepherd.theme'

type Ctx = { mode: ThemeMode; toggle: (e?: { clientX: number; clientY: number }) => void; setMode: (m: ThemeMode) => void }
const ThemeModeContext = createContext<Ctx>({ mode: 'light', toggle: () => {}, setMode: () => {} })

export const useThemeMode = () => useContext(ThemeModeContext)

export function ThemeModeProvider({ children }: { children: (mode: ThemeMode) => ReactNode }) {
  const [mode, setMode] = useState<ThemeMode>(() => (localStorage.getItem(KEY) as ThemeMode) || 'dark')
  useEffect(() => {
    document.documentElement.setAttribute('data-theme', mode)
    localStorage.setItem(KEY, mode)
  }, [mode])
  // 切换动画:View Transitions 从点击位置圆形扩散揭开新主题;不支持的浏览器直接切。
  const toggle = useCallback((e?: { clientX: number; clientY: number }) => {
    const flip = (m: ThemeMode): ThemeMode => (m === 'dark' ? 'light' : 'dark')
    const doc = document as Document & { startViewTransition?: (cb: () => void) => { ready: Promise<void> } }
    if (!doc.startViewTransition || !e) {
      setMode(flip)
      return
    }
    const { clientX: x, clientY: y } = e
    const r = Math.hypot(Math.max(x, window.innerWidth - x), Math.max(y, window.innerHeight - y))
    const vt = doc.startViewTransition(() => {
      // useEffect 是异步的,快照回调内需同步改 DOM:直接落 data-theme + flushSync 渲染 antd 主题。
      const next = (document.documentElement.getAttribute('data-theme') === 'dark' ? 'light' : 'dark') as ThemeMode
      document.documentElement.setAttribute('data-theme', next)
      flushSync(() => setMode(next))
    })
    vt.ready.then(() => {
      document.documentElement.animate(
        { clipPath: [`circle(0px at ${x}px ${y}px)`, `circle(${r}px at ${x}px ${y}px)`] },
        { duration: 420, easing: 'ease-in-out', pseudoElement: '::view-transition-new(root)' },
      )
    }).catch(() => undefined)
  }, [])
  const value = useMemo(() => ({ mode, toggle, setMode }), [mode, toggle])
  return <ThemeModeContext.Provider value={value}>{children(mode)}</ThemeModeContext.Provider>
}
