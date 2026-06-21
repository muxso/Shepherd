import { lazy, Suspense } from 'react'
import { Navigate, Route, Routes } from 'react-router-dom'
import { Spin } from 'antd'
import { useApp } from './context'
import Login from './pages/Login'
import AppShell from './components/AppShell'

// 路由级懒加载:按页拆分 bundle,首屏只下登录/外壳,其余进入时按需加载。
const Home = lazy(() => import('./pages/Home'))
const ApiDefinitions = lazy(() => import('./pages/ApiDefinitions'))
const Scenarios = lazy(() => import('./pages/Scenarios'))
const TestPlans = lazy(() => import('./pages/TestPlans'))
const Perf = lazy(() => import('./pages/Perf'))
const Requirements = lazy(() => import('./pages/Requirements'))
const Bugs = lazy(() => import('./pages/Bugs'))
const Skills = lazy(() => import('./pages/Skills'))
const Mcp = lazy(() => import('./pages/Mcp'))
const FunctionalCases = lazy(() => import('./pages/FunctionalCases'))
const OrganizationsPage = lazy(() => import('./pages/resources').then((m) => ({ default: m.OrganizationsPage })))
const RolesPage = lazy(() => import('./pages/resources').then((m) => ({ default: m.RolesPage })))
const UsersPage = lazy(() => import('./pages/resources').then((m) => ({ default: m.UsersPage })))
const ProjectsPage = lazy(() => import('./pages/resources').then((m) => ({ default: m.ProjectsPage })))
const EnvironmentsPage = lazy(() => import('./pages/resources').then((m) => ({ default: m.EnvironmentsPage })))
const ResourcePoolsPage = lazy(() => import('./pages/resources').then((m) => ({ default: m.ResourcePoolsPage })))

export default function App() {
  const { token } = useApp()
  if (!token) return <Login />

  return (
    <AppShell>
      <Suspense fallback={<div style={{ display: 'flex', justifyContent: 'center', paddingTop: 120 }}><Spin size="large" /></div>}>
        <Routes>
          <Route path="/" element={<Navigate to="/home" replace />} />
          <Route path="/home" element={<Home />} />
          <Route path="/api/definition" element={<ApiDefinitions />} />
          <Route path="/api/scenario" element={<Scenarios />} />
          <Route path="/functional-case" element={<FunctionalCases />} />
          <Route path="/environment" element={<EnvironmentsPage />} />
          <Route path="/resource-pool" element={<ResourcePoolsPage />} />
          <Route path="/organization" element={<OrganizationsPage />} />
          <Route path="/role" element={<RolesPage />} />
          <Route path="/user" element={<UsersPage />} />
          <Route path="/project" element={<ProjectsPage />} />
          <Route path="/test-plan" element={<TestPlans />} />
          <Route path="/perf" element={<Perf />} />
          <Route path="/requirement" element={<Requirements />} />
          <Route path="/bug" element={<Bugs />} />
          <Route path="/skill" element={<Skills />} />
          <Route path="/mcp" element={<Mcp />} />
          <Route path="*" element={<Navigate to="/home" replace />} />
        </Routes>
      </Suspense>
    </AppShell>
  )
}
