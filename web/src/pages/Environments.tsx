import { useEffect, useState } from 'react'
import { Button, Empty, Input, Select, Space, Switch, Table, Tabs, Typography } from 'antd'
import { PlusOutlined, DeleteOutlined, SaveOutlined, ReloadOutlined } from '@ant-design/icons'
import type { ColumnsType } from 'antd/es/table'
import { message } from '../feedback'
import { api, ApiError, type Environment } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'

type Row = { k: string; v: string }
/** Env variable row. Only name→value persists (backend `variables` is a flat map); type/tags/desc are UI-only. */
type VarRow = { name: string; type: string; value: string; tags: string; desc: string }
type TFn = (key: string, fallback?: string) => string

function KvEditor({ rows, onChange, t }: { rows: Row[]; onChange: (r: Row[]) => void; t: TFn }) {
  const set = (i: number, patch: Partial<Row>) => onChange(rows.map((r, idx) => (idx === i ? { ...r, ...patch } : r)))
  return (
    <div>
      {rows.map((r, i) => (
        <Space key={i} style={{ display: 'flex', marginBottom: 8 }} align="start">
          <Input style={{ width: 240 }} placeholder={t('env.key', '名')} value={r.k} onChange={(e) => set(i, { k: e.target.value })} className="ms-mono" />
          <Input style={{ width: 380 }} placeholder={t('env.value', '值')} value={r.v} onChange={(e) => set(i, { v: e.target.value })} className="ms-mono" />
          <Button type="text" icon={<DeleteOutlined />} onClick={() => onChange(rows.filter((_, idx) => idx !== i))} />
        </Space>
      ))}
      <Button size="small" icon={<PlusOutlined />} onClick={() => onChange([...rows, { k: '', v: '' }])}>
        {t('env.addRow', '加一行')}
      </Button>
    </div>
  )
}

/** Env variable table (ref #19). */
function VarTable({ rows, onChange, t }: { rows: VarRow[]; onChange: (r: VarRow[]) => void; t: TFn }) {
  const set = (i: number, patch: Partial<VarRow>) => onChange(rows.map((r, idx) => (idx === i ? { ...r, ...patch } : r)))
  const cols: ColumnsType<VarRow & { _i: number }> = [
    { title: t('env.varName', '参数名称'), dataIndex: 'name', render: (v: string, _r, i) => <Input size="small" value={v} placeholder={t('a.input', '请输入')} onChange={(e) => set(i, { name: e.target.value })} className="ms-mono" /> },
    { title: t('env.varType', '类型'), dataIndex: 'type', width: 120, render: (v: string, _r, i) => <Select size="small" value={v || '常量'} style={{ width: '100%' }} onChange={(val) => set(i, { type: val })} options={[{ value: '常量', label: t('env.constant', '常量') }, { value: '列表', label: t('env.list', '列表') }]} /> },
    { title: t('env.varValue', '参数值'), dataIndex: 'value', render: (v: string, _r, i) => <Input size="small" value={v} onChange={(e) => set(i, { value: e.target.value })} className="ms-mono" /> },
    { title: t('env.tags', '标签'), dataIndex: 'tags', width: 200, render: (v: string, _r, i) => <Input size="small" value={v} placeholder={t('apidef.addTag', '添加标签,回车结束')} onChange={(e) => set(i, { tags: e.target.value })} /> },
    { title: t('env.varDesc', '描述'), dataIndex: 'desc', render: (v: string, _r, i) => <Input size="small" value={v} onChange={(e) => set(i, { desc: e.target.value })} /> },
    { title: '', width: 44, render: (_v, _r, i) => <Button type="text" size="small" danger icon={<DeleteOutlined />} onClick={() => onChange(rows.filter((_, idx) => idx !== i))} /> },
  ]
  return (
    <div>
      <Table<VarRow & { _i: number }> size="small" rowKey="_i" pagination={false} columns={cols} dataSource={rows.map((r, i) => ({ ...r, _i: i }))} locale={{ emptyText: t('apidef.none', '无') }} />
      <Button size="small" icon={<PlusOutlined />} onClick={() => onChange([...rows, { name: '', type: '常量', value: '', tags: '', desc: '' }])} style={{ marginTop: 8 }}>
        {t('env.addRow', '加一行')}
      </Button>
    </div>
  )
}

