import { useCallback, useEffect, useRef, useState } from 'react'
import {
  Button,
  Card,
  Descriptions,
  Drawer,
  Empty,
  Input,
  Progress,
  Segmented,
  Select,
  Space,
  Switch,
  Table,
  Tag,
  Timeline,
  Tooltip,
} from 'antd'
import type { ColumnsType } from 'antd/es/table'
import { ReloadOutlined, SearchOutlined } from '@ant-design/icons'
import { message, modal } from '../feedback'
import { api, ApiError, type DeliveryEvent, type TaskCenterItem } from '../api'
import { useI18n } from '../i18n'

// 执行状态 → Tag 颜色。
const STATUS_COLOR: Record<string, string> = {
  DISPATCHED: 'default',
  RUNNING: 'processing',
  DELIVERED: 'success',
  FAILED: 'error',
  STOPPED: 'warning',
}
// 执行结果 → Tag 颜色。
const RESULT_COLOR: Record<string, string> = {
  SUCCESS: 'success',
  FAILED: 'error',
  STOPPED: 'warning',
  PENDING: 'blue',
}
// 执行者 → 展示名(执行方式)。
const EXECUTOR_LABEL: Record<string, string> = {
  CLAUDE_CODE: 'Claude Code',
  CODEX: 'Codex',
  OPENCODE: 'OpenCode',
  CODEBUDDY: 'CodeBuddy',
}
const TERMINAL = new Set(['DELIVERED', 'FAILED', 'STOPPED'])

function fmtTime(ms: number): string {
  if (!ms) return '—'
  try {
    return new Date(ms).toLocaleString()
  } catch {
    return '—'
  }
}

