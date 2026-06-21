import { useEffect, useState } from 'react'
import { Button, Card, Col, Descriptions, Drawer, Empty, Form, Input, Modal, Row, Select, Space, Statistic, Table, Tabs, Tag, Typography } from 'antd'
import { message, modal } from '../feedback'
import { useNavigate } from 'react-router-dom'
import { BranchesOutlined, FlagOutlined, PartitionOutlined, PlayCircleOutlined, SendOutlined } from '@ant-design/icons'
import {
  api,
  ApiError,
  type ApiCase,
  type DeliveryEvent,
  type Requirement,
  type Task,
  type VerificationReport,
} from '../api'
import { useApp } from '../context'
import { Workspace, WorkList, useWorkTabs } from '../components/Workspace'
import { regAdd, regList, type RegItem } from '../registry'
import { useI18n } from '../i18n'

const toLines = (s: string) => s.split('\n').map((x) => x.trim()).filter(Boolean)
const taskColor = (s: string) => (s === 'VERIFIED' ? 'green' : s === 'FAILED' ? 'red' : s === 'PENDING' ? 'default' : 'blue')

// 需求与编排合一:需求列表 → 详情 Tab(需求信息/版本/基线/拆分 → 拆分图任务+运行+交付+验证)。
export default function Requirements() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const [items, setItems] = useState<RegItem[]>([])
  const [createOpen, setCreateOpen] = useState(false)
  const tabs = useWorkTabs()

  // 列表以后端为准(含 CLI/API 建的需求),叠加本地注册表的 meta(拆分/验证链接)。
  const loadList = async () => {
    const local = regList('requirement', projectId)
    const localById = new Map(local.map((r) => [r.id, r]))
    try {
      const page = await api.requirements(projectId)
      setItems(page.items.map((r) => localById.get(r.id) || { id: r.id, label: r.title, createdAt: 0 }))
    } catch {
      setItems(local) // 后端不可用时回落本地
    }
  }
  useEffect(() => {
    loadList()
    tabs.reset()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId])

  if (!projectId) return <div style={{ padding: 48 }}><Empty description={t('common.selectProject', '请先在顶部选择项目')} /></div>

  const detailTabs = items
    .filter((r) => tabs.openIds.includes(r.id))
    .map((r) => ({
      key: r.id,
      label: r.label,
      children: <RequirementDetail key={r.id} reqId={r.id} projectId={projectId} onChanged={loadList} />,
    }))

  return (
    <>
      <Workspace
        listLabel={t('req.allRequirements', '全部需求')}
        activeKey={tabs.activeKey}
        onChange={tabs.setActiveKey}
        onClose={tabs.close}
        tabs={detailTabs}
        listContent={
          <WorkList<RegItem>
            onNew={() => setCreateOpen(true)}
            newLabel={t('req.new', '新建需求')}
            data={items}
            onRowClick={(r) => tabs.open(r.id)}
            emptyText={t('req.empty', '暂无需求')}
            columns={[
              { title: t('req.title', '标题'), dataIndex: 'label' },
              { title: t('req.decomposed', '已拆分'), dataIndex: 'meta', width: 100, render: (m?: Record<string, string>) => (m?.decompositionId ? <Tag color="geekblue">{t('req.yes', '是')}</Tag> : '—') },
            ]}
          />
        }
      />
      <Modal title={t('req.new', '新建需求')} open={createOpen} onCancel={() => setCreateOpen(false)} footer={null} destroyOnHidden>
        <Form
          layout="vertical"
          onFinish={async (v: { title: string; criteria: string }) => {
            try {
              const r = await api.createRequirement({ projectId, title: v.title, acceptanceCriteria: toLines(v.criteria || '') })
              message.success(t('req.created', '需求已创建'))
              regAdd('requirement', projectId, { id: r.id, label: v.title, createdAt: Date.now() })
              loadList()
              setCreateOpen(false)
              tabs.open(r.id)
            } catch (e) {
              message.error(e instanceof ApiError ? e.message : t('req.createFailed', '创建失败'))
            }
          }}
        >
          <Form.Item name="title" label={t('req.title', '标题')} rules={[{ required: true }]}><Input placeholder={t('req.titlePlaceholder', '如:用户登录')} autoFocus /></Form.Item>
          <Form.Item name="criteria" label={t('req.criteria', '验收标准(每行一条)')}><Input.TextArea rows={4} placeholder={t('req.criteriaPlaceholder', '登录成功\n错误密码拒绝')} /></Form.Item>
          <Button type="primary" htmlType="submit" block>{t('a.create', '创建')}</Button>
        </Form>
      </Modal>
    </>
  )
}

