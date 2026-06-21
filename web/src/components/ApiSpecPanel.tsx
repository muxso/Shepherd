import { useEffect, useState } from 'react'
import { Button, Empty, Input, InputNumber, Radio, Segmented, Select, Space, Table, Tabs, Tag, Tooltip } from 'antd'
import { CopyOutlined, PlusOutlined, DeleteOutlined, SaveOutlined } from '@ant-design/icons'
import type { ColumnsType } from 'antd/es/table'
import {
  api,
  ApiError,
  type ApiBodyType,
  type ApiDefinition,
  type ApiModule,
  type ApiSpec,
  type ApiSpecKV,
  type ApiSpecResponse,
} from '../api'
import { message } from '../feedback'
import { useI18n } from '../i18n'

const API_STATUSES = ['DRAFT', 'DEBUGGING', 'COMPLETED', 'DEPRECATED']

/** 复制文本到剪贴板(带轻提示)。navigator.clipboard 在非安全上下文可能缺失,降级 execCommand。 */
async function copy(text: string, ok: string) {
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
    message.error('复制失败')
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
})

/**
 * 接口「预览」(只读)/「定义」(可编辑)/「新建」(create:受控、无 id)共用面板。
 * create 模式由父组件托管 spec(value/onChange),不自行加载/保存,保存按钮也交给父级(新建接口 Tab)。
 */
export default function ApiSpecPanel({
  definition,
  mode,
  value,
  onChange,
}: {
  definition: ApiDefinition
  mode: 'preview' | 'define' | 'create'
  value?: ApiSpec
  onChange?: (s: ApiSpec) => void
}) {
  const { t } = useI18n()
  const create = mode === 'create'
  const editable = mode === 'define' || create
  const [innerSpec, setInnerSpec] = useState<ApiSpec>(emptySpec())
  const [loading, setLoading] = useState(!create)
  const [saving, setSaving] = useState(false)
  const [dirty, setDirty] = useState(false)
  // create 模式用父级受控 spec;否则用内部状态(load/save)。
  const spec = create ? value ?? emptySpec() : innerSpec

  useEffect(() => {
    if (create) return
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
  }, [definition.id, create])

  const patch = (p: Partial<ApiSpec>) => {
    if (create) {
      onChange?.({ ...spec, ...p })
      return
    }
    setInnerSpec((s) => ({ ...s, ...p }))
    setDirty(true)
  }

  const save = async () => {
    setSaving(true)
    try {
      await api.updateDefinitionSpec(definition.id, spec)
      message.success(t('apidef.specSaved', '定义已保存'))
      setDirty(false)
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('apidef.saveFailed', '保存失败'))
    } finally {
      setSaving(false)
    }
  }

  if (loading) return <div style={{ padding: 24, color: '#999' }}>{t('a.loading', '加载中…')}</div>

  // 预览(只读):平铺各段。
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

  // 定义(可编辑):MeterSphere 风子标签。
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
      label: 'REST',
      children: <KVSection title={t('apidef.restParams', 'REST 路径参数')} rows={spec.restParams || []} editable onChange={(rows) => patch({ restParams: rows })} hideTitle />,
    },
    {
      key: 'auth',
      label: t('apidef.auth', '认证'),
      children: <AuthEditor spec={spec} patch={patch} />,
    },
    {
      key: 'resp',
      label: `${t('apidef.responses', '响应')}${spec.responses?.length ? ` (${spec.responses.length})` : ''}`,
      children: <ResponsesSection responses={spec.responses || []} editable onChange={(rows) => patch({ responses: rows })} />,
    },
    {
      key: 'settings',
      label: t('apidef.settings', '设置'),
      children: <SettingsTab definition={definition} />,
    },
  ]

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
      {/* create 模式由「新建接口」Tab 顶部的保存按钮统一提交,这里不再重复。 */}
      {!create && (
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <Button type="primary" icon={<SaveOutlined />} loading={saving} disabled={!dirty} onClick={save}>
            {t('a.save', '保存')}
          </Button>
          {dirty && <span style={{ color: '#ef6c00', fontSize: 12 }}>{t('apidef.unsaved', '有未保存修改')}</span>}
        </div>
      )}
      {/* 下划线子标签(对齐 MeterSphere:基本信息/请求头/请求体/…/设置)。 */}
      <Tabs items={tabs} size="small" />
    </div>
  )
}

/** 设置:对齐参考图占位(当前接口元信息只读;状态/模块在「基本信息」维护)。 */
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

/** 基本信息:描述 / 所属模块 / 标签 / 状态。描述/标签存 spec(随保存);模块/状态为定义级,即时写入。 */
function BasicInfo({ definition, spec, patch, create }: { definition: ApiDefinition; spec: ApiSpec; patch: (p: Partial<ApiSpec>) => void; create?: boolean }) {
  const { t } = useI18n()
  const [tagInput, setTagInput] = useState('')
  const tags = spec.tags || []
  const [modules, setModules] = useState<ApiModule[]>([])
  const [moduleId, setModuleId] = useState<string | undefined>(definition.moduleId || undefined)
  const [status, setStatus] = useState(definition.status)

  useEffect(() => {
    if (create) return // 新建态:模块/状态保存后再在「定义」里维护。
    let alive = true
    api.modules(definition.projectId).then((m) => alive && setModules(Array.isArray(m) ? m : [])).catch(() => undefined)
    return () => {
      alive = false
    }
  }, [definition.projectId, create])

  // 模块/状态是定义级属性,即时写后端(不走 spec 的「保存」按钮),对齐 MeterSphere。
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
            value={moduleId}
            onChange={changeModule}
            allowClear
            placeholder={t('apidef.unfiled', '未归类')}
            options={modules.map((m) => ({ value: m.id, label: m.name }))}
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
            options={API_STATUSES.map((s) => ({ value: s, label: s }))}
          />
        </Field>
      )}
    </Space>
  )
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <div style={{ fontSize: 13, color: '#5b6470', marginBottom: 6 }}>{label}</div>
      {children}
    </div>
  )
}

