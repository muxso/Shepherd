import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { Button, Dropdown, Empty, Form, Input, Modal, Radio, Select, Space, Switch, Table, Tabs, Tag, Tree, Typography } from 'antd'
import { message } from '../feedback'
import { PlayCircleOutlined, PlusOutlined, SaveOutlined, ThunderboltOutlined, DownOutlined, LinkOutlined } from '@ant-design/icons'
import { api, ApiError, type ApiCase, type Environment, type Scenario, type ScenarioExecution, type ScenarioRunResult, type ScenarioStep } from '../api'
import { useApp } from '../context'
import { methodColor, statusColor, outcomeColor } from '../components/tags'
import { Workspace, WorkList, PaneHeader, useWorkTabs } from '../components/Workspace'
import AssertionEditor from '../components/AssertionEditor'
import { useI18n } from '../i18n'

type TFn = (key: string, fallback?: string) => string

export default function Scenarios() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const [list, setList] = useState<Scenario[]>([])
  const [loading, setLoading] = useState(false)
  const [search, setSearch] = useState('')
  const [statusKey, setStatusKey] = useState('ALL')
  const [createOpen, setCreateOpen] = useState(false)
  const tabs = useWorkTabs()

  const load = async () => {
    if (!projectId) return setList([])
    setLoading(true)
    try {
      setList(await api.scenarios(projectId))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('scenario.loadFailed', '加载场景失败'))
    } finally {
      setLoading(false)
    }
  }
  useEffect(() => {
    load()
    tabs.reset()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId])

  const treeData = useMemo(() => {
    const byStatus = new Map<string, number>()
    list.forEach((s) => byStatus.set(s.status, (byStatus.get(s.status) || 0) + 1))
    return [
      {
        key: 'ALL',
        title: `${t('scenario.allScenarios', '全部场景')} (${list.length})`,
        children: [...byStatus.entries()].map(([s, n]) => ({ key: `st:${s}`, title: `${s} (${n})` })),
      },
    ]
  }, [list])

  const filtered = useMemo(
    () =>
      list.filter((s) => {
        const st = statusKey === 'ALL' || s.status === statusKey.replace('st:', '')
        return st && s.name.toLowerCase().includes(search.toLowerCase())
      }),
    [list, search, statusKey],
  )

  if (!projectId) return <div style={{ padding: 48 }}><Empty description={t('common.selectProject', '请先在顶部选择项目')} /></div>

  const left = (
    <>
      <PaneHeader title={t('scenario.status', '状态')} />
      <div style={{ flex: 1, overflow: 'auto', padding: 8 }}>
        <Tree blockNode defaultExpandAll selectedKeys={[statusKey]} treeData={treeData} onSelect={(k) => k.length && setStatusKey(String(k[0]))} />
      </div>
    </>
  )

  const detailTabs = tabs.openIds
    .map((id) => list.find((s) => s.id === id))
    .filter((s): s is Scenario => !!s)
    .map((s) => ({ key: s.id, label: s.name, children: <ScenarioDetail scenario={s} /> }))

  return (
    <>
      <Workspace
        left={left}
        listLabel={t('scenario.allScenarios', '全部场景')}
        activeKey={tabs.activeKey}
        onChange={tabs.setActiveKey}
        onClose={tabs.close}
        tabs={detailTabs}
        listContent={
          <WorkList<Scenario>
            onNew={() => setCreateOpen(true)}
            newLabel={t('scenario.newScenario', '新建场景')}
            onSearch={setSearch}
            searchPlaceholder={t('scenario.searchName', '搜索场景名')}
            onRefresh={load}
            data={filtered}
            loading={loading}
            onRowClick={(s) => tabs.open(s.id)}
            emptyText={t('scenario.empty', '暂无场景')}
            columns={[
              { title: t('scenario.colName', '名称'), dataIndex: 'name', ellipsis: true },
              { title: t('scenario.colSteps', '步骤数'), dataIndex: 'steps', width: 100, render: (steps?: unknown[]) => <Tag color={steps?.length ? 'geekblue' : 'default'}>{steps?.length ?? 0}</Tag> },
              { title: t('scenario.colStatus', '状态'), dataIndex: 'status', width: 120, render: (s: string) => <Tag color={statusColor(s)}>{s}</Tag> },
            ]}
          />
        }
      />
      <Modal title={t('scenario.newScenario', '新建场景')} open={createOpen} onCancel={() => setCreateOpen(false)} footer={null} destroyOnHidden>
        <CreateScenarioForm
          projectId={projectId}
          onCreated={(s) => {
            setCreateOpen(false)
            load().then(() => tabs.open(s.id))
          }}
        />
      </Modal>
    </>
  )
}

