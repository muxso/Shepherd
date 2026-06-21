import { useEffect, useState } from 'react'
import { Button, Empty, Input, InputNumber, Segmented, Space, Table, Tag, Tooltip } from 'antd'
import { CopyOutlined, PlusOutlined, DeleteOutlined, SaveOutlined } from '@ant-design/icons'
import type { ColumnsType } from 'antd/es/table'
import { api, ApiError, type ApiDefinition, type ApiSpec, type ApiSpecKV, type ApiSpecResponse } from '../api'
import { message } from '../feedback'
import { useI18n } from '../i18n'

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

const emptySpec = (): ApiSpec => ({ requestHeaders: [], requestQuery: [], requestBody: '', responses: [] })

/** 接口「预览」(只读)与「定义」(可编辑)共用面板。mode=preview 渲染,define 可编辑并保存 spec。 */
export default function ApiSpecPanel({ definition, mode }: { definition: ApiDefinition; mode: 'preview' | 'define' }) {
  const { t } = useI18n()
  const editable = mode === 'define'
  const [spec, setSpec] = useState<ApiSpec>(emptySpec())
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [dirty, setDirty] = useState(false)

  useEffect(() => {
    let alive = true
    setLoading(true)
    // 始终拉最新 spec(列表接口虽带 spec,但编辑后需回读最新)。
    api
      .getDefinition(definition.id)
      .then((d) => alive && setSpec({ ...emptySpec(), ...(d.spec || {}) }))
      .catch(() => alive && setSpec(emptySpec()))
      .finally(() => alive && setLoading(false))
    return () => {
      alive = false
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [definition.id])

  const patch = (p: Partial<ApiSpec>) => {
    setSpec((s) => ({ ...s, ...p }))
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

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 18 }}>
      {editable && (
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <Button type="primary" icon={<SaveOutlined />} loading={saving} disabled={!dirty} onClick={save}>
            {t('a.save', '保存')}
          </Button>
          {dirty && <span style={{ color: '#ef6c00', fontSize: 12 }}>{t('apidef.unsaved', '有未保存修改')}</span>}
        </div>
      )}

      <KVSection
        title={t('apidef.requestHeaders', '请求头')}
        rows={spec.requestHeaders || []}
        editable={editable}
        onChange={(rows) => patch({ requestHeaders: rows })}
      />
      <KVSection
        title={t('apidef.requestQuery', 'Query 参数')}
        rows={spec.requestQuery || []}
        editable={editable}
        onChange={(rows) => patch({ requestQuery: rows })}
      />
      <BodySection
        title={t('apidef.requestBody', '请求体')}
        value={spec.requestBody || ''}
        editable={editable}
        onChange={(v) => patch({ requestBody: v })}
      />
      <ResponsesSection
        responses={spec.responses || []}
        editable={editable}
        onChange={(rows) => patch({ responses: rows })}
      />
    </div>
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

/** 键值对区块(请求头 / Query):预览=表格+复制,定义=可增删编辑 + Raw 切换。 */
function KVSection({
  title,
  rows,
  editable,
  onChange,
}: {
  title: string
  rows: ApiSpecKV[]
  editable: boolean
  onChange: (rows: ApiSpecKV[]) => void
}) {
  const { t } = useI18n()
  const [view, setView] = useState<'table' | 'raw'>('table')

  const setRow = (i: number, p: Partial<ApiSpecKV>) => onChange(rows.map((r, idx) => (idx === i ? { ...r, ...p } : r)))
  const addRow = () => onChange([...rows, { name: '', value: '', desc: '' }])
  const delRow = (i: number) => onChange(rows.filter((_, idx) => idx !== i))

  const raw = rows.filter((r) => r.name).map((r) => `${r.name}: ${r.value ?? ''}`).join('\n')

  const cols: ColumnsType<ApiSpecKV> = [
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
            {editable && (
              <Button size="small" icon={<PlusOutlined />} onClick={addRow}>
                {t('apidef.addRow', '添加')}
              </Button>
            )}
          </Space>
        }
      >
        {title} <Tag color="default" style={{ marginLeft: 4 }}>{rows.length}</Tag>
      </SectionTitle>
      {rows.length === 0 ? (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('apidef.none', '无')} style={{ margin: '8px 0' }} />
      ) : !editable && view === 'raw' ? (
        <pre className="ms-mono" style={{ background: '#f6f8fa', padding: 12, borderRadius: 6, margin: 0, fontSize: 12 }}>{raw}</pre>
      ) : (
        <Table size="small" rowKey="_k" pagination={false} columns={cols as ColumnsType<ApiSpecKV & { _k: string }>} dataSource={rows.map((r, i) => ({ ...r, _k: String(i) }))} />
      )}
    </div>
  )
}

function BodySection({
  title,
  value,
  editable,
  onChange,
}: {
  title: string
  value: string
  editable: boolean
  onChange: (v: string) => void
}) {
  const { t } = useI18n()
  return (
    <div>
      <SectionTitle
        extra={
          !editable && value ? (
            <Button size="small" icon={<CopyOutlined />} onClick={() => copy(value, t('apidef.copied', '已复制'))}>
              {t('a.copy', '复制')}
            </Button>
          ) : undefined
        }
      >
        {title}
      </SectionTitle>
      {editable ? (
        <Input.TextArea rows={6} value={value} onChange={(e) => onChange(e.target.value)} placeholder='{"key":"value"}' className="ms-mono" />
      ) : value ? (
        <pre className="ms-mono" style={{ background: '#f6f8fa', padding: 12, borderRadius: 6, margin: 0, fontSize: 12, maxHeight: 320, overflow: 'auto' }}>{value}</pre>
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

  const statusColor = (s?: number) => (s == null ? 'default' : s < 300 ? 'green' : s < 400 ? 'gold' : 'red')

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
                  <Tag color={statusColor(r.status)}>{r.status ?? '—'}</Tag>
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
