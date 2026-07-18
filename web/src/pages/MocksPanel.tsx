import { useEffect, useState } from 'react'
import { Button, Card, Empty, Input, InputNumber, Popconfirm, Segmented, Space, Switch, Table, Tabs, Tag, Typography } from 'antd'
import { message } from '../feedback'
import { DeleteOutlined, EditOutlined, PlusOutlined, ReloadOutlined } from '@ant-design/icons'
import { api, ApiError, type ApiBodyType, type ApiDefinition, type ApiMock } from '../api'
import { methodColor } from '../components/tags'
import KVEditor, { type KVRow } from '../components/KVEditor'
import MatchConditionEditor, { type MatchCond, emptyCond } from '../components/MatchConditionEditor'
import { useI18n } from '../i18n'
import EditDrawer from '../components/EditDrawer'

const MATCH_BODY_TYPES: ApiBodyType[] = ['none', 'form-data', 'x-www-form-urlencoded', 'json', 'xml', 'raw', 'binary']
const RESP_BODY_TYPES = ['json', 'xml', 'raw', 'binary'] as const

/** Best-effort JSON pretty-print; invalid input is returned unchanged. */
function formatJson(text: string): string {
  try {
    return JSON.stringify(JSON.parse(text), null, 2)
  } catch {
    return text
  }
}

export default function MocksPanel({ definition }: { definition: ApiDefinition }) {
  const { t } = useI18n()
  const [mocks, setMocks] = useState<ApiMock[]>([])
  const [loading, setLoading] = useState(false)
  const [open, setOpen] = useState(false)
  // non-null = editing that mock; null = creating.
  const [editing, setEditing] = useState<ApiMock | null>(null)

  const remove = async (mock: ApiMock) => {
    try {
      await api.deleteMock(mock.id)
      message.success(t('mock.deleted', 'Mock 已删除'))
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('mock.deleteFailed', '删除失败'))
    }
  }

  const load = async () => {
    setLoading(true)
    try {
      setMocks(await api.mocks(definition.id))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('mock.loadFailed', '加载 Mock 失败'))
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [definition.id])

  return (
    <>
      <Space style={{ marginBottom: 12 }}>
        <Button type="primary" icon={<PlusOutlined />} size="small" onClick={() => { setEditing(null); setOpen(true) }}>
          {t('mock.new', '新建 Mock')}
        </Button>
        <Button icon={<ReloadOutlined />} size="small" onClick={load} />
      </Space>
      <Table<ApiMock>
        rowKey="id"
        size="small"
        loading={loading}
        dataSource={mocks}
        pagination={false}
        locale={{ emptyText: <Empty description={t('mock.empty', '暂无 Mock 期望')} /> }}
        columns={[
          { title: t('mock.colName', '名称'), dataIndex: 'name', ellipsis: true },
          {
            title: t('mock.colStatus', '响应码'),
            dataIndex: 'responseStatus',
            width: 90,
            render: (s: number) => <Tag color={s < 400 ? 'green' : 'red'}>{s}</Tag>,
          },
          {
            title: t('mock.colBody', '响应体'),
            dataIndex: 'responseBody',
            ellipsis: true,
            render: (b: string | null) => <span className="ms-mono">{b || '—'}</span>,
          },
          {
            title: t('mock.colEnabled', '启用'),
            dataIndex: 'enabled',
            width: 70,
            render: (e: boolean) => (e ? <Tag color="green">{t('mock.yes', '是')}</Tag> : <Tag>{t('mock.no', '否')}</Tag>),
          },
          {
            title: t('mock.colCreatedBy', '创建人'),
            dataIndex: 'createdBy',
            width: 110,
            ellipsis: true,
            render: (u?: string) => (u ? <span style={{ color: 'var(--text-2)' }}>{u}</span> : <span style={{ color: 'var(--text-3)' }}>—</span>),
          },
          {
            title: t('a.actions', '操作'),
            key: 'actions',
            width: 120,
            render: (_: unknown, m: ApiMock) => (
              <Space size={4}>
                <Button type="link" size="small" icon={<EditOutlined />} onClick={() => { setEditing(m); setOpen(true) }}>
                  {t('a.edit', '编辑')}
                </Button>
                <Popconfirm
                  title={t('mock.deleteConfirm', '确认删除该 Mock?')}
                  okText={t('a.confirm', '确认')}
                  cancelText={t('a.cancel', '取消')}
                  onConfirm={() => remove(m)}
                >
                  <Button type="link" size="small" danger icon={<DeleteOutlined />} />
                </Popconfirm>
              </Space>
            ),
          },
        ]}
      />

      <EditDrawer
        title={editing ? t('mock.editTitle', '编辑 MOCK') : t('mock.newTitle', '创建 MOCK')}
        open={open}
        onCancel={() => setOpen(false)}
        footer={null}
        width={760}
      >
        <CreateMockForm
          definition={definition}
          mock={editing}
          onClose={() => setOpen(false)}
          onCreated={() => {
            setOpen(false)
            load()
          }}
        />
      </EditDrawer>
    </>
  )
}

