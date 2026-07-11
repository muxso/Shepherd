import { useEffect, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import {
  Breadcrumb,
  Button,
  Card,
  Col,
  Form,
  Input,
  InputNumber,
  Radio,
  Row,
  Segmented,
  Select,
  Space,
  Switch,
  Table,
  Tag,
  Tooltip,
} from 'antd'
import { message, modal } from '../feedback'
import { DeleteOutlined, PlusOutlined, QuestionCircleOutlined, ReloadOutlined, SearchOutlined } from '@ant-design/icons'
import type { ColumnsType } from 'antd/es/table'
import {
  api,
  ApiError,
  type Organization,
  type PoolNode,
  type ResourcePool,
  type ResourcePoolInput,
} from '../api'
import { useI18n } from '../i18n'

type TFn = (k: string, d?: string) => string

// 资源池(系统模块子标签,对齐参考图):列表页 + Node/Kubernetes 全屏新增/编辑表单。
// 后端字段:name/enabled/description/maxConcurrency/poolType/allOrg/orgIds/serverUrl/config/时间。

// ---------------- 列表页 ----------------
export function ResourcePoolsPage() {
  const { t } = useI18n()
  const nav = useNavigate()
  const [pools, setPools] = useState<ResourcePool[]>([])
  const [orgs, setOrgs] = useState<Organization[]>([])
  const [loading, setLoading] = useState(false)
  const [q, setQ] = useState('')

  const load = () => {
    setLoading(true)
    api
      .resourcePools()
      .then((p) => setPools(Array.isArray(p) ? p : []))
      .catch(() => setPools([]))
      .finally(() => setLoading(false))
    api.organizations().then((p) => setOrgs(p.items)).catch(() => setOrgs([]))
  }
  useEffect(load, [])

  const orgName = (id: string) => orgs.find((o) => o.id === id)?.name || id
  const rows = pools.filter((p) => !q || p.name.toLowerCase().includes(q.toLowerCase()))

  // 状态开关:用整行数据 PUT 回去,仅翻转 enabled。
  const toggle = async (p: ResourcePool, enabled: boolean) => {
    try {
      await api.updateResourcePool(p.id, { ...toInput(p), enabled })
      message.success(enabled ? t('pool.enabled', '已启用') : t('pool.disabled', '已停用'))
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('pool.opFailed', '操作失败'))
    }
  }

  const remove = (p: ResourcePool) => {
    modal.confirm({
      title: t('pool.deleteTitle', '删除资源池'),
      content: t('pool.deleteContent', '确认删除「{name}」?').replace('{name}', p.name),
      okButtonProps: { danger: true },
      okText: t('a.delete', '删除'),
      cancelText: t('a.cancel', '取消'),
      onOk: async () => {
        try {
          await api.deleteResourcePool(p.id)
          message.success(t('pool.deleted', '已删除'))
          load()
        } catch (e) {
          message.error(e instanceof ApiError ? e.message : t('pool.deleteFailed', '删除失败'))
        }
      },
    })
  }

  const cols: ColumnsType<ResourcePool> = [
    {
      title: t('res.colName', '名称'),
      dataIndex: 'name',
      render: (v: string, p) => (
        <a style={{ color: 'var(--brand)' }} onClick={() => nav(`/resource-pool/${p.id}/edit`)}>
          {v}
        </a>
      ),
    },
    {
      title: t('pool.status', '状态'),
      dataIndex: 'enabled',
      width: 90,
      render: (v: boolean, p) => <Switch size="small" checked={v} onChange={(c) => toggle(p, c)} />,
    },
    { title: t('pool.maxConcurrency', '最大并发数'), dataIndex: 'maxConcurrency', width: 120 },
    {
      title: t('pool.applyOrg', '应用组织'),
      dataIndex: 'allOrg',
      width: 160,
      render: (allOrg: boolean, p) =>
        allOrg ? (
          <Tag>{t('pool.allOrg', '全部组织')}</Tag>
        ) : p.orgIds.length ? (
          <Space size={4} wrap>
            {p.orgIds.map((id) => (
              <Tag key={id}>{orgName(id)}</Tag>
            ))}
          </Space>
        ) : (
          '—'
        ),
    },
    { title: t('res.colDesc', '描述'), dataIndex: 'description', render: (v: string) => v || '—' },
    { title: t('pool.type', '类型'), dataIndex: 'poolType', width: 110, render: (v: string) => v || 'Node' },
    { title: t('pool.createdAt', '创建时间'), dataIndex: 'createdAt', width: 170, render: (v: string) => v || '—' },
    { title: t('pool.updatedAt', '更新时间'), dataIndex: 'updatedAt', width: 170, render: (v: string) => v || '—' },
    {
      title: t('apidef.colAction', '操作'),
      width: 120,
      fixed: 'right',
      render: (_v, p) => (
        <Space size={4}>
          <Button type="link" size="small" onClick={() => nav(`/resource-pool/${p.id}/edit`)}>
            {t('a.edit', '编辑')}
          </Button>
          <Button type="link" size="small" danger onClick={() => remove(p)}>
            {t('a.delete', '删除')}
          </Button>
        </Space>
      ),
    },
  ]

  return (
    <div style={{ height: '100%', overflow: 'auto', padding: 12, background: 'var(--bg)' }}>
      <Card size="small" styles={{ body: { padding: 12 } }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
          <Button type="primary" onClick={() => nav('/resource-pool/new')}>
            {t('pool.add', '添加资源池')}
          </Button>
          <div style={{ flex: 1 }} />
          <Input
            allowClear
            size="small"
            prefix={<SearchOutlined style={{ color: 'var(--text-3)' }} />}
            placeholder={t('pool.searchName', '通过名称搜索')}
            style={{ width: 240 }}
            value={q}
            onChange={(e) => setQ(e.target.value)}
          />
          <Tooltip title={t('a.refresh', '刷新')}>
            <Button size="small" icon={<ReloadOutlined />} onClick={load} />
          </Tooltip>
        </div>
        <Table<ResourcePool>
          rowKey="id"
          size="middle"
          loading={loading}
          dataSource={rows}
          columns={cols}
          scroll={{ x: 1200 }}
          pagination={{
            defaultPageSize: 50,
            size: 'small',
            showSizeChanger: true,
            pageSizeOptions: [10, 20, 50, 100],
            showTotal: (n) => `${t('apidef.totalPrefix', '共')} ${n} ${t('proj.unit', '条')}`,
          }}
        />
      </Card>
    </div>
  )
}

// 视图 → 表单/提交入参(列表里翻转 enabled 时复用)。
function toInput(p: ResourcePool): ResourcePoolInput {
  return {
    name: p.name,
    enabled: p.enabled,
    description: p.description,
    maxConcurrency: p.maxConcurrency,
    poolType: p.poolType,
    allOrg: p.allOrg,
    orgIds: p.orgIds,
    serverUrl: p.serverUrl,
    config: p.config,
  }
}

// ---------------- 新增 / 编辑表单 ----------------
const NEW_NODE: PoolNode = { ip: '', port: '', concurrentNumber: 10, singleTaskConcurrentNumber: 3 }

interface FormShape {
  name: string
  description: string
  serverUrl: string
  allOrg: boolean
  orgIds: string[]
  poolType: string
  nodes: PoolNode[]
  // K8s
  k8sIp: string
  token: string
  namespace: string
  deployName: string
  concurrentNumber: number
  singleTaskConcurrentNumber: number
}

const EMPTY: FormShape = {
  name: '',
  description: '',
  serverUrl: '',
  allOrg: true,
  orgIds: [],
  poolType: 'Node',
  nodes: [{ ...NEW_NODE }],
  k8sIp: '',
  token: '',
  namespace: '',
  deployName: '',
  concurrentNumber: 10,
  singleTaskConcurrentNumber: 3,
}

function poolToForm(p: ResourcePool): FormShape {
  const c = p.config || {}
  return {
    name: p.name,
    description: p.description,
    serverUrl: p.serverUrl,
    allOrg: p.allOrg,
    orgIds: p.orgIds || [],
    poolType: p.poolType || 'Node',
    nodes: c.nodes?.length ? c.nodes : [{ ...NEW_NODE }],
    k8sIp: c.ip || '',
    token: c.token || '',
    namespace: c.namespace || '',
    deployName: c.deployName || '',
    concurrentNumber: c.concurrentNumber ?? 10,
    singleTaskConcurrentNumber: c.singleTaskConcurrentNumber ?? 3,
  }
}

// 表单值 → 提交入参:按类型组装 config 与 maxConcurrency。
function formToInput(v: FormShape): ResourcePoolInput {
  const base = {
    name: v.name,
    description: v.description,
    serverUrl: v.serverUrl,
    allOrg: v.allOrg,
    orgIds: v.allOrg ? [] : v.orgIds,
    poolType: v.poolType,
  }
  if (v.poolType === 'Kubernetes') {
    return {
      ...base,
      maxConcurrency: v.concurrentNumber || 0,
      config: {
        ip: v.k8sIp,
        token: v.token,
        namespace: v.namespace,
        deployName: v.deployName,
        concurrentNumber: v.concurrentNumber,
        singleTaskConcurrentNumber: v.singleTaskConcurrentNumber,
      },
    }
  }
  const nodes = (v.nodes || []).filter((n) => n.ip.trim())
  return {
    ...base,
    maxConcurrency: nodes.reduce((s, n) => s + (Number(n.concurrentNumber) || 0), 0),
    config: { nodes },
  }
}

function downloadText(name: string, text: string) {
  const a = document.createElement('a')
  a.href = `data:text/yaml;charset=utf-8,${encodeURIComponent(text)}`
  a.download = name
  a.click()
}

const ROLE_YAML = `apiVersion: rbac.authorization.k8s.io/v1
kind: Role
metadata:
  name: shepherd-role
rules:
  - apiGroups: ["", "apps"]
    resources: ["pods", "deployments", "daemonsets"]
    verbs: ["get", "list", "watch", "create", "delete"]
`

export function ResourcePoolForm() {
  const { t } = useI18n()
  const nav = useNavigate()
  const { id } = useParams()
  const isEdit = !!id
  const [form] = Form.useForm<FormShape>()
  const [orgs, setOrgs] = useState<Organization[]>([])
  const [saving, setSaving] = useState(false)
  const poolType = Form.useWatch('poolType', form) || 'Node'
  const allOrg = Form.useWatch('allOrg', form)
  const [nodeMode, setNodeMode] = useState<'single' | 'batch'>('single')
  const [batchText, setBatchText] = useState('')

  useEffect(() => {
    api.organizations().then((p) => setOrgs(p.items)).catch(() => setOrgs([]))
    if (isEdit && id) {
      api
        .getResourcePool(id)
        .then((p) => form.setFieldsValue(poolToForm(p)))
        .catch((e) => message.error(e instanceof ApiError ? e.message : t('pool.loadFailed', '加载失败')))
    }
  }, [id]) // eslint-disable-line react-hooks/exhaustive-deps

  // 批量文本(每行 ip:port[:max[:single]])→ 节点表。
  const applyBatch = () => {
    const nodes: PoolNode[] = batchText
      .split('\n')
      .map((l) => l.trim())
      .filter(Boolean)
      .map((l) => {
        const [ip, port, max, single] = l.split(/[:\s,]+/)
        return {
          ip: ip || '',
          port: port || '',
          concurrentNumber: Number(max) || 10,
          singleTaskConcurrentNumber: Number(single) || 3,
        }
      })
    if (nodes.length) {
      form.setFieldValue('nodes', nodes)
      setNodeMode('single')
      message.success(t('pool.batchApplied', '已解析 {n} 个节点').replace('{n}', String(nodes.length)))
    }
  }

  const submit = async (stay: boolean) => {
    let v: FormShape
    try {
      v = await form.validateFields()
    } catch {
      return
    }
    setSaving(true)
    try {
      const body = formToInput(v)
      if (isEdit && id) {
        await api.updateResourcePool(id, body)
        message.success(t('pool.saved', '已保存'))
        nav('/resource-pool')
      } else {
        await api.createResourcePool(body)
        message.success(t('pool.created', '创建成功'))
        if (stay) {
          form.resetFields()
          form.setFieldsValue(EMPTY)
        } else {
          nav('/resource-pool')
        }
      }
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('pool.saveFailed', '保存失败'))
    } finally {
      setSaving(false)
    }
  }

  const labelHelp = (label: string, help: string) => (
    <span>
      {label}{' '}
      <Tooltip title={help}>
        <QuestionCircleOutlined style={{ color: 'var(--text-3)' }} />
      </Tooltip>
    </span>
  )

  return (
    <div style={{ height: '100%', display: 'flex', flexDirection: 'column', background: 'var(--bg)' }}>
      <div style={{ padding: '10px 16px', background: 'var(--panel)', borderBottom: '1px solid var(--border-soft)' }}>
        <Breadcrumb
          items={[
            { title: <a onClick={() => nav('/resource-pool')}>{t('res.pool', '资源池')}</a> },
            { title: isEdit ? t('pool.edit', '编辑资源池') : t('pool.add', '添加资源池') },
          ]}
        />
      </div>
      <div style={{ flex: 1, overflow: 'auto', padding: 16 }}>
        <Card styles={{ body: { padding: 24 } }}>
          <div style={{ fontWeight: 600, fontSize: 15, marginBottom: 20 }}>
            {isEdit ? t('pool.edit', '编辑资源池') : t('pool.add', '添加资源池')}
          </div>
          <Form form={form} layout="vertical" initialValues={EMPTY} style={{ maxWidth: 1100 }}>
            {/* 基础信息:名称/类型/回连地址/组织范围/描述,两栏排布减少滚动 */}
            <div style={{ fontWeight: 600, marginBottom: 12 }}>{t('pool.sectionBasic', '基础信息')}</div>
            <Row gutter={24}>
              <Col span={24} lg={12}>
                <Form.Item
                  name="name"
                  label={t('pool.name', '资源池名称')}
                  rules={[{ required: true, message: t('pool.nameRequired', '请输入资源池名称') }]}
                >
                  <Input placeholder={t('pool.namePlaceholder', '请输入资源池名称')} maxLength={64} />
                </Form.Item>
              </Col>
              <Col span={24} lg={12}>
                <Form.Item name="poolType" label={t('pool.type', '类型')}>
                  <Segmented
                    options={[
                      { value: 'Node', label: 'Node' },
                      { value: 'Kubernetes', label: 'Kubernetes' },
                    ]}
                  />
                </Form.Item>
              </Col>
              <Col span={24} lg={12}>
                <Form.Item name="serverUrl" label={labelHelp(t('pool.serverUrl', '工作节点 URL'), t('pool.serverUrlHelp', 'MS 的部署地址,用于回连工作节点'))}>
                  <Input placeholder={t('pool.serverUrlPlaceholder', 'MS的部署地址')} />
                </Form.Item>
              </Col>
              <Col span={24} lg={12}>
                <Form.Item label={t('pool.applyOrg', '应用组织')}>
                  <Form.Item name="allOrg" noStyle>
                    <Radio.Group>
                      <Radio value={true}>
                        {labelHelp(t('pool.allOrg', '全部组织'), t('pool.allOrgHelp', '所有组织都可使用该资源池'))}
                      </Radio>
                      <Radio value={false}>{t('pool.specifiedOrg', '指定组织')}</Radio>
                    </Radio.Group>
                  </Form.Item>
                </Form.Item>
                {allOrg === false && (
                  <Form.Item
                    name="orgIds"
                    label={t('pool.specifiedOrg', '指定组织')}
                    rules={[{ required: true, message: t('pool.orgRequired', '请选择组织') }]}
                  >
                    <Select
                      mode="multiple"
                      allowClear
                      placeholder={t('pool.orgPlaceholder', '请选择组织')}
                      options={orgs.map((o) => ({ value: o.id, label: o.name }))}
                    />
                  </Form.Item>
                )}
              </Col>
              {/* 多行描述占整行 */}
              <Col span={24}>
                <Form.Item name="description" label={t('res.colDesc', '描述')}>
                  <Input.TextArea rows={3} placeholder={t('pool.descPlaceholder', '请对该资源池进行描述')} maxLength={500} />
                </Form.Item>
              </Col>
            </Row>

            {/* 容量/配置:随 poolType 切换的字段 */}
            <div style={{ fontWeight: 600, marginTop: 8, marginBottom: 12 }}>{t('pool.sectionConfig', '容量与配置')}</div>
            {poolType === 'Node' ? (
              <NodeSection
                t={t}
                mode={nodeMode}
                onMode={setNodeMode}
                batchText={batchText}
                onBatchText={setBatchText}
                onApplyBatch={applyBatch}
              />
            ) : (
              <K8sSection t={t} labelHelp={labelHelp} />
            )}
          </Form>
        </Card>
      </div>
      {/* 底部操作条 */}
      <div style={{ padding: '10px 24px', background: 'var(--panel)', borderTop: '1px solid var(--border-soft)', textAlign: 'right' }}>
        <Space>
          <Button onClick={() => nav('/resource-pool')}>{t('a.cancel', '取消')}</Button>
          {!isEdit && (
            <Button loading={saving} onClick={() => submit(true)}>
              {t('pool.saveAndContinue', '保存并继续添加')}
            </Button>
          )}
          <Button type="primary" loading={saving} onClick={() => submit(false)}>
            {isEdit ? t('a.save', '保存') : t('a.confirmAdd', '添加')}
          </Button>
        </Space>
      </div>
    </div>
  )
}

