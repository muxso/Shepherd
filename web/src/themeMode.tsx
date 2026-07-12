// Theme mode (light/dark) context: persisted in localStorage, mirrored on <html data-theme>
// (drives the CSS var switch), and feeds the matching AntD ThemeConfig to ConfigProvider.
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
  // Toggle animation: View Transitions reveal the new theme in a circle expanding from the click point; unsupported browsers switch instantly.
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
      // useEffect is async, but the snapshot callback must mutate the DOM synchronously: set data-theme directly and flushSync the antd theme render.
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
