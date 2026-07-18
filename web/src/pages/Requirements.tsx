import { useCallback, useEffect, useState } from 'react'
import { Badge, Button, Card, Col, DatePicker, Descriptions, Drawer, Dropdown, Empty, Form, Input, InputNumber, Modal, Popover, Progress, Row, Segmented, Select, Space, Spin, Statistic, Table, Tabs, Tag, Timeline, Tooltip, Typography } from 'antd'
import dayjs, { type Dayjs } from 'dayjs'
import { message, modal } from '../feedback'
import { useNavigate } from 'react-router-dom'
import { BranchesOutlined, DeleteOutlined, EditOutlined, FlagOutlined, HistoryOutlined, InboxOutlined, PartitionOutlined, PlayCircleOutlined, ProfileOutlined, ReloadOutlined, SendOutlined } from '@ant-design/icons'
import {
  api,
  ApiError,
  EXECUTOR_LABEL,
  type ApiCase,
  type ApiModule,
  type CollabStats,
  type CoverageCase,
  type DeliveryAttempt,
  type DeliveryEvent,
  type FleetRuntime,
  type FunctionalCase,
  type ProjectMember,
  type Requirement,
  type RequirementChange,
  type RequirementStage,
  type RequirementStageKey,
  type RequirementVersion,
  type Task,
  type Skill,
  type VerificationReport,
} from '../api'
import { useApp } from '../context'
import { Workspace, WorkList, useWorkTabs } from '../components/Workspace'
import { ModuleTreePanel, inSelectedModule } from '../components/ModuleTreePanel'
import { SelectProjectEmpty } from '../components/Page'
import { MarkdownEditor } from '../components/MarkdownEditor'
import { MarkdownRenderer } from '../components/MarkdownRenderer'
import { regAdd, regList, type RegItem } from '../registry'
import ContributionGrid from '../components/ContributionGrid'
import { useListView, type ListColumn } from '../components/ListView'
import { CF_GROUP, CustomFieldItem, CustomFieldItems, collectCustomValues, customFormValues, useFieldTemplate } from '../components/TemplateFields'
import { fieldLabel } from '../fieldTemplates'
import { userIdStore, type TemplateField } from '../api'
import { useI18n } from '../i18n'

const toLines = (s: string) => s.split('\n').map((x) => x.trim()).filter(Boolean)
// Current acceptance criteria: prefer the baseline version, fall back to latest version / top-level field.
const critsOf = (r: Requirement): string[] =>
  r.versions?.find((v) => v.version === r.baselineVersion)?.acceptanceCriteria ?? r.versions?.[r.versions.length - 1]?.acceptanceCriteria ?? r.acceptanceCriteria ?? []
const taskColor = (s: string) => (s === 'VERIFIED' ? 'green' : s === 'FAILED' ? 'red' : s === 'PENDING' ? 'default' : 'blue')
// Kanban columns bucket tasks by status; "in progress" merges DISPATCHED/RUNNING.
const BOARD_COLS: { key: string; tkey: string; label: string; statuses: string[] }[] = [
  { key: 'PENDING', tkey: 'req.col.pending', label: '待派发', statuses: ['PENDING'] },
  { key: 'PROGRESS', tkey: 'req.col.progress', label: '进行中', statuses: ['DISPATCHED', 'RUNNING'] },
  { key: 'DELIVERED', tkey: 'req.col.delivered', label: '已交付', statuses: ['DELIVERED'] },
  { key: 'VERIFIED', tkey: 'req.col.verified', label: '已验证', statuses: ['VERIFIED'] },
  { key: 'FAILED', tkey: 'req.col.failed', label: '失败', statuses: ['FAILED'] },
]
const reqStatusColor = (s?: string) =>
  s === 'DELIVERED' ? 'green' : s === 'BASELINED' ? 'blue' : s === 'ARCHIVED' ? 'default' : 'default'
const prioColor = (p?: string) => (p === 'P0' ? 'red' : p === 'P1' ? 'orange' : p === 'P2' ? 'blue' : 'default')
const stageTagColor = (s?: string): 'default' | 'processing' | 'success' =>
  s === 'DONE' ? 'success' : s === 'IN_PROGRESS' ? 'processing' : 'default'
// Canonical order of the 7-stage requirement pipeline.
const STAGE_ORDER: RequirementStageKey[] = ['CREATED', 'AUDIT', 'REVIEW', 'DEV', 'TEST', 'ACCEPTANCE', 'DELIVERY']
// Rebuild the stage list in canonical order to guard against missing or out-of-order stages from the backend.
const stagesOf = (r?: Requirement | null): RequirementStage[] => {
  const by = new Map((r?.stages ?? []).map((s) => [s.stage, s]))
  return STAGE_ORDER.map((k) => by.get(k) ?? { stage: k, status: 'PENDING', plannedStart: null, plannedEnd: null, startedAt: null, finishedAt: null, overdue: false })
}
const stageDotColor = (s: RequirementStage) =>
  s.overdue ? 'var(--error)' : s.status === 'DONE' ? 'var(--success)' : s.status === 'IN_PROGRESS' ? 'var(--brand)' : s.status === 'SKIPPED' ? 'var(--border)' : 'var(--text-3)'
const fmtShort = (ms?: number | null) => (ms ? dayjs(ms).format('MM-DD HH:mm') : '')
const fmtPlan = (d?: string | null) => (d ? dayjs(d).format('MM-DD') : '—')

// List row = local registry item + backend status/type/priority/tags/overdue.
// Workspace tab key for creating a requirement (same pattern as the scenario page's NEW_KEY, no modal).
const NEW_REQ_KEY = '__new_requirement__'

type ReqRow = Omit<RegItem, 'label'> & {
  status?: string
  label: React.ReactNode
  /** Plain-text title for search/filter; label is a node carrying the coverage badge. */
  titleText: string
  /** Raw requirement, for the inline expanded preview. */
  raw?: Requirement
  reqType?: string
  priority?: string
  tags?: string[]
  overdue?: boolean
}


// Columns carry key/label for the column-settings panel; label is a badge-decorated node, search uses titleText.
// The module column shares key "module" with the filter field, so ListView.withHeaderFilter attaches the header funnel automatically.
function reqColumns(t: (k: string, d?: string) => string, modules: ApiModule[], moduleOf: (r: ReqRow) => string): ListColumn<ReqRow>[] {
  return [
    { key: 'title', label: t('req.title', '标题'), title: t('req.title', '标题'), dataIndex: 'label' },
    {
      key: 'module', label: t('req.module', '所属模块'), title: t('req.module', '所属模块'), width: 130,
      render: (_v: unknown, row: ReqRow) => {
        const m = moduleOf(row)
        return m ? (modules.find((x) => x.id === m)?.name ?? m) : <span style={{ color: 'var(--text-3)' }}>{t('req.moduleUnfiled', '未规划')}</span>
      },
    },
    { key: 'reqType', label: t('req.reqType', '类型'), title: t('req.reqType', '类型'), dataIndex: 'reqType', width: 96, render: (v?: string) => (v ? <Tag>{t(`req.type.${v}`, v)}</Tag> : '—') },
    { key: 'priority', label: t('req.priority', '优先级'), title: t('req.priority', '优先级'), dataIndex: 'priority', width: 100, render: (p?: string) => (p ? <Tag color={prioColor(p)}>{p}</Tag> : '—') },
    {
      key: 'tags', label: t('req.tags', '标签'), title: t('req.tags', '标签'), dataIndex: 'tags',
      render: (tags?: string[]) =>
        tags?.length ? (
          <>
            {tags.slice(0, 2).map((tg) => <Tag key={tg} style={{ marginRight: 4 }}>{tg}</Tag>)}
            {tags.length > 2 && <Tag style={{ marginRight: 0 }}>+{tags.length - 2}</Tag>}
          </>
        ) : '—',
    },
    {
      key: 'status', label: t('req.status', '状态'), title: t('req.status', '状态'), dataIndex: 'status', width: 160,
      render: (s: string | undefined, row: ReqRow) => (
        <>
          <Tag color={reqStatusColor(s)}>{s ? t(`req.status.${s}`, s) : '—'}</Tag>
          {/* Current pipeline stage, shown next to the gate status (DRAFT/baselined/delivered) */}
          {row.raw?.currentStage && (
            <span style={{ fontSize: 12, color: 'var(--brand)', marginRight: 6 }}>{t(`req.stage.${row.raw.currentStage}`, row.raw.currentStage)}</span>
          )}
          {row.overdue && <Tag color="red" style={{ marginRight: 0 }}>{t('req.overdue', '延期')}</Tag>}
        </>
      ),
    },
    { key: 'decomposed', label: t('req.decomposed', '已拆分'), title: t('req.decomposed', '已拆分'), dataIndex: 'meta', width: 90, render: (m?: Record<string, string>) => (m?.decompositionId ? <Tag color="geekblue">{t('req.yes', '是')}</Tag> : '—') },
  ]
}

