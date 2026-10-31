import { createContext, useContext, useEffect, useState, type ReactNode } from 'react'
import { Alert, Spin } from 'antd'
import { pluginApi, type Plugin } from '../api'

const Context = createContext<{ items: Plugin[]; loading: boolean; error: string }>({ items: [], loading: true, error: '' })

export function PluginProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<Plugin[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  useEffect(() => {
    let live = true
    pluginApi.list().then(data => { if (live) setItems(data.items) })
      .catch(e => { if (live) setError(String(e.message || e)) })
      .finally(() => { if (live) setLoading(false) })
    return () => { live = false }
  }, [])
  return <Context.Provider value={{ items, loading, error }}>{children}</Context.Provider>
}

export function usePlugins() {
  const state = useContext(Context)
  return { ...state, enabled: (id: string) => state.items.some(p => p.id === id && p.enabled) }
}

export function PluginGuard({ id, children }: { id: string; children: ReactNode }) {
  const { enabled, loading, error } = usePlugins()
  if (loading) return <Spin />
  if (error) return <Alert type="error" message={error} />
  if (!enabled(id)) return <Alert type="info" message="插件未启用" description="请在系统的插件页面查看启用方式。" />
  return <>{children}</>
}