export default function TaskCenter() {
  const { t } = useI18n()
  const [tab, setTab] = useState<'realtime' | 'background'>('realtime')
  const [rows, setRows] = useState<TaskCenterItem[]>([])
  const [total, setTotal] = useState(0)
  const [loading, setLoading] = useState(false)
  const [page, setPage] = useState(1)
  const [pageSize, setPageSize] = useState(20)
  const [status, setStatus] = useState<string | undefined>()
  const [executor, setExecutor] = useState<string | undefined>()
  const [q, setQ] = useState('')
  const [auto, setAuto] = useState(true)
  const [detail, setDetail] = useState<TaskCenterItem | null>(null)

  // 自定义 active 标志:实时页只看未终态任务。
  const active = tab === 'realtime'

  const load = useCallback(async () => {
    setLoading(true)
    try {
      const p = await api.taskCenter({ active, status, executor, q: q.trim() || undefined, page, pageSize })
      setRows(p.items)
      setTotal(p.total)
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('tc.loadFailed', '加载失败'))
    } finally {
      setLoading(false)
    }
  }, [active, status, executor, q, page, pageSize, t])

  // 过滤/分页/搜索变化即加载(搜索做 300ms 去抖)。
  useEffect(() => {
    const id = setTimeout(load, q ? 300 : 0)
    return () => clearTimeout(id)
  }, [load, q])

  // 自动刷新:实时页且存在进行中任务时,每 5s 轮询(不打断列表骨架)。
  const loadRef = useRef(load)
  loadRef.current = load
  useEffect(() => {
    if (!auto || !active) return
    const id = setInterval(() => loadRef.current(), 5000)
    return () => clearInterval(id)
  }, [auto, active])

  const switchTab = (v: 'realtime' | 'background') => {
    setTab(v)
    setPage(1)
  }

  const stop = (it: TaskCenterItem) => {
    modal.confirm({
      title: t('tc.stopTitle', '停止任务'),
      content: t('tc.stopContent', '确认停止「{name}」?执行中的尝试将被中止。').replace('{name}', it.title),
      okButtonProps: { danger: true },
      okText: t('tc.stop', '停止'),
      cancelText: t('a.cancel', '取消'),
      onOk: async () => {
        try {
          await api.stopTask(it.id)
          message.success(t('tc.stopped', '已停止'))
          load()
        } catch (e) {
          message.error(e instanceof ApiError ? e.message : t('tc.stopFailed', '停止失败'))
        }
      },
    })
  }

  const remove = (it: TaskCenterItem) => {
    modal.confirm({
      title: t('tc.deleteTitle', '删除任务'),
      content: t('tc.deleteContent', '确认删除「{name}」?执行记录与事件将一并删除。').replace('{name}', it.title),
      okButtonProps: { danger: true },
      okText: t('a.delete', '删除'),
      cancelText: t('a.cancel', '取消'),
      onOk: async () => {
        try {
          await api.deleteTask(it.id)
          message.success(t('tc.deleted', '已删除'))
          load()
        } catch (e) {
          message.error(e instanceof ApiError ? e.message : t('tc.deleteFailed', '删除失败'))
        }
      },
    })
  }

  const cols: ColumnsType<TaskCenterItem> = [
    {
      title: t('tc.colName', '任务名称'),
      dataIndex: 'title',
      render: (v: string, it) => (
        <div style={{ minWidth: 0 }}>
          <a style={{ color: 'var(--brand)' }} onClick={() => setDetail(it)}>
            {v}
          </a>
          <div style={{ fontSize: 12, color: 'var(--text-3)' }}>{it.taskId}</div>
        </div>
      ),
    },
    {
      title: t('tc.colExecutor', '执行方式'),
      dataIndex: 'executor',
      width: 130,
      render: (v: string) => <Tag>{EXECUTOR_LABEL[v] || v}</Tag>,
    },
    {
      title: t('tc.colStatus', '执行状态'),
      dataIndex: 'status',
      width: 110,
      render: (v: string) => <Tag color={STATUS_COLOR[v] || 'default'}>{t(`tc.st.${v}`, v)}</Tag>,
    },
    {
      title: t('tc.colResult', '执行结果'),
      dataIndex: 'result',
      width: 100,
      render: (v: string) => <Tag color={RESULT_COLOR[v] || 'default'}>{t(`tc.res.${v}`, v)}</Tag>,
    },
    {
      title: t('tc.colProgress', '完成率'),
      dataIndex: 'completionRate',
      width: 150,
      render: (v: number, it) => (
        <Progress
          percent={v}
          size="small"
          status={it.status === 'FAILED' ? 'exception' : it.status === 'RUNNING' || it.status === 'DISPATCHED' ? 'active' : 'normal'}
        />
      ),
    },
    {
      title: t('tc.colCreated', '创建时间'),
      dataIndex: 'createdAt',
      width: 180,
      render: (v: number) => <span style={{ color: 'var(--text-2)' }}>{fmtTime(v)}</span>,
    },
    {
      title: t('apidef.colAction', '操作'),
      width: 160,
      fixed: 'right',
      render: (_v, it) => (
        <Space size={2}>
          <Button type="link" size="small" onClick={() => setDetail(it)}>
            {t('tc.detail', '执行详情')}
          </Button>
          {!TERMINAL.has(it.status) ? (
            <Button type="link" size="small" danger onClick={() => stop(it)}>
              {t('tc.stop', '停止')}
            </Button>
          ) : (
            <Button type="link" size="small" danger onClick={() => remove(it)}>
              {t('a.delete', '删除')}
            </Button>
          )}
        </Space>
      ),
    },
  ]

  return (
    <div style={{ height: '100%', overflow: 'auto', padding: 12, background: 'var(--bg)' }}>
      <Card size="small" styles={{ body: { padding: 12 } }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12, flexWrap: 'wrap' }}>
          <Segmented
            value={tab}
            onChange={(v) => switchTab(v as 'realtime' | 'background')}
            options={[
              { label: t('tc.tabRealtime', '实时任务'), value: 'realtime' },
              { label: t('tc.tabBackground', '后台任务'), value: 'background' },
            ]}
          />
          <div style={{ flex: 1 }} />
          <Select
            allowClear
            size="small"
            style={{ width: 130 }}
            placeholder={t('tc.filterStatus', '执行状态')}
            value={status}
            onChange={(v) => {
              setStatus(v)
              setPage(1)
            }}
            options={['DISPATCHED', 'RUNNING', 'DELIVERED', 'FAILED', 'STOPPED'].map((s) => ({
              value: s,
              label: t(`tc.st.${s}`, s),
            }))}
          />
          <Select
            allowClear
            size="small"
            style={{ width: 140 }}
            placeholder={t('tc.filterExecutor', '执行方式')}
            value={executor}
            onChange={(v) => {
              setExecutor(v)
              setPage(1)
            }}
            options={Object.entries(EXECUTOR_LABEL).map(([v, label]) => ({ value: v, label }))}
          />
          <Input
            allowClear
            size="small"
            prefix={<SearchOutlined style={{ color: 'var(--text-3)' }} />}
            placeholder={t('tc.search', '按名称 / ID 搜索')}
            style={{ width: 220 }}
            value={q}
            onChange={(e) => {
              setQ(e.target.value)
              setPage(1)
            }}
          />
          <Tooltip title={t('tc.autoRefresh', '自动刷新')}>
            <Switch size="small" checked={auto} onChange={setAuto} />
          </Tooltip>
          <Tooltip title={t('a.refresh', '刷新')}>
            <Button size="small" icon={<ReloadOutlined />} onClick={load} />
          </Tooltip>
        </div>
        <Table<TaskCenterItem>
          rowKey="id"
          size="middle"
          loading={loading}
          dataSource={rows}
          columns={cols}
          scroll={{ x: 1100 }}
          locale={{ emptyText: <Empty description={t('tc.empty', '暂无任务')} /> }}
          pagination={{
            current: page,
            pageSize,
            total,
            size: 'small',
            showSizeChanger: true,
            pageSizeOptions: [10, 20, 50, 100],
            showTotal: (n) => `${t('apidef.totalPrefix', '共')} ${n} ${t('proj.unit', '条')}`,
            onChange: (p, ps) => {
              setPage(p)
              setPageSize(ps)
            },
          }}
        />
      </Card>
      <TaskDetailDrawer item={detail} onClose={() => setDetail(null)} t={t} />
    </div>
  )
}

