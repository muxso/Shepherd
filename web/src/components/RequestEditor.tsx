import { useMemo, useState } from 'react'
import { AutoComplete, Button, Card, Input, Radio, Select, Space, Table, Tabs, Tag, Typography, message } from 'antd'
import { SendOutlined, PlusOutlined, DeleteOutlined } from '@ant-design/icons'
import { api, ApiError, type DebugResponse } from '../api'

const METHODS = ['GET', 'POST', 'PUT', 'DELETE', 'PATCH']
type KV = { on: boolean; key: string; value: string; desc?: string }

// MeterSphere @mock 变量(选取常用),发送前解析为随机值。
const MOCK_VARS = [
  { value: '@natural', label: '@natural — 随机自然数' },
  { value: '@integer', label: '@integer — 随机整数' },
  { value: '@bool', label: '@bool — 随机布尔' },
  { value: '@string', label: '@string — 随机字符串' },
  { value: '@guid', label: '@guid — 随机 UUID' },
  { value: '@datetime', label: '@datetime — 当前时间' },
  { value: '@email', label: '@email — 随机邮箱' },
]

function resolveMock(s: string): string {
  return s.replace(/@(\w+)(\(\s*\d+\s*,\s*\d+\s*\))?/g, (whole, name: string) => {
    switch (name) {
      case 'natural':
        return String(Math.floor(Math.random() * 100))
      case 'integer':
        return String(Math.floor(Math.random() * 200) - 100)
      case 'bool':
        return Math.random() < 0.5 ? 'true' : 'false'
      case 'string':
        return Math.random().toString(36).slice(2, 10)
      case 'guid':
        return (globalThis.crypto?.randomUUID?.() ?? String(Date.now()))
      case 'datetime':
        return new Date().toISOString()
      case 'email':
        return `user${Math.floor(Math.random() * 1000)}@example.com`
      default:
        return whole
    }
  })
}

