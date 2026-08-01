import { Navigate, Outlet, useLocation, useParams } from 'react-router-dom'
import { Button, Empty, Spin } from 'antd'
import { useApp } from '../context'
import { useI18n } from '../i18n'

/**
 * Guards project-scoped routes (`/p/:projectId/...`). The URL's project is the source of
 * truth; here we validate the recipient can actually access it. A shared link to a project
 * the viewer isn't a member of fails loud (clear message) rather than silently rendering
 * the viewer's own project's data.
 */
export function ProjectGuard() {
  const { projectId } = useParams()
  const { projects, projectsLoaded, setProjectId } = useApp()
  const { t } = useI18n()

  if (!projectsLoaded) {
    return (
      <div style={{ display: 'flex', justifyContent: 'center', paddingTop: 120 }}>
        <Spin size="large" />
      </div>
    )
  }
  const ok = projects.some((p) => p.id === projectId)
  if (!ok) {
    return (
      <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', paddingTop: 120, gap: 12 }}>
        <Empty
          description={
            <div style={{ maxWidth: 420, textAlign: 'center' }}>
              <div style={{ fontWeight: 600, marginBottom: 6 }}>{t('scope.noAccessTitle', '无法访问该项目')}</div>
              <div style={{ color: 'var(--text-3)', fontSize: 13 }}>
                {t('scope.noAccessBody', '链接指向的项目不存在，或你没有访问权限。请向分享者确认，或切换到你有权限的项目。')}
              </div>
            </div>
          }
        />
        {projects.length > 0 && (
          <Button type="primary" onClick={() => setProjectId(projects[0].id)}>
            {t('scope.switchToMine', '切换到我的项目')}
          </Button>
        )}
      </div>
    )
  }
  return <Outlet />
}

/**
 * Redirects a legacy bare project path (`/api/definition?...`) to its scoped form
 * (`/p/<defaultPid>/api/definition?...`) so old links / bookmarks keep working.
 */
export function LegacyRedirect() {
  const { projectId, projectsLoaded } = useApp()
  const loc = useLocation()
  if (!projectId) {
    // No project resolvable yet: wait for the project list, then this re-renders.
    if (!projectsLoaded) return <div style={{ display: 'flex', justifyContent: 'center', paddingTop: 120 }}><Spin size="large" /></div>
    return <Navigate to="/home" replace />
  }
  return <Navigate to={`/p/${projectId}${loc.pathname}${loc.search}`} replace />
}
