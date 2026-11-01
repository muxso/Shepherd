import { lazy, Suspense, type ComponentType, type LazyExoticComponent } from 'react'
import { usePlugins } from '.'

interface HomeExtensionProps { projectId: string }
interface HomeExtension { pluginId: string; component: LazyExoticComponent<ComponentType<HomeExtensionProps>> }
// The workbench depends on extension slots, not a vendor's APIs or components.
const homeExtensions: HomeExtension[] = [
  { pluginId: 'yunxiao', component: lazy(() => import('./yunxiao/Productivity')) },
]
export function PluginHomeExtensions(props: HomeExtensionProps) {
  const { enabled } = usePlugins()
  return <>{homeExtensions.filter(extension => enabled(extension.pluginId)).map(({ pluginId, component: Component }) =>
    <Suspense key={pluginId} fallback={null}><Component {...props} /></Suspense>)}</>
}