/** 请求体编辑器:content-type 选择(none/form-data/urlencoded/json/xml/raw/binary)。 */
function BodyEditor({ spec, patch }: { spec: ApiSpec; patch: (p: Partial<ApiSpec>) => void }) {
  const { t } = useI18n()
  const bt = spec.bodyType || 'none'
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
        <KVSection title="" rows={spec.formBody || []} editable onChange={(rows) => patch({ formBody: rows })} hideTitle />
      ) : bt === 'binary' ? (
        <div style={{ color: '#8a9099', fontSize: 13 }}>{t('apidef.binaryHint', '二进制 Body(上传文件):调试时选择文件发送')}</div>
      ) : (
        <div>
          {bt === 'json' && (
            <div style={{ textAlign: 'right', marginBottom: 6 }}>
              <Button size="small" onClick={() => patch({ requestBody: formatJson(spec.requestBody || '') })}>
                {t('apidef.format', '格式化')}
              </Button>
            </div>
          )}
          <Input.TextArea rows={8} value={spec.requestBody || ''} onChange={(e) => patch({ requestBody: e.target.value })} placeholder={bt === 'json' ? '{"key":"value"}' : bt} className="ms-mono" />
        </div>
      )}
    </div>
  )
}

/** 尽力格式化 JSON 文本;非法 JSON 原样返回。 */
function formatJson(text: string): string {
  try {
    return JSON.stringify(JSON.parse(text), null, 2)
  } catch {
    return text
  }
}

/** 认证:none / bearer / basic。 */
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

/** 键值对区块(请求头 / Query / REST / form 体):预览=表格+复制,定义=可增删编辑 + Raw 切换。 */
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
        editable ? <Input value={v} placeholder="desc" onChange={(e) => setRow(i, { desc: e.target.value })} /> : <span style={{ color: '#8a9099' }}>{v || '—'}</span>,
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
              <Button type="text" size="small" icon={<CopyOutlined />} onClick={() => copy(`${r.name}: ${r.value ?? ''}`, t('apidef.copied', '已复制'))} />
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
                <Button size="small" icon={<CopyOutlined />} onClick={() => copy(raw, t('apidef.copied', '已复制'))}>
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
        <pre className="ms-mono" style={{ background: '#f6f8fa', padding: 12, borderRadius: 6, margin: 0, fontSize: 12 }}>{raw}</pre>
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

/** 预览模式的请求体只读视图(显示 content-type + 内容)。 */
function BodyView({ spec }: { spec: ApiSpec }) {
  const { t } = useI18n()
  const bt = spec.bodyType || (spec.requestBody ? 'raw' : 'none')
  const isForm = bt === 'form-data' || bt === 'x-www-form-urlencoded'
  return (
    <div>
      <SectionTitle
        extra={
          spec.requestBody ? (
            <Button size="small" icon={<CopyOutlined />} onClick={() => copy(spec.requestBody || '', t('apidef.copied', '已复制'))}>
              {t('a.copy', '复制')}
            </Button>
          ) : undefined
        }
      >
        {t('apidef.requestBody', '请求体')} <Tag color="blue" style={{ marginLeft: 4 }}>{bt}</Tag>
      </SectionTitle>
      {bt === 'none' ? (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('apidef.noBody', '请求没有 Body')} style={{ margin: '8px 0' }} />
      ) : isForm ? (
        <KVSection title="" rows={spec.formBody || []} editable={false} onChange={() => {}} hideTitle />
      ) : spec.requestBody ? (
        <pre className="ms-mono" style={{ background: '#f6f8fa', padding: 12, borderRadius: 6, margin: 0, fontSize: 12, maxHeight: 320, overflow: 'auto' }}>{spec.requestBody}</pre>
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

  const sc = (s?: number) => (s == null ? 'default' : s < 300 ? 'green' : s < 400 ? 'gold' : 'red')

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
          {responses.map((r, i) => (
            <div key={i} style={{ border: '1px solid #eef0f2', borderRadius: 6, padding: 10 }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8 }}>
                <span style={{ color: '#8a9099', fontSize: 12 }}>{t('apidef.statusCode', '状态码')}</span>
                {editable ? (
                  <InputNumber min={100} max={599} value={r.status} onChange={(v) => setRow(i, { status: v ?? undefined })} />
                ) : (
                  <Tag color={sc(r.status)}>{r.status ?? '—'}</Tag>
                )}
                <div style={{ flex: 1 }} />
                {!editable && r.body && (
                  <Button type="text" size="small" icon={<CopyOutlined />} onClick={() => copy(r.body || '', t('apidef.copied', '已复制'))} />
                )}
                {editable && <Button type="text" danger size="small" icon={<DeleteOutlined />} onClick={() => delRow(i)} />}
              </div>
              {editable ? (
                <Input.TextArea rows={4} value={r.body} onChange={(e) => setRow(i, { body: e.target.value })} placeholder={t('apidef.responseBody', '响应体')} className="ms-mono" />
              ) : r.body ? (
                <pre className="ms-mono" style={{ background: '#f6f8fa', padding: 10, borderRadius: 6, margin: 0, fontSize: 12, maxHeight: 240, overflow: 'auto' }}>{r.body}</pre>
              ) : (
                <span style={{ color: '#bbb', fontSize: 12 }}>{t('apidef.none', '无')}</span>
              )}
            </div>
          ))}
        </Space>
      )}
    </div>
  )
}
