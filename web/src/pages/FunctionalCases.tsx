import { useEffect, useMemo, useState } from 'react'
import { Descriptions, Empty, Form, Input, Modal, Select, Space, Table, Tabs, Tag, Tree, message } from 'antd'
import type { ColumnsType } from 'antd/es/table'
import { api, ApiError, type CaseStep, type FunctionalCase } from '../api'
import { useApp } from '../context'
import { Workspace, WorkList, PaneHeader, useWorkTabs } from '../components/Workspace'
import StepsEditor from '../components/StepsEditor'

const PRIORITIES = ['P0', 'P1', 'P2', 'P3']
const prioColor = (p?: string) => (p === 'P0' ? 'red' : p === 'P1' ? 'orange' : 'blue')

export default function FunctionalCases() {
  const { projectId } = useApp()
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
      message.error(e instanceof ApiError ? e.message : '加载失败')
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
    cases.forEach((c) => byMod.set(c.module || '未分组', (byMod.get(c.module || '未分组') || 0) + 1))
    return [
      {
        key: 'ALL',
        title: `全部用例 (${cases.length})`,
        children: [...byMod.entries()].map(([m, n]) => ({ key: `mod:${m}`, title: `${m} (${n})` })),
      },
    ]
  }, [cases])

  const filtered = useMemo(
    () =>
      cases.filter((c) => {
        const mod = moduleKey === 'ALL' || (c.module || '未分组') === moduleKey.replace('mod:', '')
        return mod && c.name.toLowerCase().includes(search.toLowerCase())
      }),
    [cases, search, moduleKey],
  )

  if (!projectId)
    return <div style={{ padding: 48 }}><Empty description="请先在顶部选择项目" /></div>

  const columns: ColumnsType<FunctionalCase> = [
    { title: '名称', dataIndex: 'name', ellipsis: true },
    { title: '模块', dataIndex: 'module', width: 140, render: (m?: string) => m || '未分组' },
    { title: '优先级', dataIndex: 'priority', width: 90, render: (p?: string) => <Tag color={prioColor(p)}>{p || 'P2'}</Tag> },
    { title: '步骤', dataIndex: 'steps', width: 70, render: (s?: CaseStep[]) => (s?.length || 0) },
    { title: '状态', dataIndex: 'status', width: 110, render: (s?: string) => <Tag>{s || 'PREPARED'}</Tag> },
  ]

  const left = (
    <>
      <PaneHeader title="模块" />
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
        listLabel="全部用例"
        activeKey={tabs.activeKey}
        onChange={tabs.setActiveKey}
        onClose={tabs.close}
        tabs={detailTabs}
        listContent={
          <WorkList<FunctionalCase>
            onNew={() => setCreateOpen(true)}
            newLabel="新建用例"
            onSearch={setSearch}
            searchPlaceholder="搜索用例名"
            onRefresh={load}
            columns={columns}
            data={filtered}
            loading={loading}
            onRowClick={(c) => tabs.open(c.id)}
            emptyText="暂无用例"
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
  return (
    <div style={{ padding: '12px 16px', height: '100%', overflow: 'auto' }}>
      <Tabs
        items={[
          {
            key: 'info',
            label: '基本信息',
            children: (
              <Descriptions column={2} size="small" bordered>
                <Descriptions.Item label="名称">{c.name}</Descriptions.Item>
                <Descriptions.Item label="模块">{c.module || '未分组'}</Descriptions.Item>
                <Descriptions.Item label="优先级"><Tag color={prioColor(c.priority)}>{c.priority || 'P2'}</Tag></Descriptions.Item>
                <Descriptions.Item label="状态">{c.status || 'PREPARED'}</Descriptions.Item>
              </Descriptions>
            ),
          },
          {
            key: 'steps',
            label: `步骤与预期 (${c.steps?.length || 0})`,
            children: (
              <Table<CaseStep>
                rowKey={(_, i) => String(i)}
                size="small"
                pagination={false}
                dataSource={c.steps || []}
                locale={{ emptyText: <Empty description="无步骤" /> }}
                columns={[
                  { title: '#', width: 50, render: (_, __, i) => i + 1 },
                  { title: '步骤描述', dataIndex: 'step' },
                  { title: '预期结果', dataIndex: 'expected' },
                ]}
              />
            ),
          },
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
  const [form] = Form.useForm()
  const [saving, setSaving] = useState(false)
  return (
    <Modal title="新建功能用例" open={open} onCancel={onClose} onOk={() => form.submit()} confirmLoading={saving} destroyOnHidden width={680}>
      <Form
        form={form}
        layout="vertical"
        preserve={false}
        initialValues={{ priority: 'P1', module: defaultModule, steps: [{ step: '', expected: '' }] }}
        onFinish={async (v) => {
          setSaving(true)
          try {
            await api.createFunctionalCase({
              projectId,
              name: v.name,
              priority: v.priority,
              module: v.module || undefined,
              steps: (v.steps || []).filter((s: CaseStep) => s.step.trim() || s.expected.trim()),
            })
            message.success('用例已创建')
            onCreated()
          } catch (e) {
            message.error(e instanceof ApiError ? e.message : '创建失败')
          } finally {
            setSaving(false)
          }
        }}
      >
        <Form.Item name="name" label="用例名" rules={[{ required: true }]}>
          <Input placeholder="如:登录成功" />
        </Form.Item>
        <Space.Compact style={{ width: '100%' }}>
          <Form.Item name="module" label="模块" style={{ flex: 1 }}>
            <Input placeholder="如:登录" />
          </Form.Item>
          <Form.Item name="priority" label="优先级" style={{ width: 120 }}>
            <Select options={PRIORITIES.map((p) => ({ value: p, label: p }))} />
          </Form.Item>
        </Space.Compact>
        <Form.Item name="steps" label="测试步骤(步骤 + 预期结果)">
          <StepsEditor />
        </Form.Item>
      </Form>
    </Modal>
  )
}
