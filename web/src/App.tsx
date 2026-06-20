import { Navigate, Route, Routes } from 'react-router-dom'
import { useApp } from './context'
import Login from './pages/Login'
import AppShell from './components/AppShell'
import ApiDefinitions from './pages/ApiDefinitions'
import Scenarios from './pages/Scenarios'
import {
  OrganizationsPage,
  RolesPage,
  UsersPage,
  ProjectsPage,
  FunctionalCasesPage,
  EnvironmentsPage,
  ResourcePoolsPage,
  Placeholder,
} from './pages/resources'

export default function App() {
  const { token } = useApp()
  if (!token) return <Login />

  return (
    <AppShell>
      <Routes>
        <Route path="/" element={<Navigate to="/api/definition" replace />} />
        <Route path="/api/definition" element={<ApiDefinitions />} />
        <Route path="/api/scenario" element={<Scenarios />} />
        <Route path="/functional-case" element={<FunctionalCasesPage />} />
        <Route path="/environment" element={<EnvironmentsPage />} />
        <Route path="/resource-pool" element={<ResourcePoolsPage />} />
        <Route path="/organization" element={<OrganizationsPage />} />
        <Route path="/role" element={<RolesPage />} />
        <Route path="/user" element={<UsersPage />} />
        <Route path="/project" element={<ProjectsPage />} />
        {/* 下一波接入 */}
        <Route path="/test-plan" element={<Placeholder name="测试计划" />} />
        <Route path="/perf" element={<Placeholder name="性能压测" />} />
        <Route path="/requirement" element={<Placeholder name="需求(版本/基线)" />} />
        <Route path="/orchestration" element={<Placeholder name="拆分 / 交付 / 验证" />} />
        <Route path="/bug" element={<Placeholder name="缺陷" />} />
        <Route path="/skill" element={<Placeholder name="技能" />} />
        <Route path="/mcp" element={<Placeholder name="MCP 工具" />} />
        <Route path="*" element={<Navigate to="/api/definition" replace />} />
      </Routes>
    </AppShell>
  )
}