// 步骤类型 → 标签文案 + 颜色(对齐 MeterSphere)。
function makeStepMeta(t: TFn): Record<string, { label: string; color: string }> {
  return {
    REQUEST: { label: t('scenario.stepRequest', '请求'), color: 'blue' },
    CASE: { label: t('scenario.stepCase', '引用用例'), color: 'green' },
    SCENARIO: { label: t('scenario.stepScenario', '引用场景'), color: 'geekblue' },
    LOOP: { label: t('scenario.stepLoop', '循环控制器'), color: 'purple' },
    IF: { label: t('scenario.stepIf', '条件控制器'), color: 'magenta' },
    ONCE: { label: t('scenario.stepOnce', '仅一次控制器'), color: 'cyan' },
    TIMER: { label: t('scenario.stepTimer', '等待时间'), color: 'orange' },
  }
}

interface Node {
  kind: string
  content: ReactNode
  children?: Node[]
}

// 引用 id → 可读名称(用例/子场景);未命中回落短 id,避免满屏 UUID。
type NameOf = (id: string) => string

// 把控制器载荷里的一个子步骤(原始 json)规整为 Node。
function childToNode(c: any, t: TFn, nameOf: NameOf): Node {
  const kind = String(c?.kind || '').toUpperCase()
  if (kind === 'CASE') return { kind, content: <span className="ms-mono">{t('scenario.caseRef', '用例')} {nameOf(c.refId)}</span> }
  if (kind === 'REQUEST')
    return { kind, content: <Space><Tag color={methodColor(c.method || 'GET')}>{c.method || 'GET'}</Tag><span className="ms-mono">{c.url}</span></Space> }
  return controlToNode(kind, c, t, nameOf)
}

function controlToNode(kind: string, payload: any, t: TFn, nameOf: NameOf): Node {
  const children = Array.isArray(payload?.children) ? payload.children.map((c: any) => childToNode(c, t, nameOf)) : []
  let content: ReactNode = ''
  if (kind === 'LOOP') content = `${t('scenario.loopPrefix', '循环')} ${payload?.times ?? 1} ${t('scenario.loopSuffix', '次')}`
  else if (kind === 'IF') content = <span className="ms-mono">{payload?.variable} {payload?.operator} {payload?.value}</span>
  else if (kind === 'ONCE') content = t('scenario.onceOnly', '仅执行一次')
  else if (kind === 'TIMER') content = `${t('scenario.waitPrefix', '等待')} ${payload?.ms ?? 0} ms`
  return { kind, content, children: kind === 'TIMER' ? undefined : children }
}

// 顶层步骤(ScenarioStep)→ Node。
function stepToNode(s: ScenarioStep, t: TFn, nameOf: NameOf): Node {
  if (s.request) return { kind: 'REQUEST', content: <Space><Tag color={methodColor(s.request.method)}>{s.request.method}</Tag><span className="ms-mono">{s.request.url}</span></Space> }
  if (s.caseId) return { kind: 'CASE', content: <span className="ms-mono">{t('scenario.caseRef', '用例')} {nameOf(s.caseId)}</span> }
  if (s.scenarioId) return { kind: 'SCENARIO', content: <span className="ms-mono">{t('scenario.subScenario', '子场景')} {nameOf(s.scenarioId)}</span> }
  if (s.control) return controlToNode(s.kind.toUpperCase(), s.control, t, nameOf)
  return { kind: s.kind, content: '—' }
}

