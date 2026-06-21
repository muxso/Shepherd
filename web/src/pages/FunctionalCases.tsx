import { useEffect, useMemo, useState } from 'react'
import { Descriptions, Empty, Form, Input, Modal, Select, Space, Table, Tabs, Tag, Tree } from 'antd'
import { message } from '../feedback'
import type { ColumnsType } from 'antd/es/table'
import { api, ApiError, type CaseStep, type FunctionalCase } from '../api'
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
                  <div style={{ background: '#fafafa', border: '1px solid #f0f0f0', borderRadius: 6, padding: 10, minHeight: 40, whiteSpace: 'pre-wrap' }}>
                    {cf['前置条件'] || <span style={{ color: '#bbb' }}>{t('func.none', '无')}</span>}
                  </div>
                </div>
                <div>
                  <div style={{ fontWeight: 600, marginBottom: 6 }}>{t('func.stepsDesc', '步骤描述')}</div>
                  {stepsTable}
                </div>
                <div>
                  <div style={{ fontWeight: 600, marginBottom: 6 }}>{t('func.remark', '备注')}</div>
                  <div style={{ background: '#fafafa', border: '1px solid #f0f0f0', borderRadius: 6, padding: 10, minHeight: 40, whiteSpace: 'pre-wrap' }}>
                    {cf['备注'] || <span style={{ color: '#bbb' }}>{t('func.none', '无')}</span>}
                  </div>
                </div>
              </Space>
            ),
          },
          { key: 'steps', label: `${t('func.tabStepsExpected', '步骤与预期')} (${c.steps?.length || 0})`, children: stepsTable },
        ]}
      />
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
