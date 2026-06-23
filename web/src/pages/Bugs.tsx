import { useEffect, useState } from 'react'
import { Button, Empty, Form, Input, Modal, Select, Table, Tag, Tooltip, Typography } from 'antd'
import { message, modal } from '../feedback'
import { PlusOutlined, ReloadOutlined } from '@ant-design/icons'
import { api, ApiError } from '../api'
import { useApp } from '../context'
import { regAdd, regList, type RegItem } from '../registry'
import { useI18n } from '../i18n'

const STATUSES = ['NEW', 'IN_PROGRESS', 'RESOLVED', 'CLOSED', 'REJECTED', 'REOPENED']

const bugColor = (s: string) => {
  const v = s.toUpperCase()
  if (v === 'RESOLVED' || v === 'CLOSED') return 'green'
  if (v === 'REJECTED') return 'red'
  if (v === 'NEW' || v === 'REOPENED') return 'orange'
  return 'blue'
}

// 缺陷在 RegItem.meta.status 内维护当前状态(后端无 list 端点)。
export default function Bugs() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const [items, setItems] = useState<RegItem[]>([])
  const [createOpen, setCreateOpen] = useState(false)

  const refresh = () => setItems(regList('bug', projectId))
  useEffect(refresh, [projectId])

  if (!projectId)
    return (
      <div style={{ padding: 48 }}>
        <Empty description={t('common.selectProject', '请先在顶部选择项目')} />
      </div>
    )

  const changeStatus = (item: RegItem) => {
    let status = 'RESOLVED'
    modal.confirm({
      title: `${t('bug.changeStatus', '变更缺陷状态')} · ${item.label}`,
      content: (
        <Select
          defaultValue={status}
          style={{ width: '100%', marginTop: 8 }}
          onChange={(v) => (status = v)}
          options={STATUSES.map((s) => ({ value: s, label: s }))}
        />
      ),
      onOk: async () => {
        try {
          const b = await api.setBugStatus(item.id, status)
          message.success(`${t('bug.changedTo', '已变更为')} ${b.status}`)
          setItems(regAdd('bug', projectId, { ...item, meta: { ...item.meta, status: b.status } }))
        } catch (e) {
          message.error(e instanceof ApiError ? `${t('bug.changeFailedStatus', '变更失败')}:${e.status}${t('bug.illegalTransition', '(非法流转?)')}` : t('bug.changeFailed', '变更失败'))
        }
      },
    })
  }

  return (
    <div style={{ height: '100%', display: 'flex', flexDirection: 'column' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '14px 16px', background: 'var(--panel)', borderBottom: '1px solid var(--border-soft)' }}>
        <Typography.Text strong style={{ fontSize: 15 }}>
          {t('m.bug', '缺陷')}
        </Typography.Text>
        <div style={{ flex: 1 }} />
        <Button type="primary" icon={<PlusOutlined />} onClick={() => setCreateOpen(true)}>
          {t('bug.new', '新建缺陷')}
        </Button>
        <Button icon={<ReloadOutlined />} onClick={refresh} />
      </div>
      <div style={{ flex: 1, overflow: 'auto', padding: 16 }}>
        <Table<RegItem>
          rowKey="id"
          size="middle"
          dataSource={items}
          pagination={{ pageSize: 15, size: 'small' }}
          locale={{ emptyText: <Empty description={t('bug.empty', '暂无缺陷')} /> }}
          columns={[
            { title: t('bug.title', '标题'), dataIndex: 'label' },
            {
              title: t('bug.status', '状态'),
              width: 130,
              render: (_, r) => <Tag color={bugColor(r.meta?.status || 'NEW')}>{r.meta?.status || 'NEW'}</Tag>,
            },
            { title: 'ID', dataIndex: 'id', width: 110, render: (v: string) => <Tooltip title={v}><span className="ms-mono" style={{ fontSize: 12, color: 'var(--text-3)' }}>{v?.slice(0, 8)}</span></Tooltip> },
            {
              title: t('bug.action', '操作'),
              width: 120,
              render: (_, r) => (
                <Button type="link" size="small" onClick={() => changeStatus(r)}>
                  {t('bug.changeStatusBtn', '变更状态')}
                </Button>
              ),
            },
          ]}
        />
      </div>

      <Modal title={t('bug.new', '新建缺陷')} open={createOpen} onCancel={() => setCreateOpen(false)} footer={null} destroyOnHidden>
        <Form
          layout="vertical"
          initialValues={{ initialStatus: 'NEW' }}
          onFinish={async (v: { title: string; initialStatus: string }) => {
            try {
              const b = await api.createBug({ projectId, title: v.title, initialStatus: v.initialStatus })
              message.success(t('bug.created', '缺陷已创建'))
              setItems(regAdd('bug', projectId, { id: b.id, label: v.title, createdAt: Date.now(), meta: { status: b.status } }))
              setCreateOpen(false)
            } catch (e) {
              message.error(e instanceof ApiError ? e.message : t('bug.createFailed', '创建失败'))
            }
          }}
        >
          <Form.Item name="title" label={t('bug.title', '标题')} rules={[{ required: true }]}>
            <Input placeholder={t('bug.titlePlaceholder', '如:登录按钮无响应')} autoFocus />
          </Form.Item>
          <Form.Item name="initialStatus" label={t('bug.initialStatus', '初始状态')}>
            <Select options={STATUSES.map((s) => ({ value: s, label: s }))} />
          </Form.Item>
          <Button type="primary" htmlType="submit" block>
            {t('a.create', '创建')}
          </Button>
        </Form>
      </Modal>
    </div>
  )
}