// Requirements + orchestration in one page: list → detail tabs (info/versions/baseline/decomposition → task graph + runs + delivery + verification).
export default function Requirements() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const [items, setItems] = useState<ReqRow[]>([])
  // Shared project module tree (same tree as scenarios/cases): left panel + module column/filter.
  const [modules, setModules] = useState<ApiModule[]>([])
  const [selModule, setSelModule] = useState('ALL') // ALL | UNFILED | <moduleId>
  const [moduleSearch, setModuleSearch] = useState('')
  const tabs = useWorkTabs()
  // Requirement field template: resolves custom-field display names in the inline preview.
  const { fields: reqTplFields } = useFieldTemplate('requirement')

  const loadModules = () => {
    if (!projectId) { setModules([]); return }
    api.modules(projectId).then((mm) => setModules(Array.isArray(mm) ? mm : [])).catch(() => setModules([]))
  }
  // Row module = backend moduleId, but only if it still exists in the tree (deleted modules fall back to unfiled).
  const moduleOf = (r: ReqRow) => {
    const m = r.raw?.moduleId || ''
    return modules.some((x) => x.id === m) ? m : ''
  }

  // Backend is the source of truth (includes requirements created via CLI/API), overlaid with local registry meta (decomposition/verification links).
  const loadList = async () => {
    const local = regList('requirement', projectId)
    const localById = new Map(local.map((r) => [r.id, r]))
    try {
      const page = await api.requirements(projectId)
      // Fetch coverage for all requirements in parallel to badge row labels (lists are usually small).
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
        return { ...base, status: r.status, reqType: r.reqType, priority: r.priority, tags: r.tags, overdue: r.overdue, label, titleText: r.title, raw: r }
      }))
    } catch {
      setItems(local.map((r) => ({ ...r, titleText: String(r.label) }))) // backend unavailable: fall back to local registry
    }
  }
  useEffect(() => {
    loadList()
    loadModules()
    setSelModule('ALL')
    tabs.reset()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId])

  // List view/filter/column-settings trio; useListView must run before any conditional return (hook order).
  const allTags = [...new Set(items.flatMap((r) => r.tags ?? []))]
  const lv = useListView<ReqRow>({
    kind: 'requirement',
    projectId,
    searchOf: (r) => r.titleText,
    searchLabel: t('req.searchPh', '搜索标题'),
    systemViews: [
      { key: 'mine', label: t('lv.mine', '我创建的'), pred: (r) => !!r.raw?.createdBy && r.raw.createdBy === userIdStore.get() },
    ],
    fields: [
      {
        key: 'status', label: t('req.status', '状态'), type: 'enum',
        options: ['DRAFT', 'BASELINED', 'DELIVERED', 'ARCHIVED'].map((v) => ({ value: v, label: t(`req.status.${v}`, v) })),
        get: (r) => r.status,
      },
      {
        key: 'reqType', label: t('req.reqType', '类型'), type: 'enum',
        options: ['FEATURE', 'ENHANCEMENT', 'TECH_DEBT', 'BUGFIX'].map((v) => ({ value: v, label: t(`req.type.${v}`, v) })),
        get: (r) => r.reqType,
      },
      {
        key: 'priority', label: t('req.priority', '优先级'), type: 'enum',
        options: ['P0', 'P1', 'P2', 'P3'].map((v) => ({ value: v, label: v })),
        get: (r) => r.priority,
      },
      {
        key: 'tags', label: t('req.tags', '标签'), type: 'tags',
        options: allTags.map((v) => ({ value: v, label: v })),
        get: (r) => r.tags ?? [],
      },
      {
        key: 'module', label: t('req.module', '所属模块'), type: 'enum',
        options: [{ value: '', label: t('req.moduleUnfiled', '未规划') }, ...modules.map((m) => ({ value: m.id, label: m.name }))],
        get: (r) => moduleOf(r),
      },
      { key: 'overdue', label: t('req.overdueOnly', '仅看延期'), type: 'bool', get: (r) => r.overdue === true },
      // Advanced-condition-only fields (duplicate the search box/columns; not rendered in the declarative filter bar).
      { key: 'title', label: t('req.colTitle', '标题'), type: 'text', advOnly: true, get: (r) => r.titleText },
      {
        key: 'currentStage', label: t('req.currentStage', '当前阶段'), type: 'enum', advOnly: true,
        options: ['CREATED', 'AUDIT', 'REVIEW', 'DEV', 'TEST', 'ACCEPTANCE', 'DELIVERY'].map((s) => ({ value: s, label: t(`req.stage.${s}`, s) })),
        get: (r) => r.raw?.currentStage || '',
      },
      { key: 'createdBy', label: t('lv.createdBy', '创建人'), type: 'text', advOnly: true, get: (r) => r.raw?.createdBy || '' },
    ],
    columns: reqColumns(t, modules, moduleOf),
    // Left-tree filter (selecting a parent includes children) applies before views/filters; tree counts use the full item set to stay consistent with the list.
    rows: items.filter((r) => inSelectedModule(modules, selModule, moduleOf(r))),
  })

  // Inline expanded preview: key info without leaving the list; open the tab for deeper editing.
  // Compact stage strip: 7 dots colored by status, current stage highlighted with its name, overdue in red.
  const stageStrip = (r: Requirement) => (
    <div style={{ display: 'flex', gap: 6, alignItems: 'center', flexWrap: 'wrap', marginBottom: 8 }}>
      {stagesOf(r).map((s) => {
        const cur = s.stage === r.currentStage
        return (
          <span
            key={s.stage}
            title={`${t(`req.stage.${s.stage}`, s.stage)} · ${t(`req.ss.${s.status}`, s.status)}`}
            style={{
              display: 'inline-flex', alignItems: 'center', gap: 4, fontSize: 12, lineHeight: '18px',
              padding: cur ? '0 8px' : 0, borderRadius: 10,
              border: cur ? '1px solid var(--brand)' : 'none',
              background: cur ? 'var(--brand-soft)' : 'transparent',
              color: s.overdue ? 'var(--error)' : 'var(--brand)',
            }}
          >
            <span style={{ width: 8, height: 8, borderRadius: '50%', background: stageDotColor(s), display: 'inline-block' }} />
            {cur && <span>{t(`req.stage.${s.stage}`, s.stage)}</span>}
          </span>
        )
      })}
    </div>
  )
  const rowPreview = (row: ReqRow) => {
    const r = row.raw
    if (!r) return null
    const crits = critsOf(r)
    return (
      <div style={{ padding: '4px 8px', display: 'flex', gap: 32, flexWrap: 'wrap' }}>
        <div style={{ flex: '2 1 320px', minWidth: 280 }}>
          <div style={{ fontSize: 12, color: 'var(--text-3)', marginBottom: 4 }}>{t('req.description', '需求描述')}</div>
          <div style={{ fontSize: 13, color: 'var(--text-2)' }}>
            <MarkdownRenderer
              value={r.versions?.find((v) => v.version === r.baselineVersion)?.description || r.versions?.[r.versions.length - 1]?.description || ''}
            />
          </div>
          <div style={{ fontSize: 12, color: 'var(--text-3)', margin: '10px 0 4px' }}>{t('req.criteriaPlain', '验收标准')}</div>
          {crits.length ? (
            <ol style={{ margin: 0, paddingLeft: 18, fontSize: 13, color: 'var(--text-2)' }}>
              {crits.map((c, i) => <li key={i}>{c}</li>)}
            </ol>
          ) : '—'}
        </div>
        <div style={{ flex: '1 1 220px', minWidth: 200 }}>
          <div style={{ fontSize: 12, color: 'var(--text-3)', marginBottom: 4 }}>{t('req.stagePanel', '阶段进度')}</div>
          {stageStrip(r)}
          <div style={{ fontSize: 12, color: 'var(--text-3)', lineHeight: 1.9 }}>
            {r.dueDate && <div>{t('req.dueDate', '截止日期')}:{r.dueDate}{r.overdue && <Tag color="red" style={{ marginLeft: 6 }}>{t('req.overdue', '延期')}</Tag>}</div>}
            {r.createdAt ? <div>{t('req.createdAt', '创建时间')}:{new Date(r.createdAt).toLocaleString()}</div> : null}
            {r.updatedAt ? <div>{t('req.updatedAt', '更新时间')}:{new Date(r.updatedAt).toLocaleString()}</div> : null}
          </div>
          {/* Custom fields: names resolved from the field template, falling back to the raw key. */}
          {r.customFields && Object.keys(r.customFields).length > 0 && (
            <div style={{ fontSize: 12, color: 'var(--text-3)', lineHeight: 1.9, marginTop: 6 }}>
              {Object.entries(r.customFields).map(([k, v]) => {
                const f = reqTplFields.find((x) => !x.system && x.key === k)
                return <div key={k}>{f ? fieldLabel(t, 'requirement', f) : k}:<span style={{ color: 'var(--text-2)', marginLeft: 4 }}>{v}</span></div>
              })}
            </div>
          )}
          <Button size="small" type="primary" ghost style={{ marginTop: 8 }} onClick={() => tabs.open(row.id)}>
            {t('req.openDetail', '打开完整详情')}
          </Button>
        </div>
      </div>
    )
  }

  if (!projectId) return <SelectProjectEmpty />

  // Left: shared module tree (same ModuleTreePanel as the scenario page; Workspace wraps it in ResizableSider).
  const left = (
    <ModuleTreePanel
      projectId={projectId}
      modules={modules}
      items={items}
      getModuleId={moduleOf}
      selectedKey={selModule}
      onSelect={setSelModule}
      allLabel={t('req.allRequirements', '全部需求')}
      unfiledLabel={t('req.moduleUnfiled', '未规划')}
      moduleSearch={moduleSearch}
      onModuleSearch={setModuleSearch}
      searchPlaceholder={t('req.moduleSearchPh', '请输入模块名称进行搜索')}
      onModulesChanged={loadModules}
      deleteModuleContent={t('req.deleteModuleContent', '其下需求将变为未规划(不会删除需求)。')}
    />
  )
  // New requirements default to the module selected in the tree (ALL/UNFILED count as unfiled).
  const defaultModuleId = selModule !== 'ALL' && selModule !== 'UNFILED' ? selModule : ''

  // Creation runs in a workspace tab (like the scenario page, no modal); on success close it and open the detail tab.
  const detailTabs = [
    ...(tabs.openIds.includes(NEW_REQ_KEY)
      ? [{
          key: NEW_REQ_KEY,
          label: t('req.new', '新建需求'),
          children: (
            <div style={{ padding: 16, overflow: 'auto', height: '100%' }}>
              <CreateRequirementForm
                projectId={projectId}
                modules={modules}
                defaultModuleId={defaultModuleId}
                onDone={(r, title) => {
                  regAdd('requirement', projectId, { id: r.id, label: title, createdAt: Date.now() })
                  loadList()
                  tabs.close(NEW_REQ_KEY)
                  tabs.open(r.id)
                }}
              />
            </div>
          ),
        }]
      : []),
    ...items
      .filter((r) => tabs.openIds.includes(r.id))
      .map((r) => ({
        key: r.id,
        label: r.label,
        children: <RequirementDetail key={r.id} reqId={r.id} projectId={projectId} modules={modules} onChanged={loadList} onDeleted={() => { tabs.close(r.id); loadList() }} onOpen={(id) => tabs.open(id)} />,
      })),
  ]

  return (
    <Workspace
      left={left}
      leftWidth={240}
      siderKey="requirement-sider"
      listLabel={t('req.allRequirements', '全部需求')}
      activeKey={tabs.activeKey}
      onChange={tabs.setActiveKey}
      onClose={tabs.close}
      tabs={detailTabs}
      listContent={
        <WorkList<ReqRow>
          onNew={() => tabs.open(NEW_REQ_KEY)}
          newLabel={t('req.new', '新建需求')}
          extraActions={lv.toolbar}
          data={lv.rows}
          pagination={lv.pagination}
          onRowClick={(r) => tabs.open(r.id)}
          emptyText={t('req.empty', '暂无需求')}
          columns={lv.columns}
          expandable={{ expandedRowRender: rowPreview, rowExpandable: (r) => !!r.raw }}
        />
      }
    />
  )
}

