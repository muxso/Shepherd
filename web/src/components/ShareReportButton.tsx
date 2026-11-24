import { useId, useState } from 'react'
import { Button, Input, InputNumber, Modal, Space, Typography } from 'antd'
import { ShareAltOutlined } from '@ant-design/icons'
import { api, ApiError, type ReportShare } from '../api'
import { copyToClipboard } from '../clipboard'
import { message, modal } from '../feedback'
import { useI18n } from '../i18n'

export default function ShareReportButton({ createShare, size }: {
  createShare: (expiresInDays: number) => Promise<ReportShare>
  size?: 'small'
}) {
  const { t } = useI18n()
  const [open, setOpen] = useState(false)
  const [days, setDays] = useState<number | null>(7)
  const [sharing, setSharing] = useState(false)
  const valid = typeof days === 'number' && Number.isInteger(days) && days >= 1 && days <= 365

  const share = async (event: React.MouseEvent<HTMLButtonElement>) => {
    if (!valid || days === null || sharing) return
    const host = event.currentTarget.parentElement
    setSharing(true)
    try {
      const { effectiveBaseUrl } = await api.reportAddress()
      const { token, expiresAt } = await createShare(days)
      const url = new URL(`/share/report/${encodeURIComponent(token)}`, effectiveBaseUrl ?? window.location.origin).href
      const expiration = `${t('report.shareExpiresAt', '到期时间')} ${new Date(expiresAt).toLocaleString()}`
      const copied = await copyToClipboard(url, host)
      setOpen(false)
      if (copied) {
        message.success(`${t('report.shareCopied', '分享链接已复制,无需登录即可访问')} · ${expiration}`)
        return
      }
      const hint = t('report.shareCopyManually', '自动复制失败，请手动复制分享链接')
      modal.info({
        title: hint,
        content: (
          <Space direction="vertical" style={{ width: '100%' }}>
            <Input value={url} readOnly aria-label={hint} onFocus={(e) => e.target.select()} />
            <Typography.Text type="secondary">{expiration}</Typography.Text>
          </Space>
        ),
      })
    } catch (error) {
      message.error(error instanceof ApiError ? error.message : t('report.shareFailed', '生成分享链接失败'))
    } finally {
      setSharing(false)
    }
  }

  const daysId = useId()
  const daysLabel = t('report.shareDays', '有效天数')
  return (
    <>
      <Button size={size} icon={<ShareAltOutlined />} onClick={() => setOpen(true)}>
        {t('report.share', '分享报告')}
      </Button>
      <Modal
        open={open}
        title={t('report.share', '分享报告')}
        onCancel={() => { if (!sharing) setOpen(false) }}
        onOk={share}
        okText={t('report.shareGenerate', '生成并复制链接')}
        confirmLoading={sharing}
        okButtonProps={{ disabled: !valid }}
      >
        <Space direction="vertical">
          <Typography.Text type="secondary">{t('report.shareAccess', '有效期内，任何获得链接的人都可以访问报告，无需登录。')}</Typography.Text>
          <label htmlFor={daysId}>{daysLabel}</label>
          <Space>
            <InputNumber id={daysId} aria-label={daysLabel} value={days} onChange={setDays} min={1} max={365} disabled={sharing} />
            <Typography.Text type="secondary">{t('report.shareDaysRange', '天（1–365）')}</Typography.Text>
          </Space>
        </Space>
      </Modal>
    </>
  )
}
