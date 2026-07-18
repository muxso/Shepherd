import { forwardRef, useEffect, useImperativeHandle, useState } from 'react'
import { Button, Card, Empty, Input, InputNumber, Radio, Segmented, Select, Space, Table, Tabs, Tag, Tooltip, Typography } from 'antd'
import ResizableDrawer from './ResizableDrawer'
import EditDrawer from './EditDrawer'
import { CopyOutlined, PlusOutlined, DeleteOutlined, SaveOutlined, UploadOutlined } from '@ant-design/icons'
import type { ColumnsType } from 'antd/es/table'
import QueryParamTable from './QueryParamTable'
import BodySchemaTree, { schemaToJson } from './BodySchemaTree'
import ProcessorEditor from './ProcessorEditor'
import AssertionEditor from './AssertionEditor'
import {
  api,
  ApiError,
  withBodyContentType,
  type ApiBodyType,
  type ApiDefinition,
  type ApiModule,
  type ApiSpec,
  type ApiSpecKV,
  type ApiSpecResponse,
  type BodySchemaNode,
  type DebugResponse,
  type Environment,
} from '../api'
import { message } from '../feedback'
import { useI18n } from '../i18n'
import { LatencyStat, fmtDurationMs } from './TimingBreakdown'
import { statusLabel } from './tags'

const API_STATUSES = ['DRAFT', 'DEBUGGING', 'COMPLETED', 'DEPRECATED']

/** Copy text to clipboard (with toast). navigator.clipboard may be missing in insecure contexts; falls back to execCommand. */
async function copy(text: string, ok: string, fail = '复制失败') {
  try {
    if (navigator.clipboard?.writeText) await navigator.clipboard.writeText(text)
    else {
      const ta = document.createElement('textarea')
      ta.value = text
      document.body.appendChild(ta)
      ta.select()
      document.execCommand('copy')
      document.body.removeChild(ta)
    }
    message.success(ok)
  } catch {
    message.error(fail)
  }
}

const BODY_TYPES: ApiBodyType[] = ['none', 'form-data', 'x-www-form-urlencoded', 'json', 'xml', 'raw', 'binary']
export const emptySpec = (): ApiSpec => ({
  requestHeaders: [],
  requestQuery: [],
  restParams: [],
  bodyType: 'none',
  requestBody: '',
  formBody: [],
  auth: { type: 'none' },
  responses: [],
  tags: [],
  preProcessors: [],
  postProcessors: [],
  assertions: [],
})

/** Parse a cURL command (method / url / -H headers / -d body). Best-effort; returns null on failure. */
export function parseCurl(text: string): { method: string; url: string; headers: ApiSpecKV[]; body: string } | null {
  const raw = text.trim().replace(/\\\r?\n/g, ' ')
  if (!/^curl\b/.test(raw)) return null
  // Tokenize (single/double quotes supported).
  const toks: string[] = []
  const re = /"([^"]*)"|'([^']*)'|(\S+)/g
  let m: RegExpExecArray | null
  while ((m = re.exec(raw))) toks.push(m[1] ?? m[2] ?? m[3] ?? '')
  let method = ''
  let url = ''
  const headers: ApiSpecKV[] = []
  let body = ''
  for (let i = 1; i < toks.length; i++) {
    const tk = toks[i]
    if (tk === '-X' || tk === '--request') method = (toks[++i] || '').toUpperCase()
    else if (tk === '-H' || tk === '--header') {
      const h = toks[++i] || ''
      const idx = h.indexOf(':')
      if (idx > 0) headers.push({ name: h.slice(0, idx).trim(), value: h.slice(idx + 1).trim(), desc: '' })
    } else if (tk === '-d' || tk === '--data' || tk === '--data-raw' || tk === '--data-binary') body = toks[++i] || ''
    else if (tk === '-u' || tk === '--user') headers.push({ name: 'Authorization', value: `Basic ${tk}`, desc: '' })
    else if (!tk.startsWith('-') && !url) url = tk
  }
  if (!url) return null
  if (!method) method = body ? 'POST' : 'GET'
  return { method, url, headers, body }
}

/**
 * Shared panel for API "preview" (read-only) / "define" (editable) / "create" (controlled, no id).
 * In create mode the parent owns the spec (value/onChange): no self load/save, and the save button belongs to the parent (new-API tab).
 */
export type ExecMode = 'server' | 'local'
/** The request actually sent (shown in actual request / console / cURL). */
export type SentRequest = { method: string; url: string; headers: { key: string; value: string }[]; body?: string }
export interface ApiSpecPanelHandle {
  save: () => void
  /** cURL import: merge parsed headers/body into the current spec; the parent backfills request-line method/path. */
  applyCurl: (parsed: { method: string; url: string; headers: ApiSpecKV[]; body: string }) => void
  /** Execute (debug mode): server = via server-side proxy; local = direct from the browser. */
  execute: (mode?: ExecMode) => void
  /** Save as test case (debug-mode save): create a case from the current request + assertions/processors. */
  saveAsCase: () => void
}

