import { useEffect, useMemo, useState } from 'react'
import { Button, Descriptions, Empty, Form, Input, Modal, Select, Space, Table, Tabs, Tag, Tree, Upload } from 'antd'
import { DownloadOutlined, ImportOutlined } from '@ant-design/icons'
import { message } from '../feedback'
import type { ColumnsType } from 'antd/es/table'
import { api, ApiError, type CaseRequirementLink, type CaseStep, type FunctionalCase, type Requirement } from '../api'
import { useApp } from '../context'
import { Workspace, WorkList, PaneHeader, useWorkTabs } from '../components/Workspace'
import StepsEditor from '../components/StepsEditor'
import { useI18n } from '../i18n'

const PRIORITIES = ['P0', 'P1', 'P2', 'P3']
const prioColor = (p?: string) => (p === 'P0' ? 'red' : p === 'P1' ? 'orange' : 'blue')

export default function FunctionalCases() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const ungrouped = t('func.ungrouped', '未分组')
  const [cases, setCases] = useState<FunctionalCase[]>([])
  const [loading, setLoading] = useState(false)
  const [search, setSearch] = useState('')
  const [moduleKey, setModuleKey] = useState('ALL')
  const [createOpen, setCreateOpen] = useState(false)
  const tabs = useWorkTabs()

  const load = async () => {
    if (!projectId) return setCases([])
    setLoading(true)
    try {
      setCases(await api.functionalCases(projectId))
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

  // 左:按模块(自由文本)聚合成树。
  const treeData = useMemo(() => {
    const byMod = new Map<string, number>()
    cases.forEach((c) => byMod.set(c.module || ungrouped, (byMod.get(c.module || ungrouped) || 0) + 1))
    return [
      {
        key: 'ALL',
        title: `${t('func.allCases', '全部用例')} (${cases.length})`,
        children: [...byMod.entries()].map(([m, n]) => ({ key: `mod:${m}`, title: `${m} (${n})` })),
      },
    ]
  }, [cases, ungrouped, t])

  const filtered = useMemo(
    () =>
      cases.filter((c) => {
        const mod = moduleKey === 'ALL' || (c.module || ungrouped) === moduleKey.replace('mod:', '')
        return mod && c.name.toLowerCase().includes(search.toLowerCase())
      }),
    [cases, search, moduleKey, ungrouped],
  )

  // 导出 xlsx(浏览器下载)/ 导入 xlsx(选文件即上传,返回导入条数)。
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

  if (!projectId)
    return <div style={{ padding: 48 }}><Empty description={t('common.selectProject', '请先在顶部选择项目')} /></div>

  const columns: ColumnsType<FunctionalCase> = [
    { title: t('func.colName', '名称'), dataIndex: 'name', ellipsis: true },
    { title: t('func.colModule', '模块'), dataIndex: 'module', width: 140, render: (m?: string) => m || ungrouped },
    { title: t('func.colPriority', '优先级'), dataIndex: 'priority', width: 90, render: (p?: string) => <Tag color={prioColor(p)}>{p || 'P2'}</Tag> },
    { title: t('func.colSteps', '步骤'), dataIndex: 'steps', width: 70, render: (s?: CaseStep[]) => (s?.length || 0) },
    { title: t('func.colStatus', '状态'), dataIndex: 'status', width: 110, render: (s?: string) => <Tag>{s || 'PREPARED'}</Tag> },
  ]

  const left = (
    <>
      <PaneHeader title={t('func.colModule', '模块')} />
      <div style={{ flex: 1, overflow: 'auto', padding: 8 }}>
        <Tree blockNode defaultExpandAll selectedKeys={[moduleKey]} treeData={treeData} onSelect={(k) => k.length && setModuleKey(String(k[0]))} />
      </div>
    </>
  )

  const detailTabs = tabs.openIds
    .map((id) => cases.find((c) => c.id === id))
    .filter((c): c is FunctionalCase => !!c)
    .map((c) => ({ key: c.id, label: c.name, children: <CaseDetail c={c} /> }))

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
            onNew={() => setCreateOpen(true)}
            newLabel={t('func.newCase', '新建用例')}
            extraActions={
              <>
                <Upload showUploadList={false} accept=".xlsx" beforeUpload={(f) => { doImport(f as File); return false }}>
                  <Button icon={<ImportOutlined />}>{t('func.import', '导入')}</Button>
                </Upload>
                <Button icon={<DownloadOutlined />} onClick={doExport} disabled={!cases.length}>{t('func.export', '导出')}</Button>
              </>
            }
            onSearch={setSearch}
            searchPlaceholder={t('func.searchName', '搜索用例名')}
            onRefresh={load}
            columns={columns}
            data={filtered}
            loading={loading}
            onRowClick={(c) => tabs.open(c.id)}
            emptyText={t('func.emptyCase', '暂无用例')}
          />
        }
      />
      <CreateCaseModal
        open={createOpen}
        projectId={projectId}
        defaultModule={moduleKey.startsWith('mod:') ? moduleKey.replace('mod:', '') : ''}
        onClose={() => setCreateOpen(false)}
        onCreated={() => {
          setCreateOpen(false)
          load()
        }}
      />
    </>
  )
}

