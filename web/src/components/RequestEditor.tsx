import { useEffect, useMemo, useState } from 'react'
import { AutoComplete, Button, Card, Input, Radio, Select, Space, Table, Tabs, Tag, Typography } from 'antd'
import { message } from '../feedback'
import { SendOutlined, PlusOutlined, DeleteOutlined } from '@ant-design/icons'
import { api, ApiError, type DebugResponse, type Environment } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'

const METHODS = ['GET', 'POST', 'PUT', 'DELETE', 'PATCH']

// Declarative protocol schemas for the debug console (data-driven rendering). Backend protocol plugins are
// feature-gated and pluggable — adding a protocol = one schema entry (+ backend plugin), no new render branches.
// `meta` holds the protocol's extra connection params.
interface MetaField {
  key: string
  labelKey: string
  fallback: string
  secret?: boolean
}
interface ProtoSpec {
  value: string // frontend value; lowercased it is the backend protocol name (registry key)
  proto: string // backend protocol name (used to filter by the available list)
  label: string
  urlPlaceholder: string // url semantics vary per protocol (connection string / host:port / ws URL…)
  // Placeholders containing Chinese go through i18n: phKey/phFallback resolved via t() at render (overrides urlPlaceholder).
  urlPhKey?: string
  bodyPlaceholder?: string // body = payload (command / SQL / text to send…)
  bodyPhKey?: string // i18n key for the body placeholder (when it contains Chinese)
  bodyPhFallback?: string
  meta?: MetaField[] // extra params → debug/send.meta (e.g. ssh user/password, grpc method)
  httpMethod?: boolean // method dropdown shown for HTTP only
}
const PROTOCOLS: ProtoSpec[] = [
  { value: 'HTTP', proto: 'http', label: 'HTTP', urlPlaceholder: '/apis/... 或 http://...', httpMethod: true },
  { value: 'REDIS', proto: 'redis', label: 'Redis', urlPlaceholder: 'redis://host:port/0', bodyPlaceholder: 'PING / SET k v / GET k' },
  {
    value: 'SSH',
    proto: 'ssh',
    label: 'SSH',
    urlPlaceholder: 'host:port(默认 22)',
    urlPhKey: 'editor.sshUrlPlaceholder',
    bodyPlaceholder: '要执行的命令',
    bodyPhKey: 'editor.sshBodyPlaceholder',
    bodyPhFallback: '要执行的命令',
    meta: [
      { key: 'user', labelKey: 'editor.sshUser', fallback: '用户名' },
      { key: 'password', labelKey: 'editor.sshPass', fallback: '密码', secret: true },
    ],
  },
  { value: 'SQL', proto: 'sql', label: 'SQL(PG)', urlPlaceholder: 'postgres://user:pass@host:port/db', bodyPlaceholder: 'SELECT 1' },
  { value: 'MYSQL', proto: 'mysql', label: 'MySQL', urlPlaceholder: 'mysql://user:pass@host:port/db', bodyPlaceholder: 'SELECT 1' },
  { value: 'WEBSOCKET', proto: 'websocket', label: 'WebSocket', urlPlaceholder: 'ws://host:port/path', bodyPlaceholder: '(可选)发送文本', bodyPhKey: 'editor.wsBodyPlaceholder', bodyPhFallback: '(可选)发送文本' },
  {
    value: 'GRPC',
    proto: 'grpc',
    label: 'gRPC',
    urlPlaceholder: 'http://host:port',
    bodyPlaceholder: '(请求字节,可空)',
    bodyPhKey: 'editor.grpcBodyPlaceholder',
    bodyPhFallback: '(请求字节,可空)',
    meta: [{ key: 'method', labelKey: 'editor.grpcMethod', fallback: '完整方法路径,如 pkg.Service/Method' }],
  },
]
type KV = { on: boolean; key: string; value: string; desc?: string }

// Common @mock variables, resolved to random values before sending.
function makeMockVars(t: (key: string, fallback?: string) => string) {
  return [
    { value: '@natural', label: t('editor.mockNatural', '@natural — 随机自然数') },
    { value: '@integer', label: t('editor.mockInteger', '@integer — 随机整数') },
    { value: '@bool', label: t('editor.mockBool', '@bool — 随机布尔') },
    { value: '@string', label: t('editor.mockString', '@string — 随机字符串') },
    { value: '@guid', label: t('editor.mockGuid', '@guid — 随机 UUID') },
    { value: '@datetime', label: t('editor.mockDatetime', '@datetime — 当前时间') },
    { value: '@email', label: t('editor.mockEmail', '@email — 随机邮箱') },
  ]
}

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