const ApiSpecPanel = forwardRef<ApiSpecPanelHandle, {
  definition: ApiDefinition
  mode: 'preview' | 'define' | 'create' | 'debug'
  value?: ApiSpec
  onChange?: (s: ApiSpec) => void
  /** Hide the internal save button (parent request line triggers saves via ref.save()). */
  hideSave?: boolean
  /** Debug request line: method/path (owned by the parent request line; cURL import backfills them). */
  reqMethod?: string
  reqPath?: string
  /** API name (owned by the parent request line); persisted with the base fields on define/debug save. */
  reqName?: string
  /** Called after base fields/spec are saved (parent refreshes list/detail). */
  onSaved?: () => void
  /** Current exec mode (chosen in the parent request-line dropdown). */
  execMode?: ExecMode
  /** Selected environment (from the parent top-bar picker; debug execution uses its baseUrl/default headers/variables). */
  env?: Environment
  /** Called after save-as-new-case succeeds (parent switches to the cases tab and refreshes). */
  onCaseSaved?: () => void
}>(function ApiSpecPanel({ definition, mode, value, onChange, hideSave, reqMethod, reqPath, reqName, onSaved, execMode = 'server', env, onCaseSaved }, ref) {
  const { t } = useI18n()
  const create = mode === 'create'
  const debug = mode === 'debug'
  const editable = mode === 'define' || create || debug
  // Controlled = create mode, or an id-less draft (debugging a new API): parent owns the spec, no load/save here.
  const controlled = create || !definition.id
  const [innerSpec, setInnerSpec] = useState<ApiSpec>(emptySpec())
  const [loading, setLoading] = useState(!controlled)
  const [saving, setSaving] = useState(false)
  const [dirty, setDirty] = useState(false)
  const spec = controlled ? value ?? emptySpec() : innerSpec

  useEffect(() => {
    if (controlled) return
    let alive = true
    setLoading(true)
    api
      .getDefinition(definition.id)
      .then((d) => alive && setInnerSpec({ ...emptySpec(), ...(d.spec || {}) }))
      .catch(() => alive && setInnerSpec(emptySpec()))
      .finally(() => alive && setLoading(false))
    return () => {
      alive = false
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [definition.id, controlled])

  const patch = (p: Partial<ApiSpec>) => {
    if (controlled) {
      onChange?.({ ...spec, ...p })
      return
    }
    setInnerSpec((s) => ({ ...s, ...p }))
    setDirty(true)
  }

  const save = async () => {
    if (!definition.id) return // draft: the parent's save persists the spec together with creation.
    setSaving(true)
    try {
      // Base fields (name/method/path) are owned by the parent request line; persist them only when actually changed.
      const isHttp = (definition.protocol || 'HTTP').toUpperCase() === 'HTTP'
      const nameChanged = reqName !== undefined && reqName.trim() !== '' && reqName.trim() !== definition.name
      const methodChanged = isHttp && !!reqMethod && reqMethod !== definition.method
      const pathChanged = reqPath !== undefined && reqPath !== definition.path
      if (!create && (nameChanged || methodChanged || pathChanged)) {
        await api.updateDefinition(definition.id, {
          name: reqName?.trim() || definition.name,
          protocol: definition.protocol,
          method: isHttp ? reqMethod || definition.method : '',
          path: reqPath ?? definition.path,
        })
      }
      await api.updateDefinitionSpec(definition.id, spec)
      message.success(t('apidef.specSaved', '定义已保存'))
      setDirty(false)
      onSaved?.()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('apidef.saveFailed', '保存失败'))
    } finally {
      setSaving(false)
    }
  }

  // Debug execution state (environment comes from the parent top bar via the env prop).
  const [running, setRunning] = useState(false)
  const [resp, setResp] = useState<DebugResponse | null>(null)
  const [runErr, setRunErr] = useState('')
  // Last request actually sent (for actual request / console / cURL views).
  const [lastReq, setLastReq] = useState<SentRequest | null>(null)

  // Assemble request line + spec into a sendable request (URL/headers/body). Warns and returns null on failure.
  const buildRequest = (): { method: string; url: string; headers: { key: string; value: string }[]; body?: string } | null => {
    const resolveVars = (s: string): string =>
      env?.variables ? s.replace(/\{\{\s*(\w+)\s*\}\}/g, (whole, k: string) => env.variables?.[k] ?? whole) : s
    const path = (reqPath ?? definition.path ?? '').trim()
    if (!path) {
      message.warning(t('editor.urlRequired', '请输入 URL'))
      return null
    }
    const rawUrl = resolveVars(path)
    let base = rawUrl
    if (!/^https?:\/\//i.test(rawUrl)) {
      const baseUrl = env?.baseUrl?.trim().replace(/\/+$/, '')
      if (!baseUrl) {
        message.warning(t('editor.needEnvOrAbs', '相对路径需先选择带 baseUrl 的环境,或填写绝对 URL(http(s)://)'))
        return null
      }
      base = `${baseUrl}${rawUrl.startsWith('/') ? '' : '/'}${rawUrl}`
    }
    const qs = (spec.requestQuery || [])
      .filter((q) => q.name?.trim())
      .map((q) => `${encodeURIComponent(q.name)}=${encodeURIComponent(resolveVars(q.value || ''))}`)
      .join('&')
    const finalUrl = qs ? `${base}${base.includes('?') ? '&' : '?'}${qs}` : base
    const hs: { key: string; value: string }[] = []
    for (const eh of env?.headers || []) if (eh.name?.trim()) hs.push({ key: eh.name, value: resolveVars(eh.value || '') })
    for (const h of spec.requestHeaders || []) if (h.name?.trim()) hs.push({ key: h.name, value: resolveVars(h.value || '') })
    if (spec.auth?.type === 'bearer' && spec.auth.token) hs.push({ key: 'Authorization', value: `Bearer ${spec.auth.token}` })
    if (spec.auth?.type === 'basic' && spec.auth.token) hs.push({ key: 'Authorization', value: `Basic ${btoa(spec.auth.token)}` })
    // Legacy specs may have requestBody text without bodyType: fall back to raw (matching preview), still send the body, don't force a Content-Type.
    const bt: ApiBodyType = spec.bodyType || (spec.requestBody?.trim() ? 'raw' : 'none')
    const hasBody = bt !== 'none' && !!spec.requestBody?.trim()
    return {
      method: reqMethod || definition.method || 'GET',
      url: finalUrl,
      // Add a default Content-Type per body type (skipped when user/environment already set one).
      headers: withBodyContentType(hs, hasBody ? bt : 'none'),
      body: hasBody ? resolveVars(spec.requestBody || '') : undefined,
    }
  }

  // Local execution: direct from the browser (subject to CORS — the difference from server proxy).
  const localSend = async (req: { method: string; url: string; headers: { key: string; value: string }[]; body?: string }): Promise<DebugResponse> => {
    const t0 = performance.now()
    const res = await fetch(req.url, { method: req.method, headers: Object.fromEntries(req.headers.map((h) => [h.key, h.value])), body: req.body })
    const text = await res.text()
    const headers: [string, string][] = []
    res.headers.forEach((v, k) => headers.push([k, v]))
    return { status: res.status, latencyMs: Math.round(performance.now() - t0), headers, body: text }
  }

  const execute = async (m: ExecMode = execMode) => {
    const req = buildRequest()
    if (!req) return
    setLastReq(req)
    setRunning(true)
    setRunErr('')
    setResp(null)
    try {
      setResp(
        m === 'local'
          ? await localSend(req)
          : await api.debugSend({ ...req, assertions: spec.assertions, processors: spec.postProcessors }),
      )
    } catch (e) {
      setRunErr(e instanceof ApiError ? e.message : e instanceof Error ? e.message : t('editor.sendFail', '发送失败'))
    } finally {
      setRunning(false)
    }
  }

  // Save as test case: create a case from the current request + assertions/processors/tags (reference UI #9: save = save-as-case).
  const [caseModalOpen, setCaseModalOpen] = useState(false)
  const [caseName, setCaseName] = useState('')
  const [caseSaving, setCaseSaving] = useState(false)
  const kvToCase = (rows?: ApiSpecKV[]) => (rows || []).filter((r) => r.name?.trim()).map((r) => ({ key: r.name, value: r.value || '' }))
  const saveAsCase = () => {
    setCaseName(`${definition.name} - ${t('apidef.debugCase', '调试用例')}`)
    setCaseModalOpen(true)
  }
  const doSaveCase = async () => {
    if (!caseName.trim()) {
      message.warning(t('apidef.caseNameRequired', '请输入用例名称'))
      return
    }
    setCaseSaving(true)
    try {
      await api.createCase(definition.id, {
        name: caseName.trim(),
        method: reqMethod || definition.method || 'GET',
        url: reqPath || definition.path || '',
        body: spec.requestBody || undefined,
        headers: kvToCase(spec.requestHeaders),
        queryParams: kvToCase(spec.requestQuery),
        restParams: kvToCase(spec.restParams),
        auth: spec.auth?.type && spec.auth.type !== 'none' ? { type: spec.auth.type, token: spec.auth.token } : undefined,
        assertions: spec.assertions,
        processors: [...(spec.preProcessors || []), ...(spec.postProcessors || [])],
        tags: spec.tags,
      })
      message.success(t('apidef.caseSaved', '已另存为用例'))
      setCaseModalOpen(false)
      onCaseSaved?.()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('apidef.saveFailed', '保存失败'))
    } finally {
      setCaseSaving(false)
    }
  }

  const applyCurl = (parsed: { method: string; url: string; headers: ApiSpecKV[]; body: string }) => {
    patch({
      requestHeaders: [...(spec.requestHeaders || []).filter((h) => h.name), ...parsed.headers],
      ...(parsed.body ? { requestBody: parsed.body, bodyType: 'json' as ApiBodyType } : {}),
    })
  }

  // Expose save/execute/applyCurl/saveAsCase to the parent (triggered by the definition-page request-line buttons).
  useImperativeHandle(ref, () => ({ save, execute, applyCurl, saveAsCase }), [save, execute, applyCurl, saveAsCase])

  if (loading) return <div style={{ padding: 24, color: 'var(--text-3)' }}>{t('a.loading', '加载中…')}</div>

  // Preview (read-only): sections laid out flat.
  if (!editable) {
    return (
      <div style={{ display: 'flex', flexDirection: 'column', gap: 18 }}>
        <KVSection title={t('apidef.requestHeaders', '请求头')} rows={spec.requestHeaders || []} editable={false} onChange={() => {}} />
        <KVSection title={t('apidef.requestQuery', 'Query 参数')} rows={spec.requestQuery || []} editable={false} onChange={() => {}} />
        {(spec.restParams?.length ?? 0) > 0 && (
          <KVSection title={t('apidef.restParams', 'REST 路径参数')} rows={spec.restParams || []} editable={false} onChange={() => {}} />
        )}
        <BodyView spec={spec} />
        <ResponsesSection responses={spec.responses || []} editable={false} onChange={() => {}} />
      </div>
    )
  }

  // Define (editable): sub-tabs.
  const tabs = [
    {
      key: 'basic',
      label: t('apidef.basicInfo', '基本信息'),
      children: <BasicInfo definition={definition} spec={spec} patch={patch} create={create} />,
    },
    {
      key: 'headers',
      label: `${t('apidef.requestHeaders', '请求头')}${spec.requestHeaders?.length ? ` (${spec.requestHeaders.length})` : ''}`,
      children: <KVSection title={t('apidef.requestHeaders', '请求头')} rows={spec.requestHeaders || []} editable onChange={(rows) => patch({ requestHeaders: rows })} hideTitle />,
    },
    {
      key: 'body',
      label: t('apidef.requestBody', '请求体'),
      children: <BodyEditor spec={spec} patch={patch} />,
    },
    {
      key: 'query',
      label: `Query${spec.requestQuery?.length ? ` (${spec.requestQuery.length})` : ''}`,
      children: <KVSection title="Query" rows={spec.requestQuery || []} editable onChange={(rows) => patch({ requestQuery: rows })} hideTitle />,
    },
    {
      key: 'rest',
      label: `REST${spec.restParams?.length ? ` (${spec.restParams.length})` : ''}`,
      children: <KVSection title={t('apidef.restParams', 'REST 路径参数')} rows={spec.restParams || []} editable onChange={(rows) => patch({ restParams: rows })} hideTitle />,
    },
    // Pre/post/assertions: debug mode only (reference UI #9; absent in define mode).
    ...(debug
      ? [
          {
            key: 'pre',
            label: t('apidef.preProcessors', '前置'),
            children: <ProcessorEditor value={spec.preProcessors as Record<string, unknown>[] | undefined} onChange={(v) => patch({ preProcessors: v })} allowed={['script', 'sql', 'wait']} />,
          },
          {
            key: 'post',
            label: t('apidef.postProcessors', '后置'),
            children: <ProcessorEditor value={spec.postProcessors as Record<string, unknown>[] | undefined} onChange={(v) => patch({ postProcessors: v })} allowed={['extract', 'script', 'sql', 'wait']} />,
          },
          {
            key: 'assert',
            label: `${t('apidef.assertions', '断言')}${spec.assertions?.length ? ` (${spec.assertions.length})` : ''}`,
            children: <AssertionEditor value={spec.assertions as Record<string, unknown>[] | undefined} onChange={(v) => patch({ assertions: v })} />,
          },
        ]
      : []),
    {
      key: 'auth',
      label: t('apidef.auth', '认证'),
      children: <AuthEditor spec={spec} patch={patch} />,
    },
    {
      key: 'settings',
      label: t('apidef.settings', '设置'),
      children: <SettingsTab definition={definition} />,
    },
  ]

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
      {/* In create/hideSave mode the parent request line submits saves; no duplicate button here. */}
      {!create && !hideSave && (
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <Button type="primary" icon={<SaveOutlined />} loading={saving} disabled={!dirty} onClick={save}>
            {t('a.save', '保存')}
          </Button>
          {dirty && <span style={{ color: '#ef6c00', fontSize: 12 }}>{t('apidef.unsaved', '有未保存修改')}</span>}
        </div>
      )}
      <Tabs className="ms-detail-tabs" items={tabs} size="small" />
      {/* Bottom response section: define = example responses (200/404…); debug = server execution result. */}
      {debug ? (
        <DebugResultPanel
          running={running}
          resp={resp}
          err={runErr}
          req={lastReq}
          onRun={() => execute()}
          isHttp={(definition.protocol || 'HTTP').toUpperCase() === 'HTTP'}
          extractors={spec.postProcessors as Record<string, unknown>[] | undefined}
          assertions={spec.assertions as Record<string, unknown>[] | undefined}
        />
      ) : (
        <ExampleResponsesPanel responses={spec.responses || []} onChange={(rows) => patch({ responses: rows })} />
      )}
      <EditDrawer
        title={t('apidef.saveAsCase', '保存为新用例')}
        open={caseModalOpen}
        onCancel={() => setCaseModalOpen(false)}
        onOk={doSaveCase}
        okButtonProps={{ loading: caseSaving }}
        okText={t('a.save', '保存')}
        cancelText={t('a.cancel', '取消')}
      >
        <div style={{ fontSize: 13, color: 'var(--text-2)', marginBottom: 6 }}>{t('apidef.caseName', '用例名称')}</div>
        <Input value={caseName} onChange={(e) => setCaseName(e.target.value)} onPressEnter={doSaveCase} placeholder={t('apidef.caseName', '用例名称')} />
      </EditDrawer>
    </div>
  )
})

