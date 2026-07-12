import type { ReactNode } from 'react'
import { Empty, Typography } from 'antd'
import { useI18n } from '../i18n'

// Shared full-page skeleton:
//   <PageContainer>            full-height flex column
//     <PageHeader .../>        top toolbar (title + subtitle + middle content + right actions)
//     <PageBody>...</PageBody> scrollable body
//   </PageContainer>

export function PageContainer({ children }: { children: ReactNode }) {
  return <div style={{ height: '100%', display: 'flex', flexDirection: 'column' }}>{children}</div>
}

export function PageHeader({
  title,
  subtitle,
  children,
  extra,
}: {
  title: ReactNode
  subtitle?: ReactNode
  /** Content between the title and the right-side spring (e.g. stat Tags) */
  children?: ReactNode
  /** Right-aligned action area (buttons / search box) */
  extra?: ReactNode
}) {
  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 12,
        padding: '14px 16px',
        background: 'var(--panel)',
        borderBottom: '1px solid var(--border-soft)',
      }}
    >
      <Typography.Text strong style={{ fontSize: 15 }}>
        {title}
      </Typography.Text>
      {subtitle != null && (
        <Typography.Text type="secondary" style={{ fontSize: 12 }}>
          {subtitle}
        </Typography.Text>
      )}
      {children}
      <div style={{ flex: 1 }} />
      {extra}
    </div>
  )
}

export function PageBody({ children }: { children: ReactNode }) {
  return <div style={{ flex: 1, overflow: 'auto', padding: 16 }}>{children}</div>
}

// Shared empty state for pages that require a project selected in the top bar.
export function SelectProjectEmpty() {
  const { t } = useI18n()
  return (
    <div style={{ padding: 48 }}>
      <Empty description={t('common.selectProject', '请先在顶部选择项目')} />
    </div>
  )
}
