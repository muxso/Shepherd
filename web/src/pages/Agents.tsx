import { useEffect, useState } from 'react'
import { Button, Drawer, Empty, Form, Input, Modal, Space, Switch, Table, Tag, Tooltip } from 'antd'
import { message } from '../feedback'
import { PlusOutlined, ReloadOutlined, SyncOutlined, HistoryOutlined } from '@ant-design/icons'
import { api, ApiError, type RunnerAgent, type RunnerExecution, type FleetRuntime, type FleetStat } from '../api'
import { useI18n } from '../i18n'
import { PageBody, PageContainer, PageHeader } from '../components/Page'
import { useApp } from '../context'
import { useListView, type ListColumn } from '../components/ListView'

// Executor (AI agent) management: register/list remote executors (Claude Code, Codex, ...),
// refresh their self-reported protocols, view execution history. Task dispatch happens in the
// AI requirement breakdown graph (where an executor is picked).
export default function Agents() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const [agents, setAgents] = useState<RunnerAgent[]>([])
  const [loading, setLoading] = useState(false)
  const [addOpen, setAddOpen] = useState(false)
  const [execFor, setExecFor] = useState<RunnerAgent | null>(null)
  const [refreshing, setRefreshing] = useState<string | null>(null)

  const load = async () => {
    setLoading(true)
    try {
      const a = await api.runnerAgents()
      setAgents(Array.isArray(a) ? a : [])
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('agent.loadFailed', '加载执行机失败'))
    } finally {
      setLoading(false)
    }
  }
  useEffect(() => { load() }, []) // eslint-disable-line react-hooks/exhaustive-deps

  const refresh = async (a: RunnerAgent) => {
    setRefreshing(a.id)
    try {
      await api.refreshRunnerAgent(a.id)
      message.success(t('agent.refreshed', '已刷新协议'))
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('agent.refreshFailed', '刷新失败'))
    } finally {
      setRefreshing(null)
    }
  }

  const cols: ListColumn<RunnerAgent>[] = [
    { key: 'name', label: t('agent.name', '名称'), title: t('agent.name', '名称'), dataIndex: 'name', render: (v: string) => <span style={{ fontWeight: 500 }}>{v}</span> },
    { key: 'baseUrl', label: t('agent.baseUrl', '接入地址'), title: t('agent.baseUrl', '接入地址'), dataIndex: 'baseUrl', render: (v: string) => <span className="ms-mono" style={{ color: 'var(--text-2)' }}>{v}</span> },
    { key: 'protocols', label: t('agent.protocols', '支持协议'), title: t('agent.protocols', '支持协议'), dataIndex: 'protocols', render: (ps?: string[]) => (ps?.length ? <Space size={[4, 4]} wrap>{ps.map((p) => <Tag key={p} color="geekblue">{p}</Tag>)}</Space> : <span style={{ color: 'var(--text-3)' }}>—</span>) },
    { key: 'status', label: t('agent.status', '状态'), title: t('agent.status', '状态'), dataIndex: 'enabled', width: 90, render: (e: boolean) => <Tag color={e ? 'green' : 'default'}>{e ? t('agent.enabled', '启用') : t('agent.disabled', '停用')}</Tag> },
    {
      key: 'action',
      label: t('req.action', '操作'),
      title: t('req.action', '操作'),
      width: 170,
      render: (_v, a) => (
        <Space size={4}>
          <Tooltip title={t('agent.refresh', '刷新协议')}>
            <Button type="link" size="small" icon={<SyncOutlined spin={refreshing === a.id} />} onClick={() => refresh(a)}>{t('agent.refresh', '刷新')}</Button>
          </Tooltip>
          <Button type="link" size="small" icon={<HistoryOutlined />} onClick={() => setExecFor(a)}>{t('agent.executions', '执行历史')}</Button>
        </Space>
      ),
    },
  ]
  const lv = useListView<RunnerAgent>({
    kind: 'runner-agent',
    projectId,
    searchOf: (a) => `${a.name} ${a.baseUrl}`,
    searchLabel: t('agent.searchPh', '搜索名称/地址'),
    fields: [
      {
        key: 'status', label: t('agent.status', '状态'), type: 'enum',
        options: [
          { value: 'ENABLED', label: t('agent.enabled', '启用') },
          { value: 'DISABLED', label: t('agent.disabled', '停用') },
        ],
        get: (a) => (a.enabled ? 'ENABLED' : 'DISABLED'),
      },
    ],
    columns: cols,
    rows: agents,
  })

  return (
    <PageContainer>
      <PageHeader
        title={t('agent.title', '人机协同 · 执行机')}
        subtitle={t('agent.subtitle', '注册 Claude Code / Codex 等 AI 执行者;任务在「AI 需求」拆分图里派发')}
        extra={
          <>
            <Button type="primary" icon={<PlusOutlined />} onClick={() => setAddOpen(true)}>{t('agent.register', '注册执行机')}</Button>
            <Button icon={<ReloadOutlined />} onClick={load} />
          </>
        }
      />
      <PageBody>
        <FleetSection />
        {/* List toolbar lives on the section row (not the page header) so it sits next to the table it filters. */}
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginTop: 24, marginBottom: 8 }}>
          <span style={{ fontWeight: 600, color: 'var(--text-2)' }}>
            {t('agent.probeSection', '协议执行机(API / 探测)')}
          </span>
          <div style={{ flex: 1 }} />
          {lv.toolbar}
        </div>
        <Table<RunnerAgent>
          rowKey="id"
          size="middle"
          loading={loading}
          dataSource={lv.rows}
          columns={lv.columns}
          pagination={{ pageSize: 15, size: 'small' }}
          locale={{ emptyText: <Empty description={t('agent.empty', '暂无执行机,点「注册执行机」接入 AI agent')} /> }}
        />
      </PageBody>

      <RegisterAgentModal open={addOpen} onClose={() => setAddOpen(false)} onDone={() => { setAddOpen(false); load() }} />
      <ExecutionsDrawer agent={execFor} onClose={() => setExecFor(null)} />
    </PageContainer>
  )
}

