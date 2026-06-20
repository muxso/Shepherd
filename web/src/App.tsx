import { Navigate, Route, Routes } from 'react-router-dom'
import { useApp } from './context'
import Login from './pages/Login'
import AppShell from './components/AppShell'
import ApiDefinitions from './pages/ApiDefinitions'
import Scenarios from './pages/Scenarios'

export default function App() {
  const { token } = useApp()
  if (!token) return <Login />

  return (
    <AppShell>
      <Routes>
        <Route path="/" element={<Navigate to="/api/definition" replace />} />
        <Route path="/api/definition" element={<ApiDefinitions />} />
        <Route path="/api/scenario" element={<Scenarios />} />
        <Route path="*" element={<Navigate to="/api/definition" replace />} />
      </Routes>
    </AppShell>
  )
}
