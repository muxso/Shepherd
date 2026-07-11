import { useEffect, useState } from 'react'
import { Button, Empty, Segmented, Table, Tabs, Tag, Tooltip } from 'antd'
import { QuestionCircleOutlined, ReloadOutlined } from '@ant-design/icons'
import { message, modal } from '../feedback'
import { api, ApiError, type PlanStats } from '../api'
import { useApp } from '../context'
import { regList, regRemove, type RegItem } from '../registry'
import { Workspace, useWorkTabs, useOpenParam } from '../components/Workspace'
import { SelectProjectEmpty } from '../components/Page'
import { useI18n } from '../i18n'
import { useListView, type ListColumn } from '../components/ListView'
import PlanModuleTree from '../components/plan/PlanModuleTree'
import PlanFormModal from '../components/plan/PlanFormModal'
import PlanDetail, { ReportMdModal } from '../components/plan/PlanDetail'
import {
  groupIdOf,
  inPlanModule,
  isGroup,
  moduleNameOf,
  moduleOf,
  planModules,
  planRegUpdate,
  tagsOf,
  type PlanModule,
} from '../components/plan/planLocal'

// 测试计划页(照 MeterSphere 布局):顶部「计划 / 报告」线性 tab;
// 计划 tab = 左模块树 + 右(全部/计划/计划组 Segmented + 列表工具栏 + 表格 + 详情多开);
// 报告 tab = 各计划的 Markdown 报告入口。模块/分组/标签为本地 meta(见 planLocal.ts)。
export default function TestPlans() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const [plans, setPlans] = useState<RegItem[]>([])
  const [modules, setModules] = useState<PlanModule[]>([])
  const [statsMap, setStatsMap] = useState<Record<string, PlanStats>>({})
  const [moduleKey, setModuleKey] = useState('ALL')
  const [seg, setSeg] = useState<'all' | 'plan' | 'group'>('all')
  const [form, setForm] = useState<{ mode: 'plan' | 'group'; editing?: RegItem } | null>(null)
  const [runningId, setRunningId] = useState('')
  const [expandedKeys, setExpandedKeys] = useState<string[]>([])
  const [report, setReport] = useState<{ name: string; md: string } | null>(null)
  const tabs = useWorkTabs()

  const loadStats = async (list: RegItem[]) => {
    const entries = await Promise.all(
      list.filter((p) => !isGroup(p)).map((p) => api.planStats(p.id).then((s) => [p.id, s] as const).catch(() => null)),
    )
    setStatsMap(Object.fromEntries(entries.filter(Boolean) as [string, PlanStats][]))
  }

  useEffect(() => {
    const list = regList('plan', projectId)
    setPlans(list)
    setModules(planModules(projectId))
    setModuleKey('ALL')
    setSeg('all')
    setExpandedKeys([])
    loadStats(list)
    tabs.reset()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId])
  useOpenParam(tabs.open) // 支持 ?open=<planId> 深链

  const membersOf = (g: RegItem) => plans.filter((x) => !isGroup(x) && groupIdOf(x) === g.id)

  // 通过/执行/总数(计划来自 planStats;计划组聚合组内计划;无统计返回 undefined)。
  const numsOf = (p: RegItem): { total: number; pass: number; exec: number } | undefined => {
    if (isGroup(p)) {
      const ns = membersOf(p).map(numsOf).filter(Boolean) as { total: number; pass: number; exec: number }[]
      if (!ns.length) return undefined
      return ns.reduce((a, b) => ({ total: a.total + b.total, pass: a.pass + b.pass, exec: a.exec + b.exec }), { total: 0, pass: 0, exec: 0 })
    }
    const s = statsMap[p.id]
    if (!s) return undefined
    return { total: s.total, pass: Math.round(s.passRate * s.total), exec: Math.round(s.executeRate * s.total) }
  }
  // 状态口径:无结果=未开始;部分有结果=进行中;全部有结果=已完成。
  const statusOf = (p: RegItem): 'NOT_STARTED' | 'RUNNING' | 'DONE' => {
    const n = numsOf(p)
    if (!n || n.total === 0 || n.exec <= 0) return 'NOT_STARTED'
    return n.exec >= n.total ? 'DONE' : 'RUNNING'
  }
  const statusMeta: Record<string, { label: string; color: string }> = {
    NOT_STARTED: { label: t('plan.statusNotStarted', '未开始'), color: 'default' },
    RUNNING: { label: t('plan.statusRunning', '进行中'), color: 'processing' },
    DONE: { label: t('plan.statusDone', '已完成'), color: 'success' },
  }

  const run = async (p: RegItem) => {
    setRunningId(p.id)
    try {
      if (isGroup(p)) {
        const members = membersOf(p)
        if (!members.length) return void message.info(t('plan.noGroupPlans', '组内暂无计划'))
        let executed = 0
        let total = 0
        for (const m of members) {
          const r = await api.runPlan(m.id)
          executed += r.executed
          total += r.total
        }
        message.success(`${t('plan.groupRunDone', '计划组执行完成')}:${executed}/${total}`)
      } else {
        const r = await api.runPlan(p.id)
        message.success(`${t('plan.runDone', '执行完成')}:${r.executed}/${r.total}`)
      }
      loadStats(plans)
    } catch (e) {
      message.error(e instanceof ApiError ? `${t('plan.runFail', '执行失败')}:${e.status}` : t('plan.runFail', '执行失败'))
    } finally {
      setRunningId('')
    }
  }

  const remove = (p: RegItem) => {
    modal.confirm({
      title: `${isGroup(p) ? t('plan.deleteGroupConfirm', '删除计划组') : t('plan.deleteConfirm', '删除测试计划')}「${p.label}」?`,
      content: isGroup(p)
        ? t('plan.deleteGroupContent', '仅从列表移除;组内计划保留并变为不归属任何计划组。')
        : t('plan.deleteContent', '仅从本地列表移除,后端数据不受影响。'),
      okButtonProps: { danger: true },
      onOk: () => {
        if (isGroup(p)) membersOf(p).forEach((m) => planRegUpdate(projectId, m.id, { meta: { groupId: '' } }))
        regRemove('plan', projectId, p.id)
        setPlans(regList('plan', projectId))
        tabs.close(p.id)
        message.success(t('plan.deleted', '已删除'))
      },
    })
  }

  // 模块增删改:删除时把归属其下的计划改回未规划。
  const onModulesChanged = (mods: PlanModule[], removedIds?: string[]) => {
    setModules(mods)
    if (removedIds?.length) {
      plans.filter((p) => removedIds.includes(moduleOf(p))).forEach((p) => planRegUpdate(projectId, p.id, { meta: { module: '' } }))
      setPlans(regList('plan', projectId))
    }
  }

  const toggleExpand = (id: string) => setExpandedKeys((ks) => (ks.includes(id) ? ks.filter((k) => k !== id) : [...ks, id]))
  const openItem = (p: RegItem) => (isGroup(p) ? toggleExpand(p.id) : tabs.open(p.id))

  const passRateCell = (p: RegItem) => {
    const n = numsOf(p)
    if (!n || !n.total) return <span style={{ color: 'var(--text-3)' }}>—</span>
    const pr = Math.round((n.pass * 100) / n.total)
    return (
      <span style={{ display: 'inline-flex', alignItems: 'center', gap: 8 }}>
        <span style={{ width: 60, height: 6, background: 'var(--panel-2)', border: '1px solid var(--border-soft)', borderRadius: 3, overflow: 'hidden', display: 'inline-block' }}>
          <span style={{ display: 'block', width: `${pr}%`, height: '100%', background: 'var(--success)' }} />
        </span>
        <span style={{ color: 'var(--text-2)', fontSize: 12 }}>{pr}%</span>
      </span>
    )
  }
  const execResultCell = (p: RegItem) => {
    const n = numsOf(p)
    if (!n) return <span style={{ color: 'var(--text-3)' }}>—</span>
    return (
      <span style={{ fontSize: 12, color: 'var(--text-2)', whiteSpace: 'nowrap' }}>
        <span style={{ color: 'var(--success)' }}>●</span> {t('plan.pass', '通过')} {n.pass}
        <span style={{ marginLeft: 8, color: 'var(--error)' }}>●</span> {t('plan.fail', '失败')} {Math.max(0, n.exec - n.pass)}
      </span>
    )
  }
  const actionCell = (p: RegItem) => (
    <span onClick={(e) => e.stopPropagation()}>
      <Button type="link" size="small" loading={runningId === p.id} onClick={() => run(p)}>{t('plan.exec', '执行')}</Button>
      <Button type="link" size="small" onClick={() => setForm({ mode: isGroup(p) ? 'group' : 'plan', editing: p })}>{t('a.edit', '编辑')}</Button>
      <Button type="link" size="small" danger onClick={() => remove(p)}>{t('a.delete', '删除')}</Button>
    </span>
  )

  // 列表三件套(视图/筛选/列设置):useListView 必须在条件 return 之前调用。
  const allTags = [...new Set(plans.flatMap(tagsOf))]
  const allColumns: ListColumn<RegItem>[] = [
    {
      key: 'id',
      label: 'ID',
      title: 'ID',
      width: 96,
      render: (_, p) => <span className="ms-mono" style={{ color: 'var(--text-2)', fontSize: 12 }}>{p.id.slice(0, 8)}</span>,
    },
    {
      key: 'name',
      label: t('plan.colName', '测试计划名称'),
      title: t('plan.colName', '测试计划名称'),
      ellipsis: true,
      render: (_, p) => (
        <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6, minWidth: 0 }}>
          {isGroup(p) && <Tag color="processing" style={{ marginInlineEnd: 0 }}>{t('plan.groupTag', '计划组')}</Tag>}
          <span style={{ color: 'var(--brand)', cursor: 'pointer', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }} onClick={(e) => { e.stopPropagation(); openItem(p) }}>
            {p.label}
          </span>
        </span>
      ),
    },
    {
      key: 'status',
      label: t('plan.colStatus', '状态'),
      title: t('plan.colStatus', '状态'),
      width: 96,
      render: (_, p) => { const m = statusMeta[statusOf(p)]; return <Tag color={m.color}>{m.label}</Tag> },
    },
    { key: 'createdBy', label: t('plan.colCreatedBy', '创建人'), title: t('plan.colCreatedBy', '创建人'), width: 100, render: (_, p) => p.meta?.createdBy || '—' },
    {
      key: 'passRate',
      label: t('plan.colPassRate', '通过率'),
      title: (
        <span>
          {t('plan.colPassRate', '通过率')}{' '}
          <Tooltip title={t('plan.passRateTip', '通过率 = 通过用例数 / 总用例数')}>
            <QuestionCircleOutlined style={{ color: 'var(--text-3)' }} />
          </Tooltip>
        </span>
      ),
      width: 130,
      render: (_, p) => passRateCell(p),
    },
    { key: 'execResult', label: t('plan.colExecResult', '执行结果'), title: t('plan.colExecResult', '执行结果'), width: 160, render: (_, p) => execResultCell(p) },
    { key: 'caseCount', label: t('plan.colCaseCount', '用例数'), title: t('plan.colCaseCount', '用例数'), width: 76, render: (_, p) => numsOf(p)?.total ?? 0 },
    {
      key: 'tags',
      label: t('plan.colTags', '标签'),
      title: t('plan.colTags', '标签'),
      width: 150,
      render: (_, p) => { const ts = tagsOf(p); return ts.length ? ts.map((tg) => <Tag key={tg}>{tg}</Tag>) : <span style={{ color: 'var(--text-3)' }}>—</span> },
    },
    {
      key: 'module',
      label: t('plan.colModule', '所属模块'),
      title: t('plan.colModule', '所属模块'),
      width: 120,
      ellipsis: true,
      render: (_, p) => moduleNameOf(modules, moduleOf(p)) || t('plan.moduleUnfiled', '未规划'),
    },
    { key: 'createdAt', label: t('plan.colCreatedAt', '创建时间'), title: t('plan.colCreatedAt', '创建时间'), width: 170, render: (_, p) => new Date(p.createdAt).toLocaleString() },
    { key: 'action', label: t('plan.colAction', '操作'), title: t('plan.colAction', '操作'), width: 160, render: (_, p) => actionCell(p) },
  ]

  // 进入 useListView 前先按 左树模块 + Segmented(全部/计划/计划组)收敛数据集。
  const scoped = plans.filter(
    (p) => inPlanModule(modules, moduleKey, moduleOf(p)) && (seg === 'all' || (seg === 'group') === isGroup(p)),
  )
  const lv = useListView<RegItem>({
    kind: 'test-plan',
    projectId,
    searchOf: (p) => p.label,
    searchLabel: t('plan.searchPh', '搜索计划名'),
    fields: [
      {
        key: 'status',
        label: t('plan.colStatus', '状态'),
        type: 'enum',
        options: (['NOT_STARTED', 'RUNNING', 'DONE'] as const).map((v) => ({ value: v, label: statusMeta[v].label })),
        get: (p) => statusOf(p),
      },
      { key: 'tags', label: t('plan.colTags', '标签'), type: 'tags', options: allTags.map((v) => ({ value: v, label: v })), get: (p) => tagsOf(p) },
      { key: 'createdBy', label: t('plan.colCreatedBy', '创建人'), type: 'text', get: (p) => p.meta?.createdBy || '', advOnly: true },
    ],
    columns: allColumns,
    rows: scoped,
  })

  if (!projectId) return <SelectProjectEmpty />

  // 组行展开:组内计划子表(点行进详情)。
  const expandedRowRender = (g: RegItem) => (
    <Table<RegItem>
      rowKey="id"
      size="small"
      pagination={false}
      dataSource={membersOf(g)}
      locale={{ emptyText: <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('plan.noGroupPlans', '组内暂无计划')} /> }}
      onRow={(r) => ({ onClick: () => tabs.open(r.id), style: { cursor: 'pointer' } })}
      columns={[
        { title: 'ID', width: 96, render: (_, p) => <span className="ms-mono" style={{ color: 'var(--text-2)', fontSize: 12 }}>{p.id.slice(0, 8)}</span> },
        { title: t('plan.colName', '测试计划名称'), ellipsis: true, render: (_, p) => <span style={{ color: 'var(--brand)' }}>{p.label}</span> },
        { title: t('plan.colStatus', '状态'), width: 96, render: (_, p) => { const m = statusMeta[statusOf(p)]; return <Tag color={m.color}>{m.label}</Tag> } },
        { title: t('plan.colPassRate', '通过率'), width: 130, render: (_, p) => passRateCell(p) },
        { title: t('plan.colCaseCount', '用例数'), width: 76, render: (_, p) => numsOf(p)?.total ?? 0 },
        { title: t('plan.colAction', '操作'), width: 160, render: (_, p) => actionCell(p) },
      ]}
    />
  )

  const emptyText = (
    <Empty
      image={Empty.PRESENTED_IMAGE_SIMPLE}
      description={
        <span style={{ color: 'var(--text-3)' }}>
          {t('common.empty', '暂无数据')}{' '}
          <a style={{ color: 'var(--brand)' }} onClick={() => setForm({ mode: 'plan' })}>{t('plan.newPlan', '新建测试计划')}</a>
          {' '}{t('plan.or', '或')}{' '}
          <a style={{ color: 'var(--brand)' }} onClick={() => setForm({ mode: 'group' })}>{t('plan.newGroup', '新建计划组')}</a>
        </span>
      }
    />
  )

  // 右侧列表:Segmented(全部/计划/计划组)+ 列表三件套工具栏 + 表格。
  const listContent = (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '10px 12px', borderBottom: '1px solid var(--border-soft)' }}>
        <Segmented
          value={seg}
          onChange={(v) => setSeg(v as typeof seg)}
          options={[
            { value: 'all', label: t('plan.segAll', '全部') },
            { value: 'plan', label: t('plan.segPlan', '计划') },
            { value: 'group', label: t('plan.segGroup', '计划组') },
          ]}
        />
        <div style={{ flex: 1 }} />
        {lv.toolbar}
        <Tooltip title={t('plan.refreshStats', '刷新统计')}>
          <Button size="small" icon={<ReloadOutlined />} onClick={() => loadStats(plans)} />
        </Tooltip>
      </div>
      <div style={{ flex: 1, overflow: 'auto', padding: 12 }}>
        <Table<RegItem>
          rowKey="id"
          size="middle"
          dataSource={lv.rows}
          columns={lv.columns}
          expandable={{
            expandedRowKeys: expandedKeys,
            onExpandedRowsChange: (ks) => setExpandedKeys(ks.map(String)),
            rowExpandable: isGroup,
            expandedRowRender,
          }}
          onRow={(p) => ({
            onClick: (e) => {
              if ((e.target as Element).closest?.('.ant-table-row-expand-icon')) return
              openItem(p)
            },
            style: { cursor: 'pointer' },
          })}
          pagination={{ pageSize: 15, size: 'small', showTotal: (n) => t('ws.total', '共 {n} 条').replace('{n}', String(n)) }}
          locale={{ emptyText }}
        />
      </div>
    </div>
  )

  const detailTabs = plans
    .filter((p) => !isGroup(p) && tabs.openIds.includes(p.id))
    .map((p) => ({ key: p.id, label: p.label, children: <PlanDetail planId={p.id} name={p.label} projectId={projectId} /> }))

  // 计划 tab:左模块树 + 右列表(含详情多开 workspace)。
  const plansTab = (
    <Workspace
      left={
        <PlanModuleTree
          projectId={projectId}
          items={plans}
          modules={modules}
          selectedKey={moduleKey}
          onSelect={setModuleKey}
          onNewPlan={() => setForm({ mode: 'plan' })}
          onNewGroup={() => setForm({ mode: 'group' })}
          onModulesChanged={onModulesChanged}
        />
      }
      leftWidth={240}
      siderKey="plan-sider"
      listLabel={t('plan.allTestPlans', '全部测试计划')}
      activeKey={tabs.activeKey}
      onChange={tabs.setActiveKey}
      onClose={tabs.close}
      tabs={detailTabs}
      listContent={listContent}
    />
  )

  // 报告 tab:各计划的报告入口(计划组无报告,不列)。
  const openReport = async (p: RegItem) => {
    try {
      setReport({ name: p.label, md: await api.planReportMd(p.id) })
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('plan.reportFail', '获取报告失败'))
    }
  }
  const reportsTab = (
    <div style={{ padding: '12px 16px' }}>
      <Table<RegItem>
        rowKey="id"
        size="middle"
        dataSource={plans.filter((p) => !isGroup(p))}
        locale={{ emptyText: <Empty description={t('plan.reportEmpty', '暂无报告')} /> }}
        pagination={{ pageSize: 15, size: 'small', showTotal: (n) => t('ws.total', '共 {n} 条').replace('{n}', String(n)) }}
        columns={[
          { title: 'ID', width: 96, render: (_, p) => <span className="ms-mono" style={{ color: 'var(--text-2)', fontSize: 12 }}>{p.id.slice(0, 8)}</span> },
          { title: t('plan.colName', '测试计划名称'), ellipsis: true, dataIndex: 'label' },
          { title: t('plan.colStatus', '状态'), width: 110, render: (_, p) => { const m = statusMeta[statusOf(p)]; return <Tag color={m.color}>{m.label}</Tag> } },
          { title: t('plan.colPassRate', '通过率'), width: 140, render: (_, p) => passRateCell(p) },
          { title: t('plan.colCreatedAt', '创建时间'), width: 180, render: (_, p) => new Date(p.createdAt).toLocaleString() },
          {
            title: t('plan.colAction', '操作'),
            width: 120,
            render: (_, p) => <Button type="link" size="small" onClick={() => openReport(p)}>{t('plan.viewReport', '查看报告')}</Button>,
          },
        ]}
      />
    </div>
  )

  return (
    <>
      <Tabs
        className="ms-detail-tabs ms-fill-tabs"
        tabBarStyle={{ margin: 0, paddingInline: 16 }}
        items={[
          { key: 'plans', label: t('plan.tabPlans', '计划'), children: plansTab },
          { key: 'reports', label: t('plan.tabReports', '报告'), children: reportsTab },
        ]}
      />
      <PlanFormModal
        key={form ? `${form.mode}:${form.editing?.id || 'new'}` : 'closed'}
        open={!!form}
        mode={form?.mode || 'plan'}
        editing={form?.editing}
        projectId={projectId}
        modules={modules}
        groups={plans}
        onClose={() => setForm(null)}
        onSaved={(list, created) => {
          setForm(null)
          setPlans(list)
          loadStats(list)
          if (created) tabs.open(created.id)
        }}
      />
      <ReportMdModal open={!!report} name={report?.name || ''} md={report?.md || ''} onClose={() => setReport(null)} />
    </>
  )
}
