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
import { CopyOutlined, QuestionCircleOutlined, ReloadOutlined, SearchOutlined } from '@ant-design/icons'
import type { ColumnsType } from 'antd/es/table'
import {
  api,
  ApiError,
  type Organization,
  type PoolRunnerInfo,
  type ResourcePool,
  type ResourcePoolInput,
} from '../api'
import { useI18n } from '../i18n'

type TFn = (k: string, d?: string) => string

// Resource pools (system-module sub-tab, matches the reference UI): list page + full-screen Node/Kubernetes create/edit forms.
// Backend fields: name/enabled/description/maxConcurrency/poolType/allOrg/orgIds/serverUrl/config/timestamps.

// ---------------- List page ----------------
export function ResourcePoolsPage() {
  const { t } = useI18n()
  const nav = useNavigate()
  const [pools, setPools] = useState<ResourcePool[]>([])
  const [orgs, setOrgs] = useState<Organization[]>([])
  const [online, setOnline] = useState<Record<string, number>>({})
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
    // Connected pool-runner processes (WS registry), refreshed with the list.
    api.poolRunnerStatus().then(setOnline).catch(() => setOnline({}))
  }
  useEffect(load, [])

  const orgName = (id: string) => orgs.find((o) => o.id === id)?.name || id
  const rows = pools.filter((p) => !q || p.name.toLowerCase().includes(q.toLowerCase()))

  // Status toggle: PUT the whole row back with only `enabled` flipped.
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
      title: t('pool.onlineRunners', '在线执行机'),
      key: 'onlineRunners',
      width: 110,
      render: (_v, p) => {
        const n = online[p.id] ?? 0
        return (
          <Tag color={n > 0 ? 'green' : 'default'} style={{ margin: 0 }}>
            {n > 0 ? n : t('pool.offline', '无')}
          </Tag>
        )
      },
    },
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

// View model → submit input (reused when the list flips `enabled`).
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

// ---------------- Create / edit form ----------------
interface FormShape {
  name: string
  description: string
  serverUrl: string
  allOrg: boolean
  orgIds: string[]
  poolType: string
  // Node: pool-level cap; runners register themselves over WS.
  maxConcurrency: number
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
  maxConcurrency: 10,
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
    maxConcurrency: p.maxConcurrency || 10,
    k8sIp: c.ip || '',
    token: c.token || '',
    namespace: c.namespace || '',
    deployName: c.deployName || '',
    concurrentNumber: c.concurrentNumber ?? 10,
    singleTaskConcurrentNumber: c.singleTaskConcurrentNumber ?? 3,
  }
}

// Form values → submit input: assemble config and maxConcurrency per pool type.
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
  // Node pools have no manual node list: runners join over WS with the pool name.
  return { ...base, maxConcurrency: v.maxConcurrency || 0, config: {} }
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
  const watchedName = Form.useWatch('name', form)

  useEffect(() => {
    api.organizations().then((p) => setOrgs(p.items)).catch(() => setOrgs([]))
    if (isEdit && id) {
      api
        .getResourcePool(id)
        .then((p) => form.setFieldsValue(poolToForm(p)))
        .catch((e) => message.error(e instanceof ApiError ? e.message : t('pool.loadFailed', '加载失败')))
    }
  }, [id]) // eslint-disable-line react-hooks/exhaustive-deps

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
            {/* Basic info: name/type/callback URL/org scope/description; two-column layout to reduce scrolling */}
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
              <Col span={24}>
                <Form.Item name="description" label={t('res.colDesc', '描述')}>
                  <Input.TextArea rows={3} placeholder={t('pool.descPlaceholder', '请对该资源池进行描述')} maxLength={500} />
                </Form.Item>
              </Col>
            </Row>

            {/* Capacity/config: fields switch with poolType */}
            <div style={{ fontWeight: 600, marginTop: 8, marginBottom: 12 }}>{t('pool.sectionConfig', '容量与配置')}</div>
            {poolType === 'Node' ? (
              <Row gutter={24}>
                <Col span={24} lg={12}>
                  <Form.Item
                    name="maxConcurrency"
                    label={labelHelp(t('pool.maxConcurrency', '最大并发数'), t('pool.maxConcurrencyHelp', '该资源池可并行执行的任务数上限'))}
                    rules={[{ required: true, message: t('pool.maxConcurrencyRequired', '请输入最大并发数') }]}
                  >
                    <InputNumber min={1} style={{ width: 160 }} />
                  </Form.Item>
                </Col>
              </Row>
            ) : (
              <K8sSection t={t} labelHelp={labelHelp} />
            )}
          </Form>
          {/* Runner access: Node pools get workers by starting pool-runner processes
              that register over WS with the pool name — no manual IP/Port entry. */}
          {isEdit && poolType === 'Node' && <RunnerJoinPanel poolId={id!} poolName={watchedName || ''} t={t} />}
        </Card>
      </div>
      {/* Bottom action bar */}
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

