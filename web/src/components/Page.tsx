import type { ReactNode } from 'react'
import { Empty, Typography } from 'antd'
import { useI18n } from '../i18n'

// 抽象出各业务页反复手写的整页骨架:
//   <PageContainer>            整页 flex 纵向容器(占满高度)
//     <PageHeader .../>        顶部工具栏(标题 + 副标题 + 中间内容 + 右侧操作)
//     <PageBody>...</PageBody> 可滚动的主体区
//   </PageContainer>
// 之前每个页面都复制粘贴同一套内联 style,这里收敛为一处。

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
  /** 标题与右侧弹簧之间的内容(如统计 Tag) */
  children?: ReactNode
  /** 右侧操作区(按钮 / 搜索框等),自动靠右 */
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

// 未选择项目时的统一空状态(顶部需要先选项目的页面共用)。
export function SelectProjectEmpty() {
  const { t } = useI18n()
  return (
    <div style={{ padding: 48 }}>
      <Empty description={t('common.selectProject', '请先在顶部选择项目')} />
    </div>
  )
}
