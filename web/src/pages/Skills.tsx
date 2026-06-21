import { useEffect, useState } from 'react'
import { Button, Card, Empty, Form, Input, Modal, Select, Space, Table, Typography } from 'antd'
import { message } from '../feedback'
import { PlusOutlined, ReloadOutlined, MergeCellsOutlined } from '@ant-design/icons'
import { api, ApiError } from '../api'
import { useApp } from '../context'
import { regAdd, regList, type RegItem } from '../registry'
import { useI18n } from '../i18n'

export default function Skills() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const [items, setItems] = useState<RegItem[]>([])
  const [createOpen, setCreateOpen] = useState(false)
  const [composeOpen, setComposeOpen] = useState(false)

  const refresh = () => setItems(regList('skill', projectId))
  useEffect(refresh, [projectId])

  if (!projectId)
    return (
      <div style={{ padding: 48 }}>
        <Empty description={t('common.selectProject', '请先在顶部选择项目')} />
      </div>
    )

  return (
    <div style={{ height: '100%', display: 'flex', flexDirection: 'column' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '14px 16px', background: '#fff', borderBottom: '1px solid #f0f0f0' }}>
        <Typography.Text strong style={{ fontSize: 15 }}>
          {t('m.skill', '技能')}
        </Typography.Text>
        <div style={{ flex: 1 }} />
        <Button icon={<MergeCellsOutlined />} onClick={() => setComposeOpen(true)} disabled={!items.length}>
          {t('skill.compose', '组合技能')}
        </Button>
        <Button type="primary" icon={<PlusOutlined />} onClick={() => setCreateOpen(true)}>
          {t('skill.new', '新建技能')}
        </Button>
        <Button icon={<ReloadOutlined />} onClick={refresh} />
      </div>
      <div style={{ flex: 1, overflow: 'auto', padding: 16 }}>
        <Table<RegItem>
          rowKey="id"
          size="middle"
          dataSource={items}
          pagination={{ pageSize: 15, size: 'small' }}
          locale={{ emptyText: <Empty description={t('skill.empty', '暂无技能')} /> }}
          columns={[
            { title: t('skill.name', '名称'), dataIndex: 'label' },
            { title: t('skill.instructionsSummary', '指令摘要'), render: (_, r) => <Typography.Text type="secondary" ellipsis style={{ maxWidth: 460 }}>{r.meta?.instructions || '—'}</Typography.Text> },
            { title: 'ID', dataIndex: 'id', render: (v: string) => <span className="ms-mono" style={{ fontSize: 12 }}>{v}</span> },
          ]}
        />
      </div>

      <Modal title={t('skill.new', '新建技能')} open={createOpen} onCancel={() => setCreateOpen(false)} footer={null} destroyOnHidden>
        <Form
          layout="vertical"
          onFinish={async (v: { name: string; instructions: string }) => {
            try {
              const s = await api.createSkill({ projectId, name: v.name, instructions: v.instructions })
              message.success(t('skill.created', '技能已创建'))
              setItems(regAdd('skill', projectId, { id: s.id, label: v.name, createdAt: Date.now(), meta: { instructions: v.instructions } }))
              setCreateOpen(false)
            } catch (e) {
              message.error(e instanceof ApiError ? e.message : t('skill.createFailed', '创建失败'))
            }
          }}
        >
          <Form.Item name="name" label={t('skill.skillName', '技能名')} rules={[{ required: true }]}>
            <Input placeholder={t('skill.namePlaceholder', '如:六边形架构规范')} autoFocus />
          </Form.Item>
          <Form.Item name="instructions" label={t('skill.instructions', '指令')} rules={[{ required: true }]}>
            <Input.TextArea rows={4} placeholder={t('skill.instructionsPlaceholder', '遵循六边形架构,端口在 ports/,适配器在 adapters/…')} />
          </Form.Item>
          <Button type="primary" htmlType="submit" block>
            {t('a.create', '创建')}
          </Button>
        </Form>
      </Modal>

      <ComposeModal open={composeOpen} skills={items} projectId={projectId} onClose={() => setComposeOpen(false)} />
    </div>
  )
}

function ComposeModal({
  open,
  skills,
  projectId,
  onClose,
}: {
  open: boolean
  skills: RegItem[]
  projectId: string
  onClose: () => void
}) {
  const { t } = useI18n()
  const [ids, setIds] = useState<string[]>([])
  const [result, setResult] = useState('')
  const [loading, setLoading] = useState(false)

  const compose = async () => {
    setLoading(true)
    try {
      const r = await api.composeSkills(projectId, ids)
      setResult(r.instructions)
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('skill.composeFailed', '组合失败'))
    } finally {
      setLoading(false)
    }
  }

  return (
    <Modal title={t('skill.compose', '组合技能')} open={open} onCancel={onClose} footer={null} width={680} destroyOnHidden>
      <Space direction="vertical" style={{ width: '100%' }}>
        <Select
          mode="multiple"
          style={{ width: '100%' }}
          placeholder={t('skill.composePlaceholder', '选择要组合的技能')}
          value={ids}
          onChange={setIds}
          options={skills.map((s) => ({ value: s.id, label: s.label }))}
        />
        <Button type="primary" onClick={compose} loading={loading} disabled={!ids.length}>
          {t('skill.genCompose', '生成组合指令')}
        </Button>
        {result && (
          <Card size="small" title={t('skill.composedResult', '组合后的指令')}>
            <pre style={{ whiteSpace: 'pre-wrap', margin: 0, fontSize: 13 }}>{result}</pre>
          </Card>
        )}
      </Space>
    </Modal>
  )
}
