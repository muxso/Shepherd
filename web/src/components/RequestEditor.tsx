import { useEffect, useMemo, useState } from 'react'
import { AutoComplete, Button, Card, Input, Radio, Select, Space, Table, Tabs, Tag, Typography } from 'antd'
import { message } from '../feedback'
import { SendOutlined, PlusOutlined, DeleteOutlined } from '@ant-design/icons'
import { api, ApiError, type DebugResponse, type Environment } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'

const METHODS = ['GET', 'POST', 'PUT', 'DELETE', 'PATCH']

// 调试台协议 schema(声明式,数据驱动渲染)。后端协议插件 feature 门控、可动态插拔——
// 新增一个协议=加一条 schema(+ 后端插件),无需改渲染分支。meta 为该协议的额外连接参数。
interface MetaField {
  key: string
  labelKey: string
  fallback: string
  secret?: boolean
}
interface ProtoSpec {
  value: string // 前端值;小写后即后端协议名(registry key)
  proto: string // 后端协议名(用于按可用列表过滤)
  label: string
  urlPlaceholder: string // url 字段语义随协议而变(连接串/host:port/ws URL…)
  bodyPlaceholder?: string // body=载荷(命令/SQL/发送文本…)
  meta?: MetaField[] // 额外参数 → debug/send.meta(如 ssh user/password、grpc method)
  httpMethod?: boolean // 仅 HTTP 显示方法下拉
}
const PROTOCOLS: ProtoSpec[] = [
  { value: 'HTTP', proto: 'http', label: 'HTTP', urlPlaceholder: '/apis/... 或 http://...', httpMethod: true },
  { value: 'REDIS', proto: 'redis', label: 'Redis', urlPlaceholder: 'redis://host:port/0', bodyPlaceholder: 'PING / SET k v / GET k' },
  {
    value: 'SSH',
    proto: 'ssh',
    label: 'SSH',
    urlPlaceholder: 'host:port(默认 22)',
    bodyPlaceholder: '要执行的命令',
    meta: [
      { key: 'user', labelKey: 'editor.sshUser', fallback: '用户名' },
      { key: 'password', labelKey: 'editor.sshPass', fallback: '密码', secret: true },
    ],
  },
  { value: 'SQL', proto: 'sql', label: 'SQL(PG)', urlPlaceholder: 'postgres://user:pass@host:port/db', bodyPlaceholder: 'SELECT 1' },
  { value: 'MYSQL', proto: 'mysql', label: 'MySQL', urlPlaceholder: 'mysql://user:pass@host:port/db', bodyPlaceholder: 'SELECT 1' },
  { value: 'WEBSOCKET', proto: 'websocket', label: 'WebSocket', urlPlaceholder: 'ws://host:port/path', bodyPlaceholder: '(可选)发送文本' },
  {
    value: 'GRPC',
    proto: 'grpc',
    label: 'gRPC',
    urlPlaceholder: 'http://host:port',
    bodyPlaceholder: '(请求字节,可空)',
    meta: [{ key: 'method', labelKey: 'editor.grpcMethod', fallback: '完整方法路径,如 pkg.Service/Method' }],
  },
]
type KV = { on: boolean; key: string; value: string; desc?: string }

// MeterSphere @mock 变量(选取常用),发送前解析为随机值。
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