function StepRow({ node, idx, depth, t }: { node: Node; idx: number; depth: number; t: TFn }) {
  const meta = makeStepMeta(t)[node.kind] || { label: node.kind, color: 'default' }
  return (
    <>
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          padding: '8px 12px',
          marginLeft: depth * 24,
          border: '1px solid #f0f0f0',
          borderRadius: 6,
          marginBottom: 6,
          background: depth ? '#fafafa' : '#fff',
        }}
      >
        <span style={{ color: '#c9cdd4', cursor: 'grab' }}>⠿</span>
        <Switch size="small" defaultChecked disabled />
        <PlayCircleOutlined style={{ color: '#7c3aed' }} />
        <span style={{ color: '#9aa0a6', fontSize: 12, minWidth: 18 }}>{idx}</span>
        <Tag color={meta.color} style={{ margin: 0 }}>{meta.label}</Tag>
        <span style={{ flex: 1, minWidth: 0 }}>{node.content}</span>
      </div>
      {node.children?.map((c, i) => <StepRow key={i} node={c} idx={i + 1} depth={depth + 1} t={t} />)}
    </>
  )
}

// 场景详情:全标签编辑器外壳(对齐参考图 #20-#24:头部 + 基本信息/步骤/参数/前后置/断言/
// 执行历史/变更历史/设置 + 顶部右侧 环境/服务端执行/保存)。步骤详情抽屉(#25)、可编辑元信息
// (需后端 updateScenario)、报告(#26)为后续切片。
function ScenarioDetail({ scenario }: { scenario: Scenario }) {
  const { t } = useI18n()
  const [steps, setSteps] = useState<ScenarioStep[]>([])
  const [running, setRunning] = useState(false)
  const [add, setAdd] = useState<string>('') // 当前打开的添加表单类型
  const [lastRun, setLastRun] = useState<ScenarioRunResult | null>(null)
  const [nameMap, setNameMap] = useState<Record<string, string>>({})
  // 执行配置:环境 + 步骤失败规则(后端 run 已支持 environment_id/failure_strategy)。
  const [envs, setEnvs] = useState<Environment[]>([])
  const [envId, setEnvId] = useState<string>('')
  const [failureStrategy, setFailureStrategy] = useState<'CONTINUE' | 'STOP'>('CONTINUE')
  // 引用名解析:命中用例/子场景名,未命中回落短 id(前 8 位),不再满屏 UUID。
  const nameOf = (id: string) => nameMap[id] || (id ? id.slice(0, 8) : '—')

  const loadSteps = async () => {
    try {
      const s = await api.getScenario(scenario.id)
      setSteps(s.steps || [])
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('scenario.loadStepsFailed', '加载步骤失败'))
    }
  }
  useEffect(() => {
    loadSteps()
    // 拉项目用例 + 场景,建 id→名 映射供步骤展示;拉环境供执行选择。
    Promise.all([
      api.projectCases(scenario.projectId).then((p) => p.items).catch(() => []),
      api.scenarios(scenario.projectId).then((s) => s).catch(() => []),
      api.environments(scenario.projectId).then((e) => (Array.isArray(e) ? e : [])).catch(() => []),
    ]).then(([cases, scns, environments]) => {
      const m: Record<string, string> = {}
      cases.forEach((c) => (m[c.id] = `${c.method} ${c.name}`))
      scns.forEach((s) => (m[s.id] = s.name))
      setNameMap(m)
      setEnvs(environments)
      setEnvId((cur) => cur || environments.find((e) => e.enabled !== false)?.id || '')
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scenario.id])

  const run = async () => {
    setRunning(true)
    try {
      const r = await api.runScenario(scenario.id, scenario.projectId, { environmentId: envId || undefined, failureStrategy })
      setLastRun(r)
      message.success(`${t('scenario.triggered', '场景已触发执行')} · ${r.status}`)
    } catch (e) {
      message.error(e instanceof ApiError ? `${t('scenario.execFailed', '执行失败')}:${e.status}` : t('scenario.execFailed', '执行失败'))
    } finally {
      setRunning(false)
    }
  }

  const ordered = [...steps].sort((a, b) => a.order - b.order)
  const nextOrder = steps.length ? Math.max(...steps.map((s) => s.order)) + 1 : 1
  const onAdded = () => {
    setAdd('')
    loadSteps()
  }

  const stepsTab = (
    <div>
      <Space style={{ marginBottom: 12 }}>
        <Typography.Text strong style={{ fontSize: 13 }}>{t('scenario.totalPrefix', '共')} {steps.length} {t('scenario.totalSuffix', '个步骤')}</Typography.Text>
        {lastRun && <Tag color={outcomeColor(lastRun.status)} style={{ margin: 0 }}>{lastRun.status} · {lastRun.caseCount} {t('scenario.caseUnit', '用例')}</Tag>}
      </Space>
      {ordered.length === 0 ? (
        <Empty description={t('scenario.emptySteps', '暂无步骤,点「添加步骤」')} />
      ) : (
        ordered.map((s, i) => <StepRow key={s.id} node={stepToNode(s, t, nameOf)} idx={i + 1} depth={0} t={t} />)
      )}
      <div style={{ textAlign: 'center', marginTop: 10 }}>
        <Dropdown
          menu={{
            items: [
              { type: 'group', label: t('scenario.grpRequest', '请求 / 场景'), children: [
                { key: 'CASE', label: t('scenario.stepCase', '引用用例') },
                { key: 'REQUEST', label: t('scenario.customRequest', '自定义请求') },
                { key: 'SCENARIO', label: t('scenario.stepScenario', '引用场景') },
              ] },
              { type: 'group', label: t('scenario.grpLogic', '逻辑控制'), children: [
                { key: 'LOOP', label: t('scenario.stepLoop', '循环控制器') },
                { key: 'IF', label: t('scenario.stepIf', '条件控制器') },
                { key: 'ONCE', label: t('scenario.stepOnce', '仅一次控制器') },
              ] },
              { type: 'group', label: t('scenario.grpOther', '其他'), children: [{ key: 'TIMER', label: t('scenario.stepTimer', '等待时间') }] },
            ],
            onClick: ({ key }) => setAdd(key),
          }}
        >
          <Button type="dashed" icon={<PlusOutlined />} block>{t('scenario.addStep', '添加步骤')}</Button>
        </Dropdown>
      </div>
      <AddStepModal type={add} scenarioId={scenario.id} projectId={scenario.projectId} nextOrder={nextOrder} onClose={() => setAdd('')} onAdded={onAdded} />
    </div>
  )

  const tabs = [
    { key: 'basic', label: t('apidef.basicInfo', '基本信息'), children: <ScenarioBasicInfo scenario={scenario} stepCount={steps.length} /> },
    { key: 'steps', label: t('scenario.stepsTab', '步骤'), children: stepsTab },
    { key: 'params', label: t('scenario.paramsTab', '参数'), children: <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('scenario.paramsSoon', '场景参数(常规/CSV)即将接入')} style={{ margin: '32px 0' }} /> },
    { key: 'prepost', label: t('scenario.prePostTab', '前/后置'), children: <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('scenario.prePostSoon', '前/后置处理即将接入')} style={{ margin: '32px 0' }} /> },
    { key: 'assert', label: t('apidef.assertions', '断言'), children: <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('scenario.assertSoon', '场景级断言即将接入')} style={{ margin: '32px 0' }} /> },
    { key: 'exec', label: t('scenario.execHistoryTab', '执行历史'), children: <ScenarioExecutionsTab scenarioId={scenario.id} t={t} /> },
    { key: 'change', label: t('apidef.changeHistory', '变更历史'), children: <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('scenario.changeSoon', '变更历史即将接入')} style={{ margin: '32px 0' }} /> },
    { key: 'settings', label: t('apidef.settings', '设置'), children: <ScenarioSettings failureStrategy={failureStrategy} onFailureStrategy={setFailureStrategy} t={t} /> },
  ]

  return (
    <div style={{ padding: '12px 16px', height: '100%', overflow: 'auto' }}>
      {/* 顶部右侧:环境 + 服务端执行 + 保存(对齐参考图 #20)。 */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
        <div style={{ flex: 1 }} />
        <Select
          size="small"
          value={envId || undefined}
          onChange={setEnvId}
          style={{ width: 200 }}
          placeholder={t('editor.selectEnv', '选择环境')}
          allowClear
          options={envs.map((e) => ({ value: e.id, label: e.baseUrl ? `${e.name} · ${e.baseUrl}` : e.name }))}
          notFoundContent={t('editor.noEnvConfigured', '未配置环境,去「环境」页新建')}
        />
        <Dropdown.Button
          type="primary"
          icon={<DownOutlined />}
          loading={running}
          onClick={run}
          menu={{ items: [{ key: 'local', label: t('apidef.localRun', '本地执行') }], onClick: () => message.info(t('scenario.localSoon', '本地执行即将接入')) }}
        >
          <ThunderboltOutlined /> {t('apidef.serverRun', '服务端执行')}
        </Dropdown.Button>
        <Button icon={<SaveOutlined />} disabled title={t('scenario.saveSoon', '保存需后端 updateScenario(下一切片)')}>{t('a.save', '保存')}</Button>
      </div>
      {/* 头部:状态 / 等级 / [id] / 名称 / 标签 / 描述。 */}
      <div style={{ marginBottom: 14 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8, flexWrap: 'wrap' }}>
          <Tag color={statusColor(scenario.status)} style={{ margin: 0 }}>{scenario.status}</Tag>
          <span className="ms-mono" style={{ color: '#8a9099', fontSize: 12 }}>[{scenario.id.slice(0, 8)}]</span>
          <span style={{ fontWeight: 600, fontSize: 15, color: '#1f2329' }}>{scenario.name}</span>
          <LinkOutlined style={{ color: '#bbb' }} />
        </div>
      </div>
      <Tabs className="ms-detail-tabs" defaultActiveKey="steps" items={tabs} />
    </div>
  )
}

