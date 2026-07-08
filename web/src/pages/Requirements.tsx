import { useCallback, useEffect, useState } from 'react'
import { Badge, Button, Card, Col, Descriptions, Drawer, Dropdown, Empty, Form, Input, InputNumber, Modal, Progress, Row, Segmented, Select, Space, Spin, Statistic, Table, Tabs, Tag, Timeline, Tooltip, Typography } from 'antd'
import { message, modal } from '../feedback'
import { useNavigate } from 'react-router-dom'
import { BranchesOutlined, DeleteOutlined, EditOutlined, FlagOutlined, InboxOutlined, PartitionOutlined, PlayCircleOutlined, ProfileOutlined, ReloadOutlined, SendOutlined } from '@ant-design/icons'
import {
  api,
  ApiError,
  EXECUTOR_LABEL,
  type ApiCase,
  type CoverageCase,
  type DeliveryAttempt,
  type DeliveryEvent,
  type FunctionalCase,
  type Requirement,
  type RequirementVersion,
  type Task,
  type VerificationReport,
} from '../api'
import { useApp } from '../context'
import { Workspace, WorkList, useWorkTabs } from '../components/Workspace'
import { SelectProjectEmpty } from '../components/Page'
import { regAdd, regList, type RegItem } from '../registry'
import { useI18n } from '../i18n'

const toLines = (s: string) => s.split('\n').map((x) => x.trim()).filter(Boolean)
// 取需求的「当前」验收标准(优先基线版本,回落最新版本/顶层)。
const critsOf = (r: Requirement): string[] =>
  r.versions?.find((v) => v.version === r.baselineVersion)?.acceptanceCriteria ?? r.versions?.[r.versions.length - 1]?.acceptanceCriteria ?? r.acceptanceCriteria ?? []
const taskColor = (s: string) => (s === 'VERIFIED' ? 'green' : s === 'FAILED' ? 'red' : s === 'PENDING' ? 'default' : 'blue')
// 协同看板列:按任务状态归桶(进行中合并 派发/运行)。
const BOARD_COLS: { key: string; tkey: string; label: string; statuses: string[] }[] = [
  { key: 'PENDING', tkey: 'req.col.pending', label: '待派发', statuses: ['PENDING'] },
  { key: 'PROGRESS', tkey: 'req.col.progress', label: '进行中', statuses: ['DISPATCHED', 'RUNNING'] },
  { key: 'DELIVERED', tkey: 'req.col.delivered', label: '已交付', statuses: ['DELIVERED'] },
  { key: 'VERIFIED', tkey: 'req.col.verified', label: '已验证', statuses: ['VERIFIED'] },
  { key: 'FAILED', tkey: 'req.col.failed', label: '失败', statuses: ['FAILED'] },
]
// 需求状态色:DRAFT 灰 / BASELINED 蓝 / DELIVERED 绿 / ARCHIVED 灰。
const reqStatusColor = (s?: string) =>
  s === 'DELIVERED' ? 'green' : s === 'BASELINED' ? 'blue' : s === 'ARCHIVED' ? 'default' : 'default'

// 列表行 = 本地注册表项 + 后端需求状态。
type ReqRow = Omit<RegItem, 'label'> & { status?: string; label: React.ReactNode }

