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
const OrganizationsPage = lazy(() => import('./pages/OrgProjects'))
const RolesPage = lazy(() => import('./pages/UserGroups'))
const UsersPage = lazy(() => import('./pages/Users'))
const ProjectAdmin = lazy(() => import('./pages/ProjectAdmin'))
const FileManagement = lazy(() => import('./pages/FileManagement'))
const ComingSoon = lazy(() => import('./pages/ComingSoon'))
const Agents = lazy(() => import('./pages/Agents'))
const Review = lazy(() => import('./pages/Review'))
const EnvironmentsPage = lazy(() => import('./pages/Environments').then((m) => ({ default: m.EnvironmentsPage })))
const ResourcePoolsPage = lazy(() => import('./pages/ResourcePoolPage').then((m) => ({ default: m.ResourcePoolsPage })))
const ResourcePoolForm = lazy(() => import('./pages/ResourcePoolPage').then((m) => ({ default: m.ResourcePoolForm })))

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
          <Route path="/resource-pool/new" element={<ResourcePoolForm />} />
          <Route path="/resource-pool/:id/edit" element={<ResourcePoolForm />} />
          <Route path="/organization" element={<OrganizationsPage />} />
          <Route path="/role" element={<RolesPage />} />
          <Route path="/user" element={<UsersPage />} />
          <Route path="/system/params" element={<ComingSoon title="系统参数" />} />
          <Route path="/system/tasks" element={<ComingSoon title="任务中心" />} />
          <Route path="/system/plugins" element={<ComingSoon title="插件" />} />
          <Route path="/system/logs" element={<ComingSoon title="日志" />} />
          <Route path="/project" element={<ProjectAdmin />} />
          <Route path="/project/templates" element={<ComingSoon title="模板管理" />} />
          <Route path="/project/files" element={<FileManagement />} />
          <Route path="/project/messages" element={<ComingSoon title="消息管理" />} />
          <Route path="/project/scripts" element={<ComingSoon title="公共脚本" />} />
          <Route path="/project/logs" element={<ComingSoon title="日志" />} />
          <Route path="/test-plan" element={<TestPlans />} />
          <Route path="/perf" element={<Perf />} />
          <Route path="/requirement" element={<Requirements />} />
          <Route path="/review" element={<Review />} />
          <Route path="/agents" element={<Agents />} />
          <Route path="/bug" element={<Bugs />} />
          <Route path="/skill" element={<Skills />} />
          <Route path="/mcp" element={<Mcp />} />
          <Route path="*" element={<Navigate to="/home" replace />} />
        </Routes>
      </Suspense>
    </AppShell>
  )
}
