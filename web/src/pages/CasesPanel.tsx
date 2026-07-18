import { useEffect, useState } from 'react'
import { Button, Empty, Form, Input, Modal, Select, Space, Table, Tag, Tooltip, Typography } from 'antd'
import ResizableDrawer from '../components/ResizableDrawer'
import { message } from '../feedback'
import { PlusOutlined, PlayCircleOutlined, HistoryOutlined, ReloadOutlined } from '@ant-design/icons'
import {
  api,
  ApiError,
  type ApiCase,
  type ApiDefinition,
  type CaseExecution,
  type ResourcePool,
  type RunMode,
} from '../api'
import { methodColor, outcomeColor } from '../components/tags'
import CaseEditorDrawer from '../components/CaseEditorDrawer'
import { useI18n } from '../i18n'

// Case status display map (status values are Chinese as persisted by the backend; only the label is translated).
const CASE_STATUS_LABELS: Record<string, string> = {
  '进行中': 'case.statusInProgress',
  '已完成': 'case.statusCompleted',
  '已废弃': 'case.statusDeprecated',
}

export default function CasesPanel({ definition, refreshToken }: { definition: ApiDefinition; refreshToken?: number }) {
  const { t } = useI18n()
  const [cases, setCases] = useState<ApiCase[]>([])
  const [loading, setLoading] = useState(false)
  const [createOpen, setCreateOpen] = useState(false)
  const [runFor, setRunFor] = useState<ApiCase | null>(null)
  const [execFor, setExecFor] = useState<ApiCase | null>(null)

  const load = async () => {
    setLoading(true)
    try {
      setCases(await api.cases(definition.id))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('case.loadFailed', '加载用例失败'))
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [definition.id, refreshToken])

  return (
    <>
      <Space style={{ marginBottom: 12 }}>
        <Button type="primary" icon={<PlusOutlined />} size="small" onClick={() => setCreateOpen(true)}>
          {t('case.newCase', '新建用例')}
        </Button>
        <Button icon={<ReloadOutlined />} size="small" onClick={load} />
      </Space>
      <Table<ApiCase>
        rowKey="id"
        size="small"
        loading={loading}
        dataSource={cases}
        locale={{ emptyText: <Empty description={t('case.emptyCases', '暂无用例')} /> }}
        pagination={false}
        columns={[
          { title: t('case.colName', '名称'), dataIndex: 'name', ellipsis: true },
          {
            title: t('case.colPriority', '优先级'),
            dataIndex: 'priority',
            width: 80,
            render: (p?: string) => <Tag color={p === 'P0' ? 'red' : p === 'P1' ? 'orange' : 'default'}>{p || 'P0'}</Tag>,
          },
          {
            title: t('case.colStatus', '状态'),
            dataIndex: 'status',
            width: 90,
            render: (s?: string) => {
              const v = s || '进行中'
              return <Tag>{t(CASE_STATUS_LABELS[v] ?? '', v)}</Tag>
            },
          },
          {
            title: t('case.colMethod', '方法'),
            dataIndex: 'method',
            width: 90,
            render: (m: string) => <Tag color={methodColor(m)}>{m}</Tag>,
          },
          {
            title: 'URL',
            dataIndex: 'url',
            ellipsis: true,
            render: (u: string) => <span className="ms-mono">{u}</span>,
          },
          {
            title: t('case.colAssertCount', '断言数'),
            dataIndex: 'assertions',
            width: 80,
            render: (a: unknown) => (Array.isArray(a) ? a.length : 0),
          },
          {
            title: t('case.colActions', '操作'),
            width: 170,
            render: (_: unknown, c: ApiCase) => (
              <Space>
                <Tooltip title={t('case.runTip', '选择资源池后执行')}>
                  <Button type="link" size="small" icon={<PlayCircleOutlined />} onClick={() => setRunFor(c)}>
                    {t('a.run', '运行')}
                  </Button>
                </Tooltip>
                <Tooltip title={t('case.execHistory', '执行历史')}>
                  <Button type="link" size="small" icon={<HistoryOutlined />} onClick={() => setExecFor(c)}>
                    {t('case.history', '历史')}
                  </Button>
                </Tooltip>
              </Space>
            ),
          },
        ]}
      />

      <CaseEditorDrawer
        open={createOpen}
        definition={definition}
        onClose={() => setCreateOpen(false)}
        onSaved={() => {
          setCreateOpen(false)
          load()
        }}
      />
      <RunModal
        caseItem={runFor}
        projectId={definition.projectId}
        onClose={() => setRunFor(null)}
        onDone={() => setRunFor(null)}
      />
      <ExecutionsDrawer caseItem={execFor} onClose={() => setExecFor(null)} />
    </>
  )
}

