import { SshCommand } from './SshCommand'
import { RedisCommand } from './RedisCommand'
import { DatabaseQuery } from './DatabaseQuery'
import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from 'react'
import { Alert, Button, Card, Empty, Input, InputNumber, Modal, Popconfirm, Select, Space, Spin, Switch, Table, Tag, Typography } from 'antd'
import { environmentPluginApi as api, type Configuration, type Descriptor, type NewProfile, type PluginConfig, type PluginOutput, type Profile, type Slot } from './api'

const EMPTY: Configuration = { profiles: [], bindings: [], credentials: [], permissions: { manage: false, execute: false } }
interface State extends Configuration { environmentId: string | null; installed: boolean; enabled: boolean; descriptors: Descriptor[]; loading: boolean; error: string; reload: () => Promise<void> }
const Context = createContext<State | null>(null)
export function EnvironmentPluginsProvider({ environmentId, revision = 0, children }: { environmentId: string | null; revision?: number; children: ReactNode }) {
  const [data, setData] = useState<Configuration>(EMPTY)
  const [descriptors, setDescriptors] = useState<Descriptor[]>([])
  const [enabled, setEnabled] = useState(false)
  const [installed, setInstalled] = useState(false)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const generation = useRef(0)
  const reload = useCallback(async () => {
    const current = ++generation.current
    setLoading(true); setError(''); setData(EMPTY)
    try {
      const capability = await api.capabilities()
      const configuration = capability.enabled && environmentId ? await api.configuration(environmentId) : EMPTY
      if (current !== generation.current) return
      setInstalled(capability.installed ?? true); setEnabled(capability.enabled); setDescriptors(capability.items); setData(configuration)
    } catch (e) {
      if (current === generation.current) setError(e instanceof Error ? e.message : String(e))
    } finally {
      if (current === generation.current) setLoading(false)
    }
  }, [environmentId, revision])
  useEffect(() => { void reload(); return () => { generation.current++ } }, [reload])
  return <Context.Provider value={{ ...data, environmentId, installed, enabled, descriptors, loading, error, reload }}>{children}</Context.Provider>
}
const blank = (descriptor: Descriptor): PluginConfig => ({ schemaVersion: descriptor.configSchema.version, values: {}, secrets: {} })
const slotFor = (section: string): Slot => ({ pre: 'pre', post: 'post', assert: 'assert', host: 'host', display: 'display' } as Record<string, Slot>)[section] || 'connection'