// Create requirement: AI drafting (MRD/raw material → structured draft backfill) + a form rendered from the field template.
// System field order/required/visibility comes from the requirement field template (Project → Template management); custom fields are appended dynamically.
const REQ_TYPES = ['FEATURE', 'ENHANCEMENT', 'TECH_DEBT', 'BUGFIX'] as const
const PRIORITIES = ['P0', 'P1', 'P2', 'P3'] as const

function CreateRequirementForm({ projectId, modules, defaultModuleId, onDone }: { projectId: string; modules: ApiModule[]; defaultModuleId?: string; onDone: (r: Requirement, title: string) => void }) {
  const { t } = useI18n()
  const [form] = Form.useForm()
  const [raw, setRaw] = useState('')
  const [drafting, setDrafting] = useState(false)
  // Field template drives system-field order/required/visibility; custom fields render by type.
  const { fields: tplFields } = useFieldTemplate('requirement')
  // Parent candidates: existing requirements in the project (fail silently, empty dropdown is fine).
  const [parents, setParents] = useState<Requirement[]>([])
  useEffect(() => {
    api.requirements(projectId).then((p) => setParents(p.items)).catch(() => setParents([]))
  }, [projectId])
  // Project skills: selected at creation and composed into agent instructions on dispatch.
  const [skills, setSkills] = useState<Skill[]>([])
  useEffect(() => {
    if (!projectId) return
    api.skills(projectId).then(setSkills).catch(() => setSkills([]))
  }, [projectId])
  const typeLabel: Record<string, string> = {
    FEATURE: t('req.type.FEATURE', '功能'),
    ENHANCEMENT: t('req.type.ENHANCEMENT', '优化'),
    TECH_DEBT: t('req.type.TECH_DEBT', '技术债'),
    BUGFIX: t('req.type.BUGFIX', '缺陷修复'),
  }
  const draft = async () => {
    if (!raw.trim()) return
    setDrafting(true)
    try {
      const d = await api.draftRequirement(raw)
      form.setFieldsValue({
        title: d.title,
        description: d.description,
        priority: d.priority,
        criteria: d.acceptanceCriteria.length ? d.acceptanceCriteria : [''],
      })
      message.success(d.source === 'llm' ? t('req.draftedByAi', 'AI 已起草,请复核修改') : t('req.draftedHeuristic', '已按格式整理(未配置 LLM),请补充'))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('req.draftFailed', '起草失败'))
    } finally {
      setDrafting(false)
    }
  }
  // System field renderer: key → form item; required follows the template (title is always required).
  const sysItem = (f: TemplateField) => {
    const rules = f.required ? [{ required: true }] : undefined
    switch (f.key) {
      case 'title':
        return (
          <Form.Item key="title" name="title" label={t('req.title', '标题')} rules={[{ required: true }]}>
            <Input placeholder={t('req.titlePlaceholder', '如:用户登录')} />
          </Form.Item>
        )
      case 'reqType':
        return (
          <Form.Item key="reqType" name="reqType" label={t('req.reqType', '类型')} rules={rules}>
            <Select options={REQ_TYPES.map((k) => ({ value: k, label: typeLabel[k] }))} />
          </Form.Item>
        )
      case 'priority':
        return (
          <Form.Item key="priority" name="priority" label={t('req.priority', '优先级')} rules={rules}>
            <Select options={PRIORITIES.map((p) => ({ value: p, label: p }))} />
          </Form.Item>
        )
      case 'tags':
        return (
          <Form.Item key="tags" name="tags" label={t('req.tags', '标签')} rules={rules}>
            <Select mode="tags" maxCount={10} tokenSeparators={[',', ' ']} open={false} suffixIcon={null} placeholder={t('req.tagsPh', '回车添加,最多 10 个')} />
          </Form.Item>
        )
      case 'dueDate':
        return (
          <Form.Item key="dueDate" name="dueDate" label={t('req.dueDate', '截止日期')} rules={rules}>
            <DatePicker format="YYYY-MM-DD" style={{ width: '100%' }} />
          </Form.Item>
        )
      case 'parentId':
        return (
          <Form.Item key="parentId" name="parentId" label={t('req.parentReq', '父需求')} rules={rules}>
            <Select
              allowClear
              showSearch
              optionFilterProp="label"
              placeholder={t('req.parentPh', '可选:挂到某个已有需求下')}
              options={parents.map((p) => ({ value: p.id, label: p.title }))}
            />
          </Form.Item>
        )
      case 'description':
        return (
          <Form.Item key="description" name="description" label={t('req.description', '需求描述')} rules={rules}>
            <MarkdownEditor projectId={projectId} placeholder={t('req.descriptionPh', '背景:为什么做\n目标:做成什么样\n范围:边界与不做什么')} />
          </Form.Item>
        )
      case 'criteria':
        // Criteria list renders specially (one entry per item); "required" is validated in onFinish (at least one).
        return (
          <Form.List key="criteria" name="criteria">
            {(fields, { add, remove }) => (
              <Form.Item label={t('req.criteriaList', '验收标准(逐条,可判定)')} required={f.required}>
                {fields.map(({ key, ...field }) => (
                  <div key={key} style={{ display: 'flex', gap: 8, marginBottom: 8 }}>
                    <Form.Item {...field} noStyle>
                      <Input placeholder={t('req.criterionPh', '一条可判定的验收条件,如:错误密码提示明确')} />
                    </Form.Item>
                    {fields.length > 1 && (
                      <Button type="text" icon={<DeleteOutlined />} onClick={() => remove(field.name)} />
                    )}
                  </div>
                ))}
                <Button type="dashed" block onClick={() => add('')}>
                  + {t('req.addCriterion', '添加验收标准')}
                </Button>
              </Form.Item>
            )}
          </Form.List>
        )
      default:
        return null
    }
  }
  // Split template fields into a left "attributes" column (compact inputs) and a right
  // "content" column (description + acceptance criteria, the tall editors) so the form
  // lays out side-by-side and avoids long vertical scrolling on the create screen.
  const isContentField = (k: string) => k === 'description' || k === 'criteria'
  const leftFields = tplFields.filter((f) => f.enabled && !isContentField(f.key))
  const rightFields = tplFields.filter((f) => f.enabled && isContentField(f.key))

  return (
    <Form
      form={form}
      layout="vertical"
      initialValues={{ reqType: 'FEATURE', priority: 'P2', criteria: [''], moduleId: defaultModuleId ?? '' }}
      onFinish={async (v: { title: string; description?: string; reqType?: string; priority?: string; criteria?: string[]; tags?: string[]; dueDate?: Dayjs; parentId?: string; moduleId?: string; skillIds?: string[]; [CF_GROUP]?: Record<string, unknown> }) => {
        const acceptanceCriteria = (v.criteria || []).map((c) => (c || '').trim()).filter(Boolean)
        const critField = tplFields.find((f) => f.key === 'criteria')
        if (critField?.enabled && critField.required && !acceptanceCriteria.length) {
          message.warning(t('req.criteriaRequired', '请至少填写一条验收标准'))
          return
        }
        const customFields = collectCustomValues(tplFields, v[CF_GROUP])
        try {
          const r = await api.createRequirement({
            projectId,
            title: v.title,
            description: v.description?.trim() || undefined,
            acceptanceCriteria,
            priority: v.priority,
            reqType: v.reqType,
            tags: v.tags?.length ? v.tags : undefined,
            dueDate: v.dueDate ? v.dueDate.format('YYYY-MM-DD') : undefined,
            parentId: v.parentId || undefined,
            customFields: Object.keys(customFields).length ? customFields : undefined,
            moduleId: v.moduleId || undefined,
            skillIds: v.skillIds?.length ? v.skillIds : undefined,
          })
          message.success(t('req.created', '需求已创建'))
          onDone(r, v.title)
        } catch (e) {
          message.error(e instanceof ApiError ? e.message : t('req.createFailed', '创建失败'))
        }
      }}
    >
      {/* AI drafting (MRD → PRD): paste raw material, the draft backfills the form and stays editable. */}
      <div style={{ background: 'var(--panel-2)', border: '1px solid var(--border-soft)', borderRadius: 8, padding: 12, marginBottom: 16 }}>
        <div style={{ fontSize: 13, fontWeight: 600, marginBottom: 8 }}>{t('req.aiDraft', 'AI 起草(MRD 自动转 PRD)')}</div>
        <Input.TextArea
          rows={3}
          value={raw}
          onChange={(e) => setRaw(e.target.value)}
          placeholder={t('req.aiDraftPh', '粘贴 MRD/会议纪要/原始想法,AI 整理为标题、描述与逐条验收标准')}
        />
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginTop: 8 }}>
          <span style={{ fontSize: 12, color: 'var(--text-3)' }}>{t('req.aiDraftHint', '结果回填下方表单,创建前可修改')}</span>
          <Button size="small" type="primary" ghost loading={drafting} disabled={!raw.trim()} onClick={draft}>
            {t('req.aiDraftBtn', '起草')}
          </Button>
        </div>
      </div>
      <Row gutter={[20, 0]} align="top">
        {/* Left column: module + compact attribute fields + custom fields. */}
        <Col xs={24} md={11}>
          <Form.Item name="moduleId" label={t('req.module', '所属模块')}>
            <Select
              showSearch
              optionFilterProp="label"
              options={[{ value: '', label: t('req.moduleUnfiled', '未规划') }, ...modules.map((m) => ({ value: m.id, label: m.name }))]}
            />
          </Form.Item>
          <Form.Item name="skillIds" label={t('req.skills', '关联技能')}>
            <Select
              mode="multiple"
              allowClear
              showSearch
              optionFilterProp="label"
              maxTagCount="responsive"
              placeholder={t('req.skillsPh', '选择本项目技能,派发时下发到 agent')}
              options={skills.map((s) => ({ value: s.id, label: s.name }))}
            />
          </Form.Item>
          {leftFields.map((f) =>
            f.system ? sysItem(f) : <CustomFieldItem key={f.key} kind="requirement" field={f} />
          )}
        </Col>
        {/* Right column: the tall editors — description (markdown) + acceptance criteria. */}
        <Col xs={24} md={13}>
          {rightFields.map((f) =>
            f.system ? sysItem(f) : <CustomFieldItem key={f.key} kind="requirement" field={f} />
          )}
        </Col>
      </Row>
      <Button type="primary" htmlType="submit" block>
        {t('a.create', '创建')}
      </Button>
    </Form>
  )
}