// Placeholder: the backend persists only name/baseUrl/global headers/variables/enabled; the
// remaining protocol-level config tabs are stubs (tab row kept to match ref #19).
function Placeholder({ label }: { label: string }) {
  const { t } = useI18n()
  return <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={`${label} · ${t('env.notWired', '暂未接入(仅 HTTP/变量已生效)')}`} style={{ margin: '32px 0' }} />
}

// Environment management: env list on the left, editor on the right. Backend EnvironmentBody
// supports name/baseUrl/headers/variables/enabled; the other tabs are placeholders.
export function EnvironmentsPage() {
  const { projectId } = useApp()
  const { t } = useI18n()
  const [list, setList] = useState<Environment[]>([])
  const [sel, setSel] = useState<string | null>(null) // null = creating
  const [name, setName] = useState('')
  const [desc, setDesc] = useState('')
  const [baseUrl, setBaseUrl] = useState('')
  const [headers, setHeaders] = useState<Row[]>([])
  const [vars, setVars] = useState<VarRow[]>([])
  const [enabled, setEnabled] = useState(true)
  const [saving, setSaving] = useState(false)

  const reset = () => {
    setSel(null)
    setName('')
    setDesc('')
    setBaseUrl('')
    setHeaders([])
    setVars([])
    setEnabled(true)
  }
  const load = async () => {
    if (!projectId) return setList([])
    try {
      setList(await api.environments(projectId))
    } catch {
      setList([])
    }
  }
  useEffect(() => {
    load()
    reset()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId])

  const pick = (e: Environment) => {
    setSel(e.id)
    setName(e.name)
    setDesc('')
    setBaseUrl(e.baseUrl || '')
    setHeaders((e.headers || []).map((h) => ({ k: h.name, v: h.value })))
    setVars(Object.entries(e.variables || {}).map(([k, v]) => ({ name: k, type: '常量', value: v, tags: '', desc: '' })))
    setEnabled(e.enabled ?? true)
  }

  const save = async () => {
    if (!name.trim()) return message.warning(t('env.nameRequired', '请输入环境名'))
    setSaving(true)
    const body = {
      projectId: projectId!,
      name: name.trim(),
      baseUrl: baseUrl.trim(),
      headers: headers.filter((r) => r.k.trim()).map((r) => ({ name: r.k.trim(), value: r.v })),
      variables: Object.fromEntries(vars.filter((r) => r.name.trim()).map((r) => [r.name.trim(), r.value])),
      enabled,
    }
    try {
      if (sel) await api.updateEnvironment(sel, body)
      else {
        const e = await api.createEnvironment(body)
        setSel(e.id)
      }
      message.success(t('env.saved', '已保存'))
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('env.saveFailed', '保存失败'))
    } finally {
      setSaving(false)
    }
  }

  // A plain name filter is enough for the left list; no view/filter toolbar here.
  const [search, setSearch] = useState('')
  const shown = list.filter((e) => e.name.toLowerCase().includes(search.trim().toLowerCase()))

  if (!projectId)
    return (
      <div style={{ padding: 48 }}>
        <Empty description={t('common.selectProject', '请先在顶部选择项目')} />
      </div>
    )

  // Protocol-level config tabs (ref #19); only variables/HTTP/display settings are wired.
  const tabs = [
    { key: 'vars', label: t('env.variables', '环境变量'), children: <VarTable rows={vars} onChange={setVars} t={t} /> },
    {
      key: 'http',
      label: 'HTTP',
      children: (
        <Space direction="vertical" size={16} style={{ width: '100%' }}>
          <div>
            <div style={{ fontWeight: 600, marginBottom: 6 }}>baseUrl</div>
            <Input className="ms-mono" value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} placeholder="https://api.example.com" style={{ maxWidth: 640 }} />
          </div>
          <div>
            <div style={{ fontWeight: 600, marginBottom: 6 }}>{t('env.headers', '全局请求头')}</div>
            <KvEditor rows={headers} onChange={setHeaders} t={t} />
          </div>
        </Space>
      ),
    },
    { key: 'db', label: t('env.db', '数据库'), children: <Placeholder label={t('env.db', '数据库')} /> },
    { key: 'host', label: 'HOST', children: <Placeholder label="HOST" /> },
    { key: 'pre', label: t('apidef.preProcessors', '前置'), children: <Placeholder label={t('apidef.preProcessors', '前置')} /> },
    { key: 'post', label: t('apidef.postProcessors', '后置'), children: <Placeholder label={t('apidef.postProcessors', '后置')} /> },
    { key: 'assert', label: t('apidef.assertions', '断言'), children: <Placeholder label={t('apidef.assertions', '断言')} /> },
    { key: 'ssh', label: t('env.ssh', 'SSH配置'), children: <Placeholder label={t('env.ssh', 'SSH配置')} /> },
    { key: 'amqp', label: t('env.amqp', 'AMQP配置'), children: <Placeholder label={t('env.amqp', 'AMQP配置')} /> },
    { key: 'redis', label: t('env.redis', 'Redis 配置'), children: <Placeholder label={t('env.redis', 'Redis 配置')} /> },
    { key: 'tcp', label: t('env.tcp', 'TCP配置'), children: <Placeholder label={t('env.tcp', 'TCP配置')} /> },
    { key: 'mongo', label: t('env.mongo', 'MongoDB 配置'), children: <Placeholder label={t('env.mongo', 'MongoDB 配置')} /> },
    { key: 'grpc', label: t('env.grpc', 'GRPC配置'), children: <Placeholder label={t('env.grpc', 'GRPC配置')} /> },
    {
      key: 'display',
      label: t('env.display', '显示设置'),
      children: (
        <Space>
          <span style={{ fontWeight: 600 }}>{t('env.enabled', '启用')}</span>
          <Switch checked={enabled} onChange={setEnabled} />
        </Space>
      ),
    },
  ]

  return (
    <div style={{ display: 'flex', height: '100%' }}>
      {/* Left: env list + search */}
      <div style={{ width: 248, background: 'var(--panel)', borderRight: '1px solid var(--border-soft)', display: 'flex', flexDirection: 'column' }}>
        <div style={{ padding: '10px 10px 6px' }}>
          <Input.Search
            allowClear
            placeholder={t('env.searchPlaceholder', '请输入环境名称')}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 4, padding: '0 10px 8px', borderBottom: '1px solid var(--border-soft)' }}>
          <Typography.Text strong style={{ flex: 1 }}>{t('res.env', '环境')}</Typography.Text>
          <Button size="small" type="text" icon={<PlusOutlined />} style={{ color: 'var(--success)' }} title={t('a.new', '新建')} onClick={reset} />
          <Button size="small" type="text" icon={<ReloadOutlined />} onClick={load} />
        </div>
        <div style={{ flex: 1, overflow: 'auto', padding: 8 }}>
          {shown.length === 0 ? (
            <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('res.env', '环境')} />
          ) : (
            shown.map((e) => (
              <div
                key={e.id}
                onClick={() => pick(e)}
                style={{
                  padding: '8px 10px',
                  borderRadius: 6,
                  cursor: 'pointer',
                  marginBottom: 4,
                  fontSize: 13,
                  background: sel === e.id ? 'var(--brand-soft)' : 'transparent',
                  color: sel === e.id ? 'var(--brand)' : undefined,
                  fontWeight: sel === e.id ? 600 : 400,
                }}
              >
                {e.name}
              </div>
            ))
          )}
        </div>
      </div>

      {/* Right: editor (name/desc + protocol config tabs + save bar) */}
      <div style={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column' }}>
        <div style={{ flex: 1, overflow: 'auto', padding: 20 }}>
          <div style={{ marginBottom: 14, maxWidth: 980 }}>
            <div style={{ fontWeight: 600, marginBottom: 6 }}>
              <span style={{ color: 'var(--error)', marginRight: 4 }}>*</span>{t('env.name', '环境名称')}
            </div>
            <Input value={name} onChange={(e) => setName(e.target.value)} placeholder={t('env.namePlaceholder', '如:测试环境')} />
          </div>
          <div style={{ marginBottom: 14, maxWidth: 980 }}>
            <div style={{ fontWeight: 600, marginBottom: 6 }}>{t('env.desc', '描述')}</div>
            <Input.TextArea rows={2} value={desc} onChange={(e) => setDesc(e.target.value)} placeholder={t('env.descPlaceholder', '请输入描述')} />
          </div>
          <Tabs className="ms-detail-tabs" size="small" items={tabs} />
        </div>
        {/* Bottom save bar */}
        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8, padding: '10px 20px', borderTop: '1px solid var(--border-soft)', background: 'var(--panel)' }}>
          <Button onClick={reset}>{t('a.cancel', '取消')}</Button>
          <Button type="primary" icon={<SaveOutlined />} loading={saving} onClick={save}>{t('a.save', '保存')}</Button>
        </div>
      </div>
    </div>
  )
}
