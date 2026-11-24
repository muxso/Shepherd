import { useEffect, useState } from 'react'
import { Button, Card, Drawer, Empty, Input, InputNumber, Select, Space, Switch, Tag, Typography } from 'antd'
import { BellOutlined, PlusOutlined } from '@ant-design/icons'
import { ApiError, request } from '../../api'
import { useApp } from '../../context'
import { allows } from '../../components/FeatureAccess'
import { PageSkeleton, ReadError } from '../../components/Page'
import { message } from '../../feedback'

const events = {
  overview: '概览',
  radar: '个人雷达指标',
  delivery: '需求产出',
  bugs: '缺陷统计',
  authors: '团队产出',
  dwell: '状态停留时间',
  records: '明细记录',
}
type Event = keyof typeof events
type Rule = {
  id: string
  event: Event
  time: string
  weekdays: number[]
  group?: string
  recordKind?: string
  status?: string
  minimumMatches: number
  enabled: boolean
}
type Settings = {
  configured: boolean
  revision: number
  rules: Rule[]
  recent: { ruleId: string; localDate: string; outcome: string }[]
}

const weekdays = ['周一', '周二', '周三', '周四', '周五', '周六', '周日']
const recordKinds = { requirement: '需求', bug: '缺陷', commit: '提交', testcase: '测试用例' }
const outcomes: Record<string, string> = {
  running: '执行中', sent: '已发送', condition_not_met: '未满足条件',
  query_failed: '统计读取失败', delivery_failed: '机器人发送失败', interrupted: '执行中断',
}

function ruleId() {
  const bytes = crypto.getRandomValues(new Uint8Array(16))
  bytes[6] = (bytes[6] & 15) | 64
  bytes[8] = (bytes[8] & 63) | 128
  const value = Array.from(bytes, byte => byte.toString(16).padStart(2, '0')).join('')
  return `${value.slice(0, 8)}-${value.slice(8, 12)}-${value.slice(12, 16)}-${value.slice(16, 20)}-${value.slice(20)}`
}

export default function NotificationSettings({ source }: { source: string }) {
  const { permissions } = useApp()
  const [open, setOpen] = useState(false)
  if (!source || !allows(permissions, 'SYSTEM_USER', 'UPDATE') ||
      !['DEVOPS', 'PROJECT', 'REQUIREMENT', 'BUG', 'FUNCTIONAL_CASE'].every(resource => allows(permissions, resource, 'READ'))) return null

  return <>
    <Button icon={<BellOutlined />} onClick={() => setOpen(true)}>通知设置</Button>
    {open && <SettingsDrawer key={source} source={source} onClose={() => setOpen(false)} />}
  </>
}