function CaseDetail({ c }: { c: FunctionalCase }) {
  const { t } = useI18n()
  const cf = c.customFields || {}
  const [covReqs, setCovReqs] = useState<CaseRequirementLink[]>([])
  const loadCov = () => api.caseRequirements(c.id).then(setCovReqs).catch(() => setCovReqs([]))
  // 主动关联需求:选需求 → 选验收标准 → link。
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
                <Descriptions.Item label={t('func.colModule', '模块')}>{c.module || t('func.ungrouped', '未分组')}</Descriptions.Item>
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

function CreateCaseModal({
  open,
  projectId,
  defaultModule,
  onClose,
  onCreated,
}: {
  open: boolean
  projectId: string
  defaultModule: string
  onClose: () => void
  onCreated: () => void
}) {
  const { t } = useI18n()
  const [form] = Form.useForm()
  const [saving, setSaving] = useState(false)
  return (
    <Modal title={t('func.createTitle', '新建功能用例')} open={open} onCancel={onClose} onOk={() => form.submit()} confirmLoading={saving} destroyOnHidden width={680}>
      <Form
        form={form}
        layout="vertical"
        preserve={false}
        initialValues={{ priority: 'P1', module: defaultModule, steps: [{ step: '', expected: '' }] }}
        onFinish={async (v) => {
          setSaving(true)
          try {
            const customFields: Record<string, string> = {}
            if (v.prerequisite?.trim()) customFields['前置条件'] = v.prerequisite.trim()
            if (v.remark?.trim()) customFields['备注'] = v.remark.trim()
            await api.createFunctionalCase({
              projectId,
              name: v.name,
              priority: v.priority,
              module: v.module || undefined,
              steps: (v.steps || []).filter((s: CaseStep) => s.step.trim() || s.expected.trim()),
              customFields: Object.keys(customFields).length ? customFields : undefined,
            })
            message.success(t('func.created', '用例已创建'))
            onCreated()
          } catch (e) {
            message.error(e instanceof ApiError ? e.message : t('func.createFailed', '创建失败'))
          } finally {
            setSaving(false)
          }
        }}
      >
        <Form.Item name="name" label={t('func.caseName', '用例名')} rules={[{ required: true }]}>
          <Input placeholder={t('func.namePlaceholder', '如:登录成功')} />
        </Form.Item>
        <Space.Compact style={{ width: '100%' }}>
          <Form.Item name="module" label={t('func.colModule', '模块')} style={{ flex: 1 }}>
            <Input placeholder={t('func.modulePlaceholder', '如:登录')} />
          </Form.Item>
          <Form.Item name="priority" label={t('func.colPriority', '优先级')} style={{ width: 120 }}>
            <Select options={PRIORITIES.map((p) => ({ value: p, label: p }))} />
          </Form.Item>
        </Space.Compact>
        <Form.Item name="prerequisite" label={t('func.prerequisite', '前置条件')}>
          <Input.TextArea rows={2} placeholder={t('func.prerequisitePlaceholder', '如:进入管理登录页 https://...')} />
        </Form.Item>
        <Form.Item name="steps" label={t('func.testSteps', '测试步骤(步骤 + 预期结果)')}>
          <StepsEditor />
        </Form.Item>
        <Form.Item name="remark" label={t('func.remark', '备注')}>
          <Input.TextArea rows={2} />
        </Form.Item>
      </Form>
    </Modal>
  )
}
