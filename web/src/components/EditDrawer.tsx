import type { ComponentProps, ReactNode } from 'react'
import { Button, Space, type DrawerProps } from 'antd'
import ResizableDrawer from './ResizableDrawer'
import { useI18n } from '../i18n'

/**
 * Modal-compatible right drawer for create/edit forms: onOk / okText /
 * confirmLoading map onto a footer action bar so Modal call sites swap over
 * without logic changes.
 */
export default function EditDrawer({
  open,
  title,
  onCancel,
  onOk,
  okText,
  cancelText,
  confirmLoading,
  okButtonProps,
  footer,
  width = 520,
  children,
  ...rest
}: {
  open: boolean
  title?: ReactNode
  onCancel?: () => void
  onOk?: () => void
  okText?: ReactNode
  cancelText?: ReactNode
  confirmLoading?: boolean
  okButtonProps?: ComponentProps<typeof Button>
  footer?: ReactNode | null
  width?: number | string
  children?: ReactNode
} & Omit<DrawerProps, 'open' | 'title' | 'footer' | 'width' | 'onClose'>) {
  const { t } = useI18n()
  const foot =
    footer !== undefined ? (
      footer
    ) : (
      <Space style={{ display: 'flex', justifyContent: 'flex-end' }}>
        <Button onClick={onCancel}>{cancelText ?? t('a.cancel', '取消')}</Button>
        {onOk && (
          <Button type="primary" loading={confirmLoading} onClick={onOk} {...okButtonProps}>
            {okText ?? t('a.confirm', '确定')}
          </Button>
        )}
      </Space>
    )
  return (
    <ResizableDrawer open={open} title={title} onClose={onCancel} width={width} footer={foot} destroyOnHidden {...rest}>
      {children}
    </ResizableDrawer>
  )
}
