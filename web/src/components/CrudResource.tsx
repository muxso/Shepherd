import { useEffect, useState } from 'react'
import { Button, Empty, Form, Input, InputNumber, Modal, Select, Table, Typography, message } from 'antd'
import type { ColumnsType } from 'antd/es/table'
import { PlusOutlined, ReloadOutlined } from '@ant-design/icons'
import { ApiError } from '../api'
import { useApp } from '../context'

export interface FieldDef {
  name: string
  label: string
  type?: 'text' | 'textarea' | 'number' | 'select'
  options?: { value: string; label: string }[]
  required?: boolean
  placeholder?: string
  initial?: unknown
}

export interface CrudConfig<T> {
  title: string
  subtitle?: string
  /** 需要项目作用域(空项目时给提示) */
  needsProject?: boolean
  rowKey?: keyof T & string
  list: (ctx: { projectId: string }) => Promise<T[]>
  columns: ColumnsType<T>
  create?: {
    fields: FieldDef[]
    submit: (values: Record<string, unknown>, ctx: { projectId: string }) => Promise<unknown>
  }
}

// 数据驱动的资源页:工具栏(新建/刷新)+ 表格 + 由 fields 生成的新建表单。
export default function CrudResource<T extends object>({ cfg }: { cfg: CrudConfig<T> }) {
  const { projectId } = useApp()
  const [rows, setRows] = useState<T[]>([])
  const [loading, setLoading] = useState(false)
  const [open, setOpen] = useState(false)

  const load = async () => {
    if (cfg.needsProject && !projectId) {
      setRows([])
      return
    }
    setLoading(true)
    try {
      const data = await cfg.list({ projectId })
      setRows(Array.isArray(data) ? data : [])
    } catch (e) {
      setRows([])
      message.error(e instanceof ApiError ? e.message : '加载失败')
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId])

  if (cfg.needsProject && !projectId)
    return (
      <div style={{ padding: 48 }}>
        <Empty description="请先在顶部选择项目" />
      </div>
    )

  return (
    <div style={{ height: '100%', display: 'flex', flexDirection: 'column' }}>
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 12,
          padding: '14px 16px',
          background: '#fff',
          borderBottom: '1px solid #f0f0f0',
        }}
      >
        <div>
          <Typography.Text strong style={{ fontSize: 15 }}>
            {cfg.title}
          </Typography.Text>
          {cfg.subtitle && (
            <Typography.Text type="secondary" style={{ marginLeft: 8, fontSize: 12 }}>
              {cfg.subtitle}
            </Typography.Text>
          )}
        </div>
        <div style={{ flex: 1 }} />
        {cfg.create && (
          <Button type="primary" icon={<PlusOutlined />} onClick={() => setOpen(true)}>
            新建
          </Button>
        )}
        <Button icon={<ReloadOutlined />} onClick={load} />
      </div>
      <div style={{ flex: 1, overflow: 'auto', padding: 16 }}>
        <Table<T>
          rowKey={(cfg.rowKey || 'id') as string}
          size="middle"
          loading={loading}
          dataSource={rows}
          columns={cfg.columns}
          pagination={{ pageSize: 15, size: 'small', showTotal: (t) => `共 ${t} 条` }}
          locale={{ emptyText: <Empty description="暂无数据" /> }}
        />
      </div>

      {cfg.create && (
        <CreateModal
          title={`新建 · ${cfg.title}`}
          open={open}
          fields={cfg.create.fields}
          onClose={() => setOpen(false)}
          onSubmit={async (vals) => {
            await cfg.create!.submit(vals, { projectId })
            message.success('创建成功')
            setOpen(false)
            load()
          }}
        />
      )}
    </div>
  )
}

function CreateModal({
  title,
  open,
  fields,
  onClose,
  onSubmit,
}: {
  title: string
  open: boolean
  fields: FieldDef[]
  onClose: () => void
  onSubmit: (vals: Record<string, unknown>) => Promise<void>
}) {
  const [form] = Form.useForm()
  const [saving, setSaving] = useState(false)
  const initialValues = Object.fromEntries(
    fields.filter((f) => f.initial !== undefined).map((f) => [f.name, f.initial]),
  )
  return (
    <Modal
      title={title}
      open={open}
      onCancel={onClose}
      onOk={() => form.submit()}
      confirmLoading={saving}
      destroyOnHidden
    >
      <Form
        form={form}
        layout="vertical"
        preserve={false}
        initialValues={initialValues}
        onFinish={async (vals) => {
          setSaving(true)
          try {
            await onSubmit(vals)
          } catch (e) {
            message.error(e instanceof ApiError ? e.message : '创建失败')
          } finally {
            setSaving(false)
          }
        }}
      >
        {fields.map((f) => (
          <Form.Item
            key={f.name}
            name={f.name}
            label={f.label}
            rules={f.required ? [{ required: true, message: `请输入${f.label}` }] : undefined}
          >
            {f.type === 'textarea' ? (
              <Input.TextArea rows={3} placeholder={f.placeholder} />
            ) : f.type === 'number' ? (
              <InputNumber style={{ width: '100%' }} placeholder={f.placeholder} />
            ) : f.type === 'select' ? (
              <Select options={f.options} placeholder={f.placeholder} />
            ) : (
              <Input placeholder={f.placeholder} />
            )}
          </Form.Item>
        ))}
      </Form>
    </Modal>
  )
}