// AI executor fleet: remote Claude/Codex runtimes (SHEPHERD_AGENT_FLEET mode). They register and
// heartbeat outbound; the server judges liveness from heartbeats. Online status is polled every 5s.
// With fleet mode disabled the list is empty and the whole section is hidden.
function FleetSection() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const [rts, setRts] = useState<FleetRuntime[]>([])
  const [stats, setStats] = useState<FleetStat[]>([])
  const [loaded, setLoaded] = useState(false)
  useEffect(() => {
    let alive = true
    const load = () => {
      api.fleetRuntimes()
        .then((r) => { if (alive) { setRts(Array.isArray(r) ? r : []); setLoaded(true) } })
        .catch(() => { if (alive) setLoaded(true) })
      // Queue counts refresh at the same cadence as the runtime list; failures stay silent
      // (the endpoint doesn't exist when fleet mode is disabled).
      api.fleetStats().then((s) => { if (alive) setStats(Array.isArray(s) ? s : []) }).catch(() => {})
    }
    load()
    const h = setInterval(load, 5000)
    return () => { alive = false; clearInterval(h) }
  }, [])
  const now = Date.now()
  const cols: ListColumn<FleetRuntime>[] = [
    { key: 'online', label: t('fleet.status', '在线'), title: t('fleet.status', '在线'), dataIndex: 'online', width: 80, render: (o: boolean) => <Tag color={o ? 'green' : 'default'}>{o ? t('fleet.online', '在线') : t('fleet.offline', '离线')}</Tag> },
    { key: 'name', label: t('agent.name', '名称'), title: t('agent.name', '名称'), dataIndex: 'name', render: (v: string) => <span style={{ fontWeight: 500 }}>{v}</span> },
    { key: 'id', label: t('fleet.runtimeId', 'Runtime ID'), title: t('fleet.runtimeId', 'Runtime ID'), dataIndex: 'id', width: 120, render: (v: string) => <span className="ms-mono" style={{ color: 'var(--text-3)' }}>{v}</span> },
    { key: 'caps', label: t('fleet.caps', '能力'), title: t('fleet.caps', '能力'), dataIndex: 'caps', render: (cs?: string[]) => (cs?.length ? <Space size={[4, 4]} wrap>{cs.map((c) => <Tag key={c} color="geekblue">{c}</Tag>)}</Space> : '—') },
    { key: 'maxConc', label: t('fleet.maxConc', '并发上限'), title: t('fleet.maxConc', '并发上限'), dataIndex: 'maxConcurrency', width: 90, align: 'center' as const },
    { key: 'lastSeen', label: t('fleet.lastSeen', '最近心跳'), title: t('fleet.lastSeen', '最近心跳'), dataIndex: 'lastSeenMs', width: 110, render: (ms: number) => <span style={{ color: 'var(--text-3)', fontSize: 12 }}>{ms ? `${Math.max(0, Math.round((now - ms) / 1000))}s 前` : '—'}</span> },
  ]
  const lv = useListView<FleetRuntime>({
    kind: 'fleet-runtime',
    projectId,
    searchOf: (r) => r.name,
    searchLabel: t('fleet.searchPh', '搜索 runtime 名称'),
    fields: [
      { key: 'online', label: t('fleet.onlineOnly', '仅看在线'), type: 'bool', get: (r) => r.online },
      {
        key: 'cap', label: t('fleet.caps', '能力'), type: 'tags',
        options: [...new Set(rts.flatMap((r) => r.caps))].map((c) => ({ value: c, label: c })),
        get: (r) => r.caps,
      },
    ],
    columns: cols,
    rows: rts,
  })
  // Fleet not enabled (no runtimes after first fetch) → render nothing, keep the protocol-executor view clean.
  if (loaded && rts.length === 0) return null
  // Only show capabilities with backlog or in-flight work, avoiding a row of all-zero noise.
  const busy = stats.filter((s) => s.ready > 0 || s.inFlight > 0)
  return (
    <>
      <div style={{ display: 'flex', alignItems: 'center', marginBottom: 8 }}>
        <span style={{ fontWeight: 600, color: 'var(--text-2)' }}>
          {t('fleet.section', 'AI 执行者机群（远程 Claude / Codex）')}
        </span>
        <div style={{ flex: 1 }} />
        {lv.toolbar}
      </div>
      {busy.length > 0 && (
        <Space size={[8, 8]} wrap style={{ marginBottom: 12 }}>
          {busy.map((s) => (
            <Tag key={s.executor} color="geekblue" style={{ padding: '2px 10px' }}>
              <span style={{ fontWeight: 600 }}>{s.executor}</span>
              {' · '}{t('fleet.ready', '积压')} {s.ready}
              {' · '}{t('fleet.inFlight', '在飞')} {s.inFlight}
              {s.oldestInFlightMs > 0 && (
                <span style={{ color: s.oldestInFlightMs > 60000 ? 'var(--error)' : 'var(--text-3)' }}>
                  {' · '}{t('fleet.oldest', '最久')} {Math.round(s.oldestInFlightMs / 1000)}s
                </span>
              )}
            </Tag>
          ))}
        </Space>
      )}
      <Table<FleetRuntime>
        rowKey="id"
        size="middle"
        dataSource={lv.rows}
        columns={lv.columns}
        pagination={false}
        locale={{ emptyText: <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('fleet.empty', '暂无在线 runtime')} /> }}
      />
    </>
  )
}

