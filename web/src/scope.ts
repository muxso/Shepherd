// URL project scoping. The URL is the single source of truth for which project a
// view belongs to: project-scoped pages live under `/p/:projectId/...`, so a link
// (or a shared URL) always carries the project. Global pages (home / system / org /
// resource-pool) stay top-level. localStorage only holds a *fallback default* for
// bare URLs — the URL always wins.

import { useLocation, useNavigate, type NavigateOptions } from 'react-router-dom'

const PROJECT_KEY = 'shepherd.projectId' // keep in sync with context.tsx (avoid a circular import)

// Logical path prefixes (WITHOUT the `/p/:projectId` segment) that are project-scoped.
export const PROJECT_SCOPED_PREFIXES = [
  '/api',
  '/functional-case',
  '/test-plan',
  '/perf',
  '/bug',
  '/requirement',
  '/review',
  '/environment',
  '/project',
  '/agents',
  '/skill',
  '/mcp',
  '/chat',
]

/** True when `path` (a logical path, no `/p/:pid`) belongs under a project. */
export function isProjectScoped(path: string): boolean {
  return PROJECT_SCOPED_PREFIXES.some((p) => path === p || path.startsWith(p + '/'))
}

const SCOPE_RE = /^\/p\/([^/]+)(\/.*)?$/

/** Split `/p/<pid>/api/definition` → { projectId, path: '/api/definition' }. Non-scoped pathname passes through as { path }. */
export function stripScope(pathname: string): { projectId?: string; path: string } {
  const m = pathname.match(SCOPE_RE)
  if (!m) return { path: pathname }
  return { projectId: m[1], path: m[2] || '/' }
}

/** Prefix a logical path with the project segment when it's project-scoped and a pid is known.
 *  Accepts a trailing query/hash (e.g. "/test-plan?open=x") — matched on the bare path, prefixed whole. */
export function scopedPath(path: string, projectId?: string): string {
  if (!projectId) return path
  const q = path.search(/[?#]/)
  const bare = q === -1 ? path : path.slice(0, q)
  if (!isProjectScoped(bare)) return path
  return `/p/${projectId}${path}`
}

/** navigate() that auto-prefixes project-scoped targets with the current URL's project (or the
 *  remembered default when on a global page). Global targets pass through unchanged. */
export function useScopedNavigate() {
  const navigate = useNavigate()
  const loc = useLocation()
  return (to: string, opts?: NavigateOptions) => {
    const pid = stripScope(loc.pathname).projectId || localStorage.getItem(PROJECT_KEY) || undefined
    navigate(scopedPath(to, pid), opts)
  }
}