// MeterSphere 风请求编辑器:请求行 + Query/请求头/请求体/认证 子 Tab(参数值带 @mock)+ 响应多视图。
export default function RequestEditor({
  initialMethod = 'GET',
  initialUrl = '',
}: {
  initialMethod?: string
  initialUrl?: string
}) {
  const [method, setMethod] = useState(initialMethod || 'GET')
  const [url, setUrl] = useState(initialUrl)
  const [query, setQuery] = useState<KV[]>([{ on: true, key: '', value: '', desc: '' }])
  const [headers, setHeaders] = useState<KV[]>([{ on: true, key: '', value: '', desc: '' }])
  const [body, setBody] = useState('')
  const [authType, setAuthType] = useState<'none' | 'bearer' | 'basic'>('none')
  const [authToken, setAuthToken] = useState('')
  const [respLayout, setRespLayout] = useState<'tb' | 'lr'>('tb')
  const [respView, setRespView] = useState<'json' | 'raw'>('json')
  const [sending, setSending] = useState(false)
  const [resp, setResp] = useState<DebugResponse | null>(null)
  const [err, setErr] = useState('')

  const queryCount = query.filter((q) => q.key.trim()).length
  const headerCount = headers.filter((h) => h.key.trim()).length

  const send = async () => {
    if (!url.trim()) return message.warning('请输入 URL')
    // 拼 query(解析 @mock)
    const qs = query
      .filter((q) => q.on && q.key.trim())
      .map((q) => `${encodeURIComponent(q.key)}=${encodeURIComponent(resolveMock(q.value))}`)
      .join('&')
    const finalUrl = qs ? `${url}${url.includes('?') ? '&' : '?'}${qs}` : url
    const hs = headers.filter((h) => h.on && h.key.trim()).map((h) => ({ key: h.key, value: resolveMock(h.value) }))
    if (authType === 'bearer' && authToken) hs.push({ key: 'Authorization', value: `Bearer ${authToken}` })
    if (authType === 'basic' && authToken) hs.push({ key: 'Authorization', value: `Basic ${btoa(authToken)}` })
    setSending(true)
    setErr('')
    setResp(null)
    try {
      setResp(await api.debugSend({ method, url: finalUrl, headers: hs, body: body.trim() ? resolveMock(body) : undefined }))
    } catch (e) {
      setErr(e instanceof ApiError ? e.message : '发送失败')
    } finally {
      setSending(false)
    }
  }

  const respPanel = (resp || err) && (
    <Card
      size="small"
      title={
        <Space>
          响应
          {resp && <Tag color={resp.status < 400 ? 'green' : 'red'}>{resp.status}</Tag>}
          {resp && <Typography.Text type="secondary" style={{ fontSize: 12 }}>{resp.latencyMs} ms</Typography.Text>}
        </Space>
      }
    >
      {err ? (
        <Typography.Text type="danger">{err}</Typography.Text>
      ) : (
        <Tabs
          size="small"
          items={[
            {
              key: 'body',
              label: '响应体',
              children: (
                <>
                  <Space style={{ marginBottom: 8 }}>
                    <Radio.Group size="small" value={respView} onChange={(e) => setRespView(e.target.value)} optionType="button">
                      <Radio.Button value="json">JSON</Radio.Button>
                      <Radio.Button value="raw">Raw</Radio.Button>
                    </Radio.Group>
                  </Space>
                  <pre style={{ background: '#0f1419', color: '#d6deeb', padding: 12, borderRadius: 6, maxHeight: 360, overflow: 'auto', fontSize: 12, whiteSpace: 'pre-wrap', wordBreak: 'break-all' }}>
                    {respView === 'json' ? pretty(resp?.body || '') : resp?.body || '(空)'}
                  </pre>
                </>
              ),
            },
            {
              key: 'headers',
              label: `响应头 (${resp?.headers.length || 0})`,
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
            { key: 'code', label: '响应码', children: <Tag color={(resp?.status || 0) < 400 ? 'green' : 'red'} style={{ fontSize: 14 }}>{resp?.status}</Tag> },
          ]}
        />
      )}
    </Card>
  )

  const reqPanel = (
    <>
      <Space.Compact style={{ width: '100%', marginBottom: 12 }}>
        <Select value="HTTP" disabled style={{ width: 80 }} options={[{ value: 'HTTP', label: 'HTTP' }]} />
        <Select value={method} onChange={setMethod} style={{ width: 100 }} options={METHODS.map((m) => ({ value: m, label: m }))} />
        <Input value={url} onChange={(e) => setUrl(e.target.value)} placeholder="/apis/... 或 http://..." className="ms-mono" onPressEnter={send} />
        <Button type="primary" icon={<SendOutlined />} loading={sending} onClick={send}>发送</Button>
      </Space.Compact>
      <Tabs
        size="small"
        items={[
          { key: 'query', label: `Query${queryCount ? ` (${queryCount})` : ''}`, children: <KvTable rows={query} setRows={setQuery} mock /> },
          { key: 'headers', label: `请求头${headerCount ? ` (${headerCount})` : ''}`, children: <KvTable rows={headers} setRows={setHeaders} mock /> },
          { key: 'body', label: '请求体', children: <Input.TextArea rows={8} value={body} onChange={(e) => setBody(e.target.value)} placeholder='{"username":"admin"}' className="ms-mono" /> },
          {
            key: 'auth',
            label: '认证',
            children: (
              <Space direction="vertical" style={{ width: '100%' }}>
                <Radio.Group value={authType} onChange={(e) => setAuthType(e.target.value)}>
                  <Radio value="none">无</Radio>
                  <Radio value="bearer">Bearer Token</Radio>
                  <Radio value="basic">Basic (user:pass)</Radio>
                </Radio.Group>
                {authType !== 'none' && (
                  <Input value={authToken} onChange={(e) => setAuthToken(e.target.value)} placeholder={authType === 'bearer' ? 'token' : 'user:pass'} className="ms-mono" />
                )}
              </Space>
            ),
          },
        ]}
      />
    </>
  )

  return (
    <div>
      <Space style={{ marginBottom: 8 }}>
        <Typography.Text type="secondary" style={{ fontSize: 12 }}>响应布局</Typography.Text>
        <Radio.Group size="small" value={respLayout} onChange={(e) => setRespLayout(e.target.value)} optionType="button">
          <Radio.Button value="tb">上下</Radio.Button>
          <Radio.Button value="lr">左右</Radio.Button>
        </Radio.Group>
      </Space>
      {respLayout === 'lr' && (resp || err) ? (
        <div style={{ display: 'flex', gap: 12, alignItems: 'flex-start' }}>
          <div style={{ flex: 1, minWidth: 0 }}>{reqPanel}</div>
          <div style={{ flex: 1, minWidth: 0 }}>{respPanel}</div>
        </div>
      ) : (
        <>
          {reqPanel}
          <div style={{ marginTop: 12 }}>{respPanel}</div>
        </>
      )}
    </div>
  )
}

function pretty(s: string): string {
  try {
    return JSON.stringify(JSON.parse(s), null, 2)
  } catch {
    return s || '(空)'
  }
}

// 键值表(Query / Headers 通用):启用 / 名 / 值(可选 @mock 下拉)/ 描述 / 删除。
function KvTable({ rows, setRows, mock }: { rows: KV[]; setRows: (r: KV[]) => void; mock?: boolean }) {
  const options = useMemo(() => MOCK_VARS, [])
  const upd = (i: number, patch: Partial<KV>) => setRows(rows.map((r, idx) => (idx === i ? { ...r, ...patch } : r)))
  return (
    <Space direction="vertical" style={{ width: '100%' }} size={6}>
      {rows.map((r, i) => (
        <Space.Compact key={i} style={{ width: '100%' }}>
          <Input style={{ width: 200 }} placeholder="参数名" value={r.key} onChange={(e) => upd(i, { key: e.target.value })} className="ms-mono" />
          {mock ? (
            <AutoComplete
              style={{ width: 220 }}
              value={r.value}
              onChange={(v) => upd(i, { value: v })}
              options={(r.value.includes('@') ? options : [])}
              placeholder="参数值(输入 @ 选 mock)"
            />
          ) : (
            <Input style={{ width: 220 }} placeholder="参数值" value={r.value} onChange={(e) => upd(i, { value: e.target.value })} />
          )}
          <Input placeholder="描述" value={r.desc} onChange={(e) => upd(i, { desc: e.target.value })} />
          <Button icon={<DeleteOutlined />} onClick={() => setRows(rows.filter((_, idx) => idx !== i))} />
        </Space.Compact>
      ))}
      <Button icon={<PlusOutlined />} size="small" onClick={() => setRows([...rows, { on: true, key: '', value: '', desc: '' }])}>加一行</Button>
    </Space>
  )
}