/** Define-mode bottom response section: example responses. Status-code tabs (200/404…) with add; each example holds body/headers/status. */
function ExampleResponsesPanel({ responses, onChange }: { responses: ApiSpecResponse[]; onChange: (rows: ApiSpecResponse[]) => void }) {
  const { t } = useI18n()
  const [sel, setSel] = useState(0)
  const [sub, setSub] = useState<'body' | 'headers' | 'status'>('body')
  const cur = responses[sel]
  const setCur = (p: Partial<ApiSpecResponse>) => onChange(responses.map((r, i) => (i === sel ? { ...r, ...p } : r)))
  const add = () => {
    onChange([...responses, { status: 200, body: '', headers: [] }])
    setSel(responses.length)
  }
  const del = (i: number) => {
    onChange(responses.filter((_, idx) => idx !== i))
    setSel((s) => (s >= i && s > 0 ? s - 1 : s))
  }
  const sc = (s?: number) => (s == null ? 'default' : s < 300 ? 'green' : s < 400 ? 'gold' : 'red')

  return (
    <Card size="small" style={{ marginTop: 12 }} styles={{ body: { padding: 12 } }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12, flexWrap: 'wrap' }}>
        <span style={{ fontWeight: 600, fontSize: 13 }}>{t('apidef.responseContent', '响应内容')}</span>
        <div style={{ width: 12 }} />
        {responses.map((r, i) => (
          <Tag.CheckableTag key={i} checked={i === sel} onChange={() => setSel(i)} style={{ border: '1px solid var(--border-soft)' }}>
            <span style={{ color: i === sel ? undefined : undefined }}>
              <span style={{ color: sc(r.status) === 'green' ? 'var(--success)' : sc(r.status) === 'red' ? 'var(--error)' : '#d48806', fontWeight: 600 }}>●</span> {r.status ?? '—'}
            </span>
            {responses.length > 1 && (
              <DeleteOutlined style={{ marginLeft: 6, fontSize: 11 }} onClick={(e) => { e.stopPropagation(); del(i) }} />
            )}
          </Tag.CheckableTag>
        ))}
        <Button type="text" size="small" icon={<PlusOutlined />} onClick={add} />
      </div>
      {!cur ? (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('apidef.noExampleResp', '暂无示例响应')} style={{ margin: '8px 0' }} />
      ) : (
        <Tabs
          size="small"
          activeKey={sub}
          onChange={(k) => setSub(k as 'body' | 'headers' | 'status')}
          items={[
            {
              key: 'body',
              label: t('editor.respBody', '响应体'),
              children: <Input.TextArea rows={6} value={cur.body} onChange={(e) => setCur({ body: e.target.value })} placeholder={t('apidef.responseBody', '响应体')} className="ms-mono" />,
            },
            {
              key: 'headers',
              label: `${t('editor.respHeaders', '响应头')}${cur.headers?.length ? ` (${cur.headers.length})` : ''}`,
              children: <KVSection title="" rows={cur.headers || []} editable onChange={(rows) => setCur({ headers: rows })} hideTitle />,
            },
            {
              key: 'status',
              label: t('apidef.statusCode', '状态码'),
              children: <InputNumber min={100} max={599} value={cur.status} onChange={(v) => setCur({ status: v ?? undefined })} />,
            },
          ]}
        />
      )}
    </Card>
  )
}

