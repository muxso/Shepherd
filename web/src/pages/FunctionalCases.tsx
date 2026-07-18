import { useEffect, useState, type ReactNode } from 'react'
import { Button, Descriptions, Empty, Form, Input, Modal, Popconfirm, Select, Space, Table, Tabs, Tag, Upload } from 'antd'
import type { FormInstance } from 'antd'
import { DeleteOutlined, DownloadOutlined, EditOutlined, ImportOutlined } from '@ant-design/icons'
import { message } from '../feedback'
import { api, ApiError, userIdStore, type ApiModule, type CaseRequirementLink, type CaseStep, type FunctionalCase, type Requirement, type TemplateField } from '../api'
import { useApp } from '../context'
import { Workspace, WorkList, useWorkTabs } from '../components/Workspace'
import { ModuleTreePanel, inSelectedModule } from '../components/ModuleTreePanel'
import StepsEditor from '../components/StepsEditor'
import { CF_GROUP, CustomFieldItem, collectCustomValues, customFormValues, useFieldTemplate } from '../components/TemplateFields'
import { useI18n } from '../i18n'
import { SelectProjectEmpty } from '../components/Page'
import { useListView, type ListColumn } from '../components/ListView'
import CaseDetailDrawer from '../components/CaseDetailDrawer'

const PRIORITIES = ['P0', 'P1', 'P2', 'P3']
const prioColor = (p?: string) => (p === 'P0' ? 'red' : p === 'P1' ? 'orange' : 'blue')

// Key of the persistent "new case" workspace tab (shares the tab pool with detail tabs keyed by case id).
const NEW_KEY = '__new_case__'

