import { App } from 'antd'

// 全局反馈(message/modal)桥接:antd v5 的静态 message.xxx()/Modal.confirm() 不消费
// ConfigProvider 的 context(主题/语言),会告警且不走主题。这里改为从 <App> 注入的
// 实例转发——业务代码照常 `import { message, modal } from '../feedback'` 调用即可。
type AppApi = ReturnType<typeof App.useApp>
type Msg = AppApi['message']
type Mod = AppApi['modal']

let _message: Msg | null = null
let _modal: Mod | null = null

// 由 main.tsx 中 <App> 内的桥接组件在挂载后调用。
export function bindFeedback(m: Msg, md: Mod) {
  _message = m
  _modal = md
}

// 代理:实例就绪前(极早期/SSR)安全降级为 no-op,避免崩溃。
export const message = new Proxy({} as Msg, {
  get: (_t, k) => (...args: unknown[]) => (_message as unknown as Record<string, (...a: unknown[]) => unknown>)?.[k as string]?.(...args),
})
export const modal = new Proxy({} as Mod, {
  get: (_t, k) => (...args: unknown[]) => (_modal as unknown as Record<string, (...a: unknown[]) => unknown>)?.[k as string]?.(...args),
})