/** Render the sent request as a cURL command. */
function reqToCurl(req: SentRequest): string {
  const parts = [`curl -X ${req.method} '${req.url}'`]
  for (const h of req.headers) parts.push(`  -H '${h.key}: ${(h.value || '').replace(/'/g, "'\\''")}'`)
  if (req.body) parts.push(`  -d '${req.body.replace(/'/g, "'\\''")}'`)
  return parts.join(' \\\n')
}

const codeBox: React.CSSProperties = { background: 'var(--panel-2)', color: 'var(--text)', border: '1px solid var(--border-soft)', padding: 12, borderRadius: 6, maxHeight: 360, overflow: 'auto', fontSize: 12, whiteSpace: 'pre-wrap', wordBreak: 'break-all', margin: 0 }

/** Debug result panel: body/headers/actual request/console/cURL/extractions/assertions (execution triggered by the request line, env picked in the top bar). */
export function DebugResultPanel({
  running,
  resp,
  err,
  req,
  isHttp,
  extractors,
  assertions,
  onRun,
}: {
  running: boolean
  resp: DebugResponse | null
  err: string
  req: SentRequest | null
  isHttp: boolean
  extractors?: Record<string, unknown>[]
  assertions?: Record<string, unknown>[]
  /** Present = show a run-it call-to-action in the not-yet-executed placeholder. */
  onRun?: () => void
}) {
  const { t } = useI18n()
  const [view, setView] = useState<'json' | 'raw'>('json')

  // Nothing executed yet: a placeholder instead of empty response tabs.
  if (!running && !resp && !err) {
    return (
      <Card size="small" style={{ marginTop: 12 }} styles={{ body: { padding: 12 } }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8 }}>
          <span style={{ fontWeight: 600, fontSize: 13 }}>{t('apidef.responseContent', '响应内容')}</span>
        </div>
        <div style={{ textAlign: 'center', padding: '36px 0 40px', color: 'var(--text-3)' }}>
          <svg width={72} height={48} viewBox="0 0 72 48" style={{ display: 'block', margin: '0 auto 12px' }}>
            <rect x={1} y={1} width={70} height={46} rx={5} fill="var(--panel-2)" stroke="var(--border)" />
            <rect x={7} y={7} width={58} height={6} rx={2} fill="var(--brand)" opacity={0.55} />
            <rect x={7} y={18} width={40} height={5} rx={2} fill="var(--text-3)" opacity={0.4} />
            <rect x={7} y={28} width={48} height={5} rx={2} fill="var(--text-3)" opacity={0.3} />
            <rect x={7} y={38} width={30} height={5} rx={2} fill="var(--text-3)" opacity={0.25} />
          </svg>
          {onRun ? (
            <span>
              {t('editor.clickTo', '点击')}{' '}
              <a onClick={onRun}>{t('apidef.serverRun', '服务端执行')}</a>{' '}
              {t('editor.toGetResponse', '获取响应内容')}
            </span>
          ) : (
            <span>{t('apidef.notRunYet', '尚未执行')}</span>
          )}
        </div>
      </Card>
    )
  }

  // Extractors: the extractors list of post-processors with type=Extract.
  const extractRows = (extractors || [])
    .filter((p) => String(p.type) === 'Extract')
    .flatMap((p) => ((p.args as { extractors?: { variable?: string; kind?: string; expression?: string }[] })?.extractors || []))

  const items = [
    {
      key: 'body',
      label: t('editor.respBody', '响应体'),
      children: (
        <>
          <Radio.Group size="small" value={view} onChange={(e) => setView(e.target.value)} optionType="button" style={{ marginBottom: 8 }}>
            <Radio.Button value="json">JSON</Radio.Button>
            <Radio.Button value="raw">Raw</Radio.Button>
          </Radio.Group>
          <pre style={codeBox}>{view === 'json' ? formatJson(resp?.body || '') : resp?.body || t('editor.empty', '(空)')}</pre>
        </>
      ),
    },
    {
      key: 'headers',
      label: `${t('editor.respHeaders', '响应头')}${resp?.headers.length ? ` (${resp.headers.length})` : ''}`,
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
          locale={{ emptyText: t('apidef.none', '无') }}
        />
      ),
    },
    {
      key: 'actual',
      label: t('apidef.actualReq', '实际请求'),
      children: req ? (
        <pre style={codeBox}>{`${req.method} ${req.url}\n\n${req.headers.map((h) => `${h.key}: ${h.value}`).join('\n')}${req.body ? `\n\n${req.body}` : ''}`}</pre>
      ) : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('apidef.notRunYet', '尚未执行')} />
      ),
    },
    {
      key: 'console',
      label: t('apidef.console', '控制台'),
      children: (
        <pre style={codeBox}>
          {[
            req ? `→ ${req.method} ${req.url}` : t('apidef.notRunYet', '尚未执行'),
            err ? `✗ ${err}` : resp ? `← ${resp.status} · ${resp.latencyMs} ms` : running ? '… ' + t('a.loading', '加载中…') : '',
          ].filter(Boolean).join('\n')}
        </pre>
      ),
    },
    // cURL tab: HTTP only.
    ...(isHttp
      ? [{
          key: 'curl',
          label: 'cURL',
          children: req ? <pre style={codeBox}>{reqToCurl(req)}</pre> : <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('apidef.notRunYet', '尚未执行')} />,
        }]
      : []),
    {
      key: 'extract',
      label: `${t('apidef.preExtract', '提取')}${resp?.extractions?.length ? ` (${resp.extractions.length})` : extractRows.length ? ` (${extractRows.length})` : ''}`,
      // If server execution returned actual extraction values, show variable=value; otherwise show configured extractors.
      children: resp?.extractions?.length ? (
        <Table
          size="small"
          pagination={false}
          rowKey={(_, i) => String(i)}
          dataSource={resp.extractions.map(([k, v], i) => ({ k, v, _i: i }))}
          columns={[
            { title: t('apidef.extractVar', '变量'), dataIndex: 'k', width: 220, render: (v: string) => <span className="ms-mono">{v}</span> },
            { title: t('apidef.extractVal', '值'), dataIndex: 'v', render: (v: string) => <span className="ms-mono">{v}</span> },
          ]}
        />
      ) : extractRows.length ? (
        <Table
          size="small"
          pagination={false}
          rowKey={(_, i) => String(i)}
          dataSource={extractRows.map((e, i) => ({ ...e, _i: i }))}
          columns={[
            { title: t('apidef.extractVar', '变量'), dataIndex: 'variable', width: 200, render: (v: string) => <span className="ms-mono">{v || '—'}</span> },
            { title: t('apidef.extractKind', '方式'), dataIndex: 'kind', width: 120 },
            { title: t('apidef.extractExpr', '表达式'), dataIndex: 'expression', render: (v: string) => <span className="ms-mono">{v || '—'}</span> },
          ]}
        />
      ) : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('apidef.noExtract', '无提取(在「后置」配置)')} />
      ),
    },
    {
      key: 'assert',
      label: `${t('apidef.assertions', '断言')}${resp?.assertions?.length ? ` (${resp.assertions.filter((a) => a.passed).length}/${resp.assertions.length})` : assertions?.length ? ` (${assertions.length})` : ''}`,
      // Server execution returns per-assertion pass/fail; otherwise show configured assertions pending a run.
      children: resp?.assertions?.length ? (
        <Table
          size="small"
          pagination={false}
          rowKey={(_, i) => String(i)}
          dataSource={resp.assertions.map((a, i) => ({ ...a, _i: i }))}
          columns={[
            { title: '', width: 56, dataIndex: 'passed', render: (p: boolean) => <Tag color={p ? 'green' : 'red'}>{p ? t('apidef.pass', '通过') : t('apidef.fail', '失败')}</Tag> },
            { title: t('apidef.assertItem', '断言项'), dataIndex: 'item', render: (v: string) => <span className="ms-mono">{v}</span> },
            { title: t('apidef.assertCond', '条件'), dataIndex: 'condition', width: 90 },
            { title: t('apidef.assertExpected', '期望'), dataIndex: 'expected', render: (v: string) => <span className="ms-mono">{v || '—'}</span> },
            { title: t('apidef.assertActual', '实际'), dataIndex: 'actual', render: (v: string, r) => <span className="ms-mono" style={{ color: r.passed ? undefined : 'var(--error)' }}>{v || '—'}</span> },
          ]}
        />
      ) : assertions?.length ? (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('apidef.assertConfigured', '已配置断言,点「服务端执行」查看校验结果')} />
      ) : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('apidef.noAssert', '无断言(在「断言」配置)')} />
      ),
    },
  ]

  return (
    <Card size="small" style={{ marginTop: 12 }} styles={{ body: { padding: 12 } }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8 }}>
        <span style={{ fontWeight: 600, fontSize: 13 }}>{t('apidef.responseContent', '响应内容')}</span>
        <Typography.Text type="secondary" style={{ fontSize: 12 }}>{t('apidef.runResult', '执行结果')}</Typography.Text>
        <div style={{ flex: 1 }} />
        {running && <Typography.Text type="secondary" style={{ fontSize: 12 }}>{t('a.loading', '加载中…')}</Typography.Text>}
        {resp && <Tag color={resp.status < 400 ? 'green' : 'red'}>{resp.status}</Tag>}
        {resp && (
          <LatencyStat totalMs={resp.latencyMs} timings={resp.timings}>
            <Typography.Text type="secondary" style={{ fontSize: 12 }}>{fmtDurationMs(resp.latencyMs)}</Typography.Text>
          </LatencyStat>
        )}
      </div>
      <Tabs className="ms-detail-tabs" size="small" items={items} />
    </Card>
  )
}