function RequirementDetail({ reqId, projectId, onChanged }: { reqId: string; projectId: string; onChanged: () => void }) {
  const { t } = useI18n()
  const [req, setReq] = useState<Requirement | null>(null)
  const [verOpen, setVerOpen] = useState(false)
  const reg = regList('requirement', projectId).find((r) => r.id === reqId)
  const [decompId, setDecompId] = useState<string | undefined>(reg?.meta?.decompositionId)
  const [verId, setVerId] = useState<string | undefined>(reg?.meta?.verificationId)

  const load = async () => {
    try {
      setReq(await api.getRequirement(reqId))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('req.loadFailed', '加载需求失败'))
    }
  }
  useEffect(() => {
    load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reqId])

  const setBaseline = () => {
    let v = String(req?.baselineVersion ?? 1)
    modal.confirm({
      title: t('req.setBaselineTo', '定基线到版本'),
      content: <Input defaultValue={v} onChange={(e) => (v = e.target.value)} style={{ marginTop: 8 }} />,
      onOk: async () => {
        try {
          await api.setBaseline(reqId, Number(v))
          message.success(t('req.baselineUpdated', '基线已更新'))
          load()
        } catch (e) {
          message.error(e instanceof ApiError ? e.message : t('req.setBaselineFailed', '定基线失败'))
        }
      },
    })
  }

  const doBreakdown = async () => {
    try {
      const r = await api.breakdown(reqId)
      message.success(`${t('req.decomposedTo', '已拆分')}:${r.tasks.length} ${t('req.tasksUnit', '个任务')}`)
      regAdd('requirement', projectId, { id: reqId, label: req?.title || reqId, createdAt: reg?.createdAt || Date.now(), meta: { decompositionId: r.id, verificationId: r.verificationId } })
      setDecompId(r.id)
      setVerId(r.verificationId)
      onChanged()
    } catch (e) {
      message.error(e instanceof ApiError ? `${t('req.decomposeFailed', '拆分失败')}:${e.status}` : t('req.decomposeFailed', '拆分失败'))
    }
  }

  // 验收标准:后端把标准放在 versions[].acceptanceCriteria,优先取基线版本,回落最新版本/顶层字段。
  const baselineCriteria =
    req?.versions?.find((v) => v.version === req.baselineVersion)?.acceptanceCriteria ??
    req?.versions?.[req.versions.length - 1]?.acceptanceCriteria ??
    req?.acceptanceCriteria ??
    []

  return (
    <div style={{ padding: '12px 16px', height: '100%', overflow: 'auto' }}>
      <Tabs
        items={[
          {
            key: 'info',
            label: t('req.infoTab', '需求信息'),
            children: (
              <>
                <Space style={{ marginBottom: 12 }} wrap>
                  <Button icon={<BranchesOutlined />} size="small" onClick={() => setVerOpen(true)}>{t('req.addVersion', '新增版本')}</Button>
                  <Button icon={<FlagOutlined />} size="small" onClick={setBaseline}>{t('req.setBaseline', '定基线')}</Button>
                  <Button type="primary" icon={<PartitionOutlined />} size="small" onClick={doBreakdown}>{t('req.autoDecompose', '自动拆分')}</Button>
                </Space>
                <Descriptions column={1} size="small" bordered>
                  <Descriptions.Item label={t('req.title', '标题')}>{req?.title}</Descriptions.Item>
                  <Descriptions.Item label={t('req.baselineVersion', '基线版本')}>v{req?.baselineVersion}</Descriptions.Item>
                  <Descriptions.Item label={t('req.status', '状态')}>{req?.status}</Descriptions.Item>
                  <Descriptions.Item label={t('req.acceptanceCriteria', '验收标准')}>
                    {baselineCriteria.length ? (
                      <ul style={{ margin: 0, paddingLeft: 18 }}>{baselineCriteria.map((c, i) => <li key={i}>{c}</li>)}</ul>
                    ) : '—'}
                  </Descriptions.Item>
                </Descriptions>
              </>
            ),
          },
          {
            key: 'orch',
            label: t('req.orchTab', '拆分 / 交付 / 验证'),
            children: decompId ? (
              <DecompositionView decompId={decompId} verificationId={verId} projectId={projectId} />
            ) : (
              <Empty description={t('req.notDecomposedHint', '尚未拆分,去「需求信息」点「自动拆分」生成任务图')} />
            ),
          },
        ]}
      />
      <Modal title={t('req.addVersionTitle', '新增需求版本')} open={verOpen} onCancel={() => setVerOpen(false)} footer={null} destroyOnHidden>
        <Form
          layout="vertical"
          onFinish={async (v: { description: string; criteria: string }) => {
            try {
              const r = await api.addRequirementVersion(reqId, { description: v.description, acceptanceCriteria: toLines(v.criteria || '') })
              message.success(`${t('req.versionCreated', '已创建版本')} v${r.version}`)
              setVerOpen(false)
              load()
            } catch (e) {
              message.error(e instanceof ApiError ? e.message : t('req.createVersionFailed', '创建版本失败'))
            }
          }}
        >
          <Form.Item name="description" label={t('req.versionDesc', '版本说明')} rules={[{ required: true }]}><Input placeholder={t('req.versionDescPlaceholder', '如:支持飞书登录')} autoFocus /></Form.Item>
          <Form.Item name="criteria" label={t('req.criteria', '验收标准(每行一条)')}><Input.TextArea rows={4} /></Form.Item>
          <Button type="primary" htmlType="submit" block>{t('req.createVersion', '创建版本')}</Button>
        </Form>
      </Modal>
    </div>
  )
}