// 基本信息(只读快照)。可编辑(名称/模块/等级/状态/描述)需后端 updateScenario,后续切片接入。
function ScenarioBasicInfo({ scenario, stepCount }: { scenario: Scenario; stepCount: number }) {
  const { t } = useI18n()
  const field = (label: string, value: ReactNode) => (
    <div style={{ marginBottom: 14 }}>
      <div style={{ fontSize: 13, color: '#5b6470', marginBottom: 6 }}>{label}</div>
      {value}
    </div>
  )
  return (
    <div style={{ maxWidth: 560 }}>
      {field(t('scenario.name', '场景名称'), <Input value={scenario.name} readOnly />)}
      {field(t('scenario.colStatus', '状态'), <Tag color={statusColor(scenario.status)}>{scenario.status}</Tag>)}
      {field(t('scenario.colSteps', '步骤数'), <span>{stepCount}</span>)}
      {field('ID', <span className="ms-mono" style={{ fontSize: 12 }}>{scenario.id}</span>)}
      <Typography.Text type="secondary" style={{ fontSize: 12 }}>{t('scenario.metaSoon', '所属模块 / 等级 / 描述 等可编辑项需后端 updateScenario,下一切片接入')}</Typography.Text>
    </div>
  )
}

// 设置:步骤执行失败规则(对齐参考图 #24;映射到 run 的 failure_strategy)。Cookie 配置占位。
function ScenarioSettings({ failureStrategy, onFailureStrategy, t }: { failureStrategy: 'CONTINUE' | 'STOP'; onFailureStrategy: (v: 'CONTINUE' | 'STOP') => void; t: TFn }) {
  return (
    <Space direction="vertical" size={18} style={{ width: '100%' }}>
      <div>
        <div style={{ fontWeight: 600, marginBottom: 8 }}>{t('scenario.cookieConfig', 'Cookie 配置')}</div>
        <Space direction="vertical" size={8}>
          <Space><Switch disabled /><span>{t('scenario.envCookie', '环境 Cookie')}</span></Space>
          <Space><Switch disabled /><span>{t('scenario.sharedCookie', '共享 Cookie')}</span></Space>
        </Space>
        <Typography.Text type="secondary" style={{ fontSize: 12 }}>{t('scenario.cookieSoon', '(Cookie 管理即将接入)')}</Typography.Text>
      </div>
      <div>
        <div style={{ fontWeight: 600, marginBottom: 8 }}>{t('scenario.failureRule', '步骤执行失败规则')}</div>
        <Radio.Group value={failureStrategy} onChange={(e) => onFailureStrategy(e.target.value)}>
          <Radio value="CONTINUE">{t('scenario.failContinue', '忽略错误,继续执行')}</Radio>
          <Radio value="STOP">{t('scenario.failStop', '停止/结束执行')}</Radio>
        </Radio.Group>
      </div>
    </Space>
  )
}