export default function FunctionalCases() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const unfiled = t('func.unfiled', '未归类')
  const [cases, setCases] = useState<FunctionalCase[]>([])
  const [modules, setModules] = useState<ApiModule[]>([])
  const [loading, setLoading] = useState(false)
  // 'ALL' | 'UNFILED' | <moduleId>
  const [moduleKey, setModuleKey] = useState('ALL')
  const [moduleSearch, setModuleSearch] = useState('')
  const [editing, setEditing] = useState<FunctionalCase | null>(null)
  // MeterSphere-style full-screen detail drawer; null = closed.
  const [detailId, setDetailId] = useState<string | null>(null)
  const tabs = useWorkTabs()

  const load = async () => {
    if (!projectId) { setCases([]); setModules([]); return }
    setLoading(true)
    try {
      const [cs, mm] = await Promise.all([
        api.functionalCases(projectId),
        api.modules(projectId).catch(() => []),
      ])
      setCases(cs)
      setModules(Array.isArray(mm) ? mm : [])
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('func.loadFailed', '加载失败'))
    } finally {
      setLoading(false)
    }
  }
  useEffect(() => {
    load()
    tabs.reset()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId])

  // module now stores a module id; legacy data stored name text → treat values matching no module id as unfiled.
  const moduleIdOf = (c: FunctionalCase) => (modules.some((m) => m.id === c.module) ? c.module! : '')
  const moduleNameOf = (id: string) => modules.find((m) => m.id === id)?.name
  // Module column: matched module shows its name; legacy text values render as-is (gray); empty shows unfiled.
  const renderModule = (m?: string) => {
    if (!m) return unfiled
    const name = moduleNameOf(m)
    return name ?? <span style={{ color: 'var(--text-3)' }}>{m}</span>
  }

  const doExport = async () => {
    if (!projectId) return
    try {
      const blob = await api.exportFunctionalCases(projectId)
      const url = URL.createObjectURL(blob)
      const a = document.createElement('a')
      a.href = url
      a.download = 'functional-cases.xlsx'
      a.click()
      URL.revokeObjectURL(url)
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('func.exportFailed', '导出失败'))
    }
  }
  const doImport = async (file: File) => {
    if (!projectId) return
    try {
      const r = await api.importFunctionalCases(projectId, file)
      message.success(`${t('func.imported', '导入成功')}:${r.imported}`)
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('func.importFailed', '导入失败'))
    }
  }

  const doDelete = async (c: FunctionalCase) => {
    try {
      await api.deleteFunctionalCase(c.id)
      message.success(t('func.deleted', '用例已删除'))
      tabs.close(c.id)
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('func.deleteFailed', '删除失败'))
    }
  }

  // View/filter/column toolbar: useListView must be called before the conditional return below.
  const allColumns: ListColumn<FunctionalCase>[] = [
    { key: 'name', label: t('func.colName', '名称'), title: t('func.colName', '名称'), dataIndex: 'name', ellipsis: true },
    { key: 'module', label: t('func.colModule', '模块'), title: t('func.colModule', '模块'), dataIndex: 'module', width: 140, render: renderModule },
    { key: 'priority', label: t('func.colPriority', '优先级'), title: t('func.colPriority', '优先级'), dataIndex: 'priority', width: 90, render: (p?: string) => <Tag color={prioColor(p)}>{p || 'P2'}</Tag> },
    { key: 'steps', label: t('func.colSteps', '步骤'), title: t('func.colSteps', '步骤'), dataIndex: 'steps', width: 70, render: (s?: CaseStep[]) => (s?.length || 0) },
    { key: 'status', label: t('func.colStatus', '状态'), title: t('func.colStatus', '状态'), dataIndex: 'status', width: 110, render: (s?: string) => <Tag>{s || 'PREPARED'}</Tag> },
    { key: 'createdBy', label: t('func.colCreatedBy', '创建人'), title: t('func.colCreatedBy', '创建人'), dataIndex: 'createdBy', width: 110, ellipsis: true, render: (u?: string) => u ? <span style={{ color: 'var(--text-2)' }}>{u}</span> : <span style={{ color: 'var(--text-3)' }}>—</span> },
    {
      key: 'action',
      label: t('req.action', '操作'),
      title: t('req.action', '操作'),
      width: 110,
      render: (_v, c) => (
        <Space size={0} onClick={(e) => e.stopPropagation()}>
          <Button type="link" size="small" icon={<EditOutlined />} onClick={() => setEditing(c)}>{t('a.edit', '编辑')}</Button>
          <Popconfirm
            title={t('func.confirmDelete', '确认删除该用例?')}
            okText={t('a.confirm', '确定')}
            cancelText={t('a.cancel', '取消')}
            onConfirm={() => doDelete(c)}
          >
            <Button type="link" size="small" danger icon={<DeleteOutlined />}>{t('a.delete', '删除')}</Button>
          </Popconfirm>
        </Space>
      ),
    },
  ]
  const lv = useListView<FunctionalCase>({
    kind: 'functional-case',
    projectId,
    searchOf: (c) => c.name,
    searchLabel: t('func.searchName', '搜索用例名'),
    systemViews: [
      { key: 'mine', label: t('lv.mine', '我创建的'), pred: (c) => !!c.createdBy && c.createdBy === userIdStore.get() },
    ],
    fields: [
      {
        key: 'priority', label: t('func.colPriority', '优先级'), type: 'enum',
        options: PRIORITIES.map((p) => ({ value: p, label: p })),
        get: (c) => c.priority || 'P2',
      },
      {
        key: 'module', label: t('func.colModule', '模块'), type: 'enum',
        options: [unfiled, ...modules.map((m) => m.name)].map((m) => ({ value: m, label: m })),
        // Matched module → name; legacy text value → as-is; empty → unfiled.
        get: (c) => { const id = moduleIdOf(c); return id ? moduleNameOf(id)! : c.module || unfiled },
      },
      {
        key: 'status', label: t('func.colStatus', '状态'), type: 'enum',
        options: [...new Set(cases.map((c) => c.status || 'PREPARED'))].map((s) => ({ value: s, label: s })),
        get: (c) => c.status || 'PREPARED',
      },
      // Advanced-condition only (duplicates search box / columns; not rendered in the declarative filter area).
      { key: 'id', label: 'ID', type: 'text', advOnly: true, get: (c) => c.id },
      { key: 'name', label: t('func.colName', '用例名'), type: 'text', advOnly: true, get: (c) => c.name },
      { key: 'createdBy', label: t('lv.createdBy', '创建人'), type: 'text', advOnly: true, get: (c) => c.createdBy || '' },
    ],
    columns: allColumns,
    rows: cases,
  })

  if (!projectId) return <SelectProjectEmpty />

  // Module-tree filter stacks on top of toolbar filters (selecting a parent includes children; legacy text values count as unfiled).
  const visible = lv.rows.filter((c) => inSelectedModule(modules, moduleKey, moduleIdOf(c)))

  // Left: project-level module tree, same as the scenarios page.
  const left = (
    <ModuleTreePanel
      projectId={projectId}
      modules={modules}
      items={cases}
      getModuleId={moduleIdOf}
      selectedKey={moduleKey}
      onSelect={setModuleKey}
      allLabel={t('func.allCases', '全部用例')}
      unfiledLabel={unfiled}
      moduleSearch={moduleSearch}
      onModuleSearch={setModuleSearch}
      searchPlaceholder={t('func.moduleSearchPh', '请输入模块名称搜索')}
      onModulesChanged={load}
      deleteModuleContent={t('func.deleteModuleContent', '其下用例将变为未归类(不会删除用例)。')}
    />
  )

  const detailTabs = tabs.openIds.flatMap((id) => {
    if (id === NEW_KEY)
      return [{
        key: NEW_KEY,
        label: t('func.newCase', '新建用例'),
        children: (
          <NewCaseTab
            projectId={projectId}
            modules={modules}
            defaultModule={moduleKey !== 'ALL' && moduleKey !== 'UNFILED' ? moduleKey : ''}
            onCancel={() => tabs.close(NEW_KEY)}
            onCreated={(c) => { tabs.close(NEW_KEY); load().then(() => tabs.open(c.id)) }}
          />
        ),
      }]
    const c = cases.find((x) => x.id === id)
    return c ? [{ key: c.id, label: c.name, children: <CaseDetail c={c} moduleLabel={renderModule(c.module)} /> }] : []
  })

  return (
    <>
      <Workspace
        left={left}
        listLabel={t('func.allCases', '全部用例')}
        activeKey={tabs.activeKey}
        onChange={tabs.setActiveKey}
        onClose={tabs.close}
        tabs={detailTabs}
        listContent={
          <WorkList<FunctionalCase>
            onNew={() => tabs.open(NEW_KEY)}
            newLabel={t('func.newCase', '新建用例')}
            extraActions={
              <>
                {lv.toolbar}
                <Upload showUploadList={false} accept=".xlsx" beforeUpload={(f) => { doImport(f as File); return false }}>
                  <Button icon={<ImportOutlined />}>{t('func.import', '导入')}</Button>
                </Upload>
                <Button icon={<DownloadOutlined />} onClick={doExport} disabled={!cases.length}>{t('func.export', '导出')}</Button>
              </>
            }
            onRefresh={load}
            columns={lv.columns}
            data={visible}
            pagination={lv.pagination}
            loading={loading}
            onRowClick={(c) => setDetailId(c.id)}
            emptyText={t('func.emptyCase', '暂无用例')}
          />
        }
      />
      <CaseDetailDrawer
        open={!!detailId}
        caseId={detailId || ''}
        cases={visible.length ? visible : cases}
        modules={modules}
        projectId={projectId}
        onClose={() => setDetailId(null)}
        onNavigate={setDetailId}
        onEdit={(c) => setEditing(c)}
        onChanged={load}
        onDeleted={() => { setDetailId(null); load() }}
      />
      <CaseEditModal
        projectId={projectId}
        modules={modules}
        editing={editing}
        onClose={() => setEditing(null)}
        onSaved={() => {
          setEditing(null)
          load()
        }}
      />
    </>
  )
}