export function EnvironmentPluginPanel({ section }: { section: string }) {
  const state = useContext(Context)
  if (!state) throw new Error('Environment plugin provider is missing')
  const { environmentId, descriptors, profiles, bindings, credentials, reload } = state
  const available = descriptors.filter(d => d.section === section)
  const [sshCommand, setSshCommand] = useState<Profile | null>(null)
  const [redisCommand, setRedisCommand] = useState<Profile | null>(null)
  const [querying, setQuerying] = useState<Profile | null>(null)
  const [editing, setEditing] = useState<Profile | 'new' | null>(null)
  const [pluginId, setPluginId] = useState('')
  const [name, setName] = useState('')
  const [profileEnabled, setProfileEnabled] = useState(true)
  const [applied, setApplied] = useState(true)
  const [config, setConfig] = useState<PluginConfig>({ schemaVersion: 1, values: {}, secrets: {} })
  const [secretDrafts, setSecretDrafts] = useState<Record<string, string>>({})
  const [jsonDrafts, setJsonDrafts] = useState<Record<string, string>>({})
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [result, setResult] = useState<PluginOutput | null>(null)
  const descriptor = available.find(d => d.id === pluginId)
  const slot = slotFor(section)
  const locked = busy || !state.permissions.manage
  useEffect(() => { setEditing(null); setQuerying(null); setRedisCommand(null); setSshCommand(null); setSecretDrafts({}); setResult(null); setError('') }, [environmentId])
  function initialize(descriptor: Descriptor, profile?: Profile) {
    setPluginId(descriptor.id); setName(profile?.name || ''); setProfileEnabled(profile?.enabled ?? true)
    setConfig(profile ? structuredClone(profile.config) : blank(descriptor)); setSecretDrafts({}); setError(''); setResult(null)
    setJsonDrafts(Object.fromEntries(descriptor.configSchema.fields.filter(f => f.kind.type === 'json').map(f => [f.name, profile?.config.values[f.name] === undefined ? '' : JSON.stringify(profile.config.values[f.name], null, 2)])))
    setApplied(profile ? bindings.some(b => b.profileId === profile.id && b.slot === slot && b.enabled) : true)
  }
  function changeValue(field: string, value: unknown) {
    setConfig(c => { const values = { ...c.values }; if (value === undefined || value === null) delete values[field]; else values[field] = value; return { ...c, values } })
  }
  async function test(profile: Profile) {
    if (!environmentId) return
    setBusy(true); setError(''); setResult(null)
    try { setResult(await api.test(environmentId, profile.id)) }
    catch (e) { setError(e instanceof Error ? e.message : String(e)) }
    finally { setBusy(false) }
  }
  async function save(testAfter: boolean) {
    if (!environmentId || !descriptor || !editing) return
    setBusy(true); setError(''); setResult(null)
    try {
      if (!name.trim()) throw new Error('请输入配置名称')
      const next = structuredClone(config)
      for (const field of descriptor.configSchema.fields) {
        if ((field.kind.type === 'secret' || field.kind.type === 'secret_multiline')) {
          const value = secretDrafts[field.name]
          if (value) {
            const created = await api.createCredential(environmentId, `${name.trim()} · ${field.label}`, value)
            next.secrets[field.name] = created.reference
            setConfig(c => ({ ...c, secrets: { ...c.secrets, [field.name]: created.reference } }))
            setSecretDrafts(d => ({ ...d, [field.name]: '' }))
          }
        } else if (field.kind.type === 'json') {
          if (jsonDrafts[field.name]?.trim()) next.values[field.name] = JSON.parse(jsonDrafts[field.name])
          else delete next.values[field.name]
        }
      }
      const body: NewProfile = { pluginId: descriptor.id, name: name.trim(), config: next, enabled: profileEnabled }
      const saved = editing === 'new' ? await api.create(environmentId, body) : await api.update(environmentId, editing.id, editing.revision, body)
      setEditing(saved); setConfig(next)
      const existing = bindings.find(b => b.profileId === saved.id && b.slot === slot)
      if (applied) await api.bind(environmentId, saved.id, slot)
      else if (existing) await api.unbind(environmentId, saved.id, slot)
      await reload()
      if (testAfter) setResult(await api.test(environmentId, saved.id))
    } catch (e) { setError(e instanceof Error ? e.message : String(e)) }
    finally { setBusy(false) }
  }
  async function remove(profile: Profile) {
    if (!environmentId) return
    setBusy(true); setError('')
    try { await api.remove(environmentId, profile.id, profile.revision); await reload() }
    catch (e) { setError(e instanceof Error ? e.message : String(e)) }
    finally { setBusy(false) }
  }
  async function toggleBinding(profile: Profile) {
    if (!environmentId) return
    setBusy(true); setError('')
    try {
      if (bindings.some(b => b.profileId === profile.id && b.slot === slot)) await api.unbind(environmentId, profile.id, slot)
      else await api.bind(environmentId, profile.id, slot)
      await reload()
    } catch (e) { setError(e instanceof Error ? e.message : String(e)) }
    finally { setBusy(false) }
  }
  if (state.loading) return <Spin />
  if (state.error) return <Alert type="error" message={state.error} action={<Button onClick={() => void reload()}>重试</Button>} />
  if (!environmentId) return <Alert type="info" message="请先保存环境，再配置插件" />
  if (!state.installed) return <Alert type="info" message="环境配置插件未安装" description="请由管理员安装后启用。" />
  if (!state.enabled) return <Alert type="info" message="环境配置插件未启用" description="请在系统的插件页面查看状态，由管理员配置后启用。" />
  if (!available.length) return <Empty description="此项能力尚未安装可用插件" />
  const visible = profiles.filter(p => available.some(d => d.id === p.pluginId))
  return <Space direction="vertical" style={{ width: '100%' }}>
    {error && !editing && <Alert type="error" closable onClose={() => setError('')} message={error} />}
    {result && !editing && <Result output={result} />}
    <Typography.Text type="secondary">配置在项目内共享，修改会影响使用它的环境；新凭据默认仅用于当前环境。</Typography.Text>
    {!state.permissions.execute && <Typography.Text type="secondary">需启用环境并具备执行权限，才能测试连接。</Typography.Text>}
    <Button type="primary" disabled={!state.permissions.manage} onClick={() => { initialize(available[0]); setEditing('new') }}>新增连接配置</Button>
    <Table<Profile> size="small" rowKey="id" dataSource={visible} pagination={false} columns={[
      { title: '名称', dataIndex: 'name' },
      { title: '插件', render: (_, p) => available.find(d => d.id === p.pluginId)?.name || p.pluginId },
      { title: '状态', render: (_, p) => <Space><Tag color={p.enabled ? 'green' : 'default'}>{p.enabled ? '启用' : '禁用'}</Tag><Tag>{bindings.some(b => b.profileId === p.id && b.slot === slot && b.enabled) ? '已应用' : '未应用'}</Tag></Space> },
      { title: '操作', render: (_, p) => <Space wrap>
        <Button aria-label="编辑" size="small" disabled={busy} onClick={() => { const d = available.find(d => d.id === p.pluginId)!; initialize(d, p); setEditing(p) }}>编辑</Button>
        {available.find(d => d.id === p.pluginId)?.capabilities.includes('connection_test') && <Button size="small" disabled={busy || !p.enabled || !state.permissions.execute} onClick={() => void test(p)}>测试连接</Button>}
        {['postgres', 'mysql'].includes(p.pluginId) && <Button size="small" disabled={busy || !p.enabled || !state.permissions.execute || !bindings.some(b => b.profileId === p.id && b.slot === 'connection' && b.enabled)} onClick={() => setQuerying(p)}>数据库查询</Button>}
        {p.pluginId === 'redis' && <Button size="small" disabled={busy || !p.enabled || !state.permissions.execute || !bindings.some(b => b.profileId === p.id && b.slot === 'connection' && b.enabled)} onClick={() => setRedisCommand(p)}>Redis 命令</Button>}
        {p.pluginId === 'ssh' && <Button size="small" disabled={busy || !p.enabled || !state.permissions.execute || !bindings.some(b => b.profileId === p.id && b.slot === 'connection' && b.enabled)} onClick={() => setSshCommand(p)}>SSH 命令</Button>}
        <Button size="small" disabled={locked} onClick={() => void toggleBinding(p)}>{bindings.some(b => b.profileId === p.id && b.slot === slot) ? '取消应用' : '应用'}</Button>
        <Popconfirm title="删除此配置？请先取消它在各环境中的应用。" onConfirm={() => remove(p)}><Button aria-label="删除" size="small" danger disabled={locked}>删除</Button></Popconfirm>
      </Space> },
    ]} />
    {sshCommand && <SshCommand key={`${environmentId}/${sshCommand.id}`} environmentId={environmentId} profile={sshCommand} onClose={() => setSshCommand(null)} />}
    {redisCommand && <RedisCommand key={`${environmentId}/${redisCommand.id}`} environmentId={environmentId} profile={redisCommand} onClose={() => setRedisCommand(null)} />}
    {querying && <DatabaseQuery key={`${environmentId}/${querying.id}`} environmentId={environmentId} profile={querying} onClose={() => setQuerying(null)} />}
    <Modal title={editing === 'new' ? '新增插件配置' : '编辑插件配置'} open={!!editing} width={700} destroyOnHidden onCancel={() => { if (!busy) { setSecretDrafts({}); setEditing(null) } }} footer={<Space>
      <Button aria-label="关闭" disabled={busy} onClick={() => { setSecretDrafts({}); setEditing(null) }}>关闭</Button>
      <Button loading={busy} disabled={locked} onClick={() => void save(false)}>保存配置</Button>
      {descriptor?.capabilities.includes('connection_test') && <Button type="primary" loading={busy} disabled={!profileEnabled || !state.permissions.execute || !state.permissions.manage} onClick={() => void save(true)}>保存并测试</Button>}
    </Space>}>
      <Space direction="vertical" size={14} style={{ width: '100%' }}>
        {error && <Alert type="error" message={error} />}
        <label style={{ width: '100%' }}>插件<Select aria-label="插件类型" disabled={editing !== 'new' || locked} value={pluginId || undefined} style={{ width: '100%' }} options={available.map(d => ({ value: d.id, label: d.name }))} onChange={id => initialize(available.find(d => d.id === id)!)} /></label>
        <label style={{ width: '100%' }}>配置名称<Input aria-label="配置名称" value={name} disabled={locked} onChange={e => setName(e.target.value)} /></label>
        {descriptor?.configSchema.fields.map(field => <div key={field.name} style={{ width: '100%' }}>
          <Typography.Text>{field.label}{field.required ? ' *' : ''}</Typography.Text>
          {(field.kind.type === 'secret' || field.kind.type === 'secret_multiline') ? <Space direction="vertical" style={{ width: '100%' }}>
            <Select aria-label={`${field.label} credential`} value={config.secrets[field.name]} allowClear disabled={locked} style={{ width: '100%' }} placeholder="选择已保存的凭据" options={credentials.map(c => ({ value: c.id, label: c.label }))} onChange={value => setConfig(c => { const secrets = { ...c.secrets }; if (value) secrets[field.name] = value; else delete secrets[field.name]; return { ...c, secrets } })} />
            {field.kind.type === 'secret_multiline' ? <Input.TextArea aria-label={field.label} autoComplete="off" rows={6} value={secretDrafts[field.name] || ''} disabled={locked} placeholder="粘贴完整密钥，保存后不回显" onChange={e => setSecretDrafts(d => ({ ...d, [field.name]: e.target.value }))} /> : <Input.Password aria-label={field.label} autoComplete="new-password" value={secretDrafts[field.name] || ''} disabled={locked} placeholder="或填写新凭据，保存后不回显" onChange={e => setSecretDrafts(d => ({ ...d, [field.name]: e.target.value }))} />}
          </Space> : field.kind.type === 'integer' ? <InputNumber aria-label={field.label} disabled={locked} style={{ width: '100%' }} min={field.kind.min} max={field.kind.max} value={typeof config.values[field.name] === 'number' ? config.values[field.name] as number : null} placeholder="使用插件默认值" onChange={value => changeValue(field.name, value)} />
            : field.kind.type === 'choice' ? <Select aria-label={field.label} disabled={locked} style={{ width: '100%' }} allowClear placeholder="使用插件默认值" value={config.values[field.name] as string | undefined} options={field.kind.options.map(value => ({ value, label: value }))} onChange={value => changeValue(field.name, value)} />
              : field.kind.type === 'boolean' ? <Select aria-label={field.label} disabled={locked} style={{ width: '100%' }} allowClear placeholder="使用插件默认值" value={typeof config.values[field.name] === 'boolean' ? String(config.values[field.name]) : undefined} options={[{ value: 'true', label: '是' }, { value: 'false', label: '否' }]} onChange={value => changeValue(field.name, value === undefined ? undefined : value === 'true')} />
                : field.kind.type === 'json' ? <Input.TextArea aria-label={field.label} disabled={locked} rows={4} value={jsonDrafts[field.name] || ''} onChange={e => setJsonDrafts(d => ({ ...d, [field.name]: e.target.value }))} />
                  : field.kind.type === 'string_list' ? <Input.TextArea aria-label={field.label} disabled={locked} rows={3} value={(config.values[field.name] as string[] | undefined)?.join('\n') || ''} onChange={e => changeValue(field.name, e.target.value.split('\n').filter(Boolean))} />
                    : <Input aria-label={field.label} disabled={locked} maxLength={field.kind.max_length} value={String(config.values[field.name] ?? '')} onChange={e => changeValue(field.name, e.target.value || undefined)} />}
        </div>)}
        <Space><Switch aria-label="启用配置" checked={profileEnabled} disabled={locked} onChange={setProfileEnabled} />启用配置<Switch aria-label="应用到当前环境" checked={applied} disabled={locked} onChange={setApplied} />应用到当前环境</Space>
        {result && <Result output={result} />}
      </Space>
    </Modal>
  </Space>
}
function Result({ output }: { output: PluginOutput }) {
  const passed = output.status === 'success' && output.assertions.every(a => a.passed)
  return <Card size="small"><Alert type={passed ? 'success' : 'error'} message={passed ? '连接测试通过' : '连接测试未通过'} /><pre style={{ maxHeight: 260, overflow: 'auto', whiteSpace: 'pre-wrap' }}>{JSON.stringify(output.data, null, 2)}</pre>{output.assertions.filter(a => !a.passed).map(a => <div key={a.id}>{a.message}</div>)}</Card>
}