export default ApiSpecPanel

/** Settings: placeholder matching the reference UI (metadata read-only; status/module edited in basic info). */
function SettingsTab({ definition }: { definition: ApiDefinition }) {
  const { t } = useI18n()
  return (
    <Space direction="vertical" size={10} style={{ width: '100%', maxWidth: 520 }}>
      <Field label={t('apidef.protocol', '协议')}>
        <Tag color="blue">{definition.protocol}</Tag>
      </Field>
      <Field label={t('apidef.colCreatedBy', '创建人')}>
        <span>{definition.createdBy || '—'}</span>
      </Field>
      <Field label="ID">
        <span className="ms-mono" style={{ fontSize: 12 }}>{definition.id}</span>
      </Field>
    </Space>
  )
}

/** Basic info: description / module / tags / status. Description/tags live in the spec (saved with it); module/status are definition-level and written immediately. */
function BasicInfo({ definition, spec, patch, create }: { definition: ApiDefinition; spec: ApiSpec; patch: (p: Partial<ApiSpec>) => void; create?: boolean }) {
  const { t } = useI18n()
  const [tagInput, setTagInput] = useState('')
  const tags = spec.tags || []
  const [modules, setModules] = useState<ApiModule[]>([])
  const [moduleId, setModuleId] = useState<string | undefined>(definition.moduleId || undefined)
  const [status, setStatus] = useState(definition.status)

  useEffect(() => {
    if (create) return // create mode: module/status become editable in define mode after saving.
    let alive = true
    api.modules(definition.projectId).then((m) => alive && setModules(Array.isArray(m) ? m : [])).catch(() => undefined)
    return () => {
      alive = false
    }
  }, [definition.projectId, create])

  // Module/status are definition-level: written to the backend immediately (not via the spec save button).
  const changeModule = async (mid?: string) => {
    setModuleId(mid)
    try {
      await api.moveDefinition(definition.id, mid || null)
      message.success(t('apidef.moved', '已移动'))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('apidef.saveFailed', '保存失败'))
    }
  }
  const changeStatus = async (s: string) => {
    setStatus(s)
    try {
      await api.updateDefinitionStatus(definition.id, s)
      message.success(t('apidef.statusUpdated', '状态已更新'))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('apidef.saveFailed', '保存失败'))
    }
  }

  return (
    <Space direction="vertical" size={16} style={{ width: '100%', maxWidth: 720 }}>
      <Field label={t('apidef.descLabel', '描述')}>
        <Input.TextArea rows={4} value={spec.description || ''} onChange={(e) => patch({ description: e.target.value })} placeholder={t('apidef.descPlaceholder', '接口描述')} />
      </Field>
      {!create && (
        <Field label={t('apidef.ownerModule', '所属模块')}>
          <Select
            style={{ width: 280 }}
            value={moduleId || ''}
            onChange={changeModule}
            placeholder={t('apidef.unfiled', '未归类')}
            // Match the left tree: always include the unfiled option + modules (no empty-data hint when the project has no modules).
            options={[
              { value: '', label: t('apidef.unfiled', '未归类') },
              ...modules.map((m) => ({ value: m.id, label: m.name })),
            ]}
          />
        </Field>
      )}
      <Field label={t('apidef.tags', '标签')}>
        <Space size={[6, 6]} wrap>
          {tags.map((tg) => (
            <Tag key={tg} closable onClose={() => patch({ tags: tags.filter((x) => x !== tg) })}>{tg}</Tag>
          ))}
          <Input
            size="small"
            style={{ width: 140 }}
            value={tagInput}
            onChange={(e) => setTagInput(e.target.value)}
            onPressEnter={() => {
              const v = tagInput.trim()
              if (v && !tags.includes(v)) patch({ tags: [...tags, v] })
              setTagInput('')
            }}
            placeholder={t('apidef.addTag', '添加标签,回车结束')}
          />
        </Space>
      </Field>
      {!create && (
        <Field label={t('apidef.colStatus', '状态')}>
          <Select
            style={{ width: 180 }}
            value={status}
            onChange={changeStatus}
            options={API_STATUSES.map((s) => ({ value: s, label: statusLabel(s, t) }))}
          />
        </Field>
      )}
    </Space>
  )
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <div style={{ fontSize: 13, color: 'var(--text-2)', marginBottom: 6 }}>{label}</div>
      {children}
    </div>
  )
}