function SettingsDrawer({ source, onClose }: { source: string; onClose: () => void }) {
  const path = `/api/devops/sources/${encodeURIComponent(source)}/notification-settings`
  const [settings, setSettings] = useState<Settings>()
  const [rules, setRules] = useState<Rule[]>([])
  const [webhook, setWebhook] = useState('')
  const [removeWebhook, setRemoveWebhook] = useState(false)
  const [error, setError] = useState<unknown>()
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    let active = true
    request<Settings>('GET', path).then(value => {
      if (active) { setSettings(value); setRules(value.rules) }
    }).catch(error => { if (active) setError(error) })
    return () => { active = false }
  }, [path])

  const change = (id: string, update: Partial<Rule>) =>
    setRules(items => items.map(item => item.id === id ? { ...item, ...update } : item))

  const save = async () => {
    if (!settings) return
    if (rules.some(rule => !/^([01]\d|2[0-3]):[0-5]\d$/.test(rule.time) || !rule.weekdays.length)) {
      message.error('请为每条规则填写推送时间并选择推送日期。')
      return
    }
    setSaving(true)
    try {
      const result = await request<Settings>('PUT', path, {
        revision: settings.revision,
        webhook: removeWebhook ? '' : webhook.trim() || undefined,
        rules: rules.map(rule => ({ ...rule, group: rule.group?.trim() || undefined, status: rule.status?.trim() || undefined })),
      })
      setSettings(result)
      setRules(result.rules)
      setWebhook('')
      setRemoveWebhook(false)
      message.success('通知设置已保存')
    } catch (error) {
      const status = error instanceof ApiError ? error.status : undefined
      message.error(status === 409 ? '设置已被更新，请重新打开后再保存。'
        : status === 400 ? '请检查企业微信机器人的 Webhook 地址和推送规则。' : '无法保存通知设置。')
    } finally {
      setSaving(false)
    }
  }

  return <Drawer title="云效通知设置" open width="min(680px, 100vw)" onClose={onClose}
    extra={<Button type="primary" disabled={!settings} loading={saving} onClick={() => void save()}>保存</Button>}>
    {error != null ? <ReadError error={error} title="无法读取通知设置" /> : !settings ? <PageSkeleton /> :
      <Space direction="vertical" size="middle" style={{ width: '100%' }}>
        <Card size="small" title="企业微信机器人" extra={<Tag>{settings.configured && !removeWebhook ? '已配置' : '未配置'}</Tag>}>
          <Input.Password aria-label="企业微信 Webhook" value={webhook} autoComplete="new-password"
            placeholder={settings.configured && !removeWebhook ? '留空保留现有 Webhook' : '粘贴企业微信机器人 Webhook'}
            onChange={event => { setWebhook(event.target.value); setRemoveWebhook(false) }} />
          {settings.configured && <Button type="link" size="small" onClick={() => {
            setRemoveWebhook(value => !value); setWebhook('')
          }}>{removeWebhook ? '撤销移除' : '移除机器人'}</Button>}
        </Card>
        <Space style={{ justifyContent: 'space-between', width: '100%' }}>
          <Typography.Text strong>定时规则 · 上海时间</Typography.Text>
          <Button icon={<PlusOutlined />} disabled={rules.length >= 20} onClick={() => setRules(items => [...items, {
            id: ruleId(), event: 'overview', time: '09:00', weekdays: [0, 1, 2, 3, 4], minimumMatches: 1, enabled: false,
          }])}>添加规则</Button>
        </Space>
        {!rules.length && <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无定时规则" />}
        {rules.map(rule => {
          const latest = settings.recent.find(item => item.ruleId === rule.id)
          return <Card key={rule.id} size="small" title={events[rule.event]} extra={<Space>
            <Switch size="small" aria-label="启用规则" checked={rule.enabled} onChange={enabled => change(rule.id, { enabled })} />
            <Button type="link" size="small" danger onClick={() => setRules(items => items.filter(item => item.id !== rule.id))}>移除</Button>
          </Space>}>
            <Space wrap style={{ rowGap: 12 }}>
              <Select aria-label="推送事件" value={rule.event} style={{ width: 155 }}
                options={Object.entries(events).map(([value, label]) => ({ value, label }))}
                onChange={event => change(rule.id, { event, status: undefined, recordKind: undefined, group: event === 'dwell' ? undefined : rule.group })} />
              <Input type="time" aria-label="推送时间" style={{ width: 120 }} value={rule.time} onChange={event => change(rule.id, { time: event.target.value })} />
              <Select mode="multiple" aria-label="推送日期" value={rule.weekdays} maxTagCount="responsive" style={{ width: 190 }}
                options={weekdays.map((label, value) => ({ value, label }))}
                onChange={weekdays => change(rule.id, { weekdays })} />
              {rule.event !== 'dwell' && <Input aria-label="分组" placeholder="分组名称（可选）" maxLength={128} value={rule.group}
                onChange={event => change(rule.id, { group: event.target.value })} />}
              {(rule.event === 'dwell' || rule.event === 'records') && <Select aria-label="记录类型" value={rule.recordKind ?? 'requirement'} style={{ width: 110 }}
                options={Object.entries(recordKinds).filter(([key]) => rule.event !== 'dwell' || ['requirement', 'bug'].includes(key))
                  .map(([value, label]) => ({ value, label }))}
                onChange={recordKind => change(rule.id, { recordKind })} />}
              {rule.event === 'records' && <Input aria-label="记录状态" placeholder="状态（可选）" maxLength={80} value={rule.status}
                onChange={event => change(rule.id, { status: event.target.value })} />}
              <Space size={6}><span>至少匹配</span><InputNumber aria-label="最低匹配数量" min={0} max={1000000} precision={0}
                value={rule.minimumMatches} onChange={value => change(rule.id, { minimumMatches: value ?? 0 })} />
                <span>{['radar', 'delivery'].includes(rule.event) ? '位成员' : '条'}</span></Space>
            </Space>
            {latest && <div style={{ marginTop: 8 }}><Typography.Text type="secondary">
              {latest.localDate} · {outcomes[latest.outcome] ?? latest.outcome}
            </Typography.Text></div>}
          </Card>
        })}
      </Space>}
  </Drawer>
}