function RegisterAgentModal({ open, onClose, onDone }: { open: boolean; onClose: () => void; onDone: () => void }) {
  const { t } = useI18n()
  const [form] = Form.useForm<{ name: string; baseUrl: string; token?: string; enabled: boolean }>()
  const [busy, setBusy] = useState(false)
  useEffect(() => { if (open) form.setFieldsValue({ name: '', baseUrl: '', token: '', enabled: true }) }, [open, form])
  return (
    <Modal title={t('agent.register', '注册执行机')} open={open} onCancel={onClose} footer={null} destroyOnHidden>
      <Form
        form={form}
        layout="vertical"
        onFinish={async (v) => {
          setBusy(true)
          try {
            await api.registerRunnerAgent({ name: v.name.trim(), baseUrl: v.baseUrl.trim(), token: v.token?.trim() || undefined, enabled: v.enabled })
            message.success(t('agent.registered', '执行机已注册'))
            onDone()
          } catch (e) {
            message.error(e instanceof ApiError ? e.message : t('agent.registerFailed', '注册失败(确认接入地址可达 /protocols)'))
          } finally {
            setBusy(false)
          }
        }}
      >
        <Form.Item name="name" label={t('agent.name', '名称')} rules={[{ required: true }]}>
          <Input placeholder={t('agent.namePlaceholder', '如:Claude Code · 本地')} autoFocus />
        </Form.Item>
        <Form.Item name="baseUrl" label={t('agent.baseUrl', '接入地址')} rules={[{ required: true }]}>
          <Input placeholder="http://127.0.0.1:8088" className="ms-mono" />
        </Form.Item>
        <Form.Item name="token" label={t('agent.token', '鉴权 Token(可选)')}>
          <Input.Password placeholder={t('agent.tokenPlaceholder', '远程执行者的访问令牌')} />
        </Form.Item>
        <Form.Item name="enabled" label={t('agent.enabled', '启用')} valuePropName="checked">
          <Switch />
        </Form.Item>
        <Button type="primary" htmlType="submit" block loading={busy}>{t('agent.register', '注册执行机')}</Button>
      </Form>
    </Modal>
  )
}