function RunModal({
  caseItem,
  projectId,
  onClose,
  onDone,
}: {
  caseItem: ApiCase | null
  projectId: string
  onClose: () => void
  onDone: () => void
}) {
  const { t } = useI18n()
  const [pools, setPools] = useState<ResourcePool[]>([])
  const [poolId, setPoolId] = useState<string>('')
  const [mode, setMode] = useState<RunMode>('PARALLEL')
  const [running, setRunning] = useState(false)
  const [newPool, setNewPool] = useState('')

  const loadPools = async () => {
    try {
      const ps = (await api.resourcePools()).filter((p) => p.enabled)
      setPools(ps)
      if (ps.length && !ps.some((p) => p.id === poolId)) setPoolId(ps[0].id)
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('case.loadPoolFailed', '加载资源池失败'))
    }
  }

  useEffect(() => {
    if (caseItem) loadPools()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [caseItem])

  const addPool = async () => {
    if (!newPool.trim()) return
    try {
      const p = await api.createResourcePool({ name: newPool.trim() })
      setNewPool('')
      await loadPools()
      setPoolId(p.id)
      message.success(t('case.poolCreated', '资源池已创建'))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('case.poolCreateFailed', '创建资源池失败'))
    }
  }

  const run = async () => {
    if (!caseItem) return
    setRunning(true)
    try {
      const r = await api.runCase(caseItem.id, projectId, mode, poolId || undefined)
      message.success(`${t('case.executed', '已执行')}:${r.status}(${t('case.report', '报告')} ${r.reportId.slice(0, 8)})`)
      onDone()
    } catch (e) {
      message.error(e instanceof ApiError ? `${t('case.execFailed', '执行失败')}:${e.status}` : t('case.execFailed', '执行失败'))
    } finally {
      setRunning(false)
    }
  }

  return (
    <Modal
      title={caseItem ? `${t('case.runCase', '运行用例')} · ${caseItem.name}` : ''}
      open={!!caseItem}
      onCancel={onClose}
      onOk={run}
      okText={t('a.run', '运行')}
      confirmLoading={running}
      okButtonProps={{ disabled: !poolId }}
    >
      <Form layout="vertical">
        <Form.Item label={t('case.runMode', '运行模式')}>
          <Select
            value={mode}
            onChange={setMode}
            options={[
              { value: 'PARALLEL', label: t('case.parallel', '并行 PARALLEL') },
              { value: 'SERIAL', label: t('case.serial', '串行 SERIAL') },
            ]}
          />
        </Form.Item>
        <Form.Item label={t('case.pool', '资源池')} required help={!pools.length ? t('case.noPoolHelp', '暂无资源池,请在下方新建一个') : undefined}>
          <Select
            value={poolId || undefined}
            onChange={setPoolId}
            placeholder={t('case.selectPool', '选择资源池')}
            options={pools.map((p) => ({ value: p.id, label: p.name }))}
            popupRender={(menu) => (
              <>
                {menu}
                <div style={{ display: 'flex', gap: 8, padding: 8 }}>
                  <Input
                    placeholder={t('case.newPoolName', '新资源池名')}
                    value={newPool}
                    onChange={(e) => setNewPool(e.target.value)}
                    onKeyDown={(e) => e.stopPropagation()}
                  />
                  <Button type="link" onClick={addPool}>
                    {t('a.new', '新建')}
                  </Button>
                </div>
              </>
            )}
          />
        </Form.Item>
      </Form>
    </Modal>
  )
}


function ExecutionsDrawer({ caseItem, onClose }: { caseItem: ApiCase | null; onClose: () => void }) {
  const { t } = useI18n()
  const [rows, setRows] = useState<CaseExecution[]>([])
  const [loading, setLoading] = useState(false)

  useEffect(() => {
    if (!caseItem) return
    setLoading(true)
    api
      .caseExecutions(caseItem.id)
      .then((p) => setRows(p.items))
      .catch((e) => message.error(e instanceof ApiError ? e.message : t('case.loadHistoryFailed', '加载历史失败')))
      .finally(() => setLoading(false))
  }, [caseItem])

  return (
    <ResizableDrawer
      title={caseItem ? `${t('case.execHistory', '执行历史')} · ${caseItem.name}` : ''}
      open={!!caseItem}
      onClose={onClose}
      width={560}
    >
      <Table<CaseExecution>
        rowKey="reportId"
        size="small"
        loading={loading}
        dataSource={rows}
        pagination={false}
        locale={{ emptyText: <Empty description={t('case.emptyExec', '暂无执行记录')} /> }}
        columns={[
          {
            title: t('case.colOutcome', '结果'),
            dataIndex: 'outcome',
            width: 90,
            render: (o: string) => <Tag color={outcomeColor(o)}>{o}</Tag>,
          },
          { title: t('case.colTime', '时间'), dataIndex: 'executedAt', render: (v: string) => <span className="ms-mono">{v}</span> },
          {
            title: t('case.colFailures', '失败项'),
            dataIndex: 'failures',
            render: (f: unknown) => {
              const n = Array.isArray(f) ? f.length : 0
              return n ? <Typography.Text type="danger">{n} {t('case.items', '项')}</Typography.Text> : <Tag color="green">{t('case.none', '无')}</Tag>
            },
          },
        ]}
      />
    </ResizableDrawer>
  )
}
