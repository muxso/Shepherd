import { useEffect, useState } from 'react'
import { Alert, Checkbox, Form, Input, Modal } from 'antd'
import { request } from '../../api'
import { useI18n } from '../../i18n'

type Settings = {
  dashboardBase: string | null
  webhookConfigured: boolean
  messageTemplate: unknown
}

export default function NotificationSettings({ source, onClose }: { source: string; onClose: () => void }) {
  const { t } = useI18n()
  const path = '/api/sonar/sources/' + encodeURIComponent(source) + '/notification-settings'
  const [settings, setSettings] = useState<Settings>()
  const [dashboardBase, setDashboardBase] = useState('')
  const [webhookUrl, setWebhookUrl] = useState('')
  const [resetWebhook, setResetWebhook] = useState(false)
  const [template, setTemplate] = useState('')
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')

  useEffect(() => {
    let active = true
    request<Settings>('GET', path).then(result => {
      if (!active) return
      setSettings(result)
      setDashboardBase(result.dashboardBase ?? '')
      setTemplate(result.messageTemplate == null ? '' : JSON.stringify(result.messageTemplate, null, 2))
    }).catch((err: Error) => {
      if (active) setError(err.message)
    }).finally(() => {
      if (active) setLoading(false)
    })
    return () => { active = false }
  }, [path])

  async function save() {
    let messageTemplate: unknown = null
    if (template.trim()) {
      try {
        messageTemplate = JSON.parse(template)
        if (!messageTemplate || typeof messageTemplate !== 'object' || Array.isArray(messageTemplate)) throw new Error()
      } catch {
        setError(t('sonar.notify.invalidJson'))
        return
      }
    }
    setError('')
    setSaving(true)
    try {
      await request<void>('PUT', path, {
        dashboardBase: dashboardBase.trim() || null,
        messageTemplate,
        webhookUrl: resetWebhook ? null : webhookUrl.trim() || null,
        resetWebhook,
      })
      onClose()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setSaving(false)
    }
  }

  return <Modal title={t('sonar.notify.title')} open onCancel={onClose} onOk={() => void save()}
    confirmLoading={saving} okButtonProps={{ disabled: !settings }} loading={loading}>
    {error && <Alert type="error" showIcon message={error} />}
    {settings && <Form layout="vertical">
      <Alert type="info" showIcon message={t('sonar.notify.projectRules')}
        style={{ marginBottom: 16 }} />
      <Form.Item label={t('sonar.notify.dashboard')} extra={t('sonar.notify.defaultWhenBlank')}>
        <Input value={dashboardBase} onChange={event => setDashboardBase(event.target.value)} />
      </Form.Item>
      <Form.Item label={t('sonar.notify.webhook')}
        extra={t(settings.webhookConfigured ? 'sonar.notify.webhookConfigured' : 'sonar.notify.webhookInherited')}>
        <Input.Password value={webhookUrl} disabled={resetWebhook} autoComplete="new-password"
          onChange={event => setWebhookUrl(event.target.value)} />
        {settings.webhookConfigured && <Checkbox checked={resetWebhook} onChange={event => setResetWebhook(event.target.checked)}>
          {t('sonar.notify.resetWebhook')}
        </Checkbox>}
      </Form.Item>
      <Form.Item label={t('sonar.notify.template')}
        extra={<>{t('sonar.notify.defaultWhenBlank')} <code>{'${project}, ${status}, ${summary}, ${rows}, ${color}, ${dashboard}'}</code></>}>
        <Input.TextArea value={template} onChange={event => setTemplate(event.target.value)} rows={8} spellCheck={false} />
      </Form.Item>
    </Form>}
  </Modal>
}