// Functional-case coverage: per acceptance criterion, linked functional cases (add/remove) + coverage rate — the manual requirement → criterion → case chain.
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

// Stage pipeline panel: 7 cards in a row, current stage highlighted (brand border + soft background).
// Each card shows status / planned window / actual start-end / overdue; clicking opens transitions (start/done/skip) + scheduling (clearing a date sends "").
// CREATED is system-managed read-only; review/acceptance/delivery are driven by baseline/deliver actions but keep a manual override.
function StagePipeline({ req, onAction }: { req: Requirement; onAction: (stage: string, b: { status?: string; plannedStart?: string; plannedEnd?: string }) => void }) {
  const { t } = useI18n()
  return (
    <div style={{ marginTop: 12 }}>
      <div style={{ fontSize: 13, fontWeight: 600, marginBottom: 8 }}>{t('req.stagePanel', '阶段进度')}</div>
      <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
        {stagesOf(req).map((s) => {
          const cur = s.stage === req.currentStage
          const editable = s.stage !== 'CREATED' // CREATED is system-managed, read-only
          const card = (
            <div
              style={{
                flex: '1 1 128px', minWidth: 128, padding: '8px 10px', borderRadius: 8,
                border: `1px solid ${cur ? 'var(--brand)' : 'var(--border-soft)'}`,
                background: cur ? 'var(--brand-soft)' : 'var(--panel)',
                cursor: editable ? 'pointer' : 'default',
              }}
            >
              <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 6 }}>
                {s.overdue && <span style={{ width: 6, height: 6, borderRadius: '50%', background: 'var(--error)', display: 'inline-block' }} />}
                <span style={{ fontWeight: 600, fontSize: 13, color: cur ? 'var(--brand)' : 'var(--text)' }}>{t(`req.stage.${s.stage}`, s.stage)}</span>
              </div>
              <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap', marginBottom: 6 }}>
                <Tag color={stageTagColor(s.status)} style={{ marginRight: 0 }}>{t(`req.ss.${s.status}`, s.status)}</Tag>
                {s.overdue && <Tag color="red" style={{ marginRight: 0 }}>{t('req.overdue', '延期')}</Tag>}
              </div>
              <div style={{ fontSize: 12, color: 'var(--text-3)' }}>
                {t('req.plannedWindow', '计划窗口')}:{s.plannedStart || s.plannedEnd ? `${fmtPlan(s.plannedStart)} ~ ${fmtPlan(s.plannedEnd)}` : '—'}
              </div>
              {(s.startedAt || s.finishedAt) && (
                <div style={{ fontSize: 12, color: 'var(--text-3)' }}>
                  {t('req.actualWindow', '实际起止')}:{fmtShort(s.startedAt) || '—'} ~ {fmtShort(s.finishedAt) || '—'}
                </div>
              )}
            </div>
          )
          if (!editable) {
            return <Tooltip key={s.stage} title={t('req.stageSystem', '系统管理:创建需求时自动完成')}>{card}</Tooltip>
          }
          return (
            <Popover
              key={s.stage}
              trigger="click"
              title={t(`req.stage.${s.stage}`, s.stage)}
              content={
                <Space direction="vertical" size={8}>
                  <Space size={6}>
                    <Button size="small" disabled={s.status === 'IN_PROGRESS'} onClick={() => onAction(s.stage, { status: 'IN_PROGRESS' })}>{t('req.stageStart', '开始')}</Button>
                    <Button size="small" type="primary" ghost disabled={s.status === 'DONE'} onClick={() => onAction(s.stage, { status: 'DONE' })}>{t('req.stageDone', '完成')}</Button>
                    <Button size="small" disabled={s.status === 'SKIPPED'} onClick={() => onAction(s.stage, { status: 'SKIPPED' })}>{t('req.stageSkip', '跳过')}</Button>
                  </Space>
                  {/* Clearing a date sends "" so the backend clears it */}
                  <DatePicker
                    size="small" style={{ width: 190 }} allowClear placeholder={t('req.plannedStart', '计划开始')}
                    value={s.plannedStart ? dayjs(s.plannedStart) : null}
                    onChange={(d) => onAction(s.stage, { plannedStart: d ? d.format('YYYY-MM-DD') : '' })}
                  />
                  <DatePicker
                    size="small" style={{ width: 190 }} allowClear placeholder={t('req.plannedEnd', '计划结束')}
                    value={s.plannedEnd ? dayjs(s.plannedEnd) : null}
                    onChange={(d) => onAction(s.stage, { plannedEnd: d ? d.format('YYYY-MM-DD') : '' })}
                  />
                </Space>
              }
            >
              {card}
            </Popover>
          )
        })}
      </div>
    </div>
  )
}