function CaseDetail({ c, moduleLabel }: { c: FunctionalCase; moduleLabel?: ReactNode }) {
  const { t } = useI18n()
  const cf = c.customFields || {}
  const [covReqs, setCovReqs] = useState<CaseRequirementLink[]>([])
  const loadCov = () => api.caseRequirements(c.id).then(setCovReqs).catch(() => setCovReqs([]))
  // Manual requirement linking: pick requirement → pick acceptance criterion → link.
  const [linkOpen, setLinkOpen] = useState(false)
  const [reqs, setReqs] = useState<Requirement[]>([])
  const [selReq, setSelReq] = useState<Requirement | null>(null)
  const [selCrit, setSelCrit] = useState<number>()
  useEffect(() => { loadCov() }, [c.id]) // eslint-disable-line react-hooks/exhaustive-deps
  const openLink = async () => {
    setLinkOpen(true); setSelReq(null); setSelCrit(undefined)
    try { setReqs((await api.requirements(c.projectId)).items) } catch { /* ignore */ }
  }
  const critsOf = (r: Requirement) =>
    r.versions?.find((v) => v.version === r.baselineVersion)?.acceptanceCriteria ?? r.versions?.[r.versions.length - 1]?.acceptanceCriteria ?? r.acceptanceCriteria ?? []
  const pickReq = async (id: string) => {
    setSelCrit(undefined)
    try { setSelReq(await api.getRequirement(id)) } catch { setSelReq(null) }
  }
  const doLink = async () => {
    if (!selReq || selCrit == null) return message.warning(t('func.pickReqCrit', '请选择需求与验收标准'))
    try {
      await api.linkRequirementCase({ requirementId: selReq.id, criterionIndex: selCrit, functionalCaseId: c.id, projectId: c.projectId })
      message.success(t('req.linked', '已关联')); setLinkOpen(false); loadCov()
    } catch (e) { message.error(e instanceof ApiError ? e.message : t('req.linkFailed', '关联失败')) }
  }
  const unlinkReq = (l: CaseRequirementLink) =>
    api.unlinkRequirementCase({ requirementId: l.requirementId, criterionIndex: l.criterionIndex, functionalCaseId: c.id }).then(loadCov).catch(() => undefined)
  const stepsTable = (
    <Table<CaseStep>
      rowKey={(_, i) => String(i)}
      size="small"
      pagination={false}
      dataSource={c.steps || []}
      locale={{ emptyText: <Empty description={t('func.noSteps', '无步骤')} /> }}
      columns={[
        { title: t('func.colIndex', '序号'), width: 60, render: (_, __, i) => i + 1 },
        { title: t('func.colStep', '用例步骤'), dataIndex: 'step' },
        { title: t('func.colExpected', '预期结果'), dataIndex: 'expected' },
      ]}
    />
  )
  return (
    <div style={{ padding: '12px 16px', height: '100%', overflow: 'auto' }}>
      <Tabs
        items={[
          {
            key: 'info',
            label: t('func.tabInfo', '基本信息'),
            children: (
              <Descriptions column={2} size="small" bordered>
                <Descriptions.Item label={t('func.colName', '名称')}>{c.name}</Descriptions.Item>
                <Descriptions.Item label={t('func.colModule', '模块')}>{moduleLabel ?? (c.module || t('func.unfiled', '未归类'))}</Descriptions.Item>
                <Descriptions.Item label={t('func.colPriority', '优先级')}><Tag color={prioColor(c.priority)}>{c.priority || 'P2'}</Tag></Descriptions.Item>
                <Descriptions.Item label={t('func.colStatus', '状态')}>{c.status || 'PREPARED'}</Descriptions.Item>
              </Descriptions>
            ),
          },
          {
            key: 'detail',
            label: t('a.detail', '详情'),
            children: (
              <Space direction="vertical" style={{ width: '100%' }} size={16}>
                <div>
                  <div style={{ fontWeight: 600, marginBottom: 6 }}>{t('func.prerequisite', '前置条件')}</div>
                  <div style={{ background: 'var(--panel-2)', border: '1px solid var(--border-soft)', borderRadius: 6, padding: 10, minHeight: 40, whiteSpace: 'pre-wrap' }}>
                    {cf['前置条件'] || <span style={{ color: 'var(--text-3)' }}>{t('func.none', '无')}</span>}
                  </div>
                </div>
                <div>
                  <div style={{ fontWeight: 600, marginBottom: 6 }}>{t('func.stepsDesc', '步骤描述')}</div>
                  {stepsTable}
                </div>
                <div>
                  <div style={{ fontWeight: 600, marginBottom: 6 }}>{t('func.remark', '备注')}</div>
                  <div style={{ background: 'var(--panel-2)', border: '1px solid var(--border-soft)', borderRadius: 6, padding: 10, minHeight: 40, whiteSpace: 'pre-wrap' }}>
                    {cf['备注'] || <span style={{ color: 'var(--text-3)' }}>{t('func.none', '无')}</span>}
                  </div>
                </div>
              </Space>
            ),
          },
          { key: 'steps', label: `${t('func.tabStepsExpected', '步骤与预期')} (${c.steps?.length || 0})`, children: stepsTable },
          {
            key: 'coverage',
            label: `${t('func.tabCoversReq', '覆盖需求')} (${covReqs.length})`,
            children: (
              <Space direction="vertical" style={{ width: '100%' }}>
                <Button type="primary" size="small" onClick={openLink}>{t('func.linkReq', '关联需求')}</Button>
                {covReqs.length ? (
                  <Table
                    rowKey={(_, i) => String(i)}
                    size="small"
                    pagination={false}
                    dataSource={covReqs}
                    columns={[
                      { title: t('func.coverReq', '需求'), dataIndex: 'requirementTitle', render: (v: string) => v || '—' },
                      { title: t('func.coverCriterion', '验收标准'), dataIndex: 'criterionIndex', width: 120, render: (i: number) => `${t('req.criterion', '标准')} ${i + 1}` },
                      { title: t('req.action', '操作'), width: 80, render: (_v, l) => <Button type="link" size="small" danger onClick={() => unlinkReq(l)}>{t('func.unlink', '解除')}</Button> },
                    ]}
                  />
                ) : (
                  <Empty description={t('func.noCoverReq', '未关联任何需求,点上方「关联需求」')} style={{ marginTop: 16 }} />
                )}
              </Space>
            ),
          },
        ]}
      />
      <Modal open={linkOpen} title={t('func.linkReq', '关联需求')} onCancel={() => setLinkOpen(false)} onOk={doLink} okText={t('a.confirm', '确定')} cancelText={t('a.cancel', '取消')} destroyOnHidden>
        <div style={{ marginBottom: 12 }}>
          <div style={{ marginBottom: 6 }}>{t('func.pickReq', '需求')}</div>
          <Select showSearch style={{ width: '100%' }} placeholder={t('func.pickReq', '选择需求')} optionFilterProp="label" onChange={pickReq} options={reqs.map((r) => ({ value: r.id, label: r.title }))} notFoundContent={t('req.empty', '项目暂无需求')} />
        </div>
        {selReq && (
          <div>
            <div style={{ marginBottom: 6 }}>{t('func.pickCriterion', '验收标准')}</div>
            <Select style={{ width: '100%' }} value={selCrit} placeholder={t('func.pickCriterion', '选择验收标准')} onChange={setSelCrit}
              options={critsOf(selReq).map((c2, i) => ({ value: i, label: `${t('req.criterion', '标准')} ${i + 1}: ${c2}` }))}
              notFoundContent={t('func.noCriteria', '该需求没有验收标准')} />
          </div>
        )}
      </Modal>
    </div>
  )
}

