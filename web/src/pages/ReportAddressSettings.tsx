import { useEffect, useState } from 'react'
import { Button, Card, Form, Input, Spin, Typography } from 'antd'
import { api, ApiError, type ReportAddress } from '../api'
import { message } from '../feedback'
import { useI18n } from '../i18n'

export default function ReportAddressSettings() {
  const { t } = useI18n()
  const [settings, setSettings] = useState<ReportAddress | null>(null)
  const [baseUrl, setBaseUrl] = useState('')
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')

  useEffect(() => {
    let active = true
    api.reportAddress()
      .then((value) => {
        if (!active) return
        setSettings(value)
        setBaseUrl(value.baseUrl ?? '')
      })
      .catch(() => { if (active) setError(t('reportAddress.loadError', '无法读取报告访问地址')) })
      .finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [t])

  const save = async () => {
    setError('')
    setSaving(true)
    try {
      const value = await api.updateReportAddress(baseUrl.trim())
      setSettings(value)
      setBaseUrl(value.baseUrl ?? '')
      message.success(t('reportAddress.saved', '报告访问地址已保存'))
    } catch (e) {
      setError(e instanceof ApiError && e.status === 400
        ? t('reportAddress.invalid', '请输入带 http:// 或 https:// 的站点地址，仅可包含域名或 IP 及端口。')
        : t('reportAddress.saveError', '无法保存报告访问地址'))
    } finally {
      setSaving(false)
    }
  }

  return (
    <Card title={t('reportAddress.title', '报告访问地址')} style={{ maxWidth: 720 }}>
      {loading ? <Spin /> : (
        <Form layout="vertical" onFinish={save}>
          <Form.Item
            label={t('reportAddress.custom', '自定义地址')}
            tooltip={t('reportAddress.inputHint', '用于分享链接和通知。留空使用部署默认值。')}
            validateStatus={error ? 'error' : undefined}
            help={error || undefined}
          >
            <Input
              aria-label={t('reportAddress.custom', '自定义地址')}
              placeholder="https://shepherd.example.com"
              value={baseUrl}
              disabled={!settings?.canEdit || saving}
              onChange={(event) => { setBaseUrl(event.target.value); setError('') }}
            />
          </Form.Item>
          {settings && (
            <Form.Item label={t('reportAddress.current', '当前生效地址')}>
              <Typography.Text code>{settings.effectiveBaseUrl ?? window.location.origin}</Typography.Text>
            </Form.Item>
          )}
          {settings?.canEdit && <Button type="primary" htmlType="submit" loading={saving} disabled={baseUrl.trim() === (settings.baseUrl ?? '')}>{t('a.save', '保存')}</Button>}
        </Form>
      )}
    </Card>
  )
}
