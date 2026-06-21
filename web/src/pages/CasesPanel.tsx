import { useEffect, useState } from 'react'
import {
  Button,
  Drawer,
  Empty,
  Form,
  Input,
  Modal,
  Select,
  Space,
  Table,
  Tag,
  Tooltip,
  Typography,
  message,
} from 'antd'
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

export default function CasesPanel({ definition }: { definition: ApiDefinition }) {
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
      message.error(e instanceof ApiError ? e.message : '加载用例失败')
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
        <Button type="primary" icon={<PlusOutlined />} size="small" onClick={() => setCreateOpen(true)}>
          新建用例
        </Button>
        <Button icon={<ReloadOutlined />} size="small" onClick={load} />
      </Space>
      <Table<ApiCase>
        rowKey="id"
        size="small"
        loading={loading}
        dataSource={cases}
        locale={{ emptyText: <Empty description="暂无用例" /> }}
        pagination={false}
        columns={[
          { title: '名称', dataIndex: 'name', ellipsis: true },
          {
            title: '方法',
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
            title: '断言数',
            dataIndex: 'assertions',
            width: 80,
            render: (a: unknown) => (Array.isArray(a) ? a.length : 0),
          },
          {
            title: '操作',
            width: 170,
            render: (_: unknown, c: ApiCase) => (
              <Space>
                <Tooltip title="选择资源池后执行">
                  <Button type="link" size="small" icon={<PlayCircleOutlined />} onClick={() => setRunFor(c)}>
                    运行
                  </Button>
                </Tooltip>
                <Tooltip title="执行历史">
                  <Button type="link" size="small" icon={<HistoryOutlined />} onClick={() => setExecFor(c)}>
                    历史
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
      message.error(e instanceof ApiError ? e.message : '加载资源池失败')
    }
  }

  useEffect(() => {
    if (caseItem) loadPools()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [caseItem])

  const addPool = async () => {
    if (!newPool.trim()) return
    try {
      const p = await api.createResourcePool(newPool.trim())
      setNewPool('')
      await loadPools()
      setPoolId(p.id)
      message.success('资源池已创建')
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : '创建资源池失败')
    }
  }

  const run = async () => {
    if (!caseItem) return
    setRunning(true)
    try {
      const r = await api.runCase(caseItem.id, projectId, mode, poolId || undefined)
      message.success(`已执行:${r.status}(报告 ${r.reportId.slice(0, 8)})`)
      onDone()
    } catch (e) {
      message.error(e instanceof ApiError ? `执行失败:${e.status}` : '执行失败')
    } finally {
      setRunning(false)
    }
  }

  return (
    <Modal
      title={caseItem ? `运行用例 · ${caseItem.name}` : ''}
      open={!!caseItem}
      onCancel={onClose}
      onOk={run}
      okText="运行"
      confirmLoading={running}
      okButtonProps={{ disabled: !poolId }}
    >
      <Form layout="vertical">
        <Form.Item label="运行模式">
          <Select
            value={mode}
            onChange={setMode}
            options={[
              { value: 'PARALLEL', label: '并行 PARALLEL' },
              { value: 'SERIAL', label: '串行 SERIAL' },
            ]}
          />
        </Form.Item>
        <Form.Item label="资源池" required help={!pools.length ? '暂无资源池,请在下方新建一个' : undefined}>
          <Select
            value={poolId || undefined}
            onChange={setPoolId}
            placeholder="选择资源池"
            options={pools.map((p) => ({ value: p.id, label: p.name }))}
            popupRender={(menu) => (
              <>
                {menu}
                <div style={{ display: 'flex', gap: 8, padding: 8 }}>
                  <Input
                    placeholder="新资源池名"
                    value={newPool}
                    onChange={(e) => setNewPool(e.target.value)}
                    onKeyDown={(e) => e.stopPropagation()}
                  />
                  <Button type="link" onClick={addPool}>
                    新建
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
  const [rows, setRows] = useState<CaseExecution[]>([])
  const [loading, setLoading] = useState(false)

  useEffect(() => {
    if (!caseItem) return
    setLoading(true)
    api
      .caseExecutions(caseItem.id)
      .then((p) => setRows(p.items))
      .catch((e) => message.error(e instanceof ApiError ? e.message : '加载历史失败'))
      .finally(() => setLoading(false))
  }, [caseItem])

  return (
    <Drawer
      title={caseItem ? `执行历史 · ${caseItem.name}` : ''}
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
        locale={{ emptyText: <Empty description="暂无执行记录" /> }}
        columns={[
          {
            title: '结果',
            dataIndex: 'outcome',
            width: 90,
            render: (o: string) => <Tag color={outcomeColor(o)}>{o}</Tag>,
          },
          { title: '时间', dataIndex: 'executedAt', render: (t: string) => <span className="ms-mono">{t}</span> },
          {
            title: '失败项',
            dataIndex: 'failures',
            render: (f: unknown) => {
              const n = Array.isArray(f) ? f.length : 0
              return n ? <Typography.Text type="danger">{n} 项</Typography.Text> : <Tag color="green">无</Tag>
            },
          },
        ]}
      />
    </Drawer>
  )
}