// Case form body (template-driven system field order/required/visibility + StepsEditor + custom fields).
// Shared by the new-case tab and the edit modal; the caller owns the form instance and triggers form.submit().
function CaseForm({
  form,
  projectId,
  modules,
  editing,
  defaultModule = '',
  tplFields,
  onSavingChange,
  onSaved,
}: {
  form: FormInstance
  projectId: string
  modules: ApiModule[]
  editing: FunctionalCase | null
  defaultModule?: string
  /** Field template: system field order/required/visibility + custom field rendering (name is always present). */
  tplFields: TemplateField[]
  onSavingChange: (v: boolean) => void
  onSaved: (c: FunctionalCase) => void
}) {
  const { t } = useI18n()
  const fieldOf = (k: string) => tplFields.find((f) => f.key === k)
  const enabled = (k: string) => fieldOf(k)?.enabled !== false
  const cf = editing?.customFields || {}
  const initial = editing
    ? {
        name: editing.name,
        module: editing.module || '',
        priority: editing.priority || 'P2',
        prerequisite: cf['前置条件'] || '',
        remark: cf['备注'] || '',
        steps: editing.steps?.length ? editing.steps : [{ step: '', expected: '' }],
        [CF_GROUP]: customFormValues(tplFields, editing.customFields),
      }
    : { priority: 'P1', module: defaultModule, steps: [{ step: '', expected: '' }] }
  // System field renderer: key → form item; order follows the template array, required per config (name is always required).
  const sysItem = (f: TemplateField) => {
    const rules = f.required ? [{ required: true }] : undefined
    switch (f.key) {
      case 'name':
        return (
          <Form.Item key="name" name="name" label={t('func.caseName', '用例名')} rules={[{ required: true }]}>
            <Input placeholder={t('func.namePlaceholder', '如:登录成功')} />
          </Form.Item>
        )
      case 'module': {
        // Options = unfiled ('') + project modules (value = module id). When edit backfill hits a legacy
        // name value not in the module table, append a temp option so old data stays visible; saving
        // without changing it keeps the original value.
        const opts: { value: string; label: string }[] = [
          { value: '', label: t('func.unfiled', '未归类') },
          ...modules.map((m) => ({ value: m.id, label: m.name })),
        ]
        if (editing?.module && !modules.some((m) => m.id === editing.module))
          opts.push({ value: editing.module, label: `${editing.module}${t('func.legacyModuleSuffix', '(旧数据)')}` })
        return (
          <Form.Item key="module" name="module" label={t('func.colModule', '模块')} rules={rules}>
            <Select options={opts} />
          </Form.Item>
        )
      }
      case 'priority':
        return (
          <Form.Item key="priority" name="priority" label={t('func.colPriority', '优先级')} rules={rules}>
            <Select options={PRIORITIES.map((p) => ({ value: p, label: p }))} />
          </Form.Item>
        )
      case 'prerequisite':
        return (
          <Form.Item key="prerequisite" name="prerequisite" label={t('func.prerequisite', '前置条件')} rules={rules}>
            <Input.TextArea rows={2} placeholder={t('func.prerequisitePlaceholder', '如:进入管理登录页 https://...')} />
          </Form.Item>
        )
      case 'steps':
        // Steps (step + expected): rendered via StepsEditor; "required" is enforced at submit (at least one row).
        return (
          <Form.Item key="steps" name="steps" label={t('func.testSteps', '测试步骤(步骤 + 预期结果)')} required={f.required}>
            <StepsEditor />
          </Form.Item>
        )
      case 'remark':
        return (
          <Form.Item key="remark" name="remark" label={t('func.remark', '备注')} rules={rules}>
            <Input.TextArea rows={2} />
          </Form.Item>
        )
      default:
        return null
    }
  }
  return (
    <Form
      form={form}
      layout="vertical"
      preserve={false}
      initialValues={initial}
      onFinish={async (v) => {
          // When steps is hidden, keep existing steps on edit; empty for new cases.
          const steps = enabled('steps')
            ? (v.steps || []).filter((s: CaseStep) => s.step.trim() || s.expected.trim())
            : editing?.steps || []
          const stepsField = fieldOf('steps')
          if (stepsField?.enabled && stepsField.required && !steps.length) {
            message.warning(t('func.stepsRequired', '请至少填写一条测试步骤'))
            return
          }
          onSavingChange(true)
          try {
            // On edit, keep the case's other custom fields; prerequisite/remark keep their legacy Chinese keys.
            const customFields: Record<string, string> = { ...(editing?.customFields || {}) }
            const setOrDel = (k: string, val?: string) => {
              if (val?.trim()) customFields[k] = val.trim()
              else delete customFields[k]
            }
            // Hidden fields keep existing values (not rendered, so submit value is undefined).
            if (enabled('prerequisite')) setOrDel('前置条件', v.prerequisite)
            if (enabled('remark')) setOrDel('备注', v.remark)
            // Template custom fields merge into the customFields map, keyed by field key.
            const custom = collectCustomValues(tplFields, v[CF_GROUP])
            for (const f of tplFields) {
              if (f.system || !f.enabled) continue
              if (custom[f.key] !== undefined) customFields[f.key] = custom[f.key]
              else delete customFields[f.key]
            }
            const fields = Object.keys(customFields).length ? customFields : undefined
            const module = enabled('module') ? v.module || undefined : editing?.module || undefined
            const priority = enabled('priority') ? v.priority : editing?.priority
            let saved: FunctionalCase
            if (editing) {
              saved = await api.updateFunctionalCase(editing.id, {
                projectId,
                name: v.name,
                priority,
                module,
                status: editing.status, // keep original status (driven by the review flow, not editable here)
                steps,
                customFields: fields,
              })
              message.success(t('func.updated', '用例已更新'))
            } else {
              saved = await api.createFunctionalCase({
                projectId,
                name: v.name,
                priority,
                module,
                steps,
                customFields: fields,
              })
              message.success(t('func.created', '用例已创建'))
            }
            onSaved(saved)
          } catch (e) {
            message.error(e instanceof ApiError ? e.message : t('func.saveFailed', '保存失败'))
          } finally {
            onSavingChange(false)
          }
        }}
      >
      {/* Render in template array order: system fields via their renderers, custom fields by type; hidden fields skipped. */}
      {tplFields.filter((f) => f.enabled).map((f) =>
        f.system ? sysItem(f) : <CustomFieldItem key={f.key} kind="functional-case" field={f} />
      )}
    </Form>
  )
}