// MeterSphere 风请求编辑器:请求行 + Query/请求头/请求体/认证 子 Tab(参数值带 @mock)+ 响应多视图。
export default function RequestEditor({
  initialMethod = 'GET',
  initialUrl = '',
}: {
  initialMethod?: string
  initialUrl?: string
}) {
  const { t } = useI18n()
  const { projectId } = useApp()
  const [method, setMethod] = useState(initialMethod || 'GET')
  const [protocol, setProtocol] = useState('HTTP')
  const [metaValues, setMetaValues] = useState<Record<string, string>>({})
  const [availProtos, setAvailProtos] = useState<string[]>(['http'])
  // 环境:提供 baseUrl(相对路径前缀)+ 默认头 + {{变量}}。没有环境就无法对相对路径发请求——
  // 这正是「环境都没有怎么发送」的根因,这里把环境接进调试台。
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
        // 默认选中首个启用的环境,让「发送」开箱即用。
        setEnvId((cur) => cur || arr.find((e) => e.enabled !== false)?.id || '')
      })
      .catch(() => setEnvs([]))
  }, [projectId])

  // {{var}} 用环境变量解析(找不到保持原样)。
  const resolveVars = (s: string): string =>
    env?.variables ? s.replace(/\{\{\s*(\w+)\s*\}\}/g, (whole, k: string) => env.variables?.[k] ?? whole) : s
  const spec = PROTOCOLS.find((p) => p.value === protocol) || PROTOCOLS[0]
  const protoOptions = PROTOCOLS.filter((p) => availProtos.includes(p.proto)).map((p) => ({ value: p.value, label: p.label }))
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
    if (!url.trim()) return message.warning(t('editor.urlRequired', '请输入 URL'))
    // 非 HTTP 协议(redis/ssh/…):url=连接目标,body=载荷(命令),走 probe 插件。
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
    // 解析 {{变量}},再据环境 baseUrl 把相对路径补成绝对 URL。
    const raw = resolveVars(url.trim())
    let base = raw
    if (!/^https?:\/\//i.test(raw)) {
      const baseUrl = env?.baseUrl?.trim().replace(/\/+$/, '')
      if (!baseUrl) {
        // 调试发送在服务端直连目标:相对路径必须有环境 baseUrl 兜底,否则无从发起。
        return message.warning(t('editor.needEnvOrAbs', '相对路径需先选择带 baseUrl 的环境,或填写绝对 URL(http(s)://)'))
      }
      base = `${baseUrl}${raw.startsWith('/') ? '' : '/'}${raw}`
    }
    // 拼 query(解析 @mock + {{变量}})
    const qs = query
      .filter((q) => q.on && q.key.trim())
      .map((q) => `${encodeURIComponent(q.key)}=${encodeURIComponent(resolveMock(resolveVars(q.value)))}`)
      .join('&')
    const finalUrl = qs ? `${base}${base.includes('?') ? '&' : '?'}${qs}` : base
    // 环境默认头先注入,显式请求头可覆盖(同名后写优先,由后端按顺序处理)。
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
                  <pre style={{ background: '#0f1419', color: '#d6deeb', padding: 12, borderRadius: 6, maxHeight: 360, overflow: 'auto', fontSize: 12, whiteSpace: 'pre-wrap', wordBreak: 'break-all' }}>
                    {respView === 'json' ? pretty(resp?.body || '') : resp?.body || t('editor.empty', '(空)')}
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
        <Select value={protocol} onChange={setProtocol} style={{ width: 120 }} options={protoOptions} />
        {spec.httpMethod && (
          <Select value={method} onChange={setMethod} style={{ width: 100 }} options={METHODS.map((m) => ({ value: m, label: m }))} />
        )}
        {/* 环境选择器:HTTP 下提供 baseUrl + 默认头 + {{变量}}。空=无环境(需填绝对 URL)。 */}
        {spec.httpMethod && (
          <Select
            value={envId || undefined}
            onChange={setEnvId}
            style={{ width: 168 }}
            placeholder={t('editor.selectEnv', '选择环境')}
            allowClear
            options={envs.map((e) => ({ value: e.id, label: e.baseUrl ? `${e.name} · ${e.baseUrl}` : e.name }))}
            notFoundContent={t('editor.noEnvConfigured', '未配置环境,去「环境」页新建')}
          />
        )}
        <Input value={url} onChange={(e) => setUrl(e.target.value)} placeholder={spec.httpMethod ? t('editor.urlPlaceholder', '/apis/... 或 http://...') : spec.urlPlaceholder} className="ms-mono" onPressEnter={send} />
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
          { key: 'body', label: t('editor.reqBody', '请求体'), children: <Input.TextArea rows={8} value={body} onChange={(e) => setBody(e.target.value)} placeholder={spec.bodyPlaceholder || '{"username":"admin"}'} className="ms-mono" /> },
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

function pretty(s: string): string {
  try {
    return JSON.stringify(JSON.parse(s), null, 2)
  } catch {
    return s || '(空)'
  }
}

// 键值表(Query / Headers 通用):启用 / 名 / 值(可选 @mock 下拉)/ 描述 / 删除。
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
