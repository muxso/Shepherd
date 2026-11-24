import type { ReactNode } from 'react'
import { Card } from 'antd'
import { useApp } from '../context'
import { AccessDenied, PageBody, PageContainer, PageSkeleton } from './Page'

const features: Readonly<Record<string, readonly [string, string]>> = {
  '/organization': ['ORGANIZATION', 'READ'],
  '/resource-pool': ['RESOURCE_POOL', 'READ'],
  '/system/params': ['SYSTEM_PARAMETER', 'READ'],
  '/system/tasks': ['TASK_CENTER', 'READ'],
  '/system/plugins': ['PLUGIN', 'READ'],
  '/system/logs': ['SYSTEM_LOG', 'READ'],
}

export function allows(permissions: string[] | null, resource: string, action: string) {
  return permissions?.some(raw => {
    const [name, actions] = raw.toUpperCase().split(':')
    return name === resource && actions?.split('+').includes(action)
  }) ?? false
}

export function canOpenFeature(permissions: string[] | null, path: string) {
  const feature = features[path]
  return !feature || allows(permissions, ...feature)
}

export function FeatureAccess({ path, action, children }: { path: string; action?: string; children: ReactNode }) {
  const { permissions } = useApp()
  if (!permissions) return <PageSkeleton />
  const feature = features[path]
  if (!feature || allows(permissions, feature[0], action ?? feature[1])) return <>{children}</>
  return <PageContainer><PageBody><Card size="small"><AccessDenied /></Card></PageBody></PageContainer>
}