// Edit dialog only; creation lives in the NewCaseTab workspace tab.
function CaseEditModal({
  projectId,
  modules,
  editing,
  onClose,
  onSaved,
}: {
  projectId: string
  modules: ApiModule[]
  editing: FunctionalCase | null
  onClose: () => void
  onSaved: () => void
}) {
  const { t } = useI18n()
  const [form] = Form.useForm()
  const [saving, setSaving] = useState(false)
  // Template loaded up-front so field config is ready when the modal opens and edit backfill has all custom fields.
  const { fields: tplFields } = useFieldTemplate('functional-case')
  return (
    <Modal
      title={t('func.editTitle', '编辑功能用例')}
      open={!!editing}
      onCancel={onClose}
      onOk={() => form.submit()}
      confirmLoading={saving}
      destroyOnHidden
      width={680}
    >
      <CaseForm
        form={form}
        projectId={projectId}
        modules={modules}
        editing={editing}
        tplFields={tplFields}
        onSavingChange={setSaving}
        onSaved={onSaved}
      />
    </Modal>
  )
}

function NewCaseTab({
  projectId,
  modules,
  defaultModule,
  onCreated,
  onCancel,
}: {
  projectId: string
  modules: ApiModule[]
  defaultModule: string
  onCreated: (c: FunctionalCase) => void
  onCancel: () => void
}) {
  const { t } = useI18n()
  const [form] = Form.useForm()
  const [saving, setSaving] = useState(false)
  const { fields: tplFields } = useFieldTemplate('functional-case')
  return (
    <div style={{ padding: '16px 24px', height: '100%', overflow: 'auto' }}>
      <div style={{ maxWidth: 680 }}>
        <CaseForm
          form={form}
          projectId={projectId}
          modules={modules}
          editing={null}
          defaultModule={defaultModule}
          tplFields={tplFields}
          onSavingChange={setSaving}
          onSaved={onCreated}
        />
        <Space style={{ marginTop: 8 }}>
          <Button type="primary" loading={saving} onClick={() => form.submit()}>{t('a.create', '创建')}</Button>
          <Button onClick={onCancel}>{t('a.cancel', '取消')}</Button>
        </Space>
      </div>
    </div>
  )
}