/** Request body editor: none/form-data/urlencoded/json (Schema tree | Json)/xml/raw/binary. */
function BodyEditor({ spec, patch }: { spec: ApiSpec; patch: (p: Partial<ApiSpec>) => void }) {
  const { t } = useI18n()
  const bt = spec.bodyType || 'none'
  const [jsonMode, setJsonMode] = useState<'schema' | 'json'>('schema')
  const [batchOpen, setBatchOpen] = useState(false)

  return (
    <div>
      <Segmented
        size="small"
        value={bt}
        onChange={(v) => patch({ bodyType: v as ApiBodyType })}
        options={BODY_TYPES.map((x) => ({ label: x, value: x }))}
        style={{ marginBottom: 12 }}
      />
      {bt === 'none' ? (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('apidef.noBody', '请求没有 Body')} style={{ margin: '12px 0' }} />
      ) : bt === 'form-data' || bt === 'x-www-form-urlencoded' ? (
        <>
          <div style={{ textAlign: 'right', marginBottom: 6 }}>
            <Button type="link" size="small" onClick={() => setBatchOpen(true)}>{t('body.batchAdd', '批量添加')}</Button>
          </div>
          <QueryParamTable rows={(spec.formBody || []).map((r) => ({ enabled: true, key: r.name || '', type: 'string', value: r.value || '', minLen: '', maxLen: '', description: r.desc || '' }))} onChange={(rows) => patch({ formBody: rows.map((r) => ({ name: r.key, value: r.value, desc: r.description })) })} />
          <BatchAddDrawer open={batchOpen} onClose={() => setBatchOpen(false)} onApply={(rows) => patch({ formBody: [...(spec.formBody || []), ...rows] })} />
        </>
      ) : bt === 'binary' ? (
        <Space.Compact style={{ width: '100%', maxWidth: 640 }}>
          <Input placeholder={t('apidef.descLabel', '描述')} value={spec.requestBody || ''} onChange={(e) => patch({ requestBody: e.target.value })} />
          <Button icon={<UploadOutlined />} title={t('body.uploadHint', '本地上传 / 关联文件(暂未接入)')} />
        </Space.Compact>
      ) : bt === 'json' ? (
        <div>
          <div style={{ display: 'flex', alignItems: 'center', marginBottom: 8 }}>
            <Segmented size="small" value={jsonMode} onChange={(v) => setJsonMode(v as 'schema' | 'json')} options={[{ label: 'Schema', value: 'schema' }, { label: 'Json', value: 'json' }]} />
            <div style={{ flex: 1 }} />
            {jsonMode === 'json' && (
              <Button size="small" onClick={() => patch({ requestBody: formatJson(spec.requestBody || '') })}>{t('apidef.format', '格式化')}</Button>
            )}
          </div>
          {jsonMode === 'schema' ? (
            <BodySchemaTree
              nodes={spec.bodySchema || []}
              onChange={(nodes) => patch({ bodySchema: nodes, requestBody: JSON.stringify(schemaToJson(nodes), null, 2) })}
            />
          ) : (
            <Input.TextArea rows={8} value={spec.requestBody || ''} onChange={(e) => patch({ requestBody: e.target.value })} placeholder='{"key":"value"}' className="ms-mono" />
          )}
        </div>
      ) : (
        // xml / raw
        <div>
          <div style={{ textAlign: 'right', marginBottom: 6 }}>
            <Button size="small" onClick={() => patch({ requestBody: formatJson(spec.requestBody || '') })}>{t('apidef.format', '格式化')}</Button>
          </div>
          <Input.TextArea rows={8} value={spec.requestBody || ''} onChange={(e) => patch({ requestBody: e.target.value })} placeholder={bt} className="ms-mono" />
        </div>
      )}
    </div>
  )
}

/** Batch-add drawer: one `name,type,required,value` per line. */
function BatchAddDrawer({ open, onClose, onApply }: { open: boolean; onClose: () => void; onApply: (rows: ApiSpecKV[]) => void }) {
  const { t } = useI18n()
  const [text, setText] = useState('')
  const apply = () => {
    const rows: ApiSpecKV[] = text
      .split('\n')
      .map((l) => l.trim())
      .filter(Boolean)
      .map((l) => {
        const [name, , , value] = l.split(',')
        return { name: (name || '').trim(), value: (value || '').trim(), desc: '' }
      })
      .filter((r) => r.name)
    onApply(rows)
    setText('')
    onClose()
  }
  return (
    <ResizableDrawer
      title={t('body.batchAdd', '批量添加')}
      open={open}
      onClose={onClose}
      width={480}
      footer={
        <div style={{ textAlign: 'right' }}>
          <Space>
            <Button onClick={onClose}>{t('a.cancel', '取消')}</Button>
            <Button type="primary" onClick={apply}>{t('a.apply', '应用')}</Button>
          </Space>
        </div>
      }
    >
      <div style={{ color: 'var(--text-3)', fontSize: 12, marginBottom: 8 }}>{t('body.batchHint', '书写格式:参数名,类型,必填,参数值;多条记录换行分隔')}</div>
      <Input.TextArea rows={12} value={text} onChange={(e) => setText(e.target.value)} placeholder={'username,string,true,admin\npassword,string,true,123'} className="ms-mono" />
    </ResizableDrawer>
  )
}

/** Best-effort JSON formatting; invalid JSON is returned as-is. */
function formatJson(text: string): string {
  try {
    return JSON.stringify(JSON.parse(text), null, 2)
  } catch {
    return text
  }
}

