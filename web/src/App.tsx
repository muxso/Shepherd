import { lazy, Suspense, type ReactNode } from 'react'
import { Navigate, Route, Routes, useLocation } from 'react-router-dom'
import { Spin } from 'antd'
import { useApp } from './context'
import { useI18n } from './i18n'
import Login from './pages/Login'
import AppShell from './components/AppShell'
import { ProjectGuard, LegacyRedirect } from './components/RouteScope'

// Route-level lazy loading: per-page bundles; first paint only downloads login/shell, the rest load on entry.
const Home = lazy(() => import('./pages/Home'))
const Todos = lazy(() => import('./pages/Todos'))
const Follows = lazy(() => import('./pages/Follows'))
const ApiDefinitions = lazy(() => import('./pages/ApiDefinitions'))
const Scenarios = lazy(() => import('./pages/Scenarios'))
const ScenarioRecycle = lazy(() => import('./pages/ScenarioRecycle'))
const TestPlans = lazy(() => import('./pages/TestPlans'))
const Perf = lazy(() => import('./pages/Perf'))
const Requirements = lazy(() => import('./pages/Requirements'))
const Bugs = lazy(() => import('./pages/Bugs'))
const Skills = lazy(() => import('./pages/Skills'))
const Mcp = lazy(() => import('./pages/Mcp'))
const FunctionalCases = lazy(() => import('./pages/FunctionalCases'))
const OrganizationsPage = lazy(() => import('./pages/OrgProjects'))
const RolesPage = lazy(() => import('./pages/UserGroups'))
const UsersPage = lazy(() => import('./pages/Users'))
const ApiKeysPage = lazy(() => import('./pages/ApiKeys'))
const ProjectAdmin = lazy(() => import('./pages/ProjectAdmin'))
const ProjectTemplates = lazy(() => import('./pages/ProjectTemplates'))
const FileManagement = lazy(() => import('./pages/FileManagement'))
const MessageSettings = lazy(() => import('./pages/MessageSettings'))
const ComingSoon = lazy(() => import('./pages/ComingSoon'))
const Agents = lazy(() => import('./pages/Agents'))
const TaskCenter = lazy(() => import('./pages/TaskCenter'))
const Review = lazy(() => import('./pages/Review'))
const EnvironmentsPage = lazy(() => import('./pages/Environments').then((m) => ({ default: m.EnvironmentsPage })))
const ResourcePoolsPage = lazy(() => import('./pages/ResourcePoolPage').then((m) => ({ default: m.ResourcePoolsPage })))
const ResourcePoolForm = lazy(() => import('./pages/ResourcePoolPage').then((m) => ({ default: m.ResourcePoolForm })))
const PublicReport = lazy(() => import('./pages/PublicReport'))

// UI-only browsing shouldn't be blocked by login: VITE_SKIP_LOGIN=1 skips the login gate (dev builds only; production unaffected).
const SKIP_LOGIN = import.meta.env.DEV && import.meta.env.VITE_SKIP_LOGIN === '1'

export default function App() {
  const { token } = useApp()
  const { t } = useI18n()
  const loc = useLocation()

  // Public share pages render standalone — no app shell, no login gate.
  if (loc.pathname.startsWith('/share/')) {
    return (
      <Suspense fallback={<div style={{ display: 'flex', justifyContent: 'center', paddingTop: 120 }}><Spin size="large" /></div>}>
        <Routes>
          <Route path="/share/:kind/:token" element={<PublicReport />} />
          <Route path="*" element={<PublicReport />} />
        </Routes>
      </Suspense>
    )
  }

  if (!token && !SKIP_LOGIN) return <Login />

  // Project-scoped pages: mounted BOTH under `/p/:projectId/...` (canonical, shareable) and at
  // their bare path (legacy → redirected to the scoped form). Keep this list in sync with
  // PROJECT_SCOPED_PREFIXES in scope.ts.
  const projectRoutes: { path: string; element: ReactNode }[] = [
    { path: 'api/definition', element: <ApiDefinitions /> },
    { path: 'api/scenario', element: <Scenarios /> },
    { path: 'api/scenario/recycle-bin', element: <ScenarioRecycle /> },
    { path: 'functional-case', element: <FunctionalCases /> },
    { path: 'environment', element: <EnvironmentsPage /> },
    { path: 'project', element: <ProjectAdmin /> },
    { path: 'project/templates', element: <ProjectTemplates /> },
    { path: 'project/files', element: <FileManagement /> },
    { path: 'project/messages', element: <MessageSettings /> },
    { path: 'project/scripts', element: <ComingSoon title={t('proj.scriptTab', '公共脚本')} /> },
    { path: 'project/logs', element: <ComingSoon title={t('proj.logTab', '日志')} /> },
    { path: 'test-plan', element: <TestPlans /> },
    { path: 'perf', element: <Perf /> },
    { path: 'requirement', element: <Requirements /> },
    { path: 'review', element: <Review /> },
    { path: 'agents', element: <Agents /> },
    { path: 'bug', element: <Bugs /> },
    { path: 'skill', element: <Skills /> },
    { path: 'mcp', element: <Mcp /> },
  ]

  return (
    <AppShell>
      <Suspense fallback={<div style={{ display: 'flex', justifyContent: 'center', paddingTop: 120 }}><Spin size="large" /></div>}>
        <Routes>
          <Route path="/" element={<Navigate to="/home" replace />} />
          {/* Global (project-agnostic) pages */}
          <Route path="/home" element={<Home />} />
          <Route path="/todo" element={<Todos />} />
          <Route path="/follow" element={<Follows />} />
          <Route path="/resource-pool" element={<ResourcePoolsPage />} />
          <Route path="/resource-pool/new" element={<ResourcePoolForm />} />
          <Route path="/resource-pool/:id/edit" element={<ResourcePoolForm />} />
          <Route path="/organization" element={<OrganizationsPage />} />
          <Route path="/role" element={<RolesPage />} />
          <Route path="/user" element={<UsersPage />} />
          <Route path="/system/apikeys" element={<ApiKeysPage />} />
          <Route path="/system/params" element={<ComingSoon title={t('sys.params', '系统参数')} />} />
          <Route path="/system/tasks" element={<TaskCenter />} />
          <Route path="/system/plugins" element={<ComingSoon title={t('sys.plugins', '插件')} />} />
          <Route path="/system/logs" element={<ComingSoon title={t('sys.logs', '日志')} />} />
          {/* Project-scoped canonical routes: /p/:projectId/... behind the access guard */}
          <Route path="/p/:projectId" element={<ProjectGuard />}>
            {projectRoutes.map((r) => (
              <Route key={r.path} path={r.path} element={r.element} />
            ))}
          </Route>
          {/* Legacy bare project paths → redirect to the scoped form (keeps old links/bookmarks working) */}
          {projectRoutes.map((r) => (
            <Route key={`legacy-${r.path}`} path={`/${r.path}`} element={<LegacyRedirect />} />
          ))}
          <Route path="*" element={<Navigate to="/home" replace />} />
        </Routes>
      </Suspense>
    </AppShell>
  )
}
