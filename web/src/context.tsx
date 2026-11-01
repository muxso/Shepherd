import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { api, setUnauthorizedHandler, tokenStore, userStore, type Project } from './api'
import { isProjectScoped, stripScope } from './scope'

interface AppState {
  token: string
  login: (token: string) => void
  logout: () => void
  projects: Project[]
  projectsLoaded: boolean
  projectId: string
  setProjectId: (id: string) => void
  reloadProjects: () => Promise<void>
}

const Ctx = createContext<AppState | null>(null)
const PROJECT_KEY = 'shepherd.projectId'

export function AppProvider({ children }: { children: ReactNode }) {
  const loc = useLocation()
  const navigate = useNavigate()
  const [token, setToken] = useState(tokenStore.get())
  const [projects, setProjects] = useState<Project[]>([])
  const [projectsLoaded, setProjectsLoaded] = useState(false)
  // Remembered default project: fallback for global pages / bare URLs. The URL wins when present.
  const [defaultProjectId, setDefaultProjectId] = useState(localStorage.getItem(PROJECT_KEY) || '')
  const urlProjectId = stripScope(loc.pathname).projectId
  const projectId = urlProjectId || defaultProjectId

  const logout = useCallback(() => {
    tokenStore.clear()
    userStore.clear()
    setToken('')
    setProjects([])
    setProjectsLoaded(false)
  }, [])

  useEffect(() => setUnauthorizedHandler(logout), [logout])

  // Keep the remembered default in sync with the URL's project — but only once we've confirmed
  // it's accessible, so a shared link to someone else's project never pollutes the viewer's default.
  useEffect(() => {
    if (urlProjectId && urlProjectId !== defaultProjectId && projects.some((p) => p.id === urlProjectId)) {
      setDefaultProjectId(urlProjectId)
      localStorage.setItem(PROJECT_KEY, urlProjectId)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [urlProjectId, projects])

  const setProjectId = useCallback((id: string) => {
    setDefaultProjectId(id)
    localStorage.setItem(PROJECT_KEY, id)
    const { projectId: currentProject, path } = stripScope(loc.pathname)
    if (currentProject || isProjectScoped(path)) {
      // Detail/view IDs belong to the old project. Keep the logical page and
      // unrelated query settings, but never reopen another project's entity.
      const search = new URLSearchParams(loc.search)
      if (id !== currentProject) { search.delete('open'); search.delete('view') }
      const query = search.toString()
      navigate(`/p/${encodeURIComponent(id)}${path}${query ? `?${query}` : ''}`)
    }
  }, [loc.pathname, loc.search, navigate])

  const reloadProjects = useCallback(async () => {
    if (!token) return
    // Fetch projects across all orgs and flatten (fresh installs usually have one org). /organization is paginated.
    const orgs = (await api.organizations()).items
    const lists = await Promise.all(orgs.map((o) => api.projects(o.id).then((p) => p.items).catch(() => [])))
    const all = lists.flat()
    setProjects(all)
    setProjectsLoaded(true)
    // Only fix the *default* when it's missing/stale — never override the project the URL points at
    // (a shared link to an inaccessible project must fail loud in <ProjectGuard>, not silently switch).
    setDefaultProjectId(current => {
      if (!all.length || all.some(p => p.id === current)) return current
      localStorage.setItem(PROJECT_KEY, all[0].id)
      return all[0].id
    })
  }, [token])

  useEffect(() => {
    if (token) reloadProjects().catch(() => undefined)
  }, [token, reloadProjects])

  const login = useCallback((t: string) => {
    tokenStore.set(t)
    setToken(t)
  }, [])

  const value = useMemo<AppState>(
    () => ({ token, login, logout, projects, projectsLoaded, projectId, setProjectId, reloadProjects }),
    [token, login, logout, projects, projectsLoaded, projectId, setProjectId, reloadProjects],
  )
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>
}

export function useApp() {
  const v = useContext(Ctx)
  if (!v) throw new Error('useApp must be used within AppProvider')
  return v
}