// 执行历史标签(对齐参考图 #23):序号 / 状态 / 用例数 / 时间 / 操作。
function ScenarioExecutionsTab({ scenarioId, t }: { scenarioId: string; t: TFn }) {
  const [rows, setRows] = useState<ScenarioExecution[]>([])
  const [loading, setLoading] = useState(false)
  useEffect(() => {
    setLoading(true)
    api.scenarioExecutions(scenarioId).then((p) => setRows(p.items)).catch(() => setRows([])).finally(() => setLoading(false))
  }, [scenarioId])
  return (
    <Table<ScenarioExecution>
      rowKey="id"
      size="small"
      loading={loading}
      dataSource={rows}
      locale={{ emptyText: <Empty description={t('scenario.noExec', '暂无执行记录')} /> }}
      pagination={{ pageSize: 20, size: 'small' }}
      columns={[
        { title: t('scenario.colSeq', '序号'), dataIndex: 'id', render: (v: string) => <span className="ms-mono" style={{ fontSize: 12 }}>{v.slice(0, 12)}</span> },
        { title: t('scenario.colStatus', '执行状态'), dataIndex: 'status', width: 110, render: (s: string) => <Tag color={outcomeColor(s)}>{s}</Tag> },
        { title: t('scenario.caseUnit', '用例'), dataIndex: 'caseCount', width: 80 },
        { title: t('scenario.execTime', '操作时间'), dataIndex: 'createdAt', width: 200, render: (v: string) => <span className="ms-mono" style={{ fontSize: 12 }}>{v?.slice(0, 19)}</span> },
        { title: t('apidef.colAction', '操作'), width: 100, render: () => <Button type="link" size="small" disabled>{t('scenario.viewResult', '执行结果')}</Button> },
      ]}
    />
  )
}