function RequirementDetail({ reqId, projectId, modules, onChanged, onDeleted, onOpen }: { reqId: string; projectId: string; modules: ApiModule[]; onChanged: () => void; onDeleted: () => void; onOpen?: (id: string) => void }) {
  const { t } = useI18n()
  const [req, setReq] = useState<Requirement | null>(null)
  // Field template: the edit modal renders custom fields (prefill current values; submit replaces the whole map).
  const { fields: tplFields } = useFieldTemplate('requirement')
  const [cov, setCov] = useState<CoverageCase[]>([])
  const [verOpen, setVerOpen] = useState(false)
  const [verView, setVerView] = useState<RequirementVersion | null>(null) // version detail being viewed
  // Children / link candidates (all project requirements) / change-history drawer.
  const [children, setChildren] = useState<Requirement[]>([])
  const [allReqs, setAllReqs] = useState<Requirement[]>([])
  const [childPick, setChildPick] = useState<string>()
  const [changesOpen, setChangesOpen] = useState(false)
  const [changes, setChanges] = useState<RequirementChange[]>([])
  const reg = regList('requirement', projectId).find((r) => r.id === reqId)
  const [decompId, setDecompId] = useState<string | undefined>(undefined)
  const [verId, setVerId] = useState<string | undefined>(undefined)
  // Version currently decomposed / shown in the orchestration tab; defaults to the baseline, switchable via the picker.
  const [splitVersion, setSplitVersion] = useState<number | undefined>(undefined)

  const loadChildren = () => api.requirementChildren(reqId).then((r) => setChildren(r.items)).catch(() => setChildren([]))
  const load = async () => {
    try {
      const r = await api.getRequirement(reqId)
      setReq(r)
      // Restore the orchestration tab from the backend (not browser-local state) so 拆分/交付/验证
      // survive switching browsers. Show the decomposition for the selected version (default baseline).
      const v = splitVersion ?? r.baselineVersion
      setSplitVersion(v)
      api.requirementBreakdown(reqId, v).then((d) => {
        setDecompId(d.id)
        setVerId(d.verificationId)
      }).catch(() => undefined)
      api.requirementCoverage(reqId).then(setCov).catch(() => setCov([]))
      loadChildren()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('req.loadFailed', '加载需求失败'))
    }
  }
  useEffect(() => {
    load()
    api.requirements(projectId).then((p) => setAllReqs(p.items)).catch(() => setAllReqs([]))
    // Re-run when the requirement or the selected decomposition version changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reqId, splitVersion])

  // Stage transition/scheduling: PUT /requirement/:id/stage/:stage, then refresh detail + list (actual timestamps are recorded by the backend).
  const setStage = async (stage: string, b: { status?: string; plannedStart?: string; plannedEnd?: string }) => {
    try {
      await api.setRequirementStage(reqId, stage, b)
      load()
      onChanged()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('req.setStageFailed', '阶段更新失败'))
    }
  }
  // Linking/unlinking a child mutates the child's parentId.
  const linkChild = async () => {
    if (!childPick) return
    try {
      await api.setRequirementParent(childPick, reqId)
      setChildPick(undefined)
      message.success(t('req.linked', '已关联'))
      loadChildren()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('req.linkFailed', '关联失败'))
    }
  }
  const unlinkChild = async (childId: string) => {
    try {
      await api.setRequirementParent(childId, null)
      loadChildren()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('req.unlinkFailed', '解除失败'))
    }
  }
  const openChanges = () => {
    setChangesOpen(true)
    api.requirementChanges(reqId).then((r) => setChanges(r.items)).catch(() => setChanges([]))
  }

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

  // Lifecycle actions: edit / deliver / archive / delete. The backend guards status transitions; failures surface to the user.
  const [editOpen, setEditOpen] = useState(false)
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
    const version = splitVersion ?? req?.baselineVersion
    try {
      const r = await api.breakdown(reqId, version)
      message.success(`${t('req.decomposedTo', '已拆分')}:${r.tasks.length} ${t('req.tasksUnit', '个任务')} (v${version ?? '-'})`)
      regAdd('requirement', projectId, { id: reqId, label: req?.title || reqId, createdAt: reg?.createdAt || Date.now(), meta: { decompositionId: r.id, verificationId: r.verificationId } })
      setDecompId(r.id)
      setVerId(r.verificationId)
      onChanged()
    } catch (e) {
      message.error(e instanceof ApiError ? `${t('req.decomposeFailed', '拆分失败')}:${e.status}` : t('req.decomposeFailed', '拆分失败'))
    }
  }

  // Criteria live in versions[].acceptanceCriteria; prefer the baseline version, fall back to latest version / top-level field.
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
                  <Button icon={<EditOutlined />} size="small" onClick={() => setEditOpen(true)}>{t('a.edit', '编辑')}</Button>
                  <Button icon={<BranchesOutlined />} size="small" disabled={req?.status === 'ARCHIVED'} onClick={() => setVerOpen(true)}>{t('req.addVersion', '新增版本')}</Button>
                  <Button icon={<FlagOutlined />} size="small" disabled={req?.status === 'ARCHIVED'} onClick={setBaseline}>{t('req.setBaseline', '定基线')}</Button>
                  <Select
                    size="small"
                    style={{ width: 120 }}
                    value={req ? splitVersion ?? req.baselineVersion : undefined}
                    disabled={!req?.versions?.length}
                    onChange={(v) => setSplitVersion(v)}
                    options={[
                      ...(req?.baselineVersion != null ? [{ value: req.baselineVersion, label: `v${req.baselineVersion} (${t('req.baseline', '基线')})` }] : []),
                      ...(req?.versions ?? [])
                        .filter((v) => v.version !== req?.baselineVersion)
                        .map((v) => ({ value: v.version, label: `v${v.version}` })),
                    ]}
                  />
                  <Button type="primary" icon={<PartitionOutlined />} size="small" onClick={doBreakdown}>{t('req.autoDecompose', '自动拆分')}</Button>
                  <Button icon={<SendOutlined />} size="small" disabled={!(req?.status === 'BASELINED' || req?.status === 'DELIVERED')} onClick={deliver}>{t('req.deliver', '交付')}</Button>
                  <Button icon={<InboxOutlined />} size="small" disabled={req?.status === 'ARCHIVED'} onClick={archive}>{t('req.archive', '归档')}</Button>
                  <Button danger icon={<DeleteOutlined />} size="small" onClick={del}>{t('a.delete', '删除')}</Button>
                  <Button icon={<HistoryOutlined />} size="small" onClick={openChanges}>{t('req.changes', '变更记录')}</Button>
                  {(() => {
                    const n = baselineCriteria.filter((_, i) => cov.some((c) => c.criterionIndex === i)).length
                    const pct = baselineCriteria.length ? Math.round((n / baselineCriteria.length) * 100) : 0
                    return <Tag color={pct === 100 ? 'green' : pct > 0 ? 'gold' : 'default'}>{t('req.coverageRate', '覆盖率')} {n}/{baselineCriteria.length} ({pct}%)</Tag>
                  })()}
                </Space>
                <Descriptions
                  column={1}
                  size="small"
                  bordered
                  labelStyle={{ width: 120, minWidth: 120, whiteSpace: 'nowrap' }}
                  contentStyle={{ width: 'auto', whiteSpace: 'normal' }}
                >
                  <Descriptions.Item label={t('req.title', '标题')}>{req?.title}</Descriptions.Item>
                  <Descriptions.Item label={t('req.baselineVersion', '基线版本')}>v{req?.baselineVersion}</Descriptions.Item>
                  <Descriptions.Item label={t('req.status', '状态')}>{req?.status ? <Tag color={reqStatusColor(req.status)}>{t(`req.status.${req.status}`, req.status)}</Tag> : '—'}</Descriptions.Item>
                  <Descriptions.Item label={t('req.reqType', '类型')}>{req?.reqType ? <Tag>{t(`req.type.${req.reqType}`, req.reqType)}</Tag> : '—'}</Descriptions.Item>
                  <Descriptions.Item label={t('req.priority', '优先级')}>{req?.priority ? <Tag color={prioColor(req.priority)}>{req.priority}</Tag> : '—'}</Descriptions.Item>
                  <Descriptions.Item label={t('req.description', '需求描述')}>
                    {req ? <MarkdownRenderer value={req.versions?.find((v) => v.version === req.baselineVersion)?.description || req.versions?.[req.versions.length - 1]?.description || ''} /> : '—'}
                  </Descriptions.Item>
                  <Descriptions.Item label={t('req.tags', '标签')}>
                    {req?.tags?.length ? req.tags.map((tg) => <Tag key={tg}>{tg}</Tag>) : '—'}
                  </Descriptions.Item>
                  <Descriptions.Item label={t('req.dueDate', '截止日期')}>
                    {req?.dueDate ? (
                      <span>
                        {req.dueDate}
                        {req.overdue && <Tag color="red" style={{ marginLeft: 6 }}>{t('req.overdue', '延期')}</Tag>}
                      </span>
                    ) : '—'}
                  </Descriptions.Item>
                  <Descriptions.Item label={t('req.createdAt', '创建时间')}>{req?.createdAt ? new Date(req.createdAt).toLocaleString() : '—'}</Descriptions.Item>
                  <Descriptions.Item label={t('req.updatedAt', '更新时间')}>{req?.updatedAt ? new Date(req.updatedAt).toLocaleString() : '—'}</Descriptions.Item>
                  <Descriptions.Item label={t('req.acceptanceCriteria', '验收标准')}>
                    {baselineCriteria.length ? (
                      <ul style={{ margin: 0, paddingLeft: 18 }}>{baselineCriteria.map((c, i) => <li key={i}>{c}</li>)}</ul>
                    ) : '—'}
                  </Descriptions.Item>
                </Descriptions>
                {/* Stage progress: 7 pipeline cards, current stage highlighted; click to transition (start/done/skip) and schedule planned dates. */}
                {req && <StagePipeline req={req} onAction={setStage} />}
                {/* Children: requirements under this one (open/unlink), plus linking others in. */}
                <Card size="small" title={`${t('req.children', '子需求')} (${children.length})`} style={{ marginTop: 12 }}>
                  <Space.Compact style={{ width: '100%', marginBottom: children.length ? 10 : 0 }}>
                    <Select
                      size="small"
                      style={{ flex: 1 }}
                      allowClear
                      showSearch
                      optionFilterProp="label"
                      placeholder={t('req.linkChildPh', '选择需求挂到本需求下')}
                      value={childPick}
                      onChange={setChildPick}
                      options={allReqs
                        .filter((r) => r.id !== reqId && !children.some((c) => c.id === r.id))
                        .map((r) => ({ value: r.id, label: r.title }))}
                    />
                    <Button size="small" type="primary" disabled={!childPick} onClick={linkChild}>{t('req.linkChild', '关联子需求')}</Button>
                  </Space.Compact>
                  {children.map((c) => (
                    <div key={c.id} style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '4px 0', borderTop: '1px solid var(--border-soft)' }}>
                      <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{c.title}</span>
                      <Tag color={reqStatusColor(c.status)} style={{ marginRight: 0 }}>{t(`req.status.${c.status}`, c.status)}</Tag>
                      {onOpen && <Button type="link" size="small" onClick={() => onOpen(c.id)}>{t('req.open', '打开')}</Button>}
                      <Button type="link" size="small" danger onClick={() => unlinkChild(c.id)}>{t('req.unlink', '解除')}</Button>
                    </div>
                  ))}
                </Card>
                {/* Version history: "view" fetches GET /requirement/:id/version/:n. */}
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
              <DecompositionView decompId={decompId} verificationId={verId} projectId={projectId} reqId={reqId} req={req || undefined} />
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
              setSplitVersion(r.version)
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
      <Modal title={t('req.editInfo', '编辑需求信息')} open={editOpen} onCancel={() => setEditOpen(false)} footer={null} destroyOnHidden>
        <Form
          layout="vertical"
          initialValues={{
            title: req?.title,
            reqType: req?.reqType || 'FEATURE',
            priority: req?.priority || 'P2',
            tags: req?.tags || [],
            dueDate: req?.dueDate ? dayjs(req.dueDate) : undefined,
            // Module: fall back to unfiled when deleted or unset.
            moduleId: req?.moduleId && modules.some((m) => m.id === req.moduleId) ? req.moduleId : '',
            [CF_GROUP]: customFormValues(tplFields, req?.customFields),
          }}
          onFinish={async (v: { title: string; reqType: string; priority: string; tags: string[]; dueDate?: ReturnType<typeof dayjs>; moduleId?: string; [CF_GROUP]?: Record<string, unknown> }) => {
            try {
              await api.updateRequirement(reqId, {
                title: v.title.trim(),
                reqType: v.reqType,
                priority: v.priority,
                tags: v.tags,
                // empty string clears the due date (backend convention)
                dueDate: v.dueDate ? v.dueDate.format('YYYY-MM-DD') : '',
                // custom fields are replaced wholesale (removing a value drops it from the map)
                customFields: collectCustomValues(tplFields, v[CF_GROUP]),
                // module: empty string = back to unfiled (omitting leaves it untouched; always submitted here)
                moduleId: v.moduleId ?? '',
              })
              message.success(t('req.updated', '已保存'))
              setEditOpen(false)
              load(); onChanged()
            } catch (e) {
              message.error(e instanceof ApiError ? e.message : t('req.updateFailed', '保存失败'))
            }
          }}
        >
          <Form.Item name="title" label={t('req.title', '标题')} rules={[{ required: true }]}>
            <Input />
          </Form.Item>
          <Row gutter={12}>
            <Col span={12}>
              <Form.Item name="reqType" label={t('req.reqType', '类型')}>
                <Select options={['FEATURE', 'ENHANCEMENT', 'TECH_DEBT', 'BUGFIX'].map((k) => ({ value: k, label: t(`req.type.${k}`, k) }))} />
              </Form.Item>
            </Col>
            <Col span={12}>
              <Form.Item name="priority" label={t('req.priority', '优先级')}>
                <Select options={['P0', 'P1', 'P2', 'P3'].map((p) => ({ value: p, label: p }))} />
              </Form.Item>
            </Col>
          </Row>
          <Form.Item name="tags" label={t('req.tags', '标签')}>
            <Select mode="tags" maxCount={10} tokenSeparators={[',', ' ']} open={false} suffixIcon={null} placeholder={t('req.tagsPh', '输入后回车,最多 10 个')} />
          </Form.Item>
          <Form.Item name="moduleId" label={t('req.module', '所属模块')}>
            <Select
              showSearch
              optionFilterProp="label"
              options={[{ value: '', label: t('req.moduleUnfiled', '未规划') }, ...modules.map((m) => ({ value: m.id, label: m.name }))]}
            />
          </Form.Item>
          <Form.Item name="dueDate" label={t('req.dueDate', '截止日期')}>
            <DatePicker style={{ width: '100%' }} allowClear />
          </Form.Item>
          {/* Custom fields: prefilled with current values; saving replaces the whole map. */}
          <CustomFieldItems kind="requirement" fields={tplFields} />
          <Button type="primary" htmlType="submit" block>{t('a.save', '保存')}</Button>
        </Form>
      </Modal>
      {/* Change history: field-level log (time / actor / field: old → new). */}
      <Drawer title={t('req.changes', '变更记录')} open={changesOpen} onClose={() => setChangesOpen(false)} width={480}>
        {changes.length ? (
          <Timeline
            items={changes.map((c, i) => ({
              key: i,
              children: (
                <div>
                  <div style={{ color: 'var(--text-3)', fontSize: 12 }}>
                    {new Date(c.changedAt).toLocaleString()} · {c.changedBy}
                  </div>
                  <div>
                    <Tag style={{ marginRight: 6 }}>{t(`req.chg.${c.field}`, c.field)}</Tag>
                    <span style={{ color: 'var(--text-2)' }}>{c.oldValue || '—'}</span>
                    <span style={{ color: 'var(--text-3)', margin: '0 6px' }}>→</span>
                    <span>{c.newValue || '—'}</span>
                  </div>
                </div>
              ),
            }))}
          />
        ) : (
          <Empty description={t('req.noChanges', '暂无变更记录')} />
        )}
      </Drawer>
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