function AuthEditor({ spec, patch }: { spec: ApiSpec; patch: (p: Partial<ApiSpec>) => void }) {
  const { t } = useI18n()
  const auth = spec.auth || { type: 'none' }
  return (
    <Space direction="vertical" size={12} style={{ width: '100%', maxWidth: 520 }}>
      <Radio.Group value={auth.type || 'none'} onChange={(e) => patch({ auth: { ...auth, type: e.target.value } })}>
        <Radio value="none">{t('editor.authNone', '无')}</Radio>
        <Radio value="bearer">Bearer Token</Radio>
        <Radio value="basic">Basic (user:pass)</Radio>
      </Radio.Group>
      {auth.type && auth.type !== 'none' && (
        <Input value={auth.token || ''} onChange={(e) => patch({ auth: { ...auth, token: e.target.value } })} placeholder={auth.type === 'bearer' ? 'token' : 'user:pass'} className="ms-mono" />
      )}
    </Space>
  )
}

function SectionTitle({ children, extra }: { children: React.ReactNode; extra?: React.ReactNode }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', marginBottom: 8 }}>
      <span style={{ fontWeight: 600, fontSize: 13 }}>{children}</span>
      <div style={{ flex: 1 }} />
      {extra}
    </div>
  )
}

/** Key-value section (headers / query / REST / form body): preview = table + copy; define = editable rows + Raw toggle. */
function KVSection({
  title,
  rows,
  editable,
  onChange,
  hideTitle,
}: {
  title: string
  rows: ApiSpecKV[]
  editable: boolean
  onChange: (rows: ApiSpecKV[]) => void
  hideTitle?: boolean
}) {
  const { t } = useI18n()
  const [view, setView] = useState<'table' | 'raw'>('table')

  const setRow = (i: number, p: Partial<ApiSpecKV>) => onChange(rows.map((r, idx) => (idx === i ? { ...r, ...p } : r)))
  const addRow = () => onChange([...rows, { name: '', value: '', desc: '' }])
  const delRow = (i: number) => onChange(rows.filter((_, idx) => idx !== i))

  const raw = rows.filter((r) => r.name).map((r) => `${r.name}: ${r.value ?? ''}`).join('\n')

  const cols: ColumnsType<ApiSpecKV & { _k: string }> = [
    {
      title: t('apidef.kvName', '名称'),
      dataIndex: 'name',
      width: '30%',
      render: (v: string, _r, i) =>
        editable ? <Input value={v} placeholder="name" onChange={(e) => setRow(i, { name: e.target.value })} /> : <span className="ms-mono">{v}</span>,
    },
    {
      title: t('apidef.kvValue', '值'),
      dataIndex: 'value',
      width: '35%',
      render: (v: string, _r, i) =>
        editable ? <Input value={v} placeholder="value" onChange={(e) => setRow(i, { value: e.target.value })} /> : <span className="ms-mono">{v || '—'}</span>,
    },
    {
      title: t('apidef.kvDesc', '描述'),
      dataIndex: 'desc',
      render: (v: string, _r, i) =>
        editable ? <Input value={v} placeholder="desc" onChange={(e) => setRow(i, { desc: e.target.value })} /> : <span style={{ color: 'var(--text-3)' }}>{v || '—'}</span>,
    },
    editable
      ? {
          title: '',
          width: 44,
          render: (_v, _r, i) => <Button type="text" danger size="small" icon={<DeleteOutlined />} onClick={() => delRow(i)} />,
        }
      : {
          title: '',
          width: 44,
          render: (_v, r) => (
            <Tooltip title={t('a.copy', '复制')}>
              <Button type="text" size="small" icon={<CopyOutlined />} onClick={() => copy(`${r.name}: ${r.value ?? ''}`, t('apidef.copied', '已复制'), t('apidef.copyFailed', '复制失败'))} />
            </Tooltip>
          ),
        },
  ]

  return (
    <div>
      {!hideTitle && (
        <SectionTitle
          extra={
            <Space size={8}>
              {!editable && rows.length > 0 && (
                <Segmented
                  size="small"
                  value={view}
                  onChange={(v) => setView(v as 'table' | 'raw')}
                  options={[
                    { label: 'Table', value: 'table' },
                    { label: 'Raw', value: 'raw' },
                  ]}
                />
              )}
              {!editable && raw && (
                <Button size="small" icon={<CopyOutlined />} onClick={() => copy(raw, t('apidef.copied', '已复制'), t('apidef.copyFailed', '复制失败'))}>
                  {t('a.copy', '复制')}
                </Button>
              )}
            </Space>
          }
        >
          {title} <Tag color="default" style={{ marginLeft: 4 }}>{rows.length}</Tag>
        </SectionTitle>
      )}
      {rows.length === 0 && !editable ? (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('apidef.none', '无')} style={{ margin: '8px 0' }} />
      ) : !editable && view === 'raw' ? (
        <pre className="ms-mono" style={{ background: 'var(--panel-2)', padding: 12, borderRadius: 6, margin: 0, fontSize: 12 }}>{raw}</pre>
      ) : (
        <>
          <Table size="small" rowKey="_k" pagination={false} columns={cols} dataSource={rows.map((r, i) => ({ ...r, _k: String(i) }))} locale={{ emptyText: t('apidef.none', '无') }} />
          {editable && (
            <Button size="small" icon={<PlusOutlined />} onClick={addRow} style={{ marginTop: 8 }}>
              {t('apidef.addRow', '添加')}
            </Button>
          )}
        </>
      )}
    </div>
  )
}

// Infer a schema node from a sample JSON value (used by responses and bodies without bodySchema); recurses into objects/arrays.
function jsonNode(name: string, val: unknown): BodySchemaNode {
  if (Array.isArray(val)) {
    return { name, type: 'array', children: val.length ? [jsonNode('items', val[0])] : [] }
  }
  if (val && typeof val === 'object') {
    return { name, type: 'object', children: Object.entries(val as Record<string, unknown>).map(([k, v]) => jsonNode(k, v)) }
  }
  const type: BodySchemaNode['type'] =
    typeof val === 'number' ? (Number.isInteger(val) ? 'integer' : 'number') : typeof val === 'boolean' ? 'boolean' : 'string'
  return { name, type, value: val == null ? '' : String(val) }
}
function jsonToSchemaNodes(text: string): BodySchemaNode[] {
  try {
    const v = JSON.parse(text)
    if (Array.isArray(v)) return [jsonNode('[]', v[0])]
    if (v && typeof v === 'object') return Object.entries(v as Record<string, unknown>).map(([k, val]) => jsonNode(k, val))
  } catch {
    /* not JSON: no schema */
  }
  return []
}

/** Read-only schema table (name / required / type / value / description; object/array expandable). Matches the reference UI. */
function SchemaTable({ nodes }: { nodes: BodySchemaNode[] }) {
  const { t } = useI18n()
  type Row = { key: string; name: string; type: string; value?: string; desc: string; required: string; children?: Row[] }
  // bodySchema description is prefixed with a required/optional marker (matched below); split the flag from the plain description.
  const toRows = (ns: BodySchemaNode[], prefix = ''): Row[] =>
    ns.map((n, i) => {
      const d = n.description || ''
      const required = d.startsWith('必填') ? t('apidef.yes', '是') : d.startsWith('选填') ? t('apidef.no', '否') : '-'
      const desc = d.replace(/^必填( · )?|^选填( · )?/, '')
      return {
        key: `${prefix}${i}-${n.name}`,
        name: n.name,
        type: n.type,
        value: n.value,
        required,
        desc,
        children: n.children?.length ? toRows(n.children, `${prefix}${i}-`) : undefined,
      }
    })
  const cols: ColumnsType<Row> = [
    { title: t('apidef.kvName', '参数名称'), dataIndex: 'name', render: (v: string) => <span className="ms-mono">{v}</span> },
    { title: t('apidef.required', '必填'), dataIndex: 'required', width: 80 },
    { title: t('apidef.colType', '类型'), dataIndex: 'type', width: 110 },
    { title: t('apidef.kvValue', '参数值'), dataIndex: 'value', width: 180, render: (v?: string) => <span className="ms-mono">{v || '—'}</span> },
    { title: t('apidef.kvDesc', '描述'), dataIndex: 'desc', render: (v: string) => <span style={{ color: 'var(--text-3)' }}>{v || '—'}</span> },
  ]
  return <Table size="small" pagination={false} columns={cols} dataSource={toRows(nodes)} locale={{ emptyText: t('apidef.none', '无') }} />
}