function CreateMockForm({
  definition,
  mock,
  onClose,
  onCreated,
}: {
  definition: ApiDefinition
  // existing mock when editing; null/undefined = creating.
  mock?: ApiMock | null
  onClose: () => void
  onCreated: () => void
}) {
  const { t } = useI18n()
  const [name, setName] = useState('')
  const [tags, setTags] = useState<string[]>([])
  const [tagInput, setTagInput] = useState('')
  const [matchHeaders, setMatchHeaders] = useState<MatchCond[]>([emptyCond()])
  const [matchQuery, setMatchQuery] = useState<MatchCond[]>([emptyCond()])
  const [matchType, setMatchType] = useState<ApiBodyType>('json')
  const [matchBody, setMatchBody] = useState('')
  const [followDef, setFollowDef] = useState(false)
  const [respType, setRespType] = useState<(typeof RESP_BODY_TYPES)[number]>('json')
  const [respBody, setRespBody] = useState('{"ok":true}')
  const [respStatus, setRespStatus] = useState(200)
  const [respHeaders, setRespHeaders] = useState<KVRow[]>([{ key: 'Content-Type', value: 'application/json' }])
  const [respDelay, setRespDelay] = useState<number | null>(null)
  const [saving, setSaving] = useState(false)

  // Edit mode: prefill the form from the existing mock (matchRule decomposed back into header/query/body conditions).
  useEffect(() => {
    if (!mock) return
    const condsFrom = (v: unknown): MatchCond[] => {
      const rows = (Array.isArray(v) ? v : []).map((e) => {
        const [n, cond] = e as [string, { op?: string; value?: string }]
        return {
          logic: 'AND' as const,
          name: n ?? '',
          op: (cond?.op ?? 'equals') as MatchCond['op'],
          value: cond?.value ?? '',
        }
      })
      return rows.length ? rows : [emptyCond()]
    }
    const mr = (mock.matchRule ?? {}) as {
      headers?: unknown
      query?: unknown
      body?: Array<{ value?: string }>
    }
    setName(mock.name)
    setTags(mock.tags ?? [])
    setMatchHeaders(condsFrom(mr.headers))
    setMatchQuery(condsFrom(mr.query))
    setMatchBody(mr.body?.[0]?.value ?? '')
    setFollowDef(mock.followDefinition ?? false)
    setRespBody(mock.responseBody ?? '')
    setRespStatus(mock.responseStatus)
    setRespHeaders(
      mock.responseHeaders?.length
        ? mock.responseHeaders
        : [{ key: 'Content-Type', value: 'application/json' }],
    )
    setRespDelay(mock.responseDelayMs ?? null)
  }, [mock])

  const doSave = async (): Promise<boolean> => {
    if (!name.trim()) {
      message.warning(t('mock.nameRequired', '请输入期望名称'))
      return false
    }
    // Build mock-runtime ExtraConditions: headers/query are [name, {op,value}] tuples (op = equals/contains/regex); body is substring containment.
    const matchRule: Record<string, unknown> = {}
    const toConds = (rows: MatchCond[]) => rows.filter((r) => r.name.trim()).map((r) => [r.name.trim(), { op: r.op, value: r.value }])
    const mh = toConds(matchHeaders)
    const mq = toConds(matchQuery)
    if (mh.length) matchRule.headers = mh
    if (mq.length) matchRule.query = mq
    if (matchBody.trim()) matchRule.body = [{ kind: 'contains', value: matchBody.trim() }]
    const body = {
      name: name.trim(),
      matchRule,
      responseStatus: respStatus,
      responseBody: followDef ? undefined : respBody || undefined,
      enabled: mock ? mock.enabled : true,
      tags,
      followDefinition: followDef,
      responseHeaders: respHeaders.filter((h) => h.key.trim()),
      responseDelayMs: respDelay ?? 0,
    }
    setSaving(true)
    try {
      if (mock) {
        await api.updateMock(mock.id, body)
        message.success(t('mock.updated', 'Mock 已更新'))
      } else {
        await api.createMock(definition.id, body)
        message.success(t('mock.created', 'Mock 已创建'))
      }
      return true
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('mock.saveFailed', '保存失败'))
      return false
    } finally {
      setSaving(false)
    }
  }

  const reset = () => {
    setName('')
    setTags([])
    setTagInput('')
    setMatchHeaders([emptyCond()])
    setMatchQuery([emptyCond()])
    setMatchType('json')
    setMatchBody('')
    setFollowDef(false)
    setRespType('json')
    setRespBody('{"ok":true}')
    setRespStatus(200)
    setRespHeaders([{ key: 'Content-Type', value: 'application/json' }])
    setRespDelay(null)
  }

  const save = async () => {
    if (await doSave()) onCreated()
  }
  const saveAndContinue = async () => {
    if (await doSave()) {
      reset()
      message.info(t('mock.continueHint', '可继续创建下一条期望'))
    }
  }

  const countCond = (rows: MatchCond[]) => rows.filter((r) => r.name.trim()).length

  // Match-rule sub-tabs: headers / query (name + equals/contains/regex + value) / REST / body (substring). Maps to mock-runtime ExtraConditions.
  const matchTabs = [
    {
      key: 'headers',
      label: `${t('case.headers', '请求头')}${countCond(matchHeaders) ? ` (${countCond(matchHeaders)})` : ''}`,
      children: <MatchConditionEditor rows={matchHeaders} onChange={setMatchHeaders} />,
    },
    {
      key: 'query',
      label: `Query${countCond(matchQuery) ? ` (${countCond(matchQuery)})` : ''}`,
      children: <MatchConditionEditor rows={matchQuery} onChange={setMatchQuery} />,
    },
    { key: 'rest', label: 'REST', children: <Typography.Text type="secondary">{t('mock.matchRestHint', 'REST 路径参数匹配:匹配引擎暂以定义的 method+path 为基,不支持按 REST 段细化')}</Typography.Text> },
    {
      key: 'body',
      label: `${t('apidef.requestBody', '请求体')}${matchBody.trim() ? ' (1)' : ''}`,
      children: (
        <div>
          <Typography.Text type="secondary" style={{ fontSize: 12 }}>{t('mock.matchBodyContainsHint', '命中条件:请求体包含以下文本(子串)')}</Typography.Text>
          <Segmented
            size="small"
            value={matchType}
            onChange={(v) => setMatchType(v as ApiBodyType)}
            options={MATCH_BODY_TYPES.map((x) => ({ label: x, value: x }))}
            style={{ margin: '10px 0' }}
          />
          <div style={{ textAlign: 'right', marginBottom: 6 }}>
            <Button size="small" onClick={() => setMatchBody(formatJson(matchBody))}>{t('apidef.format', '格式化')}</Button>
          </div>
          <Input.TextArea rows={6} value={matchBody} onChange={(e) => setMatchBody(e.target.value)} placeholder='{"fusiongold$symbollist#0": {}}' className="ms-mono" />
        </div>
      ),
    },
  ]

  // Response sub-tabs: body / headers / status code / delay.
  const respTabs = [
    {
      key: 'body',
      label: t('case.respBody', '响应体'),
      children: (
        <div>
          <Segmented
            size="small"
            value={respType}
            onChange={(v) => setRespType(v as (typeof RESP_BODY_TYPES)[number])}
            options={RESP_BODY_TYPES.map((x) => ({ label: x, value: x }))}
            style={{ marginBottom: 10 }}
          />
          <div style={{ textAlign: 'right', marginBottom: 6 }}>
            <Button size="small" disabled={followDef} onClick={() => setRespBody(formatJson(respBody))}>{t('apidef.format', '格式化')}</Button>
          </div>
          <Input.TextArea
            rows={8}
            value={respBody}
            disabled={followDef}
            onChange={(e) => setRespBody(e.target.value)}
            placeholder='{"ok":true}'
            className="ms-mono"
          />
        </div>
      ),
    },
    {
      key: 'headers',
      label: `${t('case.respHeaders', '响应头')}${respHeaders.filter((h) => h.key.trim()).length ? ` (${respHeaders.filter((h) => h.key.trim()).length})` : ''}`,
      children: <KVEditor rows={respHeaders} onChange={setRespHeaders} namePlaceholder="Content-Type" valuePlaceholder="application/json" />,
    },
    {
      key: 'status',
      label: t('mock.statusCode', '状态码'),
      children: <InputNumber min={100} max={599} value={respStatus} onChange={(v) => setRespStatus(v ?? 200)} />,
    },
    {
      key: 'delay',
      label: t('mock.respDelay', '响应延时'),
      children: (
        <Space>
          <span>{t('case.waitMs', '等待(ms)')}</span>
          <InputNumber min={0} max={600000} value={respDelay ?? undefined} onChange={(v) => setRespDelay(v ?? null)} placeholder="0" />
        </Space>
      ),
    },
  ]

  return (
    <div>
      {/* Header card: [id] name + request type + path (ref #4) */}
      <Card size="small" styles={{ body: { padding: '10px 14px' } }} style={{ marginBottom: 12, background: 'var(--panel-2)' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12, flexWrap: 'wrap' }}>
          <span style={{ fontWeight: 600 }}>【{definition.num ?? '—'}】{definition.name}</span>
          <div style={{ flex: 1 }} />
          <span style={{ color: 'var(--text-3)', fontSize: 12 }}>{t('apidef.reqType', '请求类型')} <Tag color={methodColor(definition.method)} style={{ margin: 0 }}>{definition.method || definition.protocol}</Tag></span>
          <span style={{ color: 'var(--text-3)', fontSize: 12 }}>{t('apidef.colPath', '路径')} <span className="ms-mono" style={{ color: 'var(--text-2)' }}>{definition.path || '—'}</span></span>
        </div>
      </Card>

      <Input value={name} onChange={(e) => setName(e.target.value)} placeholder={t('mock.namePlaceholder2', '请输入期望名称')} style={{ marginBottom: 10 }} />
      <Space size={[4, 4]} wrap style={{ marginBottom: 14 }}>
        {tags.map((tg) => (
          <Tag key={tg} closable onClose={() => setTags(tags.filter((x) => x !== tg))}>{tg}</Tag>
        ))}
        <Input
          size="small"
          style={{ width: 180 }}
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

      <div style={{ fontWeight: 600, fontSize: 13, marginBottom: 8 }}>{t('mock.matchRuleTitle', '匹配规则')}</div>
      <Tabs size="small" items={matchTabs} />

      <div style={{ display: 'flex', alignItems: 'center', gap: 10, fontWeight: 600, fontSize: 13, margin: '12px 0 8px' }}>
        {t('mock.respContent', '响应内容')}
        <span style={{ fontWeight: 400, color: 'var(--text-3)', fontSize: 12 }}>
          {t('mock.followDef', '跟随 API 定义')} <Switch size="small" checked={followDef} onChange={setFollowDef} />
        </span>
      </div>
      <Tabs size="small" items={respTabs} />

      <div style={{ textAlign: 'right', marginTop: 16 }}>
        <Space>
          <Button onClick={onClose}>{t('a.cancel', '取消')}</Button>
          {!mock && (
            <Button loading={saving} onClick={saveAndContinue}>{t('case.saveContinue', '保存并继续创建')}</Button>
          )}
          <Button type="primary" loading={saving} onClick={save}>
            {mock ? t('a.save', '保存') : t('a.create', '创建')}
          </Button>
        </Space>
      </div>
    </div>
  )
}
