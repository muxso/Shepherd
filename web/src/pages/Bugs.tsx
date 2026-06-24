import { useCallback, useEffect, useState } from 'react'
import { Button, Empty, Form, Input, Modal, Select, Table, Tag, Tooltip } from 'antd'
import { message, modal } from '../feedback'
import { PlusOutlined, ReloadOutlined } from '@ant-design/icons'
import { api, ApiError, type Bug } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'
import { PageBody, PageContainer, PageHeader, SelectProjectEmpty } from '../components/Page'

const STATUSES = ['NEW', 'RESOLVED', 'CLOSED', 'REOPENED', 'REJECTED']

const bugColor = (s: string) => {
  const v = s.toUpperCase()
  if (v === 'RESOLVED' || v === 'CLOSED') return 'green'
  if (v === 'REJECTED') return 'red'
  if (v === 'NEW' || v === 'REOPENED') return 'orange'
  return 'blue'
}

// 缺陷列表/创建/状态流转全走后端(GET /bug、POST /bug、POST /bug/{id}/status)。
export default function Bugs() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const [items, setItems] = useState<Bug[]>([])
  const [loading, setLoading] = useState(false)
  const [createOpen, setCreateOpen] = useState(false)

  const refresh = useCallback(() => {
    if (!projectId) return
    setLoading(true)
    api
      .bugs(projectId)
      .then(setItems)
      .catch(() => message.error(t('bug.loadFailed', '加载缺陷失败')))
      .finally(() => setLoading(false))
  }, [projectId, t])

  useEffect(refresh, [refresh])

  if (!projectId) return <SelectProjectEmpty />

  const changeStatus = (item: Bug) => {
    let status = 'RESOLVED'
    modal.confirm({
      title: `${t('bug.changeStatus', '变更缺陷状态')} · ${item.title || item.id}`,
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
          setItems((prev) => prev.map((x) => (x.id === item.id ? { ...x, status: b.status } : x)))
        } catch (e) {
          message.error(e instanceof ApiError ? `${t('bug.changeFailedStatus', '变更失败')}:${e.status}${t('bug.illegalTransition', '(非法流转?)')}` : t('bug.changeFailed', '变更失败'))
        }
      },
    })
  }

  return (
    <PageContainer>
      <PageHeader
        title={t('m.bug', '缺陷')}
        extra={
          <>
            <Button type="primary" icon={<PlusOutlined />} onClick={() => setCreateOpen(true)}>
              {t('bug.new', '新建缺陷')}
            </Button>
            <Button icon={<ReloadOutlined />} onClick={refresh} />
          </>
        }
      />
      <PageBody>
        <Table<Bug>
          rowKey="id"
          size="middle"
          loading={loading}
          dataSource={items}
          pagination={{ pageSize: 15, size: 'small' }}
          locale={{ emptyText: <Empty description={t('bug.empty', '暂无缺陷')} /> }}
          columns={[
            { title: t('bug.title', '标题'), dataIndex: 'title' },
            {
              title: t('bug.status', '状态'),
              width: 130,
              render: (_, r) => <Tag color={bugColor(r.status || 'NEW')}>{r.status || 'NEW'}</Tag>,
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
      </PageBody>

      <Modal title={t('bug.new', '新建缺陷')} open={createOpen} onCancel={() => setCreateOpen(false)} footer={null} destroyOnHidden>
        <Form
          layout="vertical"
          initialValues={{ initialStatus: 'NEW' }}
          onFinish={async (v: { title: string; initialStatus: string }) => {
            try {
              const b = await api.createBug({ projectId, title: v.title, initialStatus: v.initialStatus })
              message.success(t('bug.created', '缺陷已创建'))
              setItems((prev) => [b, ...prev.filter((x) => x.id !== b.id)])
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
    </PageContainer>
  )
}