function CreateScenarioForm({ projectId, onCreated }: { projectId: string; onCreated: (s: Scenario) => void }) {
  const { t } = useI18n()
  const [saving, setSaving] = useState(false)
  return (
    <Form
      layout="vertical"
      onFinish={async (v: { name: string }) => {
        setSaving(true)
        try {
          const s = await api.createScenario(projectId, v.name)
          message.success(t('scenario.created', '场景已创建'))
          onCreated(s)
        } catch (e) {
          message.error(e instanceof ApiError ? e.message : t('scenario.createFailed', '创建失败'))
        } finally {
          setSaving(false)
        }
      }}
    >
      <Form.Item name="name" label={t('scenario.name', '场景名')} rules={[{ required: true }]}>
        <Input placeholder={t('scenario.namePlaceholder', '如:下单主流程')} autoFocus />
      </Form.Item>
      <Button type="primary" htmlType="submit" loading={saving} block>{t('a.create', '创建')}</Button>
    </Form>
  )
}

// 控制器子步骤(叶子)构建:CASE 引用 或 内联 REQUEST。
type Child = { kind: 'CASE'; refId: string } | { kind: 'REQUEST'; method: string; url: string }

function ChildrenBuilder({ value, onChange, projectCases }: { value: Child[]; onChange: (v: Child[]) => void; projectCases: ApiCase[] }) {
  const { t } = useI18n()
  const add = (c: Child) => onChange([...value, c])
  return (
    <Space direction="vertical" style={{ width: '100%' }} size={6}>
      {value.map((c, i) => (
        <Space.Compact key={i} style={{ width: '100%' }}>
          <Input style={{ width: 70 }} value={c.kind} disabled />
          <Input style={{ flex: 1 }} className="ms-mono" value={c.kind === 'CASE' ? `${t('scenario.caseRef', '用例')} ${c.refId}` : `${c.method} ${c.url}`} disabled />
          <Button onClick={() => onChange(value.filter((_, idx) => idx !== i))}>{t('scenario.del', '删')}</Button>
        </Space.Compact>
      ))}
      <Space>
        <Select
          key={value.length}
          size="small"
          style={{ width: 240 }}
          showSearch
          optionFilterProp="label"
          placeholder={t('scenario.addCaseChild', '+ 加用例子步骤')}
          onChange={(id) => add({ kind: 'CASE', refId: id })}
          options={projectCases.map((c) => ({ value: c.id, label: `${c.method} ${c.name}` }))}
        />
        <Button size="small" onClick={() => add({ kind: 'REQUEST', method: 'GET', url: 'http://127.0.0.1:9180/healthz' })}>{t('scenario.addRequestChild', '+ 加请求子步骤(示例)')}</Button>
      </Space>
    </Space>
  )
}