// 需求与编排合一:需求列表 → 详情 Tab(需求信息/版本/基线/拆分 → 拆分图任务+运行+交付+验证)。
export default function Requirements() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const [items, setItems] = useState<ReqRow[]>([])
  const [createOpen, setCreateOpen] = useState(false)
  const tabs = useWorkTabs()

  // 列表以后端为准(含 CLI/API 建的需求),叠加本地注册表的 meta(拆分/验证链接)。
  // 携带后端 status,让列表直观看到 DRAFT/BASELINED/DELIVERED。
  const loadList = async () => {
    const local = regList('requirement', projectId)
    const localById = new Map(local.map((r) => [r.id, r]))
    try {
      const page = await api.requirements(projectId)
      // 并行取各需求覆盖,行标签带覆盖率徽标(列表通常较小)。
      const covs = await Promise.all(page.items.map((r) => api.requirementCoverage(r.id).then((c) => [r.id, c] as const).catch(() => [r.id, []] as const)))
      const covMap: Record<string, CoverageCase[]> = Object.fromEntries(covs)
      setItems(page.items.map((r) => {
        const crits = critsOf(r)
        const covered = crits.filter((_, i) => (covMap[r.id] || []).some((c) => c.criterionIndex === i)).length
        const pct = crits.length ? Math.round((covered / crits.length) * 100) : 0
        const base = localById.get(r.id) || { id: r.id, label: r.title, createdAt: 0 }
        const label = (
          <span>{r.title}{crits.length ? <Tag color={pct === 100 ? 'green' : pct > 0 ? 'gold' : 'default'} style={{ marginLeft: 6 }}>{pct}%</Tag> : null}</span>
        )
        return { ...base, status: r.status, label }
      }))
    } catch {
      setItems(local) // 后端不可用时回落本地
    }
  }
  useEffect(() => {
    loadList()
    tabs.reset()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId])

  if (!projectId) return <SelectProjectEmpty />

  const detailTabs = items
    .filter((r) => tabs.openIds.includes(r.id))
    .map((r) => ({
      key: r.id,
      label: r.label,
      children: <RequirementDetail key={r.id} reqId={r.id} projectId={projectId} onChanged={loadList} onDeleted={() => { tabs.close(r.id); loadList() }} />,
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
          <WorkList<ReqRow>
            onNew={() => setCreateOpen(true)}
            newLabel={t('req.new', '新建需求')}
            data={items}
            onRowClick={(r) => tabs.open(r.id)}
            emptyText={t('req.empty', '暂无需求')}
            columns={[
              { title: t('req.title', '标题'), dataIndex: 'label' },
              { title: t('req.status', '状态'), dataIndex: 'status', width: 120, render: (s?: string) => <Tag color={reqStatusColor(s)}>{s ? t(`req.status.${s}`, s) : '—'}</Tag> },
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

// 需求的功能用例覆盖:逐条验收标准 → 关联的功能用例(可增删)+ 覆盖率。打通「需求→标准→功能用例」手工覆盖链。
function RequirementCoveragePanel({ reqId, projectId, criteria }: { reqId: string; projectId: string; criteria: string[] }) {
  const { t } = useI18n()
  const [cov, setCov] = useState<CoverageCase[]>([])
  const [cases, setCases] = useState<FunctionalCase[]>([])
  const load = () => api.requirementCoverage(reqId).then(setCov).catch(() => setCov([]))
  useEffect(() => {
    load()
    api.functionalCases(projectId).then(setCases).catch(() => undefined)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reqId, projectId])
  const byIdx = (i: number) => cov.filter((c) => c.criterionIndex === i)
  const link = (idx: number, caseId: string) =>
    api.linkRequirementCase({ requirementId: reqId, criterionIndex: idx, functionalCaseId: caseId, projectId }).then(load).then(() => message.success(t('req.linked', '已关联'))).catch((e) => message.error(e instanceof ApiError ? e.message : t('req.linkFailed', '关联失败')))
  const unlink = (idx: number, caseId: string) =>
    api.unlinkRequirementCase({ requirementId: reqId, criterionIndex: idx, functionalCaseId: caseId }).then(load).catch(() => undefined)

  if (!criteria.length) return <Empty description={t('req.noCriteriaHint', '该需求没有验收标准(去「需求信息」新增版本/定基线添加)')} style={{ marginTop: 40 }} />
  const covered = criteria.filter((_, i) => byIdx(i).length > 0).length
  const pct = Math.round((covered / criteria.length) * 100)
  return (
    <div>
      <Space style={{ marginBottom: 14 }} align="center">
        <span style={{ fontWeight: 600 }}>{t('req.coverageRate', '覆盖率')}: {covered}/{criteria.length}</span>
        <Progress percent={pct} size="small" style={{ width: 220 }} status={pct === 100 ? 'success' : 'active'} />
      </Space>
      {criteria.map((text, i) => {
        const linked = byIdx(i)
        return (
          <Card
            key={i}
            size="small"
            style={{ marginBottom: 10 }}
            title={
              <Space>
                <span>{t('req.criterion', '标准')} {i + 1}: {text}</span>
                {linked.length ? <Tag color="green">{t('req.covered', '已覆盖')} {linked.length}</Tag> : <Tag color="orange">{t('req.uncovered', '未覆盖')}</Tag>}
              </Space>
            }
          >
            <Space wrap>
              {linked.map((c) => (
                <Tag key={c.caseId} color="blue" closable onClose={() => unlink(i, c.caseId)}>{c.caseName}</Tag>
              ))}
              <Select
                showSearch
                size="small"
                style={{ width: 260 }}
                value={null}
                placeholder={t('req.linkCase', '+ 关联功能用例')}
                optionFilterProp="label"
                onChange={(cid: string) => link(i, cid)}
                options={cases.filter((fc) => !linked.some((l) => l.caseId === fc.id)).map((fc) => ({ value: fc.id, label: `${fc.name}${fc.module ? ` · ${fc.module}` : ''}` }))}
                notFoundContent={t('func.empty', '项目暂无功能用例')}
              />
            </Space>
          </Card>
        )
      })}
    </div>
  )
}

function RequirementDetail({ reqId, projectId, onChanged, onDeleted }: { reqId: string; projectId: string; onChanged: () => void; onDeleted: () => void }) {
  const { t } = useI18n()
  const [req, setReq] = useState<Requirement | null>(null)
  const [cov, setCov] = useState<CoverageCase[]>([])
  const [verOpen, setVerOpen] = useState(false)
  const [verView, setVerView] = useState<RequirementVersion | null>(null) // 查看的历史版本明细
  const reg = regList('requirement', projectId).find((r) => r.id === reqId)
  const [decompId, setDecompId] = useState<string | undefined>(reg?.meta?.decompositionId)
  const [verId, setVerId] = useState<string | undefined>(reg?.meta?.verificationId)

  const load = async () => {
    try {
      setReq(await api.getRequirement(reqId))
      api.requirementCoverage(reqId).then(setCov).catch(() => setCov([]))
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

  // 生命周期操作:重命名 / 交付 / 归档 / 删除(后端 PUT/POST/DELETE,带状态守卫,失败回显)。
  const rename = () => {
    let title = req?.title || ''
    modal.confirm({
      title: t('req.renameTitle', '重命名需求'),
      content: <Input defaultValue={title} onChange={(e) => (title = e.target.value)} style={{ marginTop: 8 }} />,
      onOk: async () => {
        if (!title.trim()) return
        try {
          await api.renameRequirement(reqId, title.trim())
          message.success(t('req.renamed', '已重命名'))
          load(); onChanged()
        } catch (e) {
          message.error(e instanceof ApiError ? e.message : t('req.renameFailed', '重命名失败'))
        }
      },
    })
  }
  const deliver = () => modal.confirm({
    title: t('req.deliverConfirm', '确认交付该需求?'),
    content: t('req.deliverHint', '需先定基线(BASELINED)才能交付。'),
    onOk: async () => {
      try {
        await api.deliverRequirement(reqId)
        message.success(t('req.delivered', '已交付'))
        load(); onChanged()
      } catch (e) {
        message.error(e instanceof ApiError ? e.message : t('req.deliverFailed', '交付失败'))
      }
    },
  })
  const archive = () => modal.confirm({
    title: t('req.archiveConfirm', '确认归档该需求?'),
    content: t('req.archiveHint', '归档后将冻结,无法再新增版本。'),
    okButtonProps: { danger: true },
    onOk: async () => {
      try {
        await api.archiveRequirement(reqId)
        message.success(t('req.archived', '已归档'))
        load(); onChanged()
      } catch (e) {
        message.error(e instanceof ApiError ? e.message : t('req.archiveFailed', '归档失败'))
      }
    },
  })
  const del = () => modal.confirm({
    title: t('req.deleteConfirm', '确认删除该需求?'),
    content: t('req.deleteHint', '删除后从列表移除(标题可再次使用)。'),
    okButtonProps: { danger: true },
    onOk: async () => {
      try {
        await api.deleteRequirement(reqId)
        message.success(t('req.deleted', '已删除'))
        onDeleted()
      } catch (e) {
        message.error(e instanceof ApiError ? e.message : t('req.deleteFailed', '删除失败'))
      }
    },
  })
  const viewVersion = async (n: number) => {
    try {
      setVerView(await api.getRequirementVersion(reqId, n))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('req.loadVersionFailed', '加载版本失败'))
    }
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
                  <Button icon={<EditOutlined />} size="small" onClick={rename}>{t('req.rename', '重命名')}</Button>
                  <Button icon={<BranchesOutlined />} size="small" disabled={req?.status === 'ARCHIVED'} onClick={() => setVerOpen(true)}>{t('req.addVersion', '新增版本')}</Button>
                  <Button icon={<FlagOutlined />} size="small" disabled={req?.status === 'ARCHIVED'} onClick={setBaseline}>{t('req.setBaseline', '定基线')}</Button>
                  <Button type="primary" icon={<PartitionOutlined />} size="small" onClick={doBreakdown}>{t('req.autoDecompose', '自动拆分')}</Button>
                  <Button icon={<SendOutlined />} size="small" disabled={!(req?.status === 'BASELINED' || req?.status === 'DELIVERED')} onClick={deliver}>{t('req.deliver', '交付')}</Button>
                  <Button icon={<InboxOutlined />} size="small" disabled={req?.status === 'ARCHIVED'} onClick={archive}>{t('req.archive', '归档')}</Button>
                  <Button danger icon={<DeleteOutlined />} size="small" onClick={del}>{t('a.delete', '删除')}</Button>
                  {(() => {
                    const n = baselineCriteria.filter((_, i) => cov.some((c) => c.criterionIndex === i)).length
                    const pct = baselineCriteria.length ? Math.round((n / baselineCriteria.length) * 100) : 0
                    return <Tag color={pct === 100 ? 'green' : pct > 0 ? 'gold' : 'default'}>{t('req.coverageRate', '覆盖率')} {n}/{baselineCriteria.length} ({pct}%)</Tag>
                  })()}
                </Space>
                <Descriptions column={1} size="small" bordered>
                  <Descriptions.Item label={t('req.title', '标题')}>{req?.title}</Descriptions.Item>
                  <Descriptions.Item label={t('req.baselineVersion', '基线版本')}>v{req?.baselineVersion}</Descriptions.Item>
                  <Descriptions.Item label={t('req.status', '状态')}>{req?.status ? <Tag color={reqStatusColor(req.status)}>{t(`req.status.${req.status}`, req.status)}</Tag> : '—'}</Descriptions.Item>
                  <Descriptions.Item label={t('req.acceptanceCriteria', '验收标准')}>
                    {baselineCriteria.length ? (
                      <ul style={{ margin: 0, paddingLeft: 18 }}>{baselineCriteria.map((c, i) => <li key={i}>{c}</li>)}</ul>
                    ) : '—'}
                  </Descriptions.Item>
                </Descriptions>
                {/* 版本历史:点「查看」走 GET /requirement/:id/version/:n 取该版本明细。 */}
                {!!req?.versions?.length && (
                  <Table<RequirementVersion>
                    style={{ marginTop: 12 }}
                    rowKey="version"
                    size="small"
                    pagination={false}
                    dataSource={req.versions}
                    columns={[
                      { title: t('req.version', '版本'), dataIndex: 'version', width: 80, render: (v: number) => <span>v{v}{v === req.baselineVersion ? <Tag color="blue" style={{ marginLeft: 6 }}>{t('req.baseline', '基线')}</Tag> : null}</span> },
                      { title: t('req.versionDesc', '版本说明'), dataIndex: 'description', render: (d?: string) => d || '—' },
                      { title: t('req.action', '操作'), width: 80, render: (_v, row) => <Button type="link" size="small" onClick={() => viewVersion(row.version)}>{t('req.view', '查看')}</Button> },
                    ]}
                  />
                )}
              </>
            ),
          },
          {
            key: 'coverage',
            label: t('req.coverageTab', '功能用例覆盖'),
            children: <RequirementCoveragePanel reqId={reqId} projectId={projectId} criteria={baselineCriteria} />,
          },
          {
            key: 'orch',
            label: t('req.orchTab', '拆分 / 交付 / 验证'),
            children: decompId ? (
              <DecompositionView decompId={decompId} verificationId={verId} projectId={projectId} reqId={reqId} />
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
      <Modal title={verView ? `${t('req.versionDetail', '版本明细')} · v${verView.version}` : ''} open={!!verView} onCancel={() => setVerView(null)} footer={null} destroyOnHidden>
        {verView && (
          <Descriptions column={1} size="small" bordered>
            <Descriptions.Item label={t('req.versionDesc', '版本说明')}>{verView.description || '—'}</Descriptions.Item>
            <Descriptions.Item label={t('req.acceptanceCriteria', '验收标准')}>
              {verView.acceptanceCriteria?.length ? (
                <ul style={{ margin: 0, paddingLeft: 18 }}>{verView.acceptanceCriteria.map((c, i) => <li key={i}>{c}</li>)}</ul>
              ) : '—'}
            </Descriptions.Item>
          </Descriptions>
        )}
      </Modal>
    </div>
  )
}

function DecompositionView({ decompId, verificationId, projectId, reqId }: { decompId: string; verificationId?: string; projectId: string; reqId?: string }) {
  const { t } = useI18n()
  const [tasks, setTasks] = useState<Task[]>([])
  const [cov, setCov] = useState<CoverageCase[]>([]) // 手工功能用例覆盖(与任务覆盖并看)
  useEffect(() => { if (reqId) api.requirementCoverage(reqId).then(setCov).catch(() => setCov([])) }, [reqId])
  const [running, setRunning] = useState(false)
  const [dispatching, setDispatching] = useState<Set<string>>(new Set()) // 派发中的任务 id(防重复点击)
  const [summary, setSummary] = useState<{ total: number; verified: number; failed: number; blocked: number; rounds: number } | null>(null)
  const [report, setReport] = useState<VerificationReport | null>(null)
  const [eventsFor, setEventsFor] = useState<Task | null>(null)
  const [casesFor, setCasesFor] = useState<Task | null>(null)
  const [view, setView] = useState<'table' | 'board'>('table') // 表格 / 协同看板
  // 负责人候选:人(项目用户)+ AI 执行机(runner-agent)。
  const [assignees, setAssignees] = useState<{ value: string; label: string; kind: string; id: string }[]>([])
  const nameOfAssignee = (a?: string, kind?: string) => assignees.find((o) => o.kind === kind && o.id === a)?.label || a || ''

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
    // 负责人候选:人 + AI agent(失败静默,看板/列仍可用)。
    Promise.all([api.users().then((p) => p.items).catch(() => []), api.runnerAgents().catch(() => [])]).then(([us, ag]) => {
      setAssignees([
        ...us.map((u) => ({ value: `HUMAN:${u.id}`, label: `👤 ${u.name || u.email}`, kind: 'HUMAN', id: u.id })),
        ...ag.map((a) => ({ value: `AGENT:${a.id}`, label: `🤖 ${a.name}`, kind: 'AGENT', id: a.id })),
      ])
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [decompId])

  // 指派负责人(乐观更新本地后落库)。value 形如 "HUMAN:<id>" / "AGENT:<id>" / ""(取消)。
  const assign = async (task: Task, value: string | undefined) => {
    const [kind, id] = value ? value.split(':') : ['', '']
    setTasks((ts) => ts.map((x) => (x.id === task.id ? { ...x, assignee: id, assigneeKind: kind } : x)))
    try {
      await api.setTaskAssignee(decompId, task.id, id, kind)
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('req.assignFailed', '指派失败'))
      load()
    }
  }

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
  // 依赖门控:任务的依赖须全部 Verified 才能派发(否则提前派发会撞门、卡 PENDING)。
  const statusOf = (id: string) => tasks.find((x) => x.id === id)?.status
  const depsReady = (t: Task) => (t.dependencies ?? []).every((d) => statusOf(d) === 'VERIFIED')
  const dispatch = async (task: Task, executor: string) => {
    if (dispatching.has(task.id)) return // 已在派发中,忽略重复点击
    if (!depsReady(task)) {
      message.warning(t('req.depsNotReady', '依赖任务未全部验证完成,暂不能派发'))
      return
    }
    setDispatching((s) => new Set(s).add(task.id))
    try {
      await api.createDelivery({ decompositionId: decompId, taskId: task.id, title: task.title, executor })
      message.success(`${t('req.dispatched', '已派发')} ${task.id}`)
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? `${t('req.dispatchFailed', '派发失败')}:${e.status}` : t('req.dispatchFailed', '派发失败'))
    } finally {
      setDispatching((s) => { const n = new Set(s); n.delete(task.id); return n })
    }
  }
  // 派发按钮 = 执行者选择菜单:点开选 Claude Code / Codex / OpenCode / CodeBuddy。
  const executorMenu = (task: Task) => ({
    items: Object.entries(EXECUTOR_LABEL).map(([key, label]) => ({ key, label })),
    onClick: ({ key }: { key: string }) => dispatch(task, key),
  })
  // 工作量(task point)行内编辑:乐观更新本地后落库,失败回滚重载。
  const setPoints = async (task: Task, points: number) => {
    setTasks((ts) => ts.map((x) => (x.id === task.id ? { ...x, points } : x)))
    try {
      await api.setTaskPoints(decompId, task.id, points)
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('req.setPointsFailed', '工作量保存失败'))
      load()
    }
  }
  const totalPoints = tasks.reduce((n, x) => n + (x.points || 0), 0)

  return (
    <Space direction="vertical" style={{ width: '100%' }} size={12}>
      <Space>
        <Button type="primary" icon={<PlayCircleOutlined />} size="small" loading={running} onClick={run}>{t('req.runParallel', '并行运行')}</Button>
        <Typography.Text type="secondary" style={{ fontSize: 13 }}>{t('req.totalPointsLabel', '工作量合计')} <b style={{ color: 'var(--text)' }}>{totalPoints}</b> {t('req.pointsUnit', '点')}</Typography.Text>
        <div style={{ flex: 1 }} />
        <Segmented
          size="small"
          value={view}
          onChange={(v) => setView(v as 'table' | 'board')}
          options={[{ label: t('req.viewTable', '表格'), value: 'table' }, { label: t('req.viewBoard', '协同看板'), value: 'board' }]}
        />
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
      {view === 'table' ? (
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
            {
              title: t('req.points', '工作量'), dataIndex: 'points', width: 100,
              render: (p: number | undefined, row) => (
                <InputNumber size="small" min={0} max={999} value={p ?? 0} style={{ width: 68 }} onChange={(v) => setPoints(row, Number(v ?? 0))} />
              ),
            },
            {
              title: t('req.assignee', '负责人'), dataIndex: 'assignee', width: 180,
              render: (_v, row) => (
                <Select
                  size="small" allowClear showSearch optionFilterProp="label" style={{ width: 164 }}
                  placeholder={t('req.unassigned', '未分配')}
                  value={row.assignee ? `${row.assigneeKind}:${row.assignee}` : undefined}
                  options={assignees}
                  onChange={(v) => assign(row, v)}
                />
              ),
            },
            { title: t('req.dependencies', '依赖'), dataIndex: 'dependencies', render: (d?: string[]) => (d?.length ? d.join(', ') : '—') },
            {
              title: t('req.action', '操作'),
              width: 220,
              render: (_, row) => (
                <Space>
                  <Tooltip title={depsReady(row) ? '' : t('req.depsNotReady', '依赖任务未全部验证完成,暂不能派发')}>
                    <Dropdown trigger={['click']} disabled={dispatching.has(row.id) || !depsReady(row)} menu={executorMenu(row)}>
                      <Button type="link" size="small" icon={<SendOutlined />} loading={dispatching.has(row.id)} disabled={dispatching.has(row.id) || !depsReady(row)}>{t('req.dispatch', '派发')}</Button>
                    </Dropdown>
                  </Tooltip>
                  <Button type="link" size="small" onClick={() => setCasesFor(row)}>{t('req.cases', '用例')}</Button>
                  <Button type="link" size="small" icon={<ProfileOutlined />} onClick={() => setEventsFor(row)}>{t('req.execProgress', '执行进度')}</Button>
                </Space>
              ),
            },
          ]}
        />
      ) : (
        // 协同看板:按状态分列,卡片显示工作量 + 负责人(人/AI agent),可直接指派/派发。
        <Row gutter={8} wrap={false} style={{ overflowX: 'auto', paddingBottom: 8 }}>
          {BOARD_COLS.map((col) => {
            const colTasks = tasks.filter((tk) => col.statuses.includes(tk.status))
            return (
              <Col key={col.key} flex="0 0 224px">
                <div style={{ background: 'var(--bg)', borderRadius: 8, padding: 8, minHeight: 140 }}>
                  <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '2px 4px 8px', fontWeight: 600, fontSize: 13 }}>
                    <span>{t(col.tkey, col.label)}</span><Tag>{colTasks.length}</Tag>
                  </div>
                  <Space direction="vertical" size={8} style={{ width: '100%' }}>
                    {colTasks.map((tk) => (
                      <Card key={tk.id} size="small" styles={{ body: { padding: 10 } }}>
                        <div style={{ fontWeight: 500, marginBottom: 6 }}>{tk.title}</div>
                        <Space size={[4, 4]} wrap style={{ marginBottom: 8 }}>
                          <Tag className="ms-mono" style={{ margin: 0 }}>{tk.id}</Tag>
                          <Tag color="blue" style={{ margin: 0 }}>{tk.points ?? 0} {t('req.pointsUnit', '点')}</Tag>
                          {tk.assignee ? <Tag color={tk.assigneeKind === 'AGENT' ? 'purple' : 'green'} style={{ margin: 0 }}>{nameOfAssignee(tk.assignee, tk.assigneeKind)}</Tag> : null}
                        </Space>
                        <Space.Compact style={{ width: '100%' }}>
                          <Select
                            size="small" allowClear showSearch optionFilterProp="label" style={{ flex: 1 }}
                            placeholder={t('req.assign', '指派')}
                            value={tk.assignee ? `${tk.assigneeKind}:${tk.assignee}` : undefined}
                            options={assignees}
                            onChange={(v) => assign(tk, v)}
                          />
                          {tk.status === 'PENDING' && (
                            <Tooltip title={depsReady(tk) ? '' : t('req.depsNotReady', '依赖任务未全部验证完成,暂不能派发')}>
                              <Dropdown trigger={['click']} disabled={dispatching.has(tk.id) || !depsReady(tk)} menu={executorMenu(tk)}>
                                <Button size="small" icon={<SendOutlined />} loading={dispatching.has(tk.id)} disabled={dispatching.has(tk.id) || !depsReady(tk)} />
                              </Dropdown>
                            </Tooltip>
                          )}
                          <Button size="small" icon={<ProfileOutlined />} title={t('req.execProgress', '执行进度')} onClick={() => setEventsFor(tk)} />
                        </Space.Compact>
                      </Card>
                    ))}
                    {colTasks.length === 0 && <div style={{ color: 'var(--text-3)', fontSize: 12, textAlign: 'center', padding: '12px 0' }}>—</div>}
                  </Space>
                </div>
              </Col>
            )
          })}
        </Row>
      )}
      {verificationId && (
        <Card size="small" title={t('req.verifyReport', '验证报告(覆盖链)')}>
          {report ? (
            <Space direction="vertical" size={12} style={{ width: '100%' }}>
              <Space size={32} align="center">
                <Statistic title={t('req.satisfiedCriteria', '已满足标准')} value={`${report.satisfied ?? 0}${report.total != null ? ` / ${report.total}` : ''}`} />
                {/* 手工功能用例覆盖:与任务覆盖并看 — 分母用验证报告的标准总数。 */}
                <Statistic
                  title={t('req.manualCovered', '手工用例覆盖')}
                  value={`${new Set(cov.map((c) => c.criterionIndex)).size}${report.total != null ? ` / ${report.total}` : ''}`}
                  suffix={cov.length ? <Typography.Text type="secondary" style={{ fontSize: 12 }}>{t('req.casesCount', '{n} 个用例').replace('{n}', String(cov.length))}</Typography.Text> : undefined}
                />
                <Space direction="vertical" size={2}>
                  <Typography.Text type="secondary" style={{ fontSize: 14 }}>{t('req.completeness', '完整性')}</Typography.Text>
                  <Tag color={report.complete ? 'green' : 'orange'}>{report.complete ? t('req.complete', '已完整') : t('req.hasGaps', '有缺口')}</Tag>
                </Space>
              </Space>
              {/* 缺口明细:未覆盖(无任务覆盖该标准)/ 未验证(已覆盖但未交付验证)。 */}
              {!!report.gaps?.length && (
                <div>
                  <Typography.Text type="secondary" style={{ fontSize: 13 }}>{t('req.gaps', '缺口')} ({report.gaps.length})</Typography.Text>
                  <ul style={{ margin: '6px 0 0', paddingLeft: 18 }}>
                    {report.gaps.map((g) => {
                      const manual = cov.filter((c) => c.criterionIndex === g.criterionIndex)
                      return (
                        <li key={g.criterionIndex} style={{ marginBottom: 4 }}>
                          <Tag color={g.kind === 'UNCOVERED' ? 'red' : 'gold'} style={{ marginRight: 6 }}>
                            {g.kind === 'UNCOVERED' ? t('req.gapUncovered', '未覆盖') : t('req.gapUnverified', '未验证')}
                          </Tag>
                          <span>{g.text}</span>
                          {/* 任务未覆盖但已有手工用例兜底:提示评审,避免误判为完全缺口。 */}
                          {!!manual.length && (
                            <Tag color="blue" style={{ marginLeft: 6 }}>{t('req.hasManualCase', '有手工用例')} · {manual.length}</Tag>
                          )}
                        </li>
                      )
                    })}
                  </ul>
                </div>
              )}
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
                : <span style={{ color: 'var(--text-3)' }}>—</span>,
          },
          { title: '', width: 50, render: (_, c) => <Button type="link" size="small" danger onClick={() => unlink(c.id)}>{t('req.remove', '移除')}</Button> },
        ]}
      />
    </Drawer>
  )
}

// 交付尝试状态 → Badge 状态色。
const attemptBadge = (s: string): 'processing' | 'success' | 'error' | 'warning' | 'default' =>
  s === 'RUNNING' ? 'processing' : s === 'DELIVERED' ? 'success' : s === 'FAILED' ? 'error' : s === 'DISPATCHED' ? 'warning' : 'default'
// 审计事件类型 → 颜色(时间线圆点 / 标签)。
const eventColor = (k: string): string =>
  k === 'DECISION' ? 'blue' : k === 'FILE_CHANGE' ? 'gold' : k === 'TEST_RESULT' ? 'green' : k === 'TOOL_CALL' ? 'geekblue' : k === 'VERDICT' ? 'purple' : 'default'

// 执行进度抽屉:某任务的全部交付尝试 + 每个尝试的实时审计轨迹 + 交付物/错误。
// RUNNING/DISPATCHED 期间每 3s 轮询,直到落终态(派发后能看着 AI 干活)。
function EventsDrawer({ decompId, task, onClose }: { decompId: string; task: Task | null; onClose: () => void }) {
  const { t } = useI18n()
  const [attempts, setAttempts] = useState<DeliveryAttempt[]>([])
  const [eventsByAttempt, setEventsByAttempt] = useState<Record<string, DeliveryEvent[]>>({})
  const [loading, setLoading] = useState(false)

  const load = useCallback(async () => {
    if (!task) return
    setLoading(true)
    try {
      const atts = await api.deliveries(decompId, task.id).catch(() => [])
      const ordered = [...atts].reverse() // 最近一次在前(列表按插入序返回)
      const map: Record<string, DeliveryEvent[]> = {}
      for (const a of ordered) {
        const id = a.id || a.attemptId
        if (id) map[id] = await api.deliveryEvents(id).catch(() => [])
      }
      setAttempts(ordered)
      setEventsByAttempt(map)
    } finally {
      setLoading(false)
    }
  }, [task, decompId])

  useEffect(() => {
    if (!task) return
    setAttempts([])
    setEventsByAttempt({})
    load()
  }, [task, load])

  // 有尝试在跑就轮询;全部落终态后停。
  const live = attempts.some((a) => a.status === 'RUNNING' || a.status === 'DISPATCHED')
  useEffect(() => {
    if (!task || !live) return
    const h = setInterval(load, 3000)
    return () => clearInterval(h)
  }, [task, live, load])

  return (
    <Drawer
      title={
        <Space>
          <span>{task ? `${t('req.execProgress', '执行进度')} · ${task.title}` : ''}</span>
          {live && <Badge status="processing" text={<Typography.Text type="secondary" style={{ fontSize: 12 }}>{t('req.livePolling', '实时刷新中')}</Typography.Text>} />}
        </Space>
      }
      open={!!task}
      onClose={onClose}
      width={560}
      extra={<Button size="small" icon={<ReloadOutlined />} loading={loading} onClick={load}>{t('common.refresh', '刷新')}</Button>}
    >
      {attempts.length === 0 ? (
        <Empty description={t('req.noAttempts', '暂无执行记录(先派发任务)')} />
      ) : (
        <Space direction="vertical" size={12} style={{ width: '100%' }}>
          {attempts.map((a, idx) => {
            const id = a.id || a.attemptId || String(idx)
            const evs = eventsByAttempt[id] || []
            return (
              <Card key={id} size="small" title={
                <Space size={8}>
                  <Badge status={attemptBadge(a.status)} />
                  <span>{a.status === 'RUNNING' ? t('req.attemptRunning', '执行中') : a.status === 'DELIVERED' ? t('req.attemptDelivered', '已交付') : a.status === 'FAILED' ? t('req.attemptFailed', '失败') : a.status}</span>
                  {a.executor && <Tag>{a.executor}</Tag>}
                  {idx === 0 && <Tag color="blue">{t('req.latest', '最近')}</Tag>}
                </Space>
              }>
                {a.status === 'RUNNING' && <div style={{ marginBottom: 8 }}><Spin size="small" /> <Typography.Text type="secondary">{t('req.aiWorking', 'AI 执行者正在工作…')}</Typography.Text></div>}
                {a.deliverable && (a.deliverable.reference || a.deliverable.summary) && (
                  <Descriptions size="small" column={1} style={{ marginBottom: 8 }}>
                    <Descriptions.Item label={t('req.deliverable', '交付物')}>
                      <Tag color="green">{a.deliverable.kind}</Tag>
                      <Typography.Text code copyable>{a.deliverable.reference}</Typography.Text>
                    </Descriptions.Item>
                    {a.deliverable.summary && <Descriptions.Item label={t('req.summary', '摘要')}>{a.deliverable.summary}</Descriptions.Item>}
                  </Descriptions>
                )}
                {a.error && <Typography.Text type="danger" style={{ display: 'block', marginBottom: 8 }}>{a.error}</Typography.Text>}
                {evs.length > 0 ? (
                  <Timeline
                    items={evs.map((e) => ({
                      color: eventColor(e.kind),
                      children: (
                        <span>
                          <Tag color={eventColor(e.kind)}>{e.kind}</Tag>
                          <span>{e.message || '—'}</span>
                          {typeof e.detail === 'string' && e.detail && <Typography.Text type="secondary" style={{ display: 'block', fontSize: 12 }}>{e.detail}</Typography.Text>}
                        </span>
                      ),
                    }))}
                  />
                ) : (
                  <Typography.Text type="secondary" style={{ fontSize: 12 }}>{t('req.noTrace', '暂无审计轨迹')}</Typography.Text>
                )}
              </Card>
            )
          })}
        </Space>
      )}
    </Drawer>
  )
}
