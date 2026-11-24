import { useEffect, useState } from 'react'
import { Button, Card, DatePicker, Empty, Input, Select, Space, Table, Tag, Tooltip, Typography } from 'antd'
import { ReloadOutlined } from '@ant-design/icons'
import { request } from '../api'
import { PageBody, PageContainer, PageHeader, PageSkeleton, ReadError } from '../components/Page'
import { useI18n } from '../i18n'

interface LogRow {
  id: number
  recordedAt: string
  actor: string
  actorId: string
  method: string
  event: string
  route: string
  status: number
  durationMs: number
}
interface LogPage { items: LogRow[]; nextBefore: number | null; retentionDays: number }

export default function SystemLogs() {
  const { lang, t } = useI18n()
  const [status, setStatus] = useState<string>()
  const [method, setMethod] = useState<string>()
  const [event, setEvent] = useState<string>()
  const [actor, setActor] = useState('')
  const [route, setRoute] = useState('')
  const [from, setFrom] = useState<string>()
  const [until, setUntil] = useState<string>()
  const [history, setHistory] = useState<number[]>([])
  const [revision, setRevision] = useState(0)
  const [data, setData] = useState<LogPage>()
  const [error, setError] = useState<unknown>()
  const [loading, setLoading] = useState(true)
  const before = history.at(-1)
  const time = new Intl.DateTimeFormat(lang, { dateStyle: 'medium', timeStyle: 'medium', timeZone: 'Asia/Shanghai' })

  useEffect(() => {
    let active = true
    const query = new URLSearchParams()
    if (status) query.set('status', status)
    if (method) query.set('method', method)
    if (event) query.set('event', event)
    if (actor) query.set('actor', actor)
    if (route) query.set('route', route)
    if (from) query.set('from', from)
    if (until) query.set('until', until)
    if (before != null) query.set('before', String(before))
    setLoading(true)
    setError(undefined)
    request<LogPage>('GET', '/system/logs?' + query).then(data => {
      if (active) setData(data)
    }).catch(error => {
      if (active) {
        setData(undefined)
        setError(error)
      }
    }).finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [before, status, method, event, actor, route, from, until, revision])

  const label = (name: string, fallback: string) => t('syslog.' + name, fallback)
  const eventLabels: Record<string, string> = {
    login: label('login', '登录'), logout: label('logout', '退出登录'),
    view: label('view', '查看'), change: label('change', '变更'),
  }
  return <PageContainer>
    <PageHeader title={label('title', '系统操作日志')}
      extra={<Button icon={<ReloadOutlined />} onClick={() => { setHistory([]); setRevision(value => value + 1) }}>
        {t('a.refresh', '刷新')}
      </Button>} />
    <PageBody>
      <Card size="small">
        <Space wrap>
          <DatePicker.RangePicker aria-label={label('dates', '审计时间（上海时间）')}
            onChange={dates => {
              setHistory([])
              setFrom(dates?.[0] ? dates[0].format('YYYY-MM-DD') + 'T00:00:00+08:00' : undefined)
              setUntil(dates?.[1] ? dates[1].add(1, 'day').format('YYYY-MM-DD') + 'T00:00:00+08:00' : undefined)
            }} />
          <Input.Search allowClear maxLength={128} style={{ width: 240 }} placeholder={label('actor', '操作人')}
            onSearch={value => { setHistory([]); setActor(value.trim()) }} />
          <Input.Search allowClear maxLength={128} placeholder={label('route', '接口')}
            onSearch={value => { setHistory([]); setRoute(value.trim()) }} />
          <Select allowClear style={{ width: 130 }} aria-label={label('event', '事件类型')} placeholder={label('event', '事件类型')} value={event}
            options={Object.entries(eventLabels).map(([value, label]) => ({ value, label }))}
            onChange={value => { setHistory([]); setEvent(value) }} />
          <Select allowClear style={{ width: 115 }} aria-label={label('method', '方法')} placeholder={label('method', '方法')} value={method}
            options={['GET', 'POST', 'PUT', 'PATCH', 'DELETE'].map(value => ({ value, label: value }))}
            onChange={value => { setHistory([]); setMethod(value) }} />
          <Select allowClear style={{ width: 100 }} aria-label={label('status', '状态')} placeholder={label('status', '状态')} value={status}
            options={['2', '3', '4', '5'].map(value => ({ value, label: value + 'xx' }))}
            onChange={value => { setHistory([]); setStatus(value) }} />
        </Space>
        {error != null ? <ReadError error={error} title={label('readFailed', '日志读取失败')} /> : loading ? <PageSkeleton /> :
          <Table<LogRow> rowKey="id" size="middle" tableLayout="fixed" dataSource={data?.items ?? []} pagination={false}
            style={{ marginTop: 12 }} scroll={{ x: 1120 }}
            locale={{ emptyText: <Empty description={label('empty', '暂无符合条件的审计记录。')} /> }}
            columns={[
              { title: label('actor', '操作人'), dataIndex: 'actor', width: 236, render: (value, row) => <div style={{ display: 'grid', minWidth: 0 }}>
                <Typography.Text strong ellipsis={{ tooltip: value || undefined }}>
                  {value || label('anonymous', '未识别账号')}
                </Typography.Text>
                {row.actorId && row.actorId !== value && <Typography.Text type="secondary" style={{ fontSize: 12 }} ellipsis={{ tooltip: row.actorId }}>
                  {row.actorId}
                </Typography.Text>}
              </div> },
              { title: label('event', '事件类型'), dataIndex: 'event', width: 110, render: (value, row) => <Tag>
                {eventLabels[value] || eventLabels[row.method === 'GET' ? 'view' : 'change']}
              </Tag> },
              { title: label('dates', '审计时间（上海时间）'), dataIndex: 'recordedAt', width: 210, render: value => time.format(new Date(value)) },
              { title: label('method', '方法'), dataIndex: 'method', width: 90 },
              { title: label('route', '接口'), dataIndex: 'route', ellipsis: { showTitle: false }, render: value => <Tooltip title={value}><code>{value}</code></Tooltip> },
              { title: label('status', '状态'), dataIndex: 'status', width: 85, render: value => <Tag color={value >= 400 ? 'error' : 'success'}>{value}</Tag> },
              { title: label('duration', '耗时'), dataIndex: 'durationMs', width: 100, align: 'right', render: value => value + ' ms' },
            ]} />}
        <Space wrap style={{ marginTop: 12 }}>
          {data && <Typography.Text type="secondary">{t('syslog.retention', '保留 {days} 天，到期自动清理。', { days: data.retentionDays })}</Typography.Text>}
          <Button disabled={!history.length || loading} onClick={() => setHistory(items => items.slice(0, -1))}>{label('previous', '上一页')}</Button>
          <Button disabled={data?.nextBefore == null || loading} onClick={() => {
            const next = data?.nextBefore
            if (next != null) setHistory(items => [...items, next])
          }}>{label('next', '下一页')}</Button>
        </Space>
      </Card>
    </PageBody>
  </PageContainer>
}
