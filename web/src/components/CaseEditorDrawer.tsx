import { useEffect, useState } from 'react'
import { Button, Card, Input, Radio, Segmented, Select, Space, Table, Tabs, Tag, Typography } from 'antd'
import ResizableDrawer from './ResizableDrawer'
import { message } from '../feedback'
import { SendOutlined } from '@ant-design/icons'
import { api, ApiError, contentTypeForBodyType, withBodyContentType, type ApiBodyType, type ApiDefinition, type DebugResponse } from '../api'
import { methodColor, caseStatusLabel } from './tags'
import AssertionEditor, { type Assertion } from './AssertionEditor'
import KVEditor, { type KVRow } from './KVEditor'
import ProcessorEditor, { type Processor } from './ProcessorEditor'
import QueryParamTable, { type QueryParam, emptyQueryParam } from './QueryParamTable'
import { useI18n } from '../i18n'

const BODY_TYPES: ApiBodyType[] = ['none', 'form-data', 'x-www-form-urlencoded', 'json', 'xml', 'raw', 'binary']
const PRIORITIES = ['P0', 'P1', 'P2', 'P3']
// Case status values persist to the backend as-is (Chinese values kept); labels via caseStatusLabel.
const CASE_STATUSES = ['进行中', '已完成', '已废弃']

type AuthState = { type: 'none' | 'bearer' | 'basic'; token: string }

/** Best-effort JSON pretty-print; invalid input is returned unchanged. */
function formatJson(text: string): string {
  try {
    return JSON.stringify(JSON.parse(text), null, 2)
  } catch {
    return text
  }
}

const countKV = (rows: KVRow[]) => rows.filter((r) => r.key.trim()).length

