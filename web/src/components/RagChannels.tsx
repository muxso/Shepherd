import { useEffect, useState } from 'react'
import { Alert, Button, Card, Form, Input, InputNumber, Modal, Popconfirm, Select, Space, Switch, Table, Tag, Typography } from 'antd'
import { PlusOutlined, ReloadOutlined } from '@ant-design/icons'
import { api, http, type User } from '../api'
import { useApp } from '../context'
import { message } from '../feedback'

type Conversation = { kind: 'group' | 'direct'; externalId: string; projectId: string; enabled: boolean }
type Channel = { id: string; name: string; serviceUserId: string; enabled: boolean; allowBinding: boolean; conversations: Conversation[]; status?: { connected: boolean; authenticated: boolean; reportedAt: string; lastMessageAt?: number; activeQueries: number; heartbeatAgeSeconds?: number }; pending: { kind: 'group' | 'direct'; externalId: string; senderId: string; seenAt: string }[] }
type Policy = Omit<Channel, 'id' | 'status' | 'pending'>
const when = (value?: string | number) => value ? new Date(value).toLocaleString('zh-CN', { timeZone: 'Asia/Shanghai', hour12: false }) : '尚无记录'

export default function RagChannels() {
  const { projects, projectId } = useApp()
  const [channels, setChannels] = useState<Channel[]>([])
  const [users, setUsers] = useState<User[]>([])
  const [loading, setLoading] = useState(false)
  const [saving, setSaving] = useState(false)
  const [open, setOpen] = useState(false)
  const [editing, setEditing] = useState<Channel>()
  const [codeChannel, setCodeChannel] = useState<Channel>()
  const [codeProject, setCodeProject] = useState('')
  const [minutes, setMinutes] = useState(30)
  const [code, setCode] = useState('')
  const [form] = Form.useForm<Policy>()
  const error = (e: unknown) => message.error(e instanceof Error ? e.message : '操作失败')
  const load = () => { setLoading(true); http.get<{ items: Channel[] }>('/system/rag/channels').then(r => setChannels(r.items)).catch(error).finally(() => setLoading(false)) }
  useEffect(() => { load(); api.users().then(r => setUsers(r.items)).catch(error); const timer = setInterval(load, 30000); return () => clearInterval(timer) }, [])
  const edit = (channel?: Channel, pending?: Conversation) => {
    setEditing(channel)
    form.setFieldsValue(channel ? { ...channel, conversations: [...channel.conversations.filter(p => !pending || p.kind !== pending.kind || p.externalId !== pending.externalId), ...(pending ? [pending] : [])] } : { name: '', serviceUserId: '', enabled: false, allowBinding: false, conversations: [] })
    setOpen(true)
  }
  const save = async () => {
    const value = await form.validateFields(); setSaving(true)
    const body: Policy = { name: value.name, serviceUserId: value.serviceUserId, enabled: value.enabled, allowBinding: value.allowBinding, conversations: value.conversations || [] }
    try { await (editing ? http.put(`/system/rag/channels/${encodeURIComponent(editing.id)}`, body) : http.post('/system/rag/channels', body)); setOpen(false); load(); message.success('授权已保存，下次消息核验时生效') } catch (e) { error(e) } finally { setSaving(false) }
  }
  return <Card title="企业微信 · 会话授权与连接" style={{ marginTop: 20 }} extra={<Space><Button icon={<ReloadOutlined />} onClick={load}>刷新</Button><Button type="primary" icon={<PlusOutlined />} onClick={() => edit()}>添加机器人渠道</Button></Space>}>
    <Alert type="info" showIcon title="授权和群绑定均由后台控制" description="先选择机器人的专用账号，再开启渠道并授权具体群聊或私聊用户。群绑定默认关闭；开启后可生成限定项目的一次性绑定码。机器人密钥保存在部署 Secret 中，不在此处录入。会话内所有成员都能看到机器人回复，请仅共享允许该会话访问的资料。" style={{ marginBottom: 16 }} />
    <Table rowKey="id" dataSource={channels} loading={loading} pagination={false} columns={[
      { title: '渠道', dataIndex: 'name', render: (name: string, c: Channel) => <><b>{name}</b><div style={{ color: 'var(--text-3)', fontSize: 12 }}>专用账号：{users.find(u => u.id === c.serviceUserId)?.name || c.serviceUserId}</div></> },
      { title: '授权', render: (_, c: Channel) => <Space><Tag color={c.enabled ? 'green' : undefined}>{c.enabled ? '已开启' : '已停用'}</Tag><Tag>{c.conversations.filter(p => p.enabled).length} 个会话</Tag><Tag>{c.allowBinding ? '允许绑定群' : '群绑定关闭'}</Tag></Space> },
      { title: '连接状态', render: (_, c: Channel) => <><Tag color={c.status?.connected ? 'green' : 'orange'}>{c.status?.connected ? '已连接' : '未连接或心跳过期'}</Tag><div style={{ color: 'var(--text-3)', fontSize: 12 }}>最近消息 {when(c.status?.lastMessageAt)} · 查询中 {c.status?.activeQueries || 0}</div><div style={{ color: 'var(--text-3)', fontSize: 12 }}>状态上报 {when(c.status?.reportedAt)}</div></> },
      { title: '操作', render: (_, c: Channel) => <Space wrap><Button size="small" onClick={() => edit(c)}>配置授权</Button><Button size="small" disabled={!c.enabled || !c.allowBinding} onClick={() => { setCode(''); setCodeProject(projectId); setCodeChannel(c) }}>生成绑定码</Button><Popconfirm title="删除渠道将立即撤销所有会话授权，确认删除？" onConfirm={async () => { try { await http.del(`/system/rag/channels/${encodeURIComponent(c.id)}`); load() } catch (e) { error(e) } }}><Button size="small" danger>删除</Button></Popconfirm></Space> },
    ]} expandable={{ expandedRowRender: c => <>
      <Table size="small" rowKey={r => `${r.kind}:${r.externalId}`} pagination={false} dataSource={c.conversations} columns={[{ title: '类型', render: (_, p) => p.kind === 'group' ? '群聊' : '私聊用户' }, { title: '会话／用户 ID', dataIndex: 'externalId' }, { title: '项目', render: (_, p) => projects.find(x => x.id === p.projectId)?.name || p.projectId }, { title: '状态', render: (_, p) => p.enabled ? '已授权' : '已停用' }]} />
      <h4>待授权会话（仅保存身份和接收时间，不保存消息正文）</h4>
      <Table size="small" rowKey={r => `${r.kind}:${r.externalId}`} pagination={false} dataSource={c.pending} columns={[{ title: '类型', render: (_, p) => p.kind === 'group' ? '群聊' : '私聊用户' }, { title: '会话／用户 ID', dataIndex: 'externalId' }, { title: '最近接收', render: (_, p) => when(p.seenAt) }, { title: '操作', render: (_, p) => <Button size="small" onClick={() => edit(c, { ...p, projectId: '', enabled: true })}>选择项目并授权</Button> }]} />
    </> }} />
    <Modal title={editing ? '配置机器人授权' : '添加机器人渠道'} open={open} onCancel={() => setOpen(false)} onOk={save} confirmLoading={saving} width={880} destroyOnHidden>
      <Form form={form} layout="vertical">
        <Form.Item name="name" label="渠道名称" rules={[{ required: true }]}><Input maxLength={120} /></Form.Item>
        <Form.Item name="serviceUserId" label="机器人专用账号" extra="账号须已加入授权项目；机器人 API Key 必须属于该账号，读取权限仍由其角色控制。" rules={[{ required: true }]}><Select showSearch optionFilterProp="label" options={users.map(u => ({ value: u.id, label: u.name }))} /></Form.Item>
        <Space size={32}><Form.Item name="enabled" label="开启渠道授权" valuePropName="checked"><Switch /></Form.Item><Form.Item name="allowBinding" label="允许一次性绑定群" valuePropName="checked"><Switch /></Form.Item></Space>
        <Form.List name="conversations">{(fields, { add, remove }) => <>
          {fields.map(field => <Space key={field.key} align="start" wrap>
            <Form.Item name={[field.name, 'kind']} rules={[{ required: true }]}><Select style={{ width: 100 }} options={[{ value: 'group', label: '群聊' }, { value: 'direct', label: '私聊用户' }]} /></Form.Item>
            <Form.Item name={[field.name, 'externalId']} rules={[{ required: true, whitespace: true }]}><Input placeholder="群 chatid 或私聊 userid" style={{ width: 240 }} /></Form.Item>
            <Form.Item name={[field.name, 'projectId']} rules={[{ required: true }]}><Select placeholder="授权项目" style={{ width: 200 }} options={projects.map(p => ({ value: p.id, label: p.name }))} /></Form.Item>
            <Form.Item name={[field.name, 'enabled']} valuePropName="checked"><Switch checkedChildren="开" unCheckedChildren="关" /></Form.Item><Button danger onClick={() => remove(field.name)}>移除</Button>
          </Space>)}<Button onClick={() => add({ kind: 'group', enabled: true, projectId: '' })}>添加关联会话</Button>
        </>}</Form.List>
      </Form>
    </Modal>
    <Modal title="生成一次性群绑定码" open={!!codeChannel} onCancel={() => { setCodeChannel(undefined); setCode('') }} footer={null}>
      {code ? <Alert type="success" title="绑定码仅在本窗口展示" description={<><Typography.Paragraph copyable>{`绑定 ${code}`}</Typography.Paragraph><p>在目标群内 @ 机器人发送以上指令。有效期 {minutes} 分钟，仅可使用一次；关闭群绑定开关后未使用的码也无法绑定。</p></>} /> : <Space direction="vertical" style={{ width: '100%' }}><Select aria-label="绑定项目" placeholder="选择绑定项目" style={{ width: '100%' }} value={codeProject || undefined} onChange={setCodeProject} options={projects.map(p => ({ value: p.id, label: p.name }))} /><Space>有效期<InputNumber min={1} max={1440} value={minutes} onChange={v => setMinutes(v || 30)} />分钟</Space><Button type="primary" disabled={!codeProject} onClick={async () => { try { const r = await http.post<{ code: string }>(`/system/rag/channels/${encodeURIComponent(codeChannel!.id)}/binding-code`, { projectId: codeProject, expiresMinutes: minutes }); setCode(r.code) } catch (e) { error(e) } }}>生成</Button></Space>}
    </Modal>
  </Card>
}