// ---------------- 执行详情抽屉(尝试信息 + 执行事件时间线) ----------------
function TaskDetailDrawer({
  item,
  onClose,
  t,
}: {
  item: TaskCenterItem | null
  onClose: () => void
  t: (k: string, f?: string) => string
}) {
  const [events, setEvents] = useState<DeliveryEvent[]>([])
  const [loading, setLoading] = useState(false)

  useEffect(() => {
    if (!item) return
    let alive = true
    setLoading(true)
    api
      .deliveryEvents(item.id)
      .then((list) => {
        if (alive) setEvents(list)
      })
      .catch(() => {
        if (alive) setEvents([])
      })
      .finally(() => {
        if (alive) setLoading(false)
      })
    return () => {
      alive = false
    }
  }, [item])

  return (
    <Drawer
      title={item ? `${t('tc.detail', '执行详情')} · ${item.title}` : ''}
      open={!!item}
      onClose={onClose}
      width={560}
    >
      {item && (
        <>
          <div style={{ margin: '0 0 8px', fontWeight: 600 }}>{t('tc.basicInfo', '基本信息')}</div>
          <Descriptions size="small" column={1} bordered>
            <Descriptions.Item label={t('tc.colName', '任务名称')}>{item.title}</Descriptions.Item>
            <Descriptions.Item label={t('tc.module', '所属模块')}>
              {item.module?.trim() ? item.module : <span style={{ color: 'var(--text-3)' }}>{t('tc.unfiled', '未归属')}</span>}
            </Descriptions.Item>
            <Descriptions.Item label={t('tc.colStatus', '执行状态')}>
              <Tag color={STATUS_COLOR[item.status] || 'default'}>{t(`tc.st.${item.status}`, item.status)}</Tag>
            </Descriptions.Item>
            <Descriptions.Item label={t('tc.description', '描述')}>
              {item.description?.trim() ? (
                <span style={{ whiteSpace: 'pre-wrap' }}>{item.description}</span>
              ) : (
                <span style={{ color: 'var(--text-3)' }}>{t('tc.noDescription', '暂无描述')}</span>
              )}
            </Descriptions.Item>
          </Descriptions>
          <div style={{ margin: '16px 0 8px', fontWeight: 600 }}>{t('tc.execInfo', '执行信息')}</div>
          <Descriptions size="small" column={1} bordered>
            <Descriptions.Item label={t('tc.colResult', '执行结果')}>
              <Tag color={RESULT_COLOR[item.result] || 'default'}>{t(`tc.res.${item.result}`, item.result)}</Tag>
            </Descriptions.Item>
            <Descriptions.Item label={t('tc.colExecutor', '执行方式')}>
              {EXECUTOR_LABEL[item.executor] || item.executor}
            </Descriptions.Item>
            <Descriptions.Item label={t('tc.colProgress', '完成率')}>
              <Progress percent={item.completionRate} size="small" />
            </Descriptions.Item>
            <Descriptions.Item label={t('tc.colCreated', '创建时间')}>{fmtTime(item.createdAt)}</Descriptions.Item>
            {item.runId && <Descriptions.Item label={t('tc.runId', '运行 ID')}>{item.runId}</Descriptions.Item>}
            {item.error && (
              <Descriptions.Item label={t('tc.errorInfo', '错误信息')}>
                <span style={{ color: 'var(--error, #f5222d)' }}>{item.error}</span>
              </Descriptions.Item>
            )}
          </Descriptions>
          <div style={{ margin: '16px 0 8px', fontWeight: 600 }}>{t('tc.events', '执行事件')}</div>
          {loading ? null : events.length ? (
            <Timeline
              items={events.map((e) => ({
                children: (
                  <div>
                    <Tag>{e.kind}</Tag>
                    <span>{e.message}</span>
                    {e.detail ? (
                      <pre style={{ margin: '4px 0 0', fontSize: 12, color: 'var(--text-2)', whiteSpace: 'pre-wrap' }}>
                        {typeof e.detail === 'string' ? e.detail : JSON.stringify(e.detail, null, 2)}
                      </pre>
                    ) : null}
                  </div>
                ),
              }))}
            />
          ) : (
            <Empty description={t('tc.noEvents', '暂无执行事件')} image={Empty.PRESENTED_IMAGE_SIMPLE} />
          )}
        </>
      )}
    </Drawer>
  )
}