function ExecutionsDrawer({ agent, onClose }: { agent: RunnerAgent | null; onClose: () => void }) {
  const { t } = useI18n()
  const [rows, setRows] = useState<RunnerExecution[]>([])
  const [loading, setLoading] = useState(false)
  useEffect(() => {
    if (!agent) return
    setLoading(true)
    api.runnerExecutions(agent.id).then((r) => setRows(Array.isArray(r) ? r : [])).catch(() => setRows([])).finally(() => setLoading(false))
  }, [agent])
  return (
    <Drawer title={agent ? `${t('agent.executions', '执行历史')} · ${agent.name}` : ''} open={!!agent} onClose={onClose} width={760}>
      <Table<RunnerExecution>
        rowKey="id"
        size="small"
        loading={loading}
        dataSource={rows}
        pagination={{ pageSize: 20, size: 'small' }}
        locale={{ emptyText: <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('agent.noExecutions', '暂无执行记录')} /> }}
        columns={[
          { title: t('agent.method', '方法'), dataIndex: 'method', width: 70, render: (v: string) => <Tag>{v || '—'}</Tag> },
          { title: 'URL', dataIndex: 'url', ellipsis: true, render: (v: string) => <span className="ms-mono" style={{ fontSize: 12 }}>{v}</span> },
          { title: t('agent.outcome', '结果'), dataIndex: 'outcome', width: 100, render: (v: string) => <Tag color={v === 'SUCCESS' ? 'green' : v === 'ERROR' ? 'red' : 'default'}>{v}</Tag> },
          { title: t('agent.code', '状态码'), dataIndex: 'status', width: 80, render: (v?: number) => v ?? '—' },
          { title: t('agent.elapsed', '耗时'), dataIndex: 'elapsedMs', width: 90, render: (v?: number) => (v != null ? `${v} ms` : '—') },
          { title: t('agent.executedAt', '时间'), dataIndex: 'executedAt', width: 160, render: (v: string) => <span style={{ color: 'var(--text-3)', fontSize: 12 }}>{v?.slice(0, 19) || '—'}</span> },
        ]}
      />
    </Drawer>
  )
}