// API case workbench: header info card + name/run row + priority/status/tags +
// underlined request sub-tabs (headers/body/query/REST/pre/post/assertions/auth/settings) + response area.
export default function CaseEditorDrawer({
  open,
  definition,
  onClose,
  onSaved,
}: {
  open: boolean
  definition: ApiDefinition
  onClose: () => void
  onSaved: () => void
}) {
  const { t } = useI18n()
  const [name, setName] = useState('')
  const [method, setMethod] = useState(definition.method || 'GET')
  const [url, setUrl] = useState(definition.path || '')
  const [priority, setPriority] = useState('P0')
  const [status, setStatus] = useState(CASE_STATUSES[0])
  const [tags, setTags] = useState<string[]>([])
  const [tagInput, setTagInput] = useState('')
  const [headers, setHeaders] = useState<KVRow[]>([{ key: '', value: '' }])
  const [query, setQuery] = useState<QueryParam[]>([emptyQueryParam()])
  const [rest, setRest] = useState<KVRow[]>([{ key: '', value: '' }])
  const [auth, setAuth] = useState<AuthState>({ type: 'none', token: '' })
  const [preProcessors, setPreProcessors] = useState<Processor[]>([])
  const [postProcessors, setPostProcessors] = useState<Processor[]>([])
  const [bodyType, setBodyType] = useState<ApiBodyType>('json')
  const [body, setBody] = useState('')
  const [assertions, setAssertions] = useState<Assertion[]>([{ type: 'StatusIs', args: 200 }])
  const [sending, setSending] = useState(false)
  const [saving, setSaving] = useState(false)
  const [resp, setResp] = useState<DebugResponse | null>(null)
  const [err, setErr] = useState('')

  // Reset from the current API definition on every open.
  useEffect(() => {
    if (open) reset()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, definition])

  const reset = () => {
    setName('')
    setMethod(definition.method || 'GET')
    setUrl(definition.path || '')
    setPriority('P0')
    setStatus(CASE_STATUSES[0])
    setTags([])
    setTagInput('')
    setHeaders([{ key: '', value: '' }])
    setQuery([emptyQueryParam()])
    setRest([{ key: '', value: '' }])
    setAuth({ type: 'none', token: '' })
    setPreProcessors([])
    setPostProcessors([])
    setBodyType('json')
    setBody('')
    setAssertions([{ type: 'StatusIs', args: 200 }])
    setResp(null)
    setErr('')
  }

  const send = async () => {
    if (!url.trim()) return message.warning(t('case.urlRequired', '请输入 URL'))
    setSending(true)
    setErr('')
    setResp(null)
    try {
      const hasBody = bodyType !== 'none' && !!body.trim()
      setResp(
        await api.debugSend({
          method,
          url,
          headers: withBodyContentType(headers.filter((h) => h.key.trim()), hasBody ? bodyType : 'none'),
          body: hasBody ? body.trim() : undefined,
        }),
      )
    } catch (e) {
      setErr(e instanceof ApiError ? e.message : t('case.sendFailed', '发送失败'))
    } finally {
      setSending(false)
    }
  }

  // Merge pre + post processors into the runner-consumable array (Wait pre / Extract post; script/SQL stored until an execution engine exists).
  const buildProcessors = (): unknown[] => [...preProcessors, ...postProcessors]

  const doSave = async (): Promise<boolean> => {
    if (!name.trim()) {
      message.warning(t('case.nameRequired', '请填用例名'))
      return false
    }
    if (!url.trim()) {
      message.warning(t('case.urlRequiredSave', '请填 URL'))
      return false
    }
    setSaving(true)
    const hasBody = bodyType !== 'none' && !!body.trim()
    try {
      await api.createCase(definition.id, {
        name: name.trim(),
        method,
        url,
        body: hasBody ? body.trim() : undefined,
        assertions,
        processors: buildProcessors(),
        priority,
        status,
        tags,
        // Fill a default Content-Type from the body type (never overriding a user-set one); persisted with the case so the runner sends it.
        headers: withBodyContentType(headers.filter((h) => h.key.trim()), hasBody ? bodyType : 'none'),
        queryParams: query.filter((h) => h.key.trim()).map((q) => ({ key: q.key.trim(), value: q.value, enabled: q.enabled, type: q.type, minLen: q.minLen, maxLen: q.maxLen, description: q.description })),
        restParams: rest.filter((h) => h.key.trim()),
        auth: auth.type === 'none' ? { type: 'none' } : { type: auth.type, token: auth.token },
      })
      message.success(t('case.saved', '用例已保存'))
      return true
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('case.saveFailed', '保存失败'))
      return false
    } finally {
      setSaving(false)
    }
  }

  const save = async () => {
    if (await doSave()) onSaved()
  }
  const saveAndContinue = async () => {
    if (await doSave()) {
      reset()
      message.info(t('case.continueHint', '可继续创建下一条用例'))
    }
  }

  // Request sub-tabs (underline style, per reference shot #3).
  const reqTabs = [
    {
      key: 'headers',
      label: `${t('case.headers', '请求头')}${countKV(headers) ? ` (${countKV(headers)})` : ''}`,
      children: <KVEditor rows={headers} onChange={setHeaders} />,
    },
    {
      key: 'body',
      label: `${t('apidef.requestBody', '请求体')}${body.trim() ? ' (1)' : ''}`,
      children: (
        <div>
          <Segmented
            size="small"
            value={bodyType}
            onChange={(v) => setBodyType(v as ApiBodyType)}
            options={BODY_TYPES.map((x) => ({ label: x, value: x }))}
            style={{ marginBottom: 10 }}
          />
          {bodyType === 'none' ? (
            <Typography.Text type="secondary">{t('apidef.noBody', '请求没有 Body')}</Typography.Text>
          ) : (
            <div>
              {(() => {
                const userCt = headers.find((h) => h.key.trim().toLowerCase() === 'content-type' && h.value.trim())
                const autoCt = contentTypeForBodyType(bodyType)
                if (userCt) return <Typography.Text type="secondary" style={{ fontSize: 12 }}>Content-Type: <span className="ms-mono">{userCt.value.trim()}</span>({t('case.ctFromHeader', '来自请求头')})</Typography.Text>
                if (autoCt) return <Typography.Text type="secondary" style={{ fontSize: 12 }}>Content-Type: <span className="ms-mono">{autoCt}</span>({t('case.ctAuto', '自动按 body 类型;在「请求头」中可覆盖')})</Typography.Text>
                return <Typography.Text type="secondary" style={{ fontSize: 12 }}>{t('case.ctNone', '不发送 Content-Type;如需可在「请求头」中添加')}</Typography.Text>
              })()}
              {bodyType === 'json' && (
                <div style={{ textAlign: 'right', marginBottom: 6 }}>
                  <Button size="small" onClick={() => setBody(formatJson(body))}>{t('apidef.format', '格式化')}</Button>
                </div>
              )}
              <Input.TextArea rows={8} value={body} onChange={(e) => setBody(e.target.value)} placeholder='{"username":"admin"}' className="ms-mono" />
            </div>
          )}
        </div>
      ),
    },
    {
      key: 'query',
      label: `Query${query.filter((q) => q.key.trim()).length ? ` (${query.filter((q) => q.key.trim()).length})` : ''}`,
      children: <QueryParamTable rows={query} onChange={setQuery} />,
    },
    {
      key: 'rest',
      label: `REST${countKV(rest) ? ` (${countKV(rest)})` : ''}`,
      children: <KVEditor rows={rest} onChange={setRest} namePlaceholder="name" valuePlaceholder="value" />,
    },
    {
      key: 'pre',
      label: `${t('case.preProcess', '前置')}${preProcessors.length ? ` (${preProcessors.length})` : ''}`,
      children: <ProcessorEditor value={preProcessors} onChange={setPreProcessors} allowed={['wait', 'script', 'sql']} />,
    },
    {
      key: 'post',
      label: `${t('case.postProcess', '后置')}${postProcessors.length ? ` (${postProcessors.length})` : ''}`,
      children: <ProcessorEditor value={postProcessors} onChange={setPostProcessors} allowed={['extract', 'wait', 'script', 'sql']} />,
    },
    {
      key: 'assert',
      label: `${t('case.assertions', '断言')}${assertions.length ? ` (${assertions.length})` : ''}`,
      children: <AssertionEditor value={assertions} onChange={setAssertions} />,
    },
    {
      key: 'auth',
      label: t('apidef.auth', '认证'),
      children: (
        <Space direction="vertical" size={12} style={{ width: '100%', maxWidth: 520 }}>
          <Radio.Group value={auth.type} onChange={(e) => setAuth({ ...auth, type: e.target.value })}>
            <Radio value="none">{t('editor.authNone', '无')}</Radio>
            <Radio value="bearer">Bearer Token</Radio>
            <Radio value="basic">Basic (user:pass)</Radio>
          </Radio.Group>
          {auth.type !== 'none' && (
            <Input value={auth.token} onChange={(e) => setAuth({ ...auth, token: e.target.value })} placeholder={auth.type === 'bearer' ? 'token' : 'user:pass'} className="ms-mono" />
          )}
        </Space>
      ),
    },
    {
      key: 'settings',
      label: t('apidef.settings', '设置'),
      children: <Typography.Text type="secondary">{t('case.settingsHint', '连接超时 / 重定向等高级设置:暂未接入,使用 runner 默认')}</Typography.Text>,
    },
  ]

  return (
    <ResizableDrawer
      title={t('case.createTitle', '创建用例')}
      open={open}
      onClose={onClose}
      width="68%"
      styles={{ body: { paddingTop: 12 } }}
      footer={
        <div style={{ textAlign: 'right' }}>
          <Space>
            <Button onClick={onClose}>{t('a.cancel', '取消')}</Button>
            <Button loading={saving} onClick={saveAndContinue}>{t('case.saveContinue', '保存并继续创建')}</Button>
            <Button type="primary" loading={saving} onClick={save}>{t('a.create', '创建')}</Button>
          </Space>
        </div>
      }
    >
      {/* Header info card: [id] name + request type + path (per reference shot #3) */}
      <Card size="small" styles={{ body: { padding: '10px 14px' } }} style={{ marginBottom: 12, background: 'var(--panel-2)' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12, flexWrap: 'wrap' }}>
          <span style={{ fontWeight: 600 }}>【{definition.num ?? '—'}】{definition.name}</span>
          <div style={{ flex: 1 }} />
          <span style={{ color: 'var(--text-3)', fontSize: 12 }}>{t('apidef.reqType', '请求类型')} <Tag color={methodColor(method)} style={{ margin: 0 }}>{method}</Tag></span>
          <span style={{ color: 'var(--text-3)', fontSize: 12 }}>{t('apidef.colPath', '路径')} <span className="ms-mono" style={{ color: 'var(--text-2)' }}>{url || '—'}</span></span>
        </div>
      </Card>

      {/* Name + server-side run */}
      <Space.Compact style={{ width: '100%', marginBottom: 10 }}>
        <Input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder={t('case.namePlaceholder', '请输入用例名称')}
          maxLength={255}
          showCount
        />
        <Button type="primary" icon={<SendOutlined />} loading={sending} onClick={send}>{t('apidef.serverRun', '服务端执行')}</Button>
      </Space.Compact>

      {/* Priority / status / tags */}
      <Space wrap style={{ marginBottom: 14 }}>
        <Select value={priority} onChange={setPriority} style={{ width: 110 }} options={PRIORITIES.map((p) => ({ value: p, label: p }))} />
        <Select value={status} onChange={setStatus} style={{ width: 130 }} options={CASE_STATUSES.map((s) => ({ value: s, label: caseStatusLabel(s, t) }))} />
        <Space size={[4, 4]} wrap>
          {tags.map((tg) => (
            <Tag key={tg} closable onClose={() => setTags(tags.filter((x) => x !== tg))}>{tg}</Tag>
          ))}
          <Input
            size="small"
            style={{ width: 160 }}
            value={tagInput}
            onChange={(e) => setTagInput(e.target.value)}
            onPressEnter={() => {
              const v = tagInput.trim()
              if (v && !tags.includes(v)) setTags([...tags, v])
              setTagInput('')
            }}
            placeholder={t('apidef.addTag', '添加标签,回车结束')}
          />
        </Space>
      </Space>

      <div style={{ fontWeight: 600, fontSize: 13, marginBottom: 8 }}>{t('case.requestParams', '请求参数')}</div>
      <Tabs size="small" items={reqTabs} />

      {(resp || err) && (
        <Card
          size="small"
          style={{ marginTop: 12 }}
          title={
            <Space>
              {t('case.response', '响应')}
              {resp && <Tag color={resp.status < 400 ? 'green' : 'red'}>{resp.status}</Tag>}
              {resp && <Typography.Text type="secondary" style={{ fontSize: 12 }}>{resp.latencyMs} ms</Typography.Text>}
              <Tag color={methodColor(method)}>{method}</Tag>
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
                  key: 'rbody',
                  label: t('case.respBody', '响应体'),
                  children: (
                    <pre style={{ background: 'var(--panel-2)', color: 'var(--text)', border: '1px solid var(--border-soft)', padding: 12, borderRadius: 6, maxHeight: 280, overflow: 'auto', fontSize: 12, whiteSpace: 'pre-wrap', wordBreak: 'break-all' }}>
                      {resp?.body || t('case.emptyBody', '(空)')}
                    </pre>
                  ),
                },
                {
                  key: 'rheaders',
                  label: `${t('case.respHeaders', '响应头')} (${resp?.headers.length || 0})`,
                  children: (
                    <Table
                      size="small"
                      pagination={false}
                      rowKey={(_, i) => String(i)}
                      dataSource={(resp?.headers || []).map(([k, v]) => ({ k, v }))}
                      columns={[
                        { title: t('case.headerName', '名'), dataIndex: 'k', width: 220 },
                        { title: t('case.headerValue', '值'), dataIndex: 'v', render: (v: string) => <span className="ms-mono">{v}</span> },
                      ]}
                    />
                  ),
                },
              ]}
            />
          )}
        </Card>
      )}
    </ResizableDrawer>
  )
}