/** Schema/JSON toggle (shared by preview body/responses). */
function SchemaJsonToggle({ value, onChange }: { value: 'schema' | 'json'; onChange: (v: 'schema' | 'json') => void }) {
  return (
    <Segmented size="small" value={value} onChange={(v) => onChange(v as 'schema' | 'json')} options={[{ label: 'Schema', value: 'schema' }, { label: 'JSON', value: 'json' }]} />
  )
}

/** Read-only body view in preview mode (content-type + content). */
function BodyView({ spec }: { spec: ApiSpec }) {
  const { t } = useI18n()
  const bt = spec.bodyType || (spec.requestBody ? 'raw' : 'none')
  const isForm = bt === 'form-data' || bt === 'x-www-form-urlencoded'
  const isJson = bt === 'json' || bt === 'raw'
  const [view, setView] = useState<'schema' | 'json'>('schema')
  // Prefer the imported bodySchema tree; otherwise infer from the sample JSON.
  const schemaNodes = (spec.bodySchema?.length ? spec.bodySchema : jsonToSchemaNodes(spec.requestBody || '')) as BodySchemaNode[]
  const hasSchema = isJson && schemaNodes.length > 0
  return (
    <div>
      <SectionTitle
        extra={
          spec.requestBody ? (
            <Space size={8}>
              {hasSchema && <SchemaJsonToggle value={view} onChange={setView} />}
              <Button size="small" icon={<CopyOutlined />} onClick={() => copy(spec.requestBody || '', t('apidef.copied', '已复制'), t('apidef.copyFailed', '复制失败'))}>
                {t('a.copy', '复制')}
              </Button>
            </Space>
          ) : undefined
        }
      >
        {t('apidef.requestBody', '请求体')} <Tag color="blue" style={{ marginLeft: 4 }}>{bt}</Tag>
      </SectionTitle>
      {bt === 'none' ? (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('apidef.noBody', '请求没有 Body')} style={{ margin: '8px 0' }} />
      ) : isForm ? (
        <KVSection title="" rows={spec.formBody || []} editable={false} onChange={() => {}} hideTitle />
      ) : hasSchema && view === 'schema' ? (
        <SchemaTable nodes={schemaNodes} />
      ) : spec.requestBody ? (
        <pre className="ms-mono" style={{ background: 'var(--panel-2)', padding: 12, borderRadius: 6, margin: 0, fontSize: 12, maxHeight: 320, overflow: 'auto' }}>{spec.requestBody}</pre>
      ) : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('apidef.none', '无')} style={{ margin: '8px 0' }} />
      )}
    </div>
  )
}

function ResponsesSection({
  responses,
  editable,
  onChange,
}: {
  responses: ApiSpecResponse[]
  editable: boolean
  onChange: (rows: ApiSpecResponse[]) => void
}) {
  const { t } = useI18n()
  const setRow = (i: number, p: Partial<ApiSpecResponse>) => onChange(responses.map((r, idx) => (idx === i ? { ...r, ...p } : r)))
  const addRow = () => onChange([...responses, { status: 200, body: '' }])
  const delRow = (i: number) => onChange(responses.filter((_, idx) => idx !== i))

  return (
    <div>
      <SectionTitle
        extra={
          editable ? (
            <Button size="small" icon={<PlusOutlined />} onClick={addRow}>
              {t('apidef.addResponse', '添加响应')}
            </Button>
          ) : undefined
        }
      >
        {t('apidef.responses', '响应')} <Tag color="default" style={{ marginLeft: 4 }}>{responses.length}</Tag>
      </SectionTitle>
      {responses.length === 0 ? (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('apidef.none', '无')} style={{ margin: '8px 0' }} />
      ) : (
        <Space direction="vertical" size={10} style={{ width: '100%' }}>
          {responses.map((r, i) =>
            editable ? (
              <div key={i} style={{ border: '1px solid var(--border-soft)', borderRadius: 6, padding: 10 }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8 }}>
                  <span style={{ color: 'var(--text-3)', fontSize: 12 }}>{t('apidef.statusCode', '状态码')}</span>
                  <InputNumber min={100} max={599} value={r.status} onChange={(v) => setRow(i, { status: v ?? undefined })} />
                  <div style={{ flex: 1 }} />
                  <Button type="text" danger size="small" icon={<DeleteOutlined />} onClick={() => delRow(i)} />
                </div>
                <Input.TextArea rows={4} value={r.body} onChange={(e) => setRow(i, { body: e.target.value })} placeholder={t('apidef.responseBody', '响应体')} className="ms-mono" />
              </div>
            ) : (
              <ReadonlyResponse key={i} r={r} />
            ),
          )}
        </Space>
      )}
    </div>
  )
}

/** Single response in preview mode: status code + Schema/JSON toggle + copy (matches reference UI response-body JSON). */
function ReadonlyResponse({ r }: { r: ApiSpecResponse }) {
  const { t } = useI18n()
  const [view, setView] = useState<'schema' | 'json'>('schema')
  const sc = (s?: number) => (s == null ? 'default' : s < 300 ? 'green' : s < 400 ? 'gold' : 'red')
  const nodes = jsonToSchemaNodes(r.body || '')
  const hasSchema = nodes.length > 0
  return (
    <div style={{ border: '1px solid var(--border-soft)', borderRadius: 6, padding: 10 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8 }}>
        <span style={{ color: 'var(--text-3)', fontSize: 12 }}>{t('apidef.statusCode', '状态码')}</span>
        <Tag color={sc(r.status)}>{r.status ?? '—'}</Tag>
        <div style={{ flex: 1 }} />
        {hasSchema && <SchemaJsonToggle value={view} onChange={setView} />}
        {r.body && <Button type="text" size="small" icon={<CopyOutlined />} onClick={() => copy(r.body || '', t('apidef.copied', '已复制'), t('apidef.copyFailed', '复制失败'))} />}
      </div>
      {hasSchema && view === 'schema' ? (
        <SchemaTable nodes={nodes} />
      ) : r.body ? (
        <pre className="ms-mono" style={{ background: 'var(--panel-2)', padding: 10, borderRadius: 6, margin: 0, fontSize: 12, maxHeight: 240, overflow: 'auto' }}>{r.body}</pre>
      ) : (
        <span style={{ color: 'var(--text-3)', fontSize: 12 }}>{t('apidef.none', '无')}</span>
      )}
    </div>
  )
}
