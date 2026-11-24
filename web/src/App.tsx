import { FeatureAccess } from './components/FeatureAccess'
import { lazy, Suspense, useEffect, type ReactNode } from 'react'
import { Navigate, Route, Routes, useLocation, useNavigate } from 'react-router-dom'
import { useApp } from './context'
import { useI18n } from './i18n'
import Login from './pages/Login'
import AppShell from './components/AppShell'
import { PageSkeleton } from './components/Page'
import { PluginProvider, PluginGuard } from './plugins'
import { ProjectGuard, LegacyRedirect } from './components/RouteScope'

// Route-level lazy loading: per-page bundles; first paint only downloads login/shell, the rest load on entry.
const QualityDashboard = lazy(() => import('./plugins/sonar/QualityDashboard'))
const Devops = lazy(() => import('./plugins/yunxiao/Page'))
const Plugins = lazy(() => import('./pages/Plugins'))
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
const ProjectScripts = lazy(() => import('./pages/ProjectScripts'))
const ComingSoon = lazy(() => import('./pages/ComingSoon'))
const Agents = lazy(() => import('./pages/Agents'))
const TaskCenter = lazy(() => import('./pages/TaskCenter'))
const Review = lazy(() => import('./pages/Review'))
const EnvironmentsPage = lazy(() => import('./pages/Environments').then((m) => ({ default: m.EnvironmentsPage })))
const ResourcePoolsPage = lazy(() => import('./pages/ResourcePoolPage').then((m) => ({ default: m.ResourcePoolsPage })))
const ResourcePoolForm = lazy(() => import('./pages/ResourcePoolPage').then((m) => ({ default: m.ResourcePoolForm })))
const PublicReport = lazy(() => import('./pages/PublicReport'))
const KnowledgeQA = lazy(() => import('./pages/KnowledgeQA'))
const SystemLogs = lazy(() => import('./pages/SystemLogs'))
const SystemParams = lazy(() => import('./pages/SystemParams'))

// UI-only browsing shouldn't be blocked by login: VITE_SKIP_LOGIN=1 skips the login gate (dev builds only; production unaffected).
const SKIP_LOGIN = import.meta.env.DEV && import.meta.env.VITE_SKIP_LOGIN === '1'

export default function App() {
  const { token } = useApp()
  const { t } = useI18n()
  const loc = useLocation()
  const navigate = useNavigate()
  const internalReport = /^\/share\/report\/[^/]+\/?$/.test(loc.pathname) && new URLSearchParams(loc.search).get('access') === 'project'
  useEffect(() => {
    const key = 'shepherd.reportAfterLogin'
    if (internalReport && !token) {
      sessionStorage.setItem(key, loc.pathname + loc.search)
    } else if (token) {
      const target = sessionStorage.getItem(key)
      sessionStorage.removeItem(key)
      if (!internalReport && target?.startsWith('/share/report/')) navigate(target, { replace: true })
    }
  }, [internalReport, token, loc.pathname, loc.search, navigate])

  // Report links use the standalone viewer. Internal links require a session.
  if (loc.pathname.startsWith('/share/')) {
    if (internalReport && !token && !SKIP_LOGIN) return <Login />
    return (
      <Suspense fallback={<PageSkeleton />}>
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
    { path: 'project/scripts', element: <ProjectScripts /> },
    { path: 'project/logs', element: <ComingSoon title={t('proj.logTab', '日志')} /> },
    { path: 'test-plan', element: <TestPlans /> },
    { path: 'perf', element: <Perf /> },
    { path: 'requirement', element: <Requirements /> },
    { path: 'review', element: <Review /> },
    { path: 'agents', element: <Agents /> },
    { path: 'bug', element: <Bugs /> },
    { path: 'skill', element: <Skills /> },
    { path: 'mcp', element: <Mcp /> },
    { path: 'chat', element: <KnowledgeQA /> },
  ]

  return (
    <PluginProvider><AppShell>
      <Suspense fallback={<PageSkeleton />}>
        <Routes>
          <Route path="/" element={<Navigate to="/home" replace />} />
          {/* Global (project-agnostic) pages */}
          <Route path="/home" element={<Home />} />
          <Route path="/code-quality" element={<PluginGuard id="sonar"><QualityDashboard /></PluginGuard>} />
          <Route path="/devops" element={<PluginGuard id="yunxiao"><Devops /></PluginGuard>} />
          <Route path="/todo" element={<Todos />} />
          <Route path="/follow" element={<Follows />} />
          <Route path="/resource-pool" element={<FeatureAccess path="/resource-pool"><ResourcePoolsPage /></FeatureAccess>} />
          <Route path="/resource-pool/new" element={<FeatureAccess path="/resource-pool" action="ADD"><ResourcePoolForm /></FeatureAccess>} />
          <Route path="/resource-pool/:id/edit" element={<FeatureAccess path="/resource-pool" action="UPDATE"><ResourcePoolForm /></FeatureAccess>} />
          <Route path="/organization" element={<FeatureAccess path="/organization"><OrganizationsPage /></FeatureAccess>} />
          <Route path="/role" element={<RolesPage />} />
          <Route path="/user" element={<UsersPage />} />
          <Route path="/system/apikeys" element={<ApiKeysPage />} />
          <Route path="/system/params" element={<FeatureAccess path="/system/params"><SystemParams /></FeatureAccess>} />
          {/* Preserve the old route after moving RAG configuration to system settings. */}
          <Route path="/system/rag" element={<Navigate to="/system/params" replace />} />
          <Route path="/system/tasks" element={<FeatureAccess path="/system/tasks"><TaskCenter /></FeatureAccess>} />
          <Route path="/system/plugins" element={<FeatureAccess path="/system/plugins"><Plugins /></FeatureAccess>} />
          <Route path="/system/logs" element={<FeatureAccess path="/system/logs"><SystemLogs /></FeatureAccess>} />
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
    </AppShell></PluginProvider>
  )
}
