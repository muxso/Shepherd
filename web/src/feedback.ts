import { App } from 'antd'

// Global feedback (message/modal) bridge: antd v5 static message.xxx()/Modal.confirm() don't
// consume ConfigProvider context (theme/locale) — they warn and ignore the theme. Forward to
// the instances injected by <App>; callers keep using `import { message, modal } from '../feedback'`.
type AppApi = ReturnType<typeof App.useApp>
type Msg = AppApi['message']
type Mod = AppApi['modal']

let _message: Msg | null = null
let _modal: Mod | null = null

// Called after mount by the bridge component inside <App> in main.tsx.
export function bindFeedback(m: Msg, md: Mod) {
  _message = m
  _modal = md
}

// Proxies: before the instances are ready (very early / SSR), degrade safely to no-ops.
export const message = new Proxy({} as Msg, {
  get: (_t, k) => (...args: unknown[]) => (_message as unknown as Record<string, (...a: unknown[]) => unknown>)?.[k as string]?.(...args),
})
export const modal = new Proxy({} as Mod, {
  get: (_t, k) => (...args: unknown[]) => (_modal as unknown as Record<string, (...a: unknown[]) => unknown>)?.[k as string]?.(...args),
})
