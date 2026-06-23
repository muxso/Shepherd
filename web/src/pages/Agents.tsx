import { useEffect, useState } from 'react'
import { Button, Drawer, Empty, Form, Input, Modal, Space, Switch, Table, Tag, Tooltip, Typography } from 'antd'
import { message } from '../feedback'
import { PlusOutlined, ReloadOutlined, SyncOutlined, HistoryOutlined } from '@ant-design/icons'
import type { ColumnsType } from 'antd/es/table'
import { api, ApiError, type RunnerAgent, type RunnerExecution } from '../api'
import { useI18n } from '../i18n'

// 人机协同 · 执行机(AI agent)管理:注册/列出 Claude Code、Codex 等远程执行者,
// 刷新其自报协议、查看执行历史。任务派发在「AI 需求」拆分图里进行(指定 executor)。
export default function Agents() {
  const { t } = useI18n()
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

  const cols: ColumnsType<RunnerAgent> = [
    { title: t('agent.name', '名称'), dataIndex: 'name', render: (v: string) => <span style={{ fontWeight: 500 }}>{v}</span> },
    { title: t('agent.baseUrl', '接入地址'), dataIndex: 'baseUrl', render: (v: string) => <span className="ms-mono" style={{ color: 'var(--text-2)' }}>{v}</span> },
    { title: t('agent.protocols', '支持协议'), dataIndex: 'protocols', render: (ps?: string[]) => (ps?.length ? <Space size={[4, 4]} wrap>{ps.map((p) => <Tag key={p} color="geekblue">{p}</Tag>)}</Space> : <span style={{ color: 'var(--text-3)' }}>—</span>) },
    { title: t('agent.status', '状态'), dataIndex: 'enabled', width: 90, render: (e: boolean) => <Tag color={e ? 'green' : 'default'}>{e ? t('agent.enabled', '启用') : t('agent.disabled', '停用')}</Tag> },
    {
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

  return (
    <div style={{ height: '100%', display: 'flex', flexDirection: 'column' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '14px 16px', background: 'var(--panel)', borderBottom: '1px solid var(--border-soft)' }}>
        <Typography.Text strong style={{ fontSize: 15 }}>{t('agent.title', '人机协同 · 执行机')}</Typography.Text>
        <Typography.Text type="secondary" style={{ fontSize: 12 }}>{t('agent.subtitle', '注册 Claude Code / Codex 等 AI 执行者;任务在「AI 需求」拆分图里派发')}</Typography.Text>
        <div style={{ flex: 1 }} />
        <Button type="primary" icon={<PlusOutlined />} onClick={() => setAddOpen(true)}>{t('agent.register', '注册执行机')}</Button>
        <Button icon={<ReloadOutlined />} onClick={load} />
      </div>
      <div style={{ flex: 1, overflow: 'auto', padding: 16 }}>
        <Table<RunnerAgent>
          rowKey="id"
          size="middle"
          loading={loading}
          dataSource={agents}
          columns={cols}
          pagination={{ pageSize: 15, size: 'small' }}
          locale={{ emptyText: <Empty description={t('agent.empty', '暂无执行机,点「注册执行机」接入 AI agent')} /> }}
        />
      </div>

      <RegisterAgentModal open={addOpen} onClose={() => setAddOpen(false)} onDone={() => { setAddOpen(false); load() }} />
      <ExecutionsDrawer agent={execFor} onClose={() => setExecFor(null)} />
    </div>
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