// Requirement-level human-AI collaboration: AI vs human delivery comparison + AI share / delivery quality + acceptance calendar grid.
// Same definition as the home page: task accepted with a DELIVERED delivery record = AI-delivered.
function CollabPanel({ projectId, reqId, refreshKey }: { projectId: string; reqId: string; refreshKey?: unknown }) {
  const { t } = useI18n()
  const [stats, setStats] = useState<CollabStats | null>(null)
  useEffect(() => {
    api.collabStats(projectId, reqId).then(setStats).catch(() => setStats(null))
    // refreshKey (the task list) changing may mean new acceptance results; refetch.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId, reqId, refreshKey])
  const row = stats?.items?.[0]
  if (!row || (row.aiTasks + row.humanTasks === 0 && row.aiAttempts === 0)) return null
  const totalPts = row.aiPoints + row.humanPoints
  const aiShare = totalPts > 0 ? (row.aiPoints * 100) / totalPts : row.aiTasks + row.humanTasks > 0 ? (row.aiTasks * 100) / (row.aiTasks + row.humanTasks) : 0
  const firstPassRate = row.aiTasks > 0 ? (row.aiFirstPass * 100) / row.aiTasks : 0
  return (
    <Card size="small" title={t('req.collabTitle', '人机协同')}>
      <Row gutter={[16, 16]} align="middle" justify="space-around" wrap>
        <Col xs={12} sm={12} md={6} lg={4}>
          <Progress
            type="circle"
            size={72}
            percent={Math.round(aiShare)}
            format={() => `${aiShare.toFixed(0)}%`}
            strokeColor="#1664ff"
          />
          <div style={{ marginTop: 6, fontSize: 12, color: 'var(--text-2)' }}>{t('req.aiShare', 'AI 参与度(工作量)')}</div>
        </Col>
        <Col xs={12} sm={12} md={6} lg={4}>
          <MiniSplit
            label={t('req.deliverSplit', '交付任务(AI / 人)')}
            left={row.aiTasks}
            right={row.humanTasks}
            leftColor="#1664ff"
            rightColor="#8c8c8c"
          />
        </Col>
        <Col xs={12} sm={12} md={6} lg={4}>
          <MiniSplit
            label={t('req.attemptsStat', '交付尝试(成功 / 失败)')}
            left={row.aiDelivered}
            right={row.aiFailed}
            leftColor="#52c41a"
            rightColor="#ff4d4f"
          />
        </Col>
        <Col xs={12} sm={12} md={6} lg={4}>
          <Progress
            type="circle"
            size={72}
            percent={row.aiTasks > 0 ? Math.round(firstPassRate) : 0}
            format={() => (row.aiTasks > 0 ? `${firstPassRate.toFixed(0)}%` : '—')}
            strokeColor="#52c41a"
          />
          <div style={{ marginTop: 6, fontSize: 12, color: 'var(--text-2)' }}>{t('req.firstPass', '一次交付通过率')}</div>
        </Col>
      </Row>
      <div style={{ marginTop: 16 }}>
        <ContributionGrid days={stats?.daily ?? []} metric="total" />
      </div>
    </Card>
  )
}

// Compact AI/human split bar: a single stacked progress with a center label.
function MiniSplit({ label, left, right, leftColor, rightColor }: { label: string; left: number; right: number; leftColor: string; rightColor: string }) {
  const total = left + right
  const lp = total > 0 ? Math.round((left * 100) / total) : 0
  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 13, marginBottom: 4 }}>
        <span style={{ color: leftColor, fontWeight: 600 }}>{left}</span>
        <span style={{ color: rightColor, fontWeight: 600 }}>{right}</span>
      </div>
      <Progress percent={lp} showInfo={false} strokeColor={leftColor} trailColor={rightColor} size="small" />
      <div style={{ marginTop: 6, fontSize: 12, color: 'var(--text-2)' }}>{label}</div>
    </div>
  )
}

