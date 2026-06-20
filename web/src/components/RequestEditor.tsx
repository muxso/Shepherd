import { useState } from 'react'
import { Button, Card, Input, Select, Space, Table, Tabs, Tag, Typography, message } from 'antd'
import { SendOutlined, PlusOutlined, DeleteOutlined } from '@ant-design/icons'
import { api, ApiError, type DebugResponse } from '../api'
import { methodColor } from './tags'

const METHODS = ['GET', 'POST', 'PUT', 'DELETE', 'PATCH']
type HeaderRow = { key: string; value: string }

// MeterSphere 风请求调试台:方法 + URL + Headers/Body 子 Tab + 发送 + 响应面板。
export default function RequestEditor({
  initialMethod = 'GET',
  initialUrl = '',
}: {
  initialMethod?: string
  initialUrl?: string
}) {
  const [method, setMethod] = useState(initialMethod || 'GET')
  const [url, setUrl] = useState(initialUrl)
  const [headers, setHeaders] = useState<HeaderRow[]>([{ key: '', value: '' }])
  const [body, setBody] = useState('')
  const [sending, setSending] = useState(false)
  const [resp, setResp] = useState<DebugResponse | null>(null)
  const [err, setErr] = useState('')

  const send = async () => {
    if (!url.trim()) {
      message.warning('请输入 URL')
      return
    }
    setSending(true)
    setErr('')
    setResp(null)
    try {
      const r = await api.debugSend({
        method,
        url,
        headers: headers.filter((h) => h.key.trim()),
        body: body.trim() ? body : undefined,
      })
      setResp(r)
    } catch (e) {
      setErr(e instanceof ApiError ? e.message : '发送失败')
    } finally {
      setSending(false)
    }
  }

  const setHeader = (i: number, patch: Partial<HeaderRow>) =>
    setHeaders((hs) => hs.map((h, idx) => (idx === i ? { ...h, ...patch } : h)))

  return (
    <Space direction="vertical" style={{ width: '100%' }} size={12}>
      <Space.Compact style={{ width: '100%' }}>
        <Select value={method} onChange={setMethod} style={{ width: 110 }} options={METHODS.map((m) => ({ value: m, label: m }))} />
        <Input
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          placeholder="http://127.0.0.1:9180/healthz"
          className="ms-mono"
          onPressEnter={send}
        />
        <Button type="primary" icon={<SendOutlined />} loading={sending} onClick={send}>
          发送
        </Button>
      </Space.Compact>

      <Tabs
        size="small"
        items={[
          {
            key: 'headers',
            label: 'Headers',
            children: (
              <Space direction="vertical" style={{ width: '100%' }}>
                {headers.map((h, i) => (
                  <Space.Compact key={i} style={{ width: '100%' }}>
                    <Input placeholder="名" value={h.key} onChange={(e) => setHeader(i, { key: e.target.value })} style={{ width: 200 }} />
                    <Input placeholder="值" value={h.value} onChange={(e) => setHeader(i, { value: e.target.value })} />
                    <Button icon={<DeleteOutlined />} onClick={() => setHeaders((hs) => hs.filter((_, idx) => idx !== i))} />
                  </Space.Compact>
                ))}
                <Button icon={<PlusOutlined />} size="small" onClick={() => setHeaders((hs) => [...hs, { key: '', value: '' }])}>
                  加一行
                </Button>
              </Space>
            ),
          },
          {
            key: 'body',
            label: 'Body',
            children: (
              <Input.TextArea
                rows={6}
                value={body}
                onChange={(e) => setBody(e.target.value)}
                placeholder='{"username":"admin"}'
                className="ms-mono"
              />
            ),
          },
        ]}
      />

      {(resp || err) && (
        <Card size="small" title={
          <Space>
            响应
            {resp && <Tag color={resp.status < 400 ? 'green' : 'red'}>{resp.status}</Tag>}
            {resp && <Typography.Text type="secondary" style={{ fontSize: 12 }}>{resp.latencyMs} ms</Typography.Text>}
            <Tag color={methodColor(method)}>{method}</Tag>
          </Space>
        }>
          {err ? (
            <Typography.Text type="danger">{err}</Typography.Text>
          ) : (
            <Tabs
              size="small"
              items={[
                {
                  key: 'rbody',
                  label: 'Body',
                  children: (
                    <pre style={{ background: '#0f1419', color: '#d6deeb', padding: 12, borderRadius: 6, maxHeight: 320, overflow: 'auto', fontSize: 12, whiteSpace: 'pre-wrap', wordBreak: 'break-all' }}>
                      {resp?.body || '(空)'}
                    </pre>
                  ),
                },
                {
                  key: 'rheaders',
                  label: `Headers (${resp?.headers.length || 0})`,
                  children: (
                    <Table
                      size="small"
                      pagination={false}
                      rowKey={(_, i) => String(i)}
                      dataSource={(resp?.headers || []).map(([k, v]) => ({ k, v }))}
                      columns={[
                        { title: '名', dataIndex: 'k', width: 220 },
                        { title: '值', dataIndex: 'v', render: (v: string) => <span className="ms-mono">{v}</span> },
                      ]}
                    />
                  ),
                },
              ]}
            />
          )}
        </Card>
      )}
    </Space>
  )
}
