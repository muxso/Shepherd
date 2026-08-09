import { useEffect, useMemo, useState } from 'react'
import {
  Button, Card, Col, Form, Input, InputNumber, Row, Switch, Spin, Alert, Tag,
  Table, Modal, Select, Popconfirm, Popover, Space, Empty,
} from 'antd'
import {
  SaveOutlined, ExperimentOutlined, DatabaseOutlined, RobotOutlined,
  TeamOutlined, PlusOutlined, EditOutlined, DeleteOutlined, ApiOutlined,
  CheckCircleFilled, CloseCircleFilled,
} from '@ant-design/icons'
import {
  api, ApiError, type RagConfigBody, type RagConfigView, type RagVisibilityGroup, type Role, type User,
} from '../api'
import { message } from '../feedback'
import { useI18n } from '../i18n'

// System-level RAG config, laid out in three columns matching the pipeline:
//   left = embedding (vectorize) · middle = vector store / retrieval · right = generation (output).
// Overrides the SHEPHERD_RAG_* env fallbacks and hot-reloads on the server (no restart).
// API keys are write-only: the form shows whether one is set, and only sends a key when changed.

export default function RagSettings({ embedded }: { embedded?: boolean } = {}) {
  const { t } = useI18n()
  const [form] = Form.useForm<RagConfigBody>()
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [embedKeySet, setEmbedKeySet] = useState(false)
  const [chatKeySet, setChatKeySet] = useState(false)
  const [testing, setTesting] = useState(false)
  const [testResult, setTestResult] = useState<import('../api').RagTestResult | null>(null)

  const runTest = async () => {
    setTesting(true)
    setTestResult(null)
    try {
      setTestResult(await api.testRagConfig())
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : String(e))
    } finally {
      setTesting(false)
    }
  }

  useEffect(() => {
    api
      .ragConfig()
      .then((c: RagConfigView) => {
        setEmbedKeySet(c.embedKeySet)
        setChatKeySet(c.chatKeySet)
        form.setFieldsValue({
          embedUrl: c.embedUrl,
          embedModel: c.embedModel,
          embedDim: c.embedDim,
          embedKey: '',
          chatUrl: c.chatUrl,
          chatModel: c.chatModel,
          chatKey: '',
          maxTokens: c.maxTokens,
          topK: c.topK,
          rerank: c.rerank,
        })
      })
      .catch((e) => message.error(e instanceof ApiError ? e.message : String(e)))
      .finally(() => setLoading(false))
  }, [form])

  const onSave = async () => {
    const v = await form.validateFields()
    setSaving(true)
    try {
      // Empty key field = keep the stored key; only send a non-empty value.
      const body: RagConfigBody = { ...v }
      if (!body.embedKey) delete body.embedKey
      if (!body.chatKey) delete body.chatKey
      await api.saveRagConfig(body)
      message.success(t('rag.saved', '已保存,已即时生效'))
      if (body.embedKey) setEmbedKeySet(true)
      if (body.chatKey) setChatKeySet(true)
      form.setFieldsValue({ embedKey: '', chatKey: '' })
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : String(e))
    } finally {
      setSaving(false)
    }
  }

  if (loading) {
    return (
      <div style={{ display: 'flex', justifyContent: 'center', paddingTop: 120 }}>
        <Spin size="large" />
      </div>
    )
  }

  const required = [{ required: true, message: t('rag.required', '必填') }]

  return (
    <div style={embedded ? { maxWidth: 1100 } : { maxWidth: 1240, margin: '0 auto', padding: '24px 16px' }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 16 }}>
        <h2 style={{ margin: 0, fontSize: 18 }}>
          <ExperimentOutlined style={{ marginRight: 8 }} />
          {t('sys.rag', 'RAG 配置')}
        </h2>
        <Space>
          <Button icon={<ApiOutlined />} loading={testing} onClick={runTest}>
            {t('rag.testConn', '测试连接')}
          </Button>
          <Button type="primary" icon={<SaveOutlined />} loading={saving} onClick={onSave}>
            {t('common.save', '保存')}
          </Button>
        </Space>
      </div>
      {(!embedKeySet || !chatKeySet) && (
        <Alert
          type="warning"
          showIcon
          style={{ marginBottom: 12 }}
          message={t('rag.notSet', '尚未完成配置:知识问答、AI 评审需要 Embedding 与生成模型的接口地址和 API Key。未配置时可按关键词入库,但语义检索与问答不可用。')}
        />
      )}
      {testResult && (
        <Alert
          type={testResult.embed.ok && testResult.chat.ok ? 'success' : 'error'}
          style={{ marginBottom: 12 }}
          closable
          onClose={() => setTestResult(null)}
          message={
            <Space size={24} wrap>
              <ProbeLine label={t('rag.embedSection', '向量嵌入 Embedding')} probe={testResult.embed} t={t} />
              <ProbeLine label={t('rag.chatSection', '组合输出 Generation')} probe={testResult.chat} t={t} />
            </Space>
          }
        />
      )}
      <Alert
        type="info"
        showIcon
        style={{ marginBottom: 20 }}
        message={t(
          'rag.hint',
          '用于知识问答(聊天)与需求 AI 评审的检索/生成模型。留空 API Key 表示沿用已保存的密钥,保存后即时生效。',
        )}
      />
      <Form form={form} layout="vertical" requiredMark={false}>
        <Row gutter={16}>
          {/* Left: embedding — turns text into vectors. */}
          <Col xs={24} lg={8}>
            <Card
              title={
                <span>
                  <ExperimentOutlined style={{ marginRight: 8 }} />
                  {t('rag.embedSection', '向量嵌入 Embedding')}
                </span>
              }
              styles={{ body: { paddingBottom: 4 } }}
            >
              <Form.Item name="embedUrl" label={t('rag.embedUrl', '嵌入接口地址')} rules={required}>
                <Input placeholder="https://api.openai.com/v1/embeddings" />
              </Form.Item>
              <Form.Item name="embedModel" label={t('rag.embedModel', '嵌入模型')} rules={required}>
                <Input placeholder="text-embedding-3-small" />
              </Form.Item>
              <Form.Item
                name="embedDim"
                label={t('rag.embedDim', '向量维度')}
                tooltip={t('rag.embedDimTip', '需与嵌入模型输出维度一致,修改后需重新入库')}
                rules={required}
              >
                <InputNumber min={1} max={8192} style={{ width: '100%' }} />
              </Form.Item>
              <Form.Item
                name="embedKey"
                label={t('rag.embedKey', '嵌入 API Key')}
                extra={embedKeySet ? t('rag.keySet', '已设置(留空保持不变)') : t('rag.keyUnset', '尚未设置')}
              >
                <Input.Password placeholder={embedKeySet ? '••••••••' : 'sk-...'} autoComplete="new-password" />
              </Form.Item>
            </Card>
          </Col>

          {/* Middle: vector store — where vectors live and how many are recalled. */}
          <Col xs={24} lg={8}>
            <Card
              title={
                <span>
                  <DatabaseOutlined style={{ marginRight: 8 }} />
                  {t('rag.storeSection', '向量数据库')}
                </span>
              }
              styles={{ body: { paddingBottom: 4 } }}
            >
              <Form.Item label={t('rag.storeBackend', '存储后端')}>
                <Tag color="blue">PostgreSQL</Tag>
                <span style={{ color: 'var(--muted, #999)', fontSize: 12 }}>
                  {t('rag.storeBackendNote', '内置 rag_cosine 余弦检索,无需额外扩展')}
                </span>
              </Form.Item>
              <Form.Item
                name="topK"
                label={t('rag.topK', '召回条数 Top-K')}
                tooltip={t('rag.topKTip', '每次问答从库中召回的知识块数量')}
                rules={required}
              >
                <InputNumber min={1} max={20} style={{ width: '100%' }} />
              </Form.Item>
              <Form.Item
                name="rerank"
                label={t('rag.rerank', 'LLM 重排')}
                tooltip={t('rag.rerankTip', '召回后用生成模型对相关性重新排序,更准但更慢')}
                valuePropName="checked"
              >
                <Switch />
              </Form.Item>
            </Card>
          </Col>

          {/* Right: generation — combines context into an answer. */}
          <Col xs={24} lg={8}>
            <Card
              title={
                <span>
                  <RobotOutlined style={{ marginRight: 8 }} />
                  {t('rag.chatSection', '组合输出 Generation')}
                </span>
              }
              styles={{ body: { paddingBottom: 4 } }}
            >
              <Form.Item name="chatUrl" label={t('rag.chatUrl', '生成接口地址')} rules={required}>
                <Input placeholder="https://api.openai.com/v1/chat/completions" />
              </Form.Item>
              <Form.Item name="chatModel" label={t('rag.chatModel', '生成模型')} rules={required}>
                <Input placeholder="gpt-4o-mini" />
              </Form.Item>
              <Form.Item name="maxTokens" label={t('rag.maxTokens', '生成最大 Token')} rules={required}>
                <InputNumber min={1} max={32768} style={{ width: '100%' }} />
              </Form.Item>
              <Form.Item
                name="chatKey"
                label={t('rag.chatKey', '生成 API Key')}
                extra={chatKeySet ? t('rag.keySet', '已设置(留空保持不变)') : t('rag.keyUnset', '尚未设置')}
              >
                <Input.Password placeholder={chatKeySet ? '••••••••' : 'sk-...'} autoComplete="new-password" />
              </Form.Item>
            </Card>
          </Col>
        </Row>
      </Form>

      <VisibilityGroups />
    </div>
  )
}

