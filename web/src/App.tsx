import { Navigate, Route, Routes } from 'react-router-dom'
import { useApp } from './context'
import Login from './pages/Login'
import AppShell from './components/AppShell'
import ApiDefinitions from './pages/ApiDefinitions'
import Scenarios from './pages/Scenarios'
import TestPlans from './pages/TestPlans'
import Perf from './pages/Perf'
import Requirements from './pages/Requirements'
import Bugs from './pages/Bugs'
import Skills from './pages/Skills'
import Mcp from './pages/Mcp'
import FunctionalCases from './pages/FunctionalCases'
import {
  OrganizationsPage,
  RolesPage,
  UsersPage,
  ProjectsPage,
  EnvironmentsPage,
  ResourcePoolsPage,
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
        <Route path="*" element={<Navigate to="/api/definition" replace />} />
      </Routes>
    </AppShell>
  )
}