function DecompositionView({ decompId, verificationId, projectId }: { decompId: string; verificationId?: string; projectId: string }) {
  const { t } = useI18n()
  const [tasks, setTasks] = useState<Task[]>([])
  const [running, setRunning] = useState(false)
  const [summary, setSummary] = useState<{ total: number; verified: number; failed: number; blocked: number; rounds: number } | null>(null)
  const [report, setReport] = useState<VerificationReport | null>(null)
  const [eventsFor, setEventsFor] = useState<Task | null>(null)
  const [casesFor, setCasesFor] = useState<Task | null>(null)

  const load = async () => {
    try {
      const d = await api.decomposition(decompId)
      setTasks(d.tasks || [])
      if (verificationId) api.verificationReport(verificationId).then(setReport).catch(() => undefined)
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('req.loadGraphFailed', '加载拆分图失败'))
    }
  }
  useEffect(() => {
    load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [decompId])

  const run = async () => {
    setRunning(true)
    try {
      const s = await api.runDecomposition(decompId)
      setSummary(s)
      message.success(`${t('req.runDone', '运行完成')}:${t('req.verifiedLabel', '验证')} ${s.verified}/${s.total}`)
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? `${t('req.runFailed', '运行失败')}:${e.status}` : t('req.runFailed', '运行失败'))
    } finally {
      setRunning(false)
    }
  }
  const dispatch = async (task: Task) => {
    try {
      await api.createDelivery({ decompositionId: decompId, taskId: task.id, title: task.title, executor: 'CLAUDE_CODE' })
      message.success(`${t('req.dispatched', '已派发')} ${task.id}`)
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? `${t('req.dispatchFailed', '派发失败')}:${e.status}` : t('req.dispatchFailed', '派发失败'))
    }
  }

  return (
    <Space direction="vertical" style={{ width: '100%' }} size={12}>
      <Space>
        <Button type="primary" icon={<PlayCircleOutlined />} size="small" loading={running} onClick={run}>{t('req.runParallel', '并行运行')}</Button>
      </Space>
      {summary && (
        <Row gutter={12}>
          <Col span={5}><Card size="small"><Statistic title={t('req.totalTasks', '总任务')} value={summary.total} /></Card></Col>
          <Col span={5}><Card size="small"><Statistic title={t('req.verified', '已验证')} value={summary.verified} valueStyle={{ color: '#2e7d32' }} /></Card></Col>
          <Col span={5}><Card size="small"><Statistic title={t('req.failed', '失败')} value={summary.failed} /></Card></Col>
          <Col span={5}><Card size="small"><Statistic title={t('req.blocked', '阻塞')} value={summary.blocked} /></Card></Col>
          <Col span={4}><Card size="small"><Statistic title={t('req.rounds', '轮次')} value={summary.rounds} /></Card></Col>
        </Row>
      )}
      <Table<Task>
        rowKey="id"
        size="small"
        dataSource={tasks}
        pagination={false}
        locale={{ emptyText: <Empty description={t('req.noTasks', '暂无任务')} /> }}
        columns={[
          { title: t('req.task', '任务'), dataIndex: 'title', ellipsis: true },
          { title: 'ID', dataIndex: 'id', width: 70, render: (v: string) => <span className="ms-mono">{v}</span> },
          { title: t('req.status', '状态'), dataIndex: 'status', width: 100, render: (s: string) => <Tag color={taskColor(s)}>{s}</Tag> },
          { title: t('req.dependencies', '依赖'), dataIndex: 'dependencies', render: (d?: string[]) => (d?.length ? d.join(', ') : '—') },
          {
            title: t('req.action', '操作'),
            width: 150,
            render: (_, row) => (
              <Space>
                <Button type="link" size="small" icon={<SendOutlined />} onClick={() => dispatch(row)}>{t('req.dispatch', '派发')}</Button>
                <Button type="link" size="small" onClick={() => setCasesFor(row)}>{t('req.cases', '用例')}</Button>
                <Button type="link" size="small" onClick={() => setEventsFor(row)}>{t('req.events', '事件')}</Button>
              </Space>
            ),
          },
        ]}
      />
      {verificationId && (
        <Card size="small" title={t('req.verifyReport', '验证报告(覆盖链)')}>
          {report ? (
            <Space size={32} align="center">
              <Statistic title={t('req.satisfiedCriteria', '已满足标准')} value={report.satisfied ?? 0} />
              <Space direction="vertical" size={2}>
                <Typography.Text type="secondary" style={{ fontSize: 14 }}>{t('req.completeness', '完整性')}</Typography.Text>
                <Tag color={report.complete ? 'green' : 'orange'}>{report.complete ? t('req.complete', '已完整') : t('req.hasGaps', '有缺口')}</Tag>
              </Space>
            </Space>
          ) : <Typography.Text type="secondary">{t('req.noReport', '暂无报告')}</Typography.Text>}
        </Card>
      )}
      <EventsDrawer decompId={decompId} task={eventsFor} onClose={() => setEventsFor(null)} />
      <TaskCasesDrawer decompId={decompId} projectId={projectId} task={casesFor} onClose={() => setCasesFor(null)} />
    </Space>
  )
}

