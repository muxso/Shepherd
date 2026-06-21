import { useEffect, useState } from 'react'
import { Button, Empty, Form, Input, InputNumber, Modal, Space, Switch, Table, Tag } from 'antd'
import { message } from '../feedback'
import { PlusOutlined, ReloadOutlined } from '@ant-design/icons'
import { api, ApiError, type ApiDefinition, type ApiMock } from '../api'
import { useI18n } from '../i18n'

export default function MocksPanel({ definition }: { definition: ApiDefinition }) {
  const { t } = useI18n()
  const [mocks, setMocks] = useState<ApiMock[]>([])
  const [loading, setLoading] = useState(false)
  const [open, setOpen] = useState(false)

  const load = async () => {
    setLoading(true)
    try {
      setMocks(await api.mocks(definition.id))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('mock.loadFailed', '加载 Mock 失败'))
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [definition.id])

  return (
    <>
      <Space style={{ marginBottom: 12 }}>
        <Button type="primary" icon={<PlusOutlined />} size="small" onClick={() => setOpen(true)}>
          {t('mock.new', '新建 Mock')}
        </Button>
        <Button icon={<ReloadOutlined />} size="small" onClick={load} />
      </Space>
      <Table<ApiMock>
        rowKey="id"
        size="small"
        loading={loading}
        dataSource={mocks}
        pagination={false}
        locale={{ emptyText: <Empty description={t('mock.empty', '暂无 Mock 期望')} /> }}
        columns={[
          { title: t('mock.colName', '名称'), dataIndex: 'name', ellipsis: true },
          {
            title: t('mock.colStatus', '响应码'),
            dataIndex: 'responseStatus',
            width: 90,
            render: (s: number) => <Tag color={s < 400 ? 'green' : 'red'}>{s}</Tag>,
          },
          {
            title: t('mock.colBody', '响应体'),
            dataIndex: 'responseBody',
            ellipsis: true,
            render: (b: string | null) => <span className="ms-mono">{b || '—'}</span>,
          },
          {
            title: t('mock.colEnabled', '启用'),
            dataIndex: 'enabled',
            width: 70,
            render: (e: boolean) => (e ? <Tag color="green">{t('mock.yes', '是')}</Tag> : <Tag>{t('mock.no', '否')}</Tag>),
          },
        ]}
      />

      <Modal
        title={`${t('mock.new', '新建 Mock')} · ${definition.name}`}
        open={open}
        onCancel={() => setOpen(false)}
        footer={null}
        destroyOnHidden
        width={560}
      >
        <CreateMockForm
          definition={definition}
          onCreated={() => {
            setOpen(false)
            load()
          }}
        />
      </Modal>
    </>
  )
}

function CreateMockForm({
  definition,
  onCreated,
}: {
  definition: ApiDefinition
  onCreated: () => void
}) {
  const { t } = useI18n()
  const [form] = Form.useForm()
  const [saving, setSaving] = useState(false)
  return (
    <Form
      form={form}
      layout="vertical"
      initialValues={{ responseStatus: 200, enabled: true, responseBody: '{"ok":true}' }}
      onFinish={async (v) => {
        let matchRule: unknown = {}
        if (v.matchRule?.trim()) {
          try {
            matchRule = JSON.parse(v.matchRule)
          } catch {
            message.error(t('mock.invalidJson', '匹配规则不是合法 JSON'))
            return
          }
        }
        setSaving(true)
        try {
          await api.createMock(definition.id, {
            name: v.name,
            matchRule,
            responseStatus: v.responseStatus,
            responseBody: v.responseBody || undefined,
            enabled: v.enabled,
          })
          message.success(t('mock.created', 'Mock 已创建'))
          onCreated()
        } catch (e) {
          message.error(e instanceof ApiError ? e.message : t('mock.createFailed', '创建失败'))
        } finally {
          setSaving(false)
        }
      }}
    >
      <Form.Item name="name" label={t('mock.colName', '名称')} rules={[{ required: true }]}>
        <Input placeholder={t('mock.namePlaceholder', '如:默认成功响应')} />
      </Form.Item>
      <Space>
        <Form.Item name="responseStatus" label={t('mock.colStatus', '响应码')}>
          <InputNumber min={100} max={599} />
        </Form.Item>
        <Form.Item name="enabled" label={t('mock.colEnabled', '启用')} valuePropName="checked">
          <Switch />
        </Form.Item>
      </Space>
      <Form.Item name="matchRule" label={t('mock.matchRule', '匹配规则(JSON,可选)')}>
        <Input.TextArea rows={3} className="ms-mono" placeholder='{"query":{"id":"1"}}' />
      </Form.Item>
      <Form.Item name="responseBody" label={t('mock.colBody', '响应体')}>
        <Input.TextArea rows={3} className="ms-mono" />
      </Form.Item>
      <Button type="primary" htmlType="submit" loading={saving} block>
        {t('a.create', '创建')}
      </Button>
    </Form>
  )
}