// One provider's probe result in the "测试连接" banner: green/red icon + latency or error message.
function ProbeLine({ label, probe, t }: {
  label: string
  probe: import('../api').RagProbe
  t: (key: string, fallback: string) => string
}) {
  return (
    <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
      {probe.ok ? <CheckCircleFilled style={{ color: '#52c41a' }} /> : <CloseCircleFilled style={{ color: '#ff4d4f' }} />}
      <b>{label}</b>
      <span style={{ color: 'var(--text-3)' }}>
        {probe.ok
          ? `${t('rag.connOk', '连接正常')}${probe.latencyMs != null ? ` · ${probe.latencyMs}ms` : ''}`
          : (probe.error || t('rag.connFail', '连接失败'))}
      </span>
    </span>
  )
}

// Admin-managed audience taxonomy: a visibility group bundles RBAC role names; documents are tagged
// with groups, and a caller sees a doc when they hold one of its groups' roles. Editing a group here
// re-scopes every document in it immediately (live reference, not a snapshot).
function VisibilityGroups() {
  const { t } = useI18n()
  const [groups, setGroups] = useState<RagVisibilityGroup[]>([])
  const [roles, setRoles] = useState<Role[]>([])
  const [users, setUsers] = useState<User[]>([])
  const [loading, setLoading] = useState(true)
  const [editing, setEditing] = useState<RagVisibilityGroup | null>(null)
  const [open, setOpen] = useState(false)
  const [form] = Form.useForm<{ name: string; roleNames: string[] }>()

  const load = () => {
    setLoading(true)
    Promise.all([
      api.ragGroups(),
      api.roles().then((p) => p.items).catch(() => [] as Role[]),
      api.users().then((p) => p.items).catch(() => [] as User[]),
    ])
      .then(([g, r, u]) => { setGroups(g); setRoles(r); setUsers(u) })
      .catch((e) => message.error(e instanceof ApiError ? e.message : String(e)))
      .finally(() => setLoading(false))
  }
  useEffect(load, [])

  // Who can see a group's docs = users whose roles (userGroups = role names) intersect its roleNames.
  const membersOf = (g: RagVisibilityGroup): User[] => {
    if (!g.roleNames.length) return []
    const wanted = new Set(g.roleNames)
    return users.filter((u) => (u.userGroups ?? []).some((rn) => wanted.has(rn)))
  }

  // Distinct role names for the picker (roles can repeat a name across scopes).
  const roleOptions = useMemo(() => {
    const seen = new Set<string>()
    const opts: { value: string; label: string }[] = []
    for (const r of roles) {
      if (r.name && !seen.has(r.name)) { seen.add(r.name); opts.push({ value: r.name, label: r.name }) }
    }
    return opts
  }, [roles])

  const openCreate = () => { setEditing(null); form.setFieldsValue({ name: '', roleNames: [] }); setOpen(true) }
  const openEdit = (g: RagVisibilityGroup) => {
    setEditing(g); form.setFieldsValue({ name: g.name, roleNames: g.roleNames }); setOpen(true)
  }

  const submit = async () => {
    const v = await form.validateFields()
    try {
      if (editing) await api.updateRagGroup(editing.id, { name: v.name, roleNames: v.roleNames || [] })
      else await api.createRagGroup({ name: v.name, roleNames: v.roleNames || [] })
      message.success(t('common.saved', '已保存'))
      setOpen(false); load()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : String(e))
    }
  }

  const remove = async (g: RagVisibilityGroup) => {
    try {
      await api.deleteRagGroup(g.id)
      message.success(t('common.deleted', '已删除'))
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : String(e))
    }
  }

  return (
    <Card
      style={{ marginTop: 20 }}
      title={
        <span>
          <TeamOutlined style={{ marginRight: 8 }} />
          {t('rag.groups', '知识库可见组')}
        </span>
      }
      extra={
        <Button type="primary" icon={<PlusOutlined />} onClick={openCreate}>
          {t('rag.groupNew', '新建可见组')}
        </Button>
      }
    >
      <Alert
        type="info"
        showIcon
        style={{ marginBottom: 16 }}
        message={t(
          'rag.groupsHint',
          '可见组 = 一组角色。给文档打上可见组后,只有拥有组内角色的用户能检索到该文档。改动这里的组,所有挂该组的文档实时生效。未打组的文档仅上传者与管理员可见。',
        )}
      />
      <Table<RagVisibilityGroup>
        rowKey="id"
        size="middle"
        loading={loading}
        dataSource={groups}
        pagination={false}
        locale={{ emptyText: <Empty description={t('rag.groupsEmpty', '还没有可见组')} /> }}
        columns={[
          { title: t('rag.groupName', '名称'), dataIndex: 'name', width: 220 },
          {
            title: t('rag.groupRoles', '包含角色'),
            dataIndex: 'roleNames',
            render: (rns: string[]) =>
              rns.length ? (
                <Space size={[4, 4]} wrap>
                  {rns.map((n) => <Tag key={n} color="geekblue">{n}</Tag>)}
                </Space>
              ) : (
                <span style={{ color: 'var(--text-3)' }}>{t('rag.groupNoRole', '(无角色 — 无人可见)')}</span>
              ),
          },
          {
            title: t('rag.groupMembers', '可见成员'),
            width: 130,
            render: (_: unknown, g: RagVisibilityGroup) => {
              const ms = membersOf(g)
              if (!ms.length) return <span style={{ color: 'var(--text-3)' }}>—</span>
              return (
                <Popover
                  placement="topLeft"
                  title={t('rag.groupMembersTitle', '命中该组角色的用户')}
                  content={
                    <div style={{ maxWidth: 320, maxHeight: 260, overflow: 'auto', display: 'flex', flexDirection: 'column', gap: 4 }}>
                      {ms.map((u) => (
                        <div key={u.id} style={{ fontSize: 13 }}>
                          {u.name || u.email}
                          {u.name && <span style={{ color: 'var(--text-3)', marginLeft: 6 }}>{u.email}</span>}
                        </div>
                      ))}
                    </div>
                  }
                >
                  <Tag style={{ cursor: 'pointer' }} color="blue">{t('rag.groupMemberCount', '{n} 人').replace('{n}', String(ms.length))}</Tag>
                </Popover>
              )
            },
          },
          {
            title: t('common.actions', '操作'),
            width: 140,
            render: (_: unknown, g: RagVisibilityGroup) => (
              <Space>
                <Button size="small" type="link" icon={<EditOutlined />} onClick={() => openEdit(g)}>
                  {t('common.edit', '编辑')}
                </Button>
                <Popconfirm
                  title={t('rag.groupDelConfirm', '删除后,挂此组的文档将移除该组')}
                  onConfirm={() => remove(g)}
                  okText={t('common.ok', '确定')}
                  cancelText={t('common.cancel', '取消')}
                >
                  <Button size="small" type="link" danger icon={<DeleteOutlined />}>
                    {t('common.delete', '删除')}
                  </Button>
                </Popconfirm>
              </Space>
            ),
          },
        ]}
      />
      <Modal
        open={open}
        title={editing ? t('rag.groupEdit', '编辑可见组') : t('rag.groupNew', '新建可见组')}
        onCancel={() => setOpen(false)}
        onOk={submit}
        okText={t('common.save', '保存')}
        cancelText={t('common.cancel', '取消')}
        destroyOnClose
      >
        <Form form={form} layout="vertical" requiredMark={false}>
          <Form.Item
            name="name"
            label={t('rag.groupName', '名称')}
            rules={[{ required: true, message: t('rag.required', '必填') }]}
          >
            <Input placeholder={t('rag.groupNamePh', '如:对外 / 研发内部')} />
          </Form.Item>
          <Form.Item
            name="roleNames"
            label={t('rag.groupRoles', '包含角色')}
            tooltip={t('rag.groupRolesTip', '拥有其中任一角色的用户即可看到挂此组的文档')}
          >
            <Select
              mode="multiple"
              allowClear
              options={roleOptions}
              placeholder={t('rag.groupRolesPh', '选择角色(销售 / 产品 / 研发 / 测试 …)')}
            />
          </Form.Item>
        </Form>
      </Modal>
    </Card>
  )
}