function DecompositionView({ decompId, verificationId, projectId, reqId, req }: { decompId: string; verificationId?: string; projectId: string; reqId?: string; req?: Requirement }) {
  const { t } = useI18n()
  const [tasks, setTasks] = useState<Task[]>([])
  const [cov, setCov] = useState<CoverageCase[]>([]) // manual functional-case coverage, viewed alongside task coverage
  useEffect(() => { if (reqId) api.requirementCoverage(reqId).then(setCov).catch(() => setCov([])) }, [reqId])
  const [running, setRunning] = useState(false)
  const [dispatching, setDispatching] = useState<Set<string>>(new Set()) // task ids being dispatched (guards double clicks)
  const [summary, setSummary] = useState<{ total: number; verified: number; failed: number; blocked: number; rounds: number } | null>(null)
  const [report, setReport] = useState<VerificationReport | null>(null)
  const [eventsFor, setEventsFor] = useState<Task | null>(null)
  const [casesFor, setCasesFor] = useState<Task | null>(null)
  const [view, setView] = useState<'table' | 'board'>('table')
  // Assignee candidates: humans (project users) + AI executors (runner agents).
  const [assignees, setAssignees] = useState<{ value: string; label: string; kind: string; id: string }[]>([])
  const nameOfAssignee = (a?: string, kind?: string) => assignees.find((o) => o.kind === kind && o.id === a)?.label || a || ''
  // Registered remote runtime fleet: the dispatch menu can target a specific machine.
  const [fleet, setFleet] = useState<FleetRuntime[]>([])
  const loadFleet = () => api.fleetRuntimes().then(setFleet).catch(() => setFleet([]))
  // Live filter for the dispatch runtime picker (typed in the dropdown search box).
  const [rtSearch, setRtSearch] = useState('')

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
    loadFleet()
    // Assignee candidates: project members (humans) + AI executors (runner agents).
    // Members come back as ids only, so resolve display names via /system/user/names.
    // Any failure degrades silently; the board/columns still render.
    const buildAssignees = async () => {
      const [members, ag] = await Promise.all([
        api.projectMembers(projectId).catch(() => [] as ProjectMember[]),
        api.runnerAgents().catch(() => []),
      ])
      const ids = members.map((m) => m.userId).filter(Boolean)
      const names = ids.length ? await api.userNames(ids).catch(() => ({}) as Record<string, string>) : {}
      setAssignees([
        ...members.map((m) => ({
          value: `HUMAN:${m.userId}`,
          label: `👤 ${names[m.userId] || m.userId}`,
          kind: 'HUMAN',
          id: m.userId,
        })),
        ...ag.map((a) => ({ value: `AGENT:${a.id}`, label: `🤖 ${a.name}`, kind: 'AGENT', id: a.id })),
      ])
    }
    buildAssignees()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [decompId])

  // Assign (optimistic update, then persist). value is "HUMAN:<id>" / "AGENT:<id>" / "" (unassign).
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
  // Dependency gate: all dependencies must be VERIFIED before dispatch (dispatching early hits the gate and gets stuck PENDING).
  const statusOf = (id: string) => tasks.find((x) => x.id === id)?.status
  const depsReady = (t: Task) => (t.dependencies ?? []).every((d) => statusOf(d) === 'VERIFIED')
  const dispatch = async (task: Task, executor: string, targetRuntime?: string) => {
    if (dispatching.has(task.id)) return
    if (!depsReady(task)) {
      message.warning(t('req.depsNotReady', '依赖任务未全部验证完成,暂不能派发'))
      return
    }
    // Build full requirement context so the agent runtime gets the big picture, not just the task.
    const baselineVersion = req ? req.versions?.find((v) => v.version === req.baselineVersion) : undefined
    const latestVersion = req ? req.versions?.[req.versions.length - 1] : undefined
    const reqDescription = baselineVersion?.description || latestVersion?.description || ''
    const reqCriteria = baselineVersion?.acceptanceCriteria || latestVersion?.acceptanceCriteria || req?.acceptanceCriteria || []
    const contextParts = [
      `需求标题: ${req?.title ?? ''}`,
      req?.reqType ? `需求类型: ${t(`req.type.${req.reqType}`, req.reqType)}` : '',
      req?.priority ? `优先级: ${req.priority}` : '',
      req?.dueDate ? `截止日期: ${req.dueDate}` : '',
      reqCriteria.length ? `验收标准:\n${reqCriteria.map((c, i) => `${i + 1}. ${c}`).join('\n')}` : '',
      reqDescription ? `需求描述:\n${reqDescription}` : '',
    ].filter(Boolean).join('\n\n')
    // Compose the requirement's selected skills into agent instructions so the
    // skill data "really moves" — the agent runtime receives them at dispatch time.
    let instructions: string | undefined
    if (req?.skillIds?.length) {
      try {
        const composed = await api.composeSkills(projectId, req.skillIds)
        instructions = composed.instructions || undefined
      } catch (e) {
        console.warn('compose requirement skills failed', e)
      }
    }
    setDispatching((s) => new Set(s).add(task.id))
    try {
      await api.createDelivery({
        decompositionId: decompId,
        taskId: task.id,
        title: task.title,
        description: task.description,
        acceptanceCriteria: task.acceptanceCriteria,
        executor,
        targetRuntime,
        context: contextParts || undefined,
        instructions,
      })
      message.success(`${t('req.dispatched', '已派发')} ${task.id}${targetRuntime ? ` → ${targetRuntime}` : ''}`)
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? `${t('req.dispatchFailed', '派发失败')}:${e.status}` : t('req.dispatchFailed', '派发失败'))
    } finally {
      setDispatching((s) => { const n = new Set(s); n.delete(task.id); return n })
    }
  }
  // Keep one runtime per name (reconnects create duplicate records with new ids): prefer online, then latest heartbeat.
  const fleetByName = (() => {
    const m = new Map<string, FleetRuntime>()
    for (const r of fleet) {
      const prev = m.get(r.name)
      if (!prev || (!prev.online && r.online) || (prev.online === r.online && r.lastSeenMs > prev.lastSeenMs)) m.set(r.name, r)
    }
    return [...m.values()]
  })()
  // Dispatch button = executor picker: one group per executor kind, registered runtimes first
  // (online first, offline disabled), "any online executor (queued)" last as fallback. With none
  // registered, show an explicit hint so "any" isn't mistaken for the only choice. Targeting uses
  // the runtime name, which is stable across reconnects.
  const executorMenu = (task: Task, search = '') => ({
    items: Object.entries(EXECUTOR_LABEL).map(([key, label]) => {
      const runtimes = fleetByName
        .filter((r) => r.caps.includes(key))
        .filter((r) => !search || r.name.toLowerCase().includes(search.toLowerCase()))
        .sort((a, b) => Number(b.online) - Number(a.online) || a.name.localeCompare(b.name))
      return {
        type: 'group' as const,
        key,
        label,
        children: [
          ...runtimes.map((r) => ({
            key: `${key}@@${r.name}`,
            disabled: !r.online,
            label: (
              <span>
                <Badge status={r.online ? 'success' : 'default'} /> {r.name}
                {!r.online && <span style={{ color: 'var(--text-3)', marginLeft: 6, fontSize: 12 }}>{t('req.rtOffline', '离线')}</span>}
              </span>
            ),
          })),
          ...(runtimes.length === 0
            ? [{
                key: `${key}##none`,
                disabled: true,
                label: <span style={{ color: 'var(--text-3)', fontSize: 12 }}>{t('req.noRuntime', '无已注册执行者')}</span>,
              }]
            : []),
          {
            key,
            label: <span style={{ color: 'var(--text-2)' }}>{t('req.anyRuntime', '任意在线执行者(排队)')}</span>,
          },
        ],
      }
    }),
    onClick: ({ key }: { key: string }) => {
      const [executor, target] = key.split('@@')
      dispatch(task, executor, target)
    },
  })
  // Inline task-point editing: optimistic update, then persist; reload on failure.
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
      {reqId && <CollabPanel projectId={projectId} reqId={reqId} refreshKey={tasks} />}
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
                    <Dropdown
                      trigger={['click']}
                      disabled={dispatching.has(row.id) || !depsReady(row)}
                      menu={executorMenu(row, rtSearch)}
                      onOpenChange={(o) => { if (o) loadFleet(); else setRtSearch('') }}
                      dropdownRender={(menu) => (
                        <div>
                          <div style={{ padding: 8, borderBottom: '1px solid var(--border, #f0f0f0)' }}>
                            <Input
                              autoFocus size="small" allowClear
                              placeholder={t('req.searchRuntime', '搜索执行者')}
                              value={rtSearch}
                              onChange={(e) => setRtSearch(e.target.value)}
                            />
                          </div>
                          {menu}
                        </div>
                      )}
                    >
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
        // Kanban: columns by status; cards show points + assignee (human/AI agent) with inline assign/dispatch.
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
                              <Dropdown
                                trigger={['click']}
                                disabled={dispatching.has(tk.id) || !depsReady(tk)}
                                menu={executorMenu(tk, rtSearch)}
                                onOpenChange={(o) => { if (o) loadFleet(); else setRtSearch('') }}
                                dropdownRender={(menu) => (
                                  <div>
                                    <div style={{ padding: 8, borderBottom: '1px solid var(--border, #f0f0f0)' }}>
                                      <Input
                                        autoFocus size="small" allowClear
                                        placeholder={t('req.searchRuntime', '搜索执行者')}
                                        value={rtSearch}
                                        onChange={(e) => setRtSearch(e.target.value)}
                                      />
                                    </div>
                                    {menu}
                                  </div>
                                )}
                              >
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
                {/* Manual case coverage alongside task coverage — denominator is the report's criteria total. */}
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
              {/* Gap detail: UNCOVERED (no task covers the criterion) / UNVERIFIED (covered but not delivery-verified). */}
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
                          {/* Manual cases exist despite no task coverage: flag for review so it isn't mistaken for a full gap. */}
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

// Task-linked cases + their plans: the task → case → plan chain, each clickable through to its page.
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

const attemptBadge = (s: string): 'processing' | 'success' | 'error' | 'warning' | 'default' =>
  s === 'RUNNING' ? 'processing' : s === 'DELIVERED' ? 'success' : s === 'FAILED' ? 'error' : s === 'DISPATCHED' ? 'warning' : 'default'
const eventColor = (k: string): string =>
  k === 'DECISION' ? 'blue' : k === 'FILE_CHANGE' ? 'gold' : k === 'TEST_RESULT' ? 'green' : k === 'TOOL_CALL' ? 'geekblue' : k === 'VERDICT' ? 'purple' : 'default'

// Execution progress drawer: all delivery attempts for a task + live audit trail + deliverable/errors per attempt.
// Polls every 3s while RUNNING/DISPATCHED until terminal, so the AI can be watched after dispatch.
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
      const ordered = [...atts].reverse() // most recent first (API returns insertion order)
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

  // Poll while any attempt is running; stop once all are terminal.
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
                  {a.targetRuntime && <Tag color="geekblue">@{a.targetRuntime}</Tag>}
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
