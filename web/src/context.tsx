import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from 'react'
import { api, setUnauthorizedHandler, tokenStore, userStore, type Project } from './api'

interface AppState {
  token: string
  login: (token: string) => void
  logout: () => void
  projects: Project[]
  projectId: string
  setProjectId: (id: string) => void
  reloadProjects: () => Promise<void>
}

const Ctx = createContext<AppState | null>(null)
const PROJECT_KEY = 'shepherd.projectId'

export function AppProvider({ children }: { children: ReactNode }) {
  const [token, setToken] = useState(tokenStore.get())
  const [projects, setProjects] = useState<Project[]>([])
  const [projectId, setProjectIdState] = useState(localStorage.getItem(PROJECT_KEY) || '')

  const logout = () => {
    tokenStore.clear()
    userStore.clear()
    setToken('')
    setProjects([])
  }

  useEffect(() => setUnauthorizedHandler(logout), [])

  const setProjectId = (id: string) => {
    setProjectIdState(id)
    localStorage.setItem(PROJECT_KEY, id)
  }

  const reloadProjects = async () => {
    if (!token) return
    // 取所有组织的项目并铺平(系统初装通常单组织)。/organization 为分页结构。
    const orgs = (await api.organizations()).items
    const lists = await Promise.all(orgs.map((o) => api.projects(o.id).then((p) => p.items).catch(() => [])))
    const all = lists.flat()
    setProjects(all)
    if (all.length && !all.some((p) => p.id === projectId)) setProjectId(all[0].id)
  }

  useEffect(() => {
    if (token) reloadProjects().catch(() => undefined)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [token])

  const login = (t: string) => {
    tokenStore.set(t)
    setToken(t)
  }

  const value = useMemo<AppState>(
    () => ({ token, login, logout, projects, projectId, setProjectId, reloadProjects }),
    [token, projects, projectId],
  )
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>
}

export function useApp() {
  const v = useContext(Ctx)
  if (!v) throw new Error('useApp must be used within AppProvider')
  return v
}
