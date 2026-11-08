import { useEffect, useState } from 'react'
import { Alert, Button, Drawer, Form, Input, Modal, Popconfirm, Select, Space, Table, Tag } from 'antd'
import { ReadOutlined } from '@ant-design/icons'
import { api, http } from '../api'
import { useApp } from '../context'
import { message } from '../feedback'

type Entry = { documentId: string; title: string; version: number; updatedAt: number; visibilityGroups: string[] }
type Document = Entry & { text: string; sourceUrl: string; expectedVersion: number }
export default function OnboardingKnowledge({ onAsk }: { onAsk?: (question: string) => void }) {
  const { projectId, projects } = useApp()
  const [open, setOpen] = useState(false)
  const [items, setItems] = useState<Entry[]>([])
  const [groups, setGroups] = useState<{ value: string; label: string }[]>([])
  const [editing, setEditing] = useState<Document>()
  const [editor, setEditor] = useState(false)
  const [loading, setLoading] = useState(false)
  const [saving, setSaving] = useState(false)
  const [form] = Form.useForm<Document>()
  const error = (e: unknown) => message.error(e instanceof Error ? e.message : '操作失败')
  const load = () => { if (!projectId) return; setLoading(true); http.get<{ items: Entry[] }>(`/rag/onboarding?projectId=${encodeURIComponent(projectId)}`).then(r => setItems(r.items)).catch(error).finally(() => setLoading(false)) }
  useEffect(() => { if (open) { load(); api.ragGroups().then(r => setGroups(r.map(g => ({ value: g.id, label: g.name })))).catch(error) } }, [open, projectId])
  const edit = async (entry?: Entry) => {
    try {
      const value = entry ? await http.get<Document>(`/rag/onboarding/${encodeURIComponent(entry.documentId)}`) : undefined
      setEditing(value)
      form.setFieldsValue(value ? { ...value, expectedVersion: value.version } : { title: '', text: '', sourceUrl: '', visibilityGroups: [], expectedVersion: 0 })
      setEditor(true)
    } catch (e) { error(e) }
  }
  const publish = async () => {
    const value = await form.validateFields(); setSaving(true)
    try {
      const result = await http.post<{ version: number; embedded: boolean }>('/rag/onboarding', { projectId, documentId: editing?.documentId, title: value.title, text: value.text, sourceUrl: value.sourceUrl || '', visibilityGroups: value.visibilityGroups || [], expectedVersion: editing?.version || 0 })
      message.success(`已发布第 ${result.version} 版${result.embedded ? '，可用于问答' : '，暂按关键词检索，可在配置 Embedding 后回填'}`)
      setEditor(false); load()
    } catch (e) { error(e) } finally { setSaving(false) }
  }
  return <>
    <Button aria-label="入职知识" icon={<ReadOutlined />} disabled={!projectId} onClick={() => setOpen(true)}>入职知识</Button>
    <Drawer title={`新人入职知识 · ${projects.find(p => p.id === projectId)?.name || '当前项目'}`} open={open} onClose={() => setOpen(false)} width={920} extra={<Button type="primary" onClick={() => edit()}>维护指南／FAQ</Button>}>
      <Alert type="info" showIcon title="维护真实制度与入职资料，发布后用于网页和机器人问答" description="支持 Markdown 指南、常见问答、原始来源链接和版本更新。可见组控制谁能检索；留空仅维护者与管理员可见。要让企微回答，请关联机器人专用账号所在角色的可见组。未维护的制度不会自动生成。" style={{ marginBottom: 16 }} />
      {onAsk && <Space wrap style={{ marginBottom: 16 }}>{['新人入职需要办理哪些手续？', '新员工如何申请账号和权限？', '入职资料里有哪些常见问题？'].map(q => <Button key={q} onClick={() => { setOpen(false); onAsk(q) }}>{q}</Button>)}</Space>}
      <Table loading={loading} rowKey="documentId" dataSource={items} pagination={{ pageSize: 10 }} columns={[
        { title: '资料', dataIndex: 'title' }, { title: '版本', render: (_, r) => <Tag>v{r.version}</Tag> },
        { title: '最近更新（上海）', render: (_, r) => new Date(r.updatedAt).toLocaleString('zh-CN', { timeZone: 'Asia/Shanghai', hour12: false }) },
        { title: '可见范围', render: (_, r) => r.visibilityGroups.length ? r.visibilityGroups.map(id => <Tag key={id}>{groups.find(g => g.value === id)?.label || id}</Tag>) : '维护者与管理员' },
        { title: '操作', render: (_, r) => <Space><Button size="small" onClick={() => edit(r)}>查看／修订</Button><Popconfirm title="删除后该资料将不再参与问答，确认？" onConfirm={async () => { try { await api.deleteRagDoc(r.documentId); load() } catch (e) { error(e) } }}><Button size="small" danger>删除</Button></Popconfirm></Space> },
      ]} />
    </Drawer>
    <Modal title={editing ? `修订入职资料 · 当前 v${editing.version}` : '维护入职指南／FAQ'} open={editor} onCancel={() => setEditor(false)} onOk={publish} okText="发布并更新问答知识" confirmLoading={saving} width={850} destroyOnHidden>
      <Form form={form} layout="vertical">
        <Form.Item name="title" label="资料标题" rules={[{ required: true, whitespace: true }]}><Input maxLength={200} placeholder="例如：新人账号申请指南" /></Form.Item>
        <Form.Item name="sourceUrl" label="原始制度／文档链接" rules={[{ validator: (_, value) => !value || /^https?:\/\/[^\s]+$/.test(value) || /^\/(?!\/)/.test(value) ? Promise.resolve() : Promise.reject(new Error('请填写有效链接')) }]}><Input placeholder="维护资料的真实来源，可留空" /></Form.Item>
        <Form.Item name="visibilityGroups" label="可见组"><Select mode="multiple" allowClear options={groups} placeholder="留空仅维护者与管理员可见" /></Form.Item>
        <Form.Item name="text" label="指南内容／常见问答（Markdown）" extra="建议写清维护负责人、适用范围、生效日期、办理入口，以及问题和实际答案。请核实后发布；不预置公司制度。" rules={[{ required: true, whitespace: true }]}><Input.TextArea rows={15} maxLength={200000} showCount placeholder={'# 入职指南\n\n## 适用范围与维护负责人\n\n## 办理步骤与入口\n\n## 常见问题\n### 问题\n实际答案及来源'} /></Form.Item>
      </Form>
    </Modal>
  </>
}
