import { createContext, useCallback, useContext, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { flushSync } from 'react-dom'
import { api, SESSION_CHANGE_EVENT, tokenStore, userIdStore, type UserThemePreference } from './api'
import type { ThemeMode } from './theme'
import { applyThemePalette, DEFAULT_PRIMARY_COLOR, normalizeThemeColor } from './themeColor'
const KEY = 'shepherd.theme', CACHE = 'shepherd.theme.user.'
const valid = (value: unknown): value is UserThemePreference => !!value && typeof value === 'object' && ['light', 'dark'].includes((value as UserThemePreference).mode) && !!normalizeThemeColor((value as UserThemePreference).primaryColor || '')
function cached(id: string): UserThemePreference | null { try { const value: unknown = JSON.parse(localStorage.getItem(CACHE + id) || 'null'); return valid(value) ? value : null } catch { return null } }
type Ctx = { mode: ThemeMode; primaryColor: string; toggle: (e?: { clientX: number; clientY: number }) => void; setMode: (m: ThemeMode) => void; saveTheme: (value: UserThemePreference) => Promise<void> }
const ThemeModeContext = createContext<Ctx>({ mode: 'dark', primaryColor: DEFAULT_PRIMARY_COLOR, toggle: () => {}, setMode: () => {}, saveTheme: async () => {} })
export const useThemeMode = () => useContext(ThemeModeContext)
export function ThemeModeProvider({ children }: { children: (mode: ThemeMode, color: string) => ReactNode }) {
  const [mode, setModeState] = useState<ThemeMode>(() => cached(userIdStore.get())?.mode || (localStorage.getItem(KEY) === 'light' ? 'light' : 'dark'))
  const [primaryColor, setPrimaryColor] = useState(() => cached(userIdStore.get())?.primaryColor || DEFAULT_PRIMARY_COLOR)
  const generation = useRef(0), lastToken = useRef<string | null>(null)
  const apply = useCallback((value: UserThemePreference) => { document.documentElement.dataset.theme=value.mode;applyThemePalette(value.primaryColor,value.mode);setModeState(value.mode);setPrimaryColor(value.primaryColor) }, [])
  useLayoutEffect(() => { document.documentElement.dataset.theme = mode; applyThemePalette(primaryColor, mode); localStorage.setItem(KEY, mode) }, [mode, primaryColor])
  useEffect(() => {
    let live = true, timer = 0
    const load = () => {
      window.clearTimeout(timer)
      timer = window.setTimeout(async () => {
        const token = tokenStore.get(), serial = ++generation.current, prior = lastToken.current
        lastToken.current = token
        if (prior !== token) {
          const value = token ? cached(userIdStore.get()) : null
          setPrimaryColor(value?.primaryColor || DEFAULT_PRIMARY_COLOR)
          if (value) apply(value); else if (prior != null) apply({mode:'dark',primaryColor:DEFAULT_PRIMARY_COLOR})
        }
        if (!token) return
        try {
          const response = await api.myTheme()
          if (!live || generation.current !== serial || token !== tokenStore.get() || typeof response.userId !== 'string') return
          if (valid(response.theme)) { apply(response.theme); localStorage.setItem(CACHE + response.userId, JSON.stringify(response.theme)) }
        } catch { /* Keep the account cache available during temporary connection failures. */ }
      }, 40)
    }
    const storage = (event: StorageEvent) => { if (event.key === 'shepherd.token' || event.key === 'shepherd.userId' || event.key?.startsWith(CACHE)) load() }
    load(); window.addEventListener(SESSION_CHANGE_EVENT, load); window.addEventListener('storage', storage)
    return () => { live = false; window.clearTimeout(timer); window.removeEventListener(SESSION_CHANGE_EVENT, load); window.removeEventListener('storage', storage) }
  }, [apply])
  const saveTheme = useCallback(async (value: UserThemePreference) => {
    const token = tokenStore.get(), serial = ++generation.current
    if (!token) throw new Error('请登录后保存主题')
    const response = await api.saveMyTheme(value)
    if (!valid(response.theme) || typeof response.userId !== 'string') throw new Error('主题保存结果不可用')
    localStorage.setItem(CACHE + response.userId, JSON.stringify(response.theme))
    if (token === tokenStore.get() && generation.current === serial) { apply(response.theme) }
  }, [apply])
  const setMode = useCallback((value: ThemeMode) => { apply({mode:value,primaryColor}); if (tokenStore.get()) void saveTheme({ mode: value, primaryColor }).catch(() => undefined) }, [primaryColor, saveTheme, apply])
  const toggle = useCallback((event?: { clientX: number; clientY: number }) => {
    const next: ThemeMode = mode === 'dark' ? 'light' : 'dark', doc = document as Document & { startViewTransition?: (callback: () => void) => { ready: Promise<void> } }
    if (!doc.startViewTransition || !event) { setMode(next); return }
    const { clientX: x, clientY: y } = event, radius = Math.hypot(Math.max(x, window.innerWidth - x), Math.max(y, window.innerHeight - y))
    const transition = doc.startViewTransition(() => flushSync(() => setMode(next)))
    transition.ready.then(() => document.documentElement.animate({ clipPath: [`circle(0px at ${x}px ${y}px)`, `circle(${radius}px at ${x}px ${y}px)`] }, { duration: 420, easing: 'ease-in-out', pseudoElement: '::view-transition-new(root)' })).catch(() => undefined)
  }, [mode, setMode])
  const value = useMemo(() => ({ mode, primaryColor, toggle, setMode, saveTheme }), [mode, primaryColor, toggle, setMode, saveTheme])
  return <ThemeModeContext.Provider value={value}>{children(mode, primaryColor)}</ThemeModeContext.Provider>
}
