import { useEffect, useState } from 'react'
import { Button, Card, Empty, Form, Input, Modal, Select, Space, Switch, Table, Tag, Tooltip, Typography } from 'antd'
import { message, modal } from '../feedback'
import { PlusOutlined, ReloadOutlined, MergeCellsOutlined } from '@ant-design/icons'
import { api, ApiError, type Skill } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'
import { PageBody, PageContainer, PageHeader, SelectProjectEmpty } from '../components/Page'
import { useListView, type ListColumn } from '../components/ListView'

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

  if (!projectId) return <SelectProjectEmpty />

  return <SkillsList items={items} loading={loading} projectId={projectId} refresh={refresh} createOpen={createOpen} setCreateOpen={setCreateOpen} composeOpen={composeOpen} setComposeOpen={setComposeOpen} edit={edit} setEdit={setEdit} del={del} t={t} />
}

// List + view/filter/column-settings trio. useListView is a hook, so this lives in a child component to avoid calling hooks after the parent's conditional return.
function SkillsList({ items, loading, projectId, refresh, createOpen, setCreateOpen, composeOpen, setComposeOpen, edit, setEdit, del, t }: {
  items: Skill[]
  loading: boolean
  projectId: string
  refresh: () => void
  createOpen: boolean
  setCreateOpen: (v: boolean) => void
  composeOpen: boolean
  setComposeOpen: (v: boolean) => void
  edit: Skill | null
  setEdit: (s: Skill | null) => void
  del: (s: Skill) => void
  t: (k: string, d?: string) => string
}) {
  const allColumns: ListColumn<Skill>[] = [
    { key: 'name', label: t('skill.name', '名称'), title: t('skill.name', '名称'), dataIndex: 'name', render: (v: string) => <span style={{ fontWeight: 500 }}>{v}</span> },
    { key: 'desc', label: t('skill.descLabel', '描述'), title: t('skill.descLabel', '描述'), dataIndex: 'description', render: (v?: string) => v || <span style={{ color: 'var(--text-3)' }}>—</span> },
    { key: 'instructions', label: t('skill.instructionsSummary', '指令摘要'), title: t('skill.instructionsSummary', '指令摘要'), dataIndex: 'instructions', render: (v?: string) => <Typography.Text type="secondary" ellipsis style={{ maxWidth: 360 }}>{v || '—'}</Typography.Text> },
    { key: 'enabled', label: t('skill.enabled', '启用'), title: t('skill.enabled', '启用'), dataIndex: 'enabled', width: 80, render: (v: boolean) => <Tag color={v ? 'green' : 'default'}>{v ? t('skill.on', '启用') : t('skill.off', '停用')}</Tag> },
    { key: 'id', label: 'ID', title: 'ID', dataIndex: 'id', width: 100, render: (v: string) => <Tooltip title={v}><span className="ms-mono" style={{ fontSize: 12, color: 'var(--text-3)' }}>{v?.slice(0, 8)}</span></Tooltip> },
    {
      key: 'action',
      label: t('req.action', '操作'),
      title: t('req.action', '操作'),
      width: 130,
      render: (_v, s) => (
        <Space size={4}>
          <Button type="link" size="small" onClick={() => setEdit(s)}>{t('a.edit', '编辑')}</Button>
          <Button type="link" size="small" danger onClick={() => del(s)}>{t('a.delete', '删除')}</Button>
        </Space>
      ),
    },
  ]
  const lv = useListView<Skill>({
    kind: 'skill',
    projectId,
    searchOf: (s) => s.name,
    searchLabel: t('skill.searchPh', '搜索名称'),
    fields: [
      {
        key: 'enabled',
        label: t('skill.enabled', '启用'),
        type: 'enum',
        options: [
          { value: 'on', label: t('skill.on', '启用') },
          { value: 'off', label: t('skill.off', '停用') },
        ],
        get: (s) => (s.enabled ? 'on' : 'off'),
      },
    ],
    columns: allColumns,
    rows: items,
  })

  return (
    <PageContainer>
      <PageHeader
        title={t('m.skill', '技能')}
        extra={
          <>
            {lv.toolbar}
            <Button icon={<MergeCellsOutlined />} onClick={() => setComposeOpen(true)} disabled={!items.length}>
              {t('skill.compose', '组合技能')}
            </Button>
            <Button type="primary" icon={<PlusOutlined />} onClick={() => setCreateOpen(true)}>
              {t('skill.new', '新建技能')}
            </Button>
            <Button icon={<ReloadOutlined />} onClick={refresh} />
          </>
        }
      />
      <PageBody>
        <Table<Skill>
          rowKey="id"
          size="middle"
          loading={loading}
          dataSource={lv.rows}
          pagination={{ pageSize: 15, size: 'small' }}
          locale={{ emptyText: <Empty description={t('skill.empty', '暂无技能')} /> }}
          columns={lv.columns}
        />
      </PageBody>

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
    </PageContainer>
  )
}

type SkillFormValues = { name: string; description: string; instructions: string; enabled: boolean }

// Shared create/edit form modal; edit mode adds an "enabled" switch.
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