// Node:单个添加(节点表)/ 批量添加(文本)。
function NodeSection({
  t,
  mode,
  onMode,
  batchText,
  onBatchText,
  onApplyBatch,
}: {
  t: TFn
  mode: 'single' | 'batch'
  onMode: (m: 'single' | 'batch') => void
  batchText: string
  onBatchText: (s: string) => void
  onApplyBatch: () => void
}) {
  return (
    <Form.Item label={t('pool.addNode', '添加节点')}>
      <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 8 }}>
        <Segmented
          size="small"
          value={mode}
          onChange={(v) => onMode(v as 'single' | 'batch')}
          options={[
            { value: 'single', label: t('pool.singleAdd', '单个添加') },
            { value: 'batch', label: t('pool.batchAdd', '批量添加') },
          ]}
        />
      </div>
      {mode === 'single' ? (
        <Card size="small" styles={{ body: { padding: 12 } }}>
          <Form.List name="nodes">
            {(fields, { add, remove }) => (
              <>
                <div style={{ display: 'flex', gap: 8, padding: '0 4px 6px', color: 'var(--text-2)', fontSize: 13 }}>
                  <div style={{ flex: 2 }}>
                    IP <span style={{ color: '#f5222d' }}>*</span>
                  </div>
                  <div style={{ flex: 2 }}>
                    Port <span style={{ color: '#f5222d' }}>*</span>
                  </div>
                  <div style={{ flex: 2 }}>
                    {t('pool.maxConcurrency', '最大并发数')} <span style={{ color: '#f5222d' }}>*</span>
                  </div>
                  <div style={{ flex: 2 }}>
                    {t('pool.singleTaskMax', '单个任务最大并发数')} <span style={{ color: '#f5222d' }}>*</span>
                  </div>
                  <div style={{ width: 32 }} />
                </div>
                {fields.map((field) => (
                  <div key={field.key} style={{ display: 'flex', gap: 8, marginBottom: 8 }}>
                    <Form.Item
                      name={[field.name, 'ip']}
                      style={{ flex: 2, marginBottom: 0 }}
                      rules={[{ required: true, message: t('pool.ipRequired', '请输入 IP 地址') }]}
                    >
                      <Input placeholder={t('pool.ipPlaceholder', '请输入 IP 地址')} />
                    </Form.Item>
                    <Form.Item
                      name={[field.name, 'port']}
                      style={{ flex: 2, marginBottom: 0 }}
                      rules={[{ required: true, message: t('pool.portRequired', '请输入 Port') }]}
                    >
                      <Input placeholder={t('pool.portPlaceholder', '请输入 Port')} />
                    </Form.Item>
                    <Form.Item name={[field.name, 'concurrentNumber']} style={{ flex: 2, marginBottom: 0 }}>
                      <InputNumber min={1} style={{ width: '100%' }} />
                    </Form.Item>
                    <Form.Item name={[field.name, 'singleTaskConcurrentNumber']} style={{ flex: 2, marginBottom: 0 }}>
                      <InputNumber min={1} style={{ width: '100%' }} />
                    </Form.Item>
                    <Button
                      type="text"
                      icon={<DeleteOutlined />}
                      style={{ width: 32 }}
                      disabled={fields.length <= 1}
                      onClick={() => remove(field.name)}
                    />
                  </div>
                ))}
                <Button type="link" icon={<PlusOutlined />} onClick={() => add({ ...NEW_NODE })} style={{ paddingLeft: 4 }}>
                  {t('pool.addNode', '添加节点')}
                </Button>
              </>
            )}
          </Form.List>
        </Card>
      ) : (
        <Card size="small" styles={{ body: { padding: 12 } }}>
          <Input.TextArea
            rows={6}
            value={batchText}
            onChange={(e) => onBatchText(e.target.value)}
            placeholder={t('pool.batchPlaceholder', '每行一个节点,格式:IP:Port:最大并发数:单个任务最大并发数\n例如:127.0.0.1:8082:10:3')}
          />
          <Button type="primary" size="small" style={{ marginTop: 8 }} onClick={onApplyBatch}>
            {t('pool.parseBatch', '解析为节点')}
          </Button>
        </Card>
      )}
    </Form.Item>
  )
}