// 任务关联用例 + 用例所属计划:打通 任务→用例→计划,均可点进对应页。
function TaskCasesDrawer({ decompId, projectId, task, onClose }: { decompId: string; projectId: string; task: Task | null; onClose: () => void }) {
  const { t } = useI18n()
  const nav = useNavigate()
  const [linked, setLinked] = useState<ApiCase[]>([])
  const [plansOf, setPlansOf] = useState<Record<string, { planId: string; name: string }[]>>({})
  const [projCases, setProjCases] = useState<ApiCase[]>([])
  const [pick, setPick] = useState('')

  const load = async () => {
    if (!task) return
    const cs = await api.taskCases(decompId, task.id).catch(() => [])
    setLinked(cs)
    const map: Record<string, { planId: string; name: string }[]> = {}
    await Promise.all(cs.map((c) => api.plansByCase(c.id).then((ps) => { map[c.id] = ps }).catch(() => undefined)))
    setPlansOf(map)
  }
  useEffect(() => {
    if (task) {
      load()
      api.projectCases(projectId).then((p) => setProjCases(p.items)).catch(() => undefined)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [task])

  const linkCase = async () => {
    if (!task || !pick) return
    await api.linkTaskCase(decompId, task.id, pick).catch((e) => message.error(e instanceof ApiError ? e.message : t('req.linkFailed', '关联失败')))
    setPick('')
    load()
  }
  const unlink = async (caseId: string) => {
    if (!task) return
    await api.unlinkTaskCase(decompId, task.id, caseId)
    load()
  }

  return (
    <Drawer title={task ? `${t('req.taskCases', '任务用例')} · ${task.title}` : ''} open={!!task} onClose={onClose} width={560}>
      <Space.Compact style={{ width: '100%', marginBottom: 12 }}>
        <Select
          style={{ flex: 1 }}
          showSearch
          placeholder={t('req.linkCasePlaceholder', '选择项目接口用例关联到本任务')}
          value={pick || undefined}
          onChange={setPick}
          optionFilterProp="label"
          options={projCases.map((c) => ({ value: c.id, label: `${c.method} ${c.name}` }))}
        />
        <Button type="primary" onClick={linkCase} disabled={!pick}>{t('req.link', '关联')}</Button>
      </Space.Compact>
      <Table<ApiCase>
        rowKey="id"
        size="small"
        dataSource={linked}
        pagination={false}
        locale={{ emptyText: <Empty description={t('req.noLinkedCases', '未关联用例')} /> }}
        columns={[
          {
            title: t('req.case', '用例'),
            render: (_, c) => (
              <a onClick={() => c.apiDefinitionId && nav(`/api/definition?open=${c.apiDefinitionId}`)}>{c.method} {c.name}</a>
            ),
          },
          {
            title: t('req.belongPlan', '所属计划'),
            render: (_, c) =>
              (plansOf[c.id] || []).length
                ? (plansOf[c.id] || []).map((p) => (
                    <Tag key={p.planId} color="geekblue" style={{ cursor: 'pointer' }} onClick={() => nav(`/test-plan?open=${p.planId}`)}>{p.name}</Tag>
                  ))
                : <span style={{ color: '#bbb' }}>—</span>,
          },
          { title: '', width: 50, render: (_, c) => <Button type="link" size="small" danger onClick={() => unlink(c.id)}>{t('req.remove', '移除')}</Button> },
        ]}
      />
    </Drawer>
  )
}

function EventsDrawer({ decompId, task, onClose }: { decompId: string; task: Task | null; onClose: () => void }) {
  const { t } = useI18n()
  const [events, setEvents] = useState<DeliveryEvent[]>([])
  useEffect(() => {
    if (!task) return
    setEvents([])
    api
      .deliveries(decompId, task.id)
      .then(async (atts) => {
        const all: DeliveryEvent[] = []
        for (const a of atts) {
          const id = a.id || a.attemptId
          if (id) all.push(...(await api.deliveryEvents(id).catch(() => [])))
        }
        setEvents(all)
      })
      .catch(() => undefined)
  }, [task, decompId])
  return (
    <Drawer title={task ? `${t('req.deliveryEvents', '交付事件')} · ${task.title}` : ''} open={!!task} onClose={onClose} width={520}>
      <Table<DeliveryEvent>
        rowKey={(_, i) => String(i)}
        size="small"
        dataSource={events}
        pagination={false}
        locale={{ emptyText: <Empty description={t('req.noEvents', '暂无事件(先派发任务)')} /> }}
        columns={[
          { title: '#', dataIndex: 'seq', width: 50, render: (v?: number) => v ?? '—' },
          { title: t('req.eventType', '类型'), dataIndex: 'kind', width: 100, render: (k: string) => <Tag>{k}</Tag> },
          { title: t('req.eventMessage', '消息'), dataIndex: 'message', render: (m?: string) => m || '—' },
        ]}
      />
    </Drawer>
  )
}
