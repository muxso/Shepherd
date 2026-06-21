import { useEffect, useState } from 'react'
import { Button, Empty, Input, Space, Switch, Typography } from 'antd'
import { PlusOutlined, DeleteOutlined, SaveOutlined, ReloadOutlined } from '@ant-design/icons'
import { message } from '../feedback'
import { api, ApiError, type Environment } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'

type Row = { k: string; v: string }
type TFn = (key: string, fallback?: string) => string

// 键值行编辑器(请求头/变量共用)。
function KvEditor({ rows, onChange, t }: { rows: Row[]; onChange: (r: Row[]) => void; t: TFn }) {
  const set = (i: number, patch: Partial<Row>) => onChange(rows.map((r, idx) => (idx === i ? { ...r, ...patch } : r)))
  return (
    <div>
      {rows.map((r, i) => (
        <Space key={i} style={{ display: 'flex', marginBottom: 8 }} align="start">
          <Input style={{ width: 220 }} placeholder={t('env.key', '名')} value={r.k} onChange={(e) => set(i, { k: e.target.value })} />
          <Input style={{ width: 360 }} placeholder={t('env.value', '值')} value={r.v} onChange={(e) => set(i, { v: e.target.value })} />
          <Button type="text" icon={<DeleteOutlined />} onClick={() => onChange(rows.filter((_, idx) => idx !== i))} />
        </Space>
      ))}
      <Button size="small" icon={<PlusOutlined />} onClick={() => onChange([...rows, { k: '', v: '' }])}>
        {t('env.addRow', '加一行')}
      </Button>
    </div>
  )
}

// 环境管理:左侧环境列表 + 右侧编辑器(名称/baseUrl/全局请求头/变量/启用)。
// 后端 EnvironmentBody 早已支持 headers+variables,这里把它们接出来。
export function EnvironmentsPage() {
  const { projectId } = useApp()
  const { t } = useI18n()
  const [list, setList] = useState<Environment[]>([])
  const [sel, setSel] = useState<string | null>(null) // null = 新建
  const [name, setName] = useState('')
  const [baseUrl, setBaseUrl] = useState('')
  const [headers, setHeaders] = useState<Row[]>([])
  const [vars, setVars] = useState<Row[]>([])
  const [enabled, setEnabled] = useState(true)
  const [saving, setSaving] = useState(false)

  const reset = () => {
    setSel(null)
    setName('')
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
    setBaseUrl(e.baseUrl || '')
    setHeaders((e.headers || []).map((h) => ({ k: h.name, v: h.value })))
    setVars(Object.entries(e.variables || {}).map(([k, v]) => ({ k, v })))
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
      variables: Object.fromEntries(vars.filter((r) => r.k.trim()).map((r) => [r.k.trim(), r.v])),
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

  if (!projectId)
    return (
      <div style={{ padding: 48 }}>
        <Empty description={t('common.selectProject', '请先在顶部选择项目')} />
      </div>
    )

  return (
    <div style={{ display: 'flex', height: '100%' }}>
      {/* 左:环境列表 */}
      <div style={{ width: 240, background: '#fff', borderRight: '1px solid #f0f0f0', display: 'flex', flexDirection: 'column' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '10px 12px', borderBottom: '1px solid #f5f5f5' }}>
          <Typography.Text strong style={{ flex: 1 }}>{t('res.env', '环境')}</Typography.Text>
          <Button size="small" type="primary" icon={<PlusOutlined />} onClick={reset}>{t('a.new', '新建')}</Button>
          <Button size="small" icon={<ReloadOutlined />} onClick={load} />
        </div>
        <div style={{ flex: 1, overflow: 'auto', padding: 8 }}>
          {list.length === 0 ? (
            <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('res.env', '环境')} />
          ) : (
            list.map((e) => (
              <div
                key={e.id}
                onClick={() => pick(e)}
                style={{
                  padding: '8px 10px',
                  borderRadius: 6,
                  cursor: 'pointer',
                  marginBottom: 4,
                  background: sel === e.id ? '#f3eaff' : 'transparent',
                  color: sel === e.id ? '#7c3aed' : undefined,
                }}
              >
                {e.name}
              </div>
            ))
          )}
        </div>
      </div>

      {/* 右:编辑器 */}
      <div style={{ flex: 1, overflow: 'auto', padding: 20, maxWidth: 760 }}>
        <Space direction="vertical" size={16} style={{ width: '100%' }}>
          <div>
            <div style={{ fontWeight: 600, marginBottom: 6 }}>{t('env.name', '环境名称')}</div>
            <Input value={name} onChange={(e) => setName(e.target.value)} placeholder={t('env.namePlaceholder', '如:测试环境')} />
          </div>
          <div>
            <div style={{ fontWeight: 600, marginBottom: 6 }}>baseUrl</div>
            <Input className="ms-mono" value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} placeholder="https://api.example.com" />
          </div>
          <div>
            <div style={{ fontWeight: 600, marginBottom: 6 }}>{t('env.headers', '全局请求头')}</div>
            <KvEditor rows={headers} onChange={setHeaders} t={t} />
          </div>
          <div>
            <div style={{ fontWeight: 600, marginBottom: 6 }}>{t('env.variables', '环境变量')}</div>
            <KvEditor rows={vars} onChange={setVars} t={t} />
          </div>
          <Space>
            <span style={{ fontWeight: 600 }}>{t('env.enabled', '启用')}</span>
            <Switch checked={enabled} onChange={setEnabled} />
          </Space>
          <Space>
            <Button type="primary" icon={<SaveOutlined />} loading={saving} onClick={save}>{t('a.save', '保存')}</Button>
            {sel && <Button onClick={reset}>{t('a.new', '新建')}</Button>}
          </Space>
        </Space>
      </div>
    </div>
  )
}