// Request editor: request line + query/headers/body/auth sub-tabs (values support @mock) + multi-view response.
export default function RequestEditor({
  initialMethod = 'GET',
  initialUrl = '',
  lockedProtocol,
  embedded = false,
}: {
  initialMethod?: string
  initialUrl?: string
  /** When embedded in API-definition debug: protocol comes from the definition and is locked (an HTTP definition can't switch to Redis/SSH…). */
  lockedProtocol?: string
  /** Embedded mode: method/path follow the definition line; the request line keeps only env + send (no duplicate protocol/method/path). */
  embedded?: boolean
}) {
  const { t } = useI18n()
  const { projectId } = useApp()
  const [method, setMethod] = useState(initialMethod || 'GET')
  // When locked, use the definition protocol (uppercased to match PROTOCOLS.value); otherwise default HTTP, switchable.
  const [protocol, setProtocol] = useState(() => (lockedProtocol ? lockedProtocol.toUpperCase() : 'HTTP'))
  const [metaValues, setMetaValues] = useState<Record<string, string>>({})
  const [availProtos, setAvailProtos] = useState<string[]>(['http'])
  // Environment: provides baseUrl (prefix for relative paths) + default headers + {{variables}}.
  // Without one, relative paths cannot be sent, so the debug console wires environments in.
  const [envs, setEnvs] = useState<Environment[]>([])
  const [envId, setEnvId] = useState<string>('')
  const env = envs.find((e) => e.id === envId)
  useEffect(() => {
    api.debugProtocols().then((r) => setAvailProtos(r.protocols)).catch(() => undefined)
  }, [])
  useEffect(() => {
    if (!projectId) {
      setEnvs([])
      return
    }
    api
      .environments(projectId)
      .then((list) => {
        const arr = Array.isArray(list) ? list : []
        setEnvs(arr)
        // Default to the first enabled environment so send works out of the box.
        setEnvId((cur) => cur || arr.find((e) => e.enabled !== false)?.id || '')
      })
      .catch(() => setEnvs([]))
  }, [projectId])

  // Resolve {{var}} with environment variables (unknown vars kept as-is).
  const resolveVars = (s: string): string =>
    env?.variables ? s.replace(/\{\{\s*(\w+)\s*\}\}/g, (whole, k: string) => env.variables?.[k] ?? whole) : s
  const spec = PROTOCOLS.find((p) => p.value === protocol) || PROTOCOLS[0]
  const protoOptions = PROTOCOLS.filter((p) => availProtos.includes(p.proto)).map((p) => ({ value: p.value, label: p.label }))
  const [url, setUrl] = useState(initialUrl)
  // Embedded in definition debug: method/path follow the definition line and track its edits (standalone console unaffected).
  useEffect(() => {
    if (!lockedProtocol) return
    setMethod(initialMethod || 'GET')
    setUrl(initialUrl)
  }, [lockedProtocol, initialMethod, initialUrl])
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
    if (!url.trim()) return message.warning(t('editor.urlRequired', '请输入 URL'))
    // Non-HTTP protocols (redis/ssh/…): url = connection target, body = payload (command), sent via the probe plugin.
    if (protocol !== 'HTTP') {
      setSending(true)
      setErr('')
      setResp(null)
      try {
        const meta = spec.meta?.length
          ? Object.fromEntries(spec.meta.map((f) => [f.key, metaValues[f.key] || '']))
          : undefined
        setResp(await api.debugSend({ protocol: protocol.toLowerCase(), method, url: url.trim(), body: body.trim() || undefined, meta }))
      } catch (e) {
        setErr(e instanceof ApiError ? e.message : t('editor.sendFail', '发送失败'))
      } finally {
        setSending(false)
      }
      return
    }
    // Resolve {{variables}}, then absolutize relative paths with the environment baseUrl.
    const raw = resolveVars(url.trim())
    let base = raw
    if (!/^https?:\/\//i.test(raw)) {
      const baseUrl = env?.baseUrl?.trim().replace(/\/+$/, '')
      if (!baseUrl) {
        // Debug send connects from the server: relative paths need an environment baseUrl, otherwise nothing to dial.
        return message.warning(t('editor.needEnvOrAbs', '相对路径需先选择带 baseUrl 的环境,或填写绝对 URL(http(s)://)'))
      }
      base = `${baseUrl}${raw.startsWith('/') ? '' : '/'}${raw}`
    }
    // Build the query string (resolving @mock + {{variables}})
    const qs = query
      .filter((q) => q.on && q.key.trim())
      .map((q) => `${encodeURIComponent(q.key)}=${encodeURIComponent(resolveMock(resolveVars(q.value)))}`)
      .join('&')
    const finalUrl = qs ? `${base}${base.includes('?') ? '&' : '?'}${qs}` : base
    // Inject environment default headers first; explicit headers can override (last write wins, backend processes in order).
    const hs: { key: string; value: string }[] = []
    for (const eh of env?.headers || []) if (eh.name?.trim()) hs.push({ key: eh.name, value: resolveVars(eh.value || '') })
    for (const h of headers.filter((h) => h.on && h.key.trim())) hs.push({ key: h.key, value: resolveMock(resolveVars(h.value)) })
    if (authType === 'bearer' && authToken) hs.push({ key: 'Authorization', value: `Bearer ${authToken}` })
    if (authType === 'basic' && authToken) hs.push({ key: 'Authorization', value: `Basic ${btoa(authToken)}` })
    setSending(true)
    setErr('')
    setResp(null)
    try {
      setResp(await api.debugSend({ method, url: finalUrl, headers: hs, body: body.trim() ? resolveMock(resolveVars(body)) : undefined }))
    } catch (e) {
      setErr(e instanceof ApiError ? e.message : t('editor.sendFail', '发送失败'))
    } finally {
      setSending(false)
    }
  }

  const respPanel = (resp || err) && (
    <Card
      size="small"
      title={
        <Space>
          {t('editor.response', '响应')}
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
              label: t('editor.respBody', '响应体'),
              children: (
                <>
                  <Space style={{ marginBottom: 8 }}>
                    <Radio.Group size="small" value={respView} onChange={(e) => setRespView(e.target.value)} optionType="button">
                      <Radio.Button value="json">JSON</Radio.Button>
                      <Radio.Button value="raw">Raw</Radio.Button>
                    </Radio.Group>
                  </Space>
                  <pre style={{ background: 'var(--panel-2)', color: 'var(--text)', border: '1px solid var(--border-soft)', padding: 12, borderRadius: 6, maxHeight: 360, overflow: 'auto', fontSize: 12, whiteSpace: 'pre-wrap', wordBreak: 'break-all' }}>
                    {respView === 'json' ? pretty(resp?.body || '', t('editor.empty', '(空)')) : resp?.body || t('editor.empty', '(空)')}
                  </pre>
                </>
              ),
            },
            {
              key: 'headers',
              label: `${t('editor.respHeaders', '响应头')} (${resp?.headers.length || 0})`,
              children: (
                <Table
                  size="small"
                  pagination={false}
                  rowKey={(_, i) => String(i)}
                  dataSource={(resp?.headers || []).map(([k, v]) => ({ k, v }))}
                  columns={[
                    { title: t('editor.colName', '名'), dataIndex: 'k', width: 220 },
                    { title: t('editor.colValue', '值'), dataIndex: 'v', render: (v: string) => <span className="ms-mono">{v}</span> },
                  ]}
                />
              ),
            },
            { key: 'code', label: t('editor.respCode', '响应码'), children: <Tag color={(resp?.status || 0) < 400 ? 'green' : 'red'} style={{ fontSize: 14 }}>{resp?.status}</Tag> },
          ]}
        />
      )}
    </Card>
  )

  const reqPanel = (
    <>
      <Space.Compact style={{ width: '100%', marginBottom: 12 }}>
        {/* Embedded mode: protocol/method/path live in the definition line above; keep only env + send to avoid two request lines. */}
        {!embedded && (
          lockedProtocol ? (
            <Select value={protocol} disabled style={{ width: 120 }} options={[{ value: protocol, label: spec.label }]} />
          ) : (
            <Select value={protocol} onChange={setProtocol} style={{ width: 120 }} options={protoOptions} />
          )
        )}
        {!embedded && spec.httpMethod && (
          <Select value={method} onChange={setMethod} style={{ width: 100 }} popupMatchSelectWidth={false} options={METHODS.map((m) => ({ value: m, label: m }))} />
        )}
        {/* Env picker: for HTTP provides baseUrl + default headers + {{variables}}. Empty = no env (absolute URL required). */}
        {spec.httpMethod && (
          <Select
            value={envId || undefined}
            onChange={setEnvId}
            style={embedded ? { flex: 1, minWidth: 0 } : { width: 168 }}
            placeholder={t('editor.selectEnv', '选择环境')}
            allowClear
            options={envs.map((e) => ({ value: e.id, label: e.baseUrl ? `${e.name} · ${e.baseUrl}` : e.name }))}
            notFoundContent={t('editor.noEnvConfigured', '未配置环境,去「环境」页新建')}
          />
        )}
        {!embedded && (
          <Input value={url} onChange={(e) => setUrl(e.target.value)} placeholder={spec.httpMethod ? t('editor.urlPlaceholder', '/apis/... 或 http://...') : spec.urlPhKey ? t(spec.urlPhKey, spec.urlPlaceholder) : spec.urlPlaceholder} className="ms-mono" onPressEnter={send} />
        )}
        <Button type="primary" icon={<SendOutlined />} loading={sending} onClick={send}>{t('a.send', '发送')}</Button>
      </Space.Compact>
      {spec.httpMethod && env?.baseUrl && (
        <Typography.Text type="secondary" style={{ fontSize: 12, display: 'block', marginTop: -6, marginBottom: 10 }}>
          {t('editor.effectiveUrl', '实际请求')}: <span className="ms-mono">{/^https?:\/\//i.test(url.trim()) ? url.trim() : `${env.baseUrl.replace(/\/+$/, '')}${url.trim().startsWith('/') ? '' : '/'}${url.trim()}`}</span>
        </Typography.Text>
      )}
      {spec.meta?.length ? (
        <Space.Compact style={{ width: '100%', marginBottom: 12 }}>
          {spec.meta.map((f) => {
            const C = f.secret ? Input.Password : Input
            return (
              <C
                key={f.key}
                style={{ flex: 1 }}
                value={metaValues[f.key] || ''}
                onChange={(e) => setMetaValues((v) => ({ ...v, [f.key]: e.target.value }))}
                placeholder={t(f.labelKey, f.fallback)}
                className={f.secret ? undefined : 'ms-mono'}
              />
            )
          })}
        </Space.Compact>
      ) : null}
      <Tabs
        size="small"
        items={[
          { key: 'query', label: `Query${queryCount ? ` (${queryCount})` : ''}`, children: <KvTable rows={query} setRows={setQuery} mock /> },
          { key: 'headers', label: `${t('editor.reqHeaders', '请求头')}${headerCount ? ` (${headerCount})` : ''}`, children: <KvTable rows={headers} setRows={setHeaders} mock /> },
          { key: 'body', label: t('editor.reqBody', '请求体'), children: <Input.TextArea rows={8} value={body} onChange={(e) => setBody(e.target.value)} placeholder={(spec.bodyPhKey ? t(spec.bodyPhKey, spec.bodyPhFallback) : spec.bodyPlaceholder) || '{"username":"admin"}'} className="ms-mono" /> },
          {
            key: 'auth',
            label: t('editor.auth', '认证'),
            children: (
              <Space direction="vertical" style={{ width: '100%' }}>
                <Radio.Group value={authType} onChange={(e) => setAuthType(e.target.value)}>
                  <Radio value="none">{t('editor.authNone', '无')}</Radio>
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
        <Typography.Text type="secondary" style={{ fontSize: 12 }}>{t('editor.respLayout', '响应布局')}</Typography.Text>
        <Radio.Group size="small" value={respLayout} onChange={(e) => setRespLayout(e.target.value)} optionType="button">
          <Radio.Button value="tb">{t('editor.layoutTb', '上下')}</Radio.Button>
          <Radio.Button value="lr">{t('editor.layoutLr', '左右')}</Radio.Button>
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

function pretty(s: string, empty = '(空)'): string {
  try {
    return JSON.stringify(JSON.parse(s), null, 2)
  } catch {
    return s || empty
  }
}

// Key-value table (shared by query/headers): enabled / name / value (optional @mock dropdown) / description / delete.
function KvTable({ rows, setRows, mock }: { rows: KV[]; setRows: (r: KV[]) => void; mock?: boolean }) {
  const { t } = useI18n()
  const options = useMemo(() => makeMockVars(t), [t])
  const upd = (i: number, patch: Partial<KV>) => setRows(rows.map((r, idx) => (idx === i ? { ...r, ...patch } : r)))
  return (
    <Space direction="vertical" style={{ width: '100%' }} size={6}>
      {rows.map((r, i) => (
        <Space.Compact key={i} style={{ width: '100%' }}>
          <Input style={{ width: 200 }} placeholder={t('editor.paramName', '参数名')} value={r.key} onChange={(e) => upd(i, { key: e.target.value })} className="ms-mono" />
          {mock ? (
            <AutoComplete
              style={{ width: 220 }}
              value={r.value}
              onChange={(v) => upd(i, { value: v })}
              options={(r.value.includes('@') ? options : [])}
              placeholder={t('editor.paramValueMock', '参数值(输入 @ 选 mock)')}
            />
          ) : (
            <Input style={{ width: 220 }} placeholder={t('editor.paramValue', '参数值')} value={r.value} onChange={(e) => upd(i, { value: e.target.value })} />
          )}
          <Input placeholder={t('editor.desc', '描述')} value={r.desc} onChange={(e) => upd(i, { desc: e.target.value })} />
          <Button icon={<DeleteOutlined />} onClick={() => setRows(rows.filter((_, idx) => idx !== i))} />
        </Space.Compact>
      ))}
      <Button icon={<PlusOutlined />} size="small" onClick={() => setRows([...rows, { on: true, key: '', value: '', desc: '' }])}>{t('editor.addRow', '加一行')}</Button>
    </Space>
  )
}