// 按类型分发的添加步骤弹窗:CASE/REQUEST/SCENARIO 叶子 + LOOP/IF/ONCE/TIMER 控制器(含子步骤)。
function AddStepModal({
  type,
  scenarioId,
  projectId,
  nextOrder,
  onClose,
  onAdded,
}: {
  type: string
  scenarioId: string
  projectId: string
  nextOrder: number
  onClose: () => void
  onAdded: () => void
}) {
  const { t } = useI18n()
  const [form] = Form.useForm()
  const [saving, setSaving] = useState(false)
  const [children, setChildren] = useState<Child[]>([])
  const [projCases, setProjCases] = useState<ApiCase[]>([])
  const [scns, setScns] = useState<Scenario[]>([])
  const isControl = ['LOOP', 'IF', 'ONCE'].includes(type)

  useEffect(() => {
    if (type) {
      setChildren([])
      form.resetFields()
      // CASE / 控制器 需要项目用例下拉;SCENARIO 需要场景下拉。
      if (isControl || type === 'CASE') api.projectCases(projectId).then((p) => setProjCases(p.items)).catch(() => undefined)
      if (type === 'SCENARIO') api.scenarios(projectId).then(setScns).catch(() => undefined)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [type])

  const submit = async (v: any) => {
    setSaving(true)
    try {
      const childPayload = children.map((c) => (c.kind === 'CASE' ? { kind: 'CASE', refId: c.refId } : { kind: 'REQUEST', method: c.method, url: c.url, assertions: [] }))
      if (type === 'CASE' || type === 'SCENARIO') {
        await api.addStep(scenarioId, { kind: type, order: nextOrder, refId: v.refId })
      } else if (type === 'REQUEST') {
        await api.addStep(scenarioId, { kind: 'REQUEST', order: nextOrder, request: { method: v.method, url: v.url, body: v.body || null, assertions: v.assertions || [] } })
      } else if (type === 'TIMER') {
        await api.addStep(scenarioId, { kind: 'TIMER', order: nextOrder, control: { ms: Number(v.ms) || 1000 } })
      } else if (type === 'LOOP') {
        await api.addStep(scenarioId, { kind: 'LOOP', order: nextOrder, control: { times: Number(v.times) || 1, children: childPayload } })
      } else if (type === 'IF') {
        await api.addStep(scenarioId, { kind: 'IF', order: nextOrder, control: { variable: v.variable, operator: v.operator, value: v.value, children: childPayload } })
      } else if (type === 'ONCE') {
        await api.addStep(scenarioId, { kind: 'ONCE', order: nextOrder, control: { children: childPayload } })
      }
      message.success(t('scenario.stepAdded', '步骤已添加'))
      onAdded()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('scenario.addFailed', '添加失败'))
    } finally {
      setSaving(false)
    }
  }

  const title = (makeStepMeta(t)[type]?.label || t('scenario.step', '步骤'))
  return (
    <Modal title={`${t('scenario.addPrefix', '添加')} · ${title}`} open={!!type} onCancel={onClose} onOk={() => form.submit()} confirmLoading={saving} destroyOnHidden width={620}>
      <Form form={form} layout="vertical" initialValues={{ method: 'GET', operator: '等于', times: 3, ms: 1000, assertions: [{ type: 'StatusIs', args: 200 }] }} onFinish={submit}>
        {(type === 'CASE' || type === 'SCENARIO') && (
          <Form.Item name="refId" label={type === 'CASE' ? t('scenario.stepCase', '引用用例') : t('scenario.refSubScenario', '引用子场景')} rules={[{ required: true }]}>
            <Select
              showSearch
              optionFilterProp="label"
              placeholder={type === 'CASE' ? t('scenario.selectProjCase', '选择项目接口用例') : t('scenario.selectSubScenario', '选择子场景')}
              options={
                type === 'CASE'
                  ? projCases.map((c) => ({ value: c.id, label: `${c.method} ${c.name}` }))
                  : scns.map((s) => ({ value: s.id, label: s.name }))
              }
              notFoundContent={type === 'CASE' ? t('scenario.noProjCase', '项目暂无接口用例') : t('scenario.noScenario', '项目暂无场景')}
            />
          </Form.Item>
        )}
        {type === 'REQUEST' && (
          <>
            <Space.Compact style={{ width: '100%' }}>
              <Form.Item name="method" label={t('scenario.method', '方法')} style={{ width: 120 }}><Select options={['GET', 'POST', 'PUT', 'DELETE', 'PATCH'].map((m) => ({ value: m, label: m }))} /></Form.Item>
              <Form.Item name="url" label="URL" style={{ flex: 1 }} rules={[{ required: true }]}><Input className="ms-mono" placeholder="http://127.0.0.1:9180/healthz" /></Form.Item>
            </Space.Compact>
            <Form.Item name="body" label={t('scenario.bodyOptional', '请求体(可选)')}><Input.TextArea rows={2} className="ms-mono" /></Form.Item>
            <Form.Item name="assertions" label={t('scenario.assertions', '断言')}><AssertionEditor /></Form.Item>
          </>
        )}
        {type === 'TIMER' && (
          <Form.Item name="ms" label={t('scenario.waitDuration', '等待时长 (ms)')} rules={[{ required: true }]}><Input type="number" /></Form.Item>
        )}
        {type === 'LOOP' && <Form.Item name="times" label={t('scenario.loopTimes', '循环次数')} rules={[{ required: true }]}><Input type="number" /></Form.Item>}
        {type === 'IF' && (
          <Space.Compact style={{ width: '100%' }}>
            <Form.Item name="variable" label={t('scenario.variable', '变量')} style={{ flex: 1 }} rules={[{ required: true }]}><Input className="ms-mono" placeholder="${count}" /></Form.Item>
            <Form.Item name="operator" label={t('scenario.operator', '操作符')} style={{ width: 110 }}><Select options={[['等于', t('scenario.opEq', '等于')], ['不等于', t('scenario.opNe', '不等于')], ['大于', t('scenario.opGt', '大于')], ['小于', t('scenario.opLt', '小于')], ['包含', t('scenario.opContains', '包含')]].map(([v, label]) => ({ value: v, label }))} /></Form.Item>
            <Form.Item name="value" label={t('scenario.value', '值')} style={{ width: 140 }} rules={[{ required: true }]}><Input /></Form.Item>
          </Space.Compact>
        )}
        {isControl && (
          <Form.Item label={t('scenario.childSteps', '子步骤(控制器内执行)')}>
            <ChildrenBuilder value={children} onChange={setChildren} projectCases={projCases} />
          </Form.Item>
        )}
      </Form>
    </Modal>
  )
}