// Kubernetes:IP/Token/命名空间/Deploy Name + 并发,两栏排布。
function K8sSection({ t, labelHelp }: { t: TFn; labelHelp: (l: string, h: string) => React.ReactNode }) {
  return (
    <Row gutter={24}>
      <Col span={24} lg={12}>
        <Form.Item
          name="k8sIp"
          label={t('pool.k8sIp', 'IP 地址/域名')}
          extra={t('pool.k8sIpHint', '例如:100.0.0.100 或 example.com')}
          rules={[{ required: true, message: t('pool.k8sIpRequired', '请输入 IP 地址/域名') }]}
        >
          <Input placeholder="example.com" />
        </Form.Item>
      </Col>
      <Col span={24} lg={12}>
        <Form.Item name="token" label="Token" rules={[{ required: true, message: t('pool.tokenRequired', '请输入 Token') }]}>
          <Input.Password placeholder={t('pool.tokenPlaceholder', '请输入 Token')} />
        </Form.Item>
      </Col>
      <Col span={24} lg={12}>
        <Form.Item
          name="namespace"
          label={t('pool.namespace', '命名空间')}
          rules={[{ required: true, message: t('pool.namespaceRequired', '请输入命名空间') }]}
        >
          <Space.Compact style={{ width: '100%' }}>
            <Input placeholder={t('pool.namespacePlaceholder', '使用K8S资源池需要部署Role.yaml文件')} />
            <Button onClick={() => downloadText('Role.yaml', ROLE_YAML)}>{t('pool.downloadYaml', '下载 YAML 文件')}</Button>
          </Space.Compact>
        </Form.Item>
      </Col>
      <Col span={24} lg={12}>
        <Form.Item
          name="deployName"
          label="Deploy Name"
          rules={[{ required: true, message: t('pool.deployRequired', '请输入 Deploy Name') }]}
        >
          <Space.Compact style={{ width: '100%' }}>
            <Input placeholder={t('pool.deployPlaceholder', '执行接口测试需要部署 Daemonset.yaml 或 Deployment.yaml 文件')} />
            <Button onClick={() => downloadText('Daemonset.yaml', `# Daemonset\nkind: DaemonSet\n`)}>
              {t('pool.downloadYaml', '下载 YAML 文件')}
            </Button>
          </Space.Compact>
        </Form.Item>
      </Col>
      <Col span={24} lg={12}>
        <Form.Item
          name="concurrentNumber"
          label={labelHelp(t('pool.maxConcurrency', '最大并发数'), t('pool.maxConcurrencyHelp', '该资源池可并行执行的任务数上限'))}
          rules={[{ required: true }]}
        >
          <InputNumber min={1} style={{ width: 160 }} />
        </Form.Item>
      </Col>
      <Col span={24} lg={12}>
        <Form.Item
          name="singleTaskConcurrentNumber"
          label={labelHelp(t('pool.singleTaskMax', '单个任务最大并发数'), t('pool.singleTaskMaxHelp', '单个任务可占用的并发数上限'))}
          rules={[{ required: true }]}
        >
          <InputNumber min={1} style={{ width: 160 }} />
        </Form.Item>
      </Col>
    </Row>
  )
}