// Runner access panel (Node pool edit view): copyable join command using the
// pool NAME plus a live table of the runners registered over WS.
function RunnerJoinPanel({ poolId, poolName, t }: { poolId: string; poolName: string; t: TFn }) {
  const nav = useNavigate()
  const [runners, setRunners] = useState<PoolRunnerInfo[]>([])
  useEffect(() => {
    const tick = () =>
      api.poolRunnerStatusDetail().then((d) => setRunners(d[poolId] || [])).catch(() => setRunners([]))
    tick()
    const timer = window.setInterval(tick, 5000)
    return () => window.clearInterval(timer)
  }, [poolId])

  const proto = window.location.protocol === 'https:' ? 'wss' : 'ws'
  const cmd = `SHEPHERD_SERVER_WS=${proto}://${window.location.host}/api/pool-runner/ws SHEPHERD_POOL_NAME='${poolName}' SHEPHERD_RUNNER_KEY=<sak_...> cargo r -p pool-runner`
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(cmd)
      message.success(t('pool.copied', '已复制'))
    } catch {
      message.error(t('pool.copyFailed', '复制失败'))
    }
  }

  const cols: ColumnsType<PoolRunnerInfo> = [
    { title: t('pool.runnerName', '名称'), dataIndex: 'name' },
    { title: t('pool.runnerCap', '并发上限'), dataIndex: 'maxConcurrent', width: 110 },
    {
      title: t('pool.runnerInFlight', '在跑'),
      dataIndex: 'inFlight',
      width: 90,
      render: (v: number) => <Tag color={v > 0 ? 'processing' : 'default'} style={{ margin: 0 }}>{v}</Tag>,
    },
  ]

  return (
    <div style={{ maxWidth: 1100, marginTop: 8 }}>
      <div style={{ fontWeight: 600, marginBottom: 12 }}>{t('pool.sectionJoin', '执行机接入')}</div>
      <div style={{ color: 'var(--text-2)', fontSize: 13, marginBottom: 8 }}>
        {t('pool.joinCmdHint', '在执行机上运行以下命令即可接入本资源池;')}
        <a onClick={() => nav('/system/apikeys')}>{t('pool.joinKeyHint', '将 <sak_...> 替换为 API Key(API Keys 页生成)')}</a>
      </div>
      <div
        className="ms-mono"
        style={{
          display: 'flex',
          alignItems: 'flex-start',
          gap: 8,
          padding: '10px 12px',
          borderRadius: 8,
          border: '1px solid var(--border-soft)',
          background: 'var(--panel-2)',
          fontSize: 12.5,
          wordBreak: 'break-all',
          marginBottom: 12,
        }}
      >
        <code style={{ flex: 1 }}>{cmd}</code>
        <Tooltip title={t('pool.copyCmd', '复制命令')}>
          <Button size="small" icon={<CopyOutlined />} onClick={copy} />
        </Tooltip>
      </div>
      <div style={{ color: 'var(--text-2)', fontSize: 13, marginBottom: 6 }}>{t('pool.onlineRunners', '在线执行机')}</div>
      <Table<PoolRunnerInfo>
        rowKey={(r) => r.name}
        size="small"
        dataSource={runners}
        columns={cols}
        pagination={false}
        locale={{ emptyText: t('pool.noOnlineRunners', '暂无在线执行机,复制上方命令启动') }}
      />
    </div>
  )
}

// Kubernetes pool: IP/Token/namespace/deploy name + concurrency, two-column layout.
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
