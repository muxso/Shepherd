import { Button } from 'antd'
import { DownOutlined } from '@ant-design/icons'

export default function ScenarioStepExpand({ expanded, label, onToggle }: {
  expanded?: boolean
  label?: string
  onToggle?: () => void
}) {
  if (!onToggle) return <span className="ms-step-expand-spacer" aria-hidden="true" />
  return <Button type="text" size="small" className="ms-step-detail-toggle"
    aria-label={label} aria-expanded={!!expanded} icon={<DownOutlined rotate={expanded ? 0 : -90} />}
    onClick={event => { event.stopPropagation(); onToggle() }} />
}
