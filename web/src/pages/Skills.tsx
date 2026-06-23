import { useEffect, useState } from 'react'
import { Button, Card, Empty, Form, Input, Modal, Select, Space, Switch, Table, Tag, Tooltip, Typography } from 'antd'
import { message, modal } from '../feedback'
import { PlusOutlined, ReloadOutlined, MergeCellsOutlined } from '@ant-design/icons'
import { api, ApiError, type Skill } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'

export default function Skills() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const [items, setItems] = useState<Skill[]>([])
  const [loading, setLoading] = useState(false)
  const [createOpen, setCreateOpen] = useState(false)
  const [composeOpen, setComposeOpen] = useState(false)
  const [edit, setEdit] = useState<Skill | null>(null)

  const refresh = async () => {
    if (!projectId) { setItems([]); return }
    setLoading(true)
    try {
      const ss = await api.skills(projectId)
      setItems(Array.isArray(ss) ? ss : [])
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('skill.loadFailed', '加载技能失败'))
    } finally {
      setLoading(false)
    }
  }
  useEffect(() => { refresh() }, [projectId]) // eslint-disable-line react-hooks/exhaustive-deps

  const del = (s: Skill) => modal.confirm({
    title: `${t('skill.deleteConfirm', '删除技能')}「${s.name}」?`,
    okButtonProps: { danger: true },
    onOk: async () => {
      try {
        await api.deleteSkill(s.id)
        message.success(t('skill.deleted', '已删除'))
        refresh()
      } catch (e) {
        message.error(e instanceof ApiError ? e.message : t('skill.deleteFailed', '删除失败'))
      }
    },
  })

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
        <Table<Skill>
          rowKey="id"
          size="middle"
          loading={loading}
          dataSource={items}
          pagination={{ pageSize: 15, size: 'small' }}
          locale={{ emptyText: <Empty description={t('skill.empty', '暂无技能')} /> }}
          columns={[
            { title: t('skill.name', '名称'), dataIndex: 'name', render: (v: string) => <span style={{ fontWeight: 500 }}>{v}</span> },
            { title: t('skill.descLabel', '描述'), dataIndex: 'description', render: (v?: string) => v || <span style={{ color: '#bbb' }}>—</span> },
            { title: t('skill.instructionsSummary', '指令摘要'), dataIndex: 'instructions', render: (v?: string) => <Typography.Text type="secondary" ellipsis style={{ maxWidth: 360 }}>{v || '—'}</Typography.Text> },
            { title: t('skill.enabled', '启用'), dataIndex: 'enabled', width: 80, render: (v: boolean) => <Tag color={v ? 'green' : 'default'}>{v ? t('skill.on', '启用') : t('skill.off', '停用')}</Tag> },
            { title: 'ID', dataIndex: 'id', width: 100, render: (v: string) => <Tooltip title={v}><span className="ms-mono" style={{ fontSize: 12, color: '#8a9099' }}>{v?.slice(0, 8)}</span></Tooltip> },
            {
              title: t('req.action', '操作'),
              width: 130,
              render: (_v, s) => (
                <Space size={4}>
                  <Button type="link" size="small" onClick={() => setEdit(s)}>{t('a.edit', '编辑')}</Button>
                  <Button type="link" size="small" danger onClick={() => del(s)}>{t('a.delete', '删除')}</Button>
                </Space>
              ),
            },
          ]}
        />
      </div>

      <SkillFormModal
        open={createOpen}
        onClose={() => setCreateOpen(false)}
        title={t('skill.new', '新建技能')}
        onSubmit={async (v) => { await api.createSkill({ projectId, name: v.name, description: v.description, instructions: v.instructions }); message.success(t('skill.created', '技能已创建')) }}
        onDone={refresh}
      />
      <SkillFormModal
        open={!!edit}
        onClose={() => setEdit(null)}
        title={t('skill.edit', '编辑技能')}
        initial={edit ?? undefined}
        onSubmit={async (v) => { if (!edit) return; await api.updateSkill(edit.id, { name: v.name, description: v.description, instructions: v.instructions, includes: edit.includes, enabled: v.enabled }); message.success(t('skill.saved', '已保存')) }}
        onDone={refresh}
      />

      <ComposeModal open={composeOpen} skills={items} projectId={projectId} onClose={() => setComposeOpen(false)} />
    </div>
  )
}

type SkillFormValues = { name: string; description: string; instructions: string; enabled: boolean }

// 新建 / 编辑共用表单弹窗。编辑态多一个「启用」开关。
function SkillFormModal({ open, title, initial, onClose, onSubmit, onDone }: {
  open: boolean
  title: string
  initial?: Skill
  onClose: () => void
  onSubmit: (v: SkillFormValues) => Promise<void>
  onDone: () => void
}) {
  const { t } = useI18n()
  const [form] = Form.useForm<SkillFormValues>()
  const [busy, setBusy] = useState(false)
  useEffect(() => {
    if (open) form.setFieldsValue({ name: initial?.name ?? '', description: initial?.description ?? '', instructions: initial?.instructions ?? '', enabled: initial?.enabled ?? true })
  }, [open, initial, form])

  return (
    <Modal title={title} open={open} onCancel={onClose} footer={null} destroyOnHidden>
      <Form
        form={form}
        layout="vertical"
        onFinish={async (v) => {
          setBusy(true)
          try {
            await onSubmit(v)
            onClose()
            onDone()
          } catch (e) {
            message.error(e instanceof ApiError ? e.message : t('skill.saveFailed', '保存失败'))
          } finally {
            setBusy(false)
          }
        }}
      >
        <Form.Item name="name" label={t('skill.skillName', '技能名')} rules={[{ required: true }]}>
          <Input placeholder={t('skill.namePlaceholder', '如:六边形架构规范')} autoFocus />
        </Form.Item>
        <Form.Item name="description" label={t('skill.descLabel', '描述')}>
          <Input placeholder={t('skill.descPlaceholder', '一句话说明该技能用途')} />
        </Form.Item>
        <Form.Item name="instructions" label={t('skill.instructions', '指令')} rules={[{ required: true }]}>
          <Input.TextArea rows={5} placeholder={t('skill.instructionsPlaceholder', '遵循六边形架构,端口在 ports/,适配器在 adapters/…')} />
        </Form.Item>
        {initial && (
          <Form.Item name="enabled" label={t('skill.enabled', '启用')} valuePropName="checked">
            <Switch />
          </Form.Item>
        )}
        <Button type="primary" htmlType="submit" block loading={busy}>
          {initial ? t('a.save', '保存') : t('a.create', '创建')}
        </Button>
      </Form>
    </Modal>
  )
}

function ComposeModal({
  open,
  skills,
  projectId,
  onClose,
}: {
  open: boolean
  skills: Skill[]
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
          options={skills.map((s) => ({ value: s.id, label: s.name }))}
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
