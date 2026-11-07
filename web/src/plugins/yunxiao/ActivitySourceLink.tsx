import type { ReactNode } from 'react'
import { ExportOutlined } from '@ant-design/icons'
import { sourceHref } from './activityModel'
export default function ActivitySourceLink({ url, children }: { url?: string | null; children: ReactNode }) {
  const href = sourceHref(url)
  return href ? <a className="yx-activity-source-link" href={href} target="_blank" rel="noopener noreferrer">{children}<ExportOutlined aria-hidden /></a> : <span>{children}</span>
}
