import { useEffect, useRef, useState, type ReactNode } from 'react'
import {
  Avatar, Badge, Button, Drawer, Dropdown, Empty, Input, Modal, Popconfirm, Segmented,
  Select, Table, Tabs, Tag, Tooltip, Upload,
} from 'antd'
import {
  CloseOutlined, DeleteOutlined, DownOutlined, EditOutlined, EllipsisOutlined,
  FullscreenOutlined, InboxOutlined, LeftOutlined,
  PaperClipOutlined, PlusOutlined, RightOutlined, SearchOutlined, ShareAltOutlined,
  StarFilled, StarOutlined, UserOutlined,
} from '@ant-design/icons'
import { MarkdownEditor } from './MarkdownEditor'
import { message } from '../feedback'
import {
  api, ApiError,
  type ApiModule, type Bug, type CaseBugLink, type CaseChange, type CaseDependencyLink,
  type CasePlanLink, type CaseRequirementLink, type CaseReviewLink, type CaseStep,
  type CommentItem, type FunctionalCase, type ProjectFile,
} from '../api'
import { useI18n } from '../i18n'

const PRIORITIES = ['P0', 'P1', 'P2', 'P3']
const prioDot = (p?: string) =>
  p === 'P0' ? 'var(--error)' : p === 'P1' ? 'var(--warning, #ff7d00)' : p === 'P3' ? 'var(--text-3)' : 'var(--brand)'

/** Attachment files live in ms_project_file, scoped to the case via a synthetic module id. */
const attachmentModule = (caseId: string) => `case-file:${caseId}`

/** Full body for PUT /functional-case/{id}: the API has PUT semantics, so send every field. */
const fullBody = (c: FunctionalCase) => ({
  projectId: c.projectId,
  name: c.name,
  priority: c.priority,
  status: c.status,
  module: c.module,
  tags: c.tags || [],
  steps: c.steps || [],
  customFields: c.customFields,
})

export default function CaseDetailDrawer({
  open,
  caseId,
  cases,
  modules,
  projectId,
  onClose,
  onNavigate,
  onEdit,
  onChanged,
  onDeleted,
}: {
  open: boolean
  caseId: string
  cases: FunctionalCase[]
  modules: ApiModule[]
  projectId: string
  onClose: () => void
  onNavigate: (id: string) => void
  onEdit: (c: FunctionalCase) => void
  onChanged: () => void
  onDeleted: () => void
}) {
  const { t } = useI18n()
  const [detail, setDetail] = useState<FunctionalCase | null>(null)
  const [names, setNames] = useState<Record<string, string>>({})
  const [following, setFollowing] = useState(false)
  const [fullscreen, setFullscreen] = useState(false)
  const [activeTab, setActiveTab] = useState('info')
  const [changeCount, setChangeCount] = useState(0)

  const load = () => {
    if (!caseId) return
    api.getFunctionalCase(caseId).then(setDetail).catch(() => setDetail(null))
    api.caseChanges(caseId).then((c) => setChangeCount(c.length)).catch(() => setChangeCount(0))
    api.followStatus(projectId, 'FUNCTIONAL_CASE', caseId)
      .then((s) => setFollowing(s.following))
      .catch(() => setFollowing(false))
  }
  useEffect(() => {
    if (open) load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [caseId, open])
  useEffect(() => {
    const ids = [...new Set(cases.map((c) => c.createdBy).filter(Boolean))] as string[]
    if (ids.length) api.userNames(ids).then(setNames).catch(() => setNames({}))
  }, [cases])

  const idx = cases.findIndex((c) => c.id === caseId)
  const nameOf = (uid?: string) => (uid ? names[uid] || uid : '—')

  const save = async (patch: Partial<ReturnType<typeof fullBody>>) => {
    if (!detail) return
    try {
      await api.updateFunctionalCase(detail.id, { ...fullBody(detail), ...patch })
      load()
      onChanged()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('func.saveFailed', '保存失败'))
    }
  }

  const toggleFollow = async () => {
    const b = { projectId, entityType: 'FUNCTIONAL_CASE', entityId: caseId }
    try {
      const s = following ? await api.unfollow(b) : await api.follow(b)
      setFollowing(s.following)
    } catch {
      message.error(t('funcd.followFailed', '关注失败'))
    }
  }

  const share = () => {
    const url = `${location.origin}${location.pathname}#/case/functional?caseId=${caseId}`
    navigator.clipboard?.writeText(url)
    message.success(t('funcd.linkCopied', '链接已复制'))
  }

  const doDelete = async () => {
    try {
      await api.deleteFunctionalCase(caseId)
      message.success(t('func.deleted', '用例已删除'))
      onDeleted()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('func.deleteFailed', '删除失败'))
    }
  }

  const c = detail

  return (
    <Drawer
      open={open}
      onClose={onClose}
      width={fullscreen ? '100%' : '60%'}
      closable={false}
      destroyOnHidden
      styles={{ body: { padding: 0, display: 'flex', flexDirection: 'column' } }}
    >
      {/* Header: priority selector + [num] name; actions on the right */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '10px 16px', borderBottom: '1px solid var(--border-soft)' }}>
        <Select
          size="small"
          value={c?.priority || 'P2'}
          style={{ width: 86 }}
          onChange={(p) => save({ priority: p })}
          options={PRIORITIES.map((p) => ({
            value: p,
            label: (
              <span>
                <span style={{ display: 'inline-block', width: 8, height: 8, borderRadius: 4, background: prioDot(p), marginRight: 6 }} />
                {p}
              </span>
            ),
          }))}
        />
        <span style={{ fontSize: 16, fontWeight: 600 }}>
          [ {c?.num || '—'} ]&nbsp;&nbsp;{c?.name || ''}
        </span>
        <span style={{ flex: 1 }} />
        <Button size="small" icon={<LeftOutlined />} disabled={idx <= 0} onClick={() => onNavigate(cases[idx - 1].id)} />
        <Button size="small" icon={<RightOutlined />} disabled={idx < 0 || idx >= cases.length - 1} onClick={() => onNavigate(cases[idx + 1].id)} />
        <Button type="text" icon={<EditOutlined />} onClick={() => c && onEdit(c)}>{t('a.edit', '编辑')}</Button>
        <Button type="text" icon={<ShareAltOutlined />} onClick={share}>{t('funcd.share', '分享')}</Button>
        <Button
          type="text"
          icon={following ? <StarFilled style={{ color: 'var(--warning, #ff7d00)' }} /> : <StarOutlined />}
          onClick={toggleFollow}
        >
          {t('funcd.follow', '关注')}
        </Button>
        <Dropdown
          menu={{
            items: [{ key: 'del', danger: true, icon: <DeleteOutlined />, label: t('a.delete', '删除') }],
            onClick: ({ key }) => {
              if (key === 'del')
                Modal.confirm({ title: t('func.confirmDelete', '确认删除该用例?'), onOk: doDelete })
            },
          }}
        >
          <Button type="text" icon={<EllipsisOutlined />}>{t('funcd.more', '更多')}</Button>
        </Dropdown>
        <Button type="text" icon={<FullscreenOutlined />} onClick={() => setFullscreen((v) => !v)}>{t('funcd.fullscreen', '全屏')}</Button>
        <Button type="text" icon={<CloseOutlined />} onClick={onClose} />
      </div>

      <div style={{ flex: 1, display: 'flex', minHeight: 0 }}>
        <div style={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column' }}>
          <Tabs
            activeKey={activeTab}
            onChange={setActiveTab}
            style={{ flex: 1, minHeight: 0, padding: '0 16px' }}
            tabBarExtraContent={{ right: <Button type="text" size="small">{t('funcd.displaySettings', '显示设置')}</Button> }}
            items={[
              { key: 'info', label: t('func.tabInfo', '基本信息'), children: c ? <InfoTab c={c} modules={modules} nameOf={nameOf} onSaveTags={(tags) => save({ tags })} /> : null },
              { key: 'detail', label: t('a.detail', '详情'), children: c ? <DetailTab c={c} projectId={projectId} onSave={save} /> : null },
              { key: 'case', label: t('nav.case', '用例'), children: <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('g.empty', '暂无数据')} style={{ marginTop: 48 }} /> },
              { key: 'req', label: t('home.req', '需求'), children: <RequirementTab caseId={caseId} projectId={projectId} /> },
              { key: 'bug', label: t('home.bug', '缺陷'), children: <BugTab caseId={caseId} projectId={projectId} nameOf={nameOf} /> },
              { key: 'deps', label: t('funcd.deps', '依赖关系'), children: <DependencyTab caseId={caseId} projectId={projectId} cases={cases} nameOf={nameOf} /> },
              { key: 'review', label: t('funcd.caseReview', '用例评审'), children: <ReviewTab caseId={caseId} /> },
              { key: 'plan', label: t('home.plan', '测试计划'), children: <PlanTab caseId={caseId} /> },
              { key: 'comment', label: t('funcd.comments', '评论'), children: <CommentTab caseId={caseId} nameOf={nameOf} /> },
              {
                key: 'history',
                label: (
                  <span>
                    {t('funcd.changeHistory', '变更历史')}
                    {changeCount > 0 && <Badge count={changeCount} size="small" color="var(--text-3)" style={{ marginLeft: 6 }} />}
                  </span>
                ),
                children: <HistoryTab caseId={caseId} nameOf={nameOf} />,
              },
            ]}
          />
          {activeTab !== 'info' && <CommentBar caseId={caseId} onPosted={() => setActiveTab('comment')} />}
        </div>
      </div>
    </Drawer>
  )
}

/** Basic info tab: module / level / creator / created-at / editable tags. */
function InfoTab({
  c, modules, nameOf, onSaveTags,
}: {
  c: FunctionalCase
  modules: ApiModule[]
  nameOf: (u?: string) => string
  onSaveTags: (tags: string[]) => void
}) {
  const { t } = useI18n()
  const [tagInput, setTagInput] = useState('')
  const moduleName = modules.find((m) => m.id === c.module)?.name || c.module || t('funcd.unplanned', '未规划用例')
  const row = (label: ReactNode, value: ReactNode) => (
    <div style={{ display: 'flex', padding: '14px 0' }}>
      <div style={{ width: 140, color: 'var(--text-2)' }}>{label}</div>
      <div style={{ flex: 1 }}>{value}</div>
    </div>
  )
  const addTag = () => {
    const v = tagInput.trim()
    if (!v) return
    if ((c.tags || []).includes(v)) { setTagInput(''); return }
    onSaveTags([...(c.tags || []), v])
    setTagInput('')
  }
  return (
    <div style={{ padding: '4px 8px', overflow: 'auto' }}>
      {row(t('funcd.belongModule', '所属模块'), moduleName)}
      {row(
        <span>{t('funcd.caseLevel', '用例等级')} <span style={{ color: 'var(--error)' }}>*</span></span>,
        <span>
          <span style={{ display: 'inline-block', width: 8, height: 8, borderRadius: 4, background: prioDot(c.priority), marginRight: 6 }} />
          {c.priority || 'P2'}
        </span>,
      )}
      {row(t('lv.createdBy', '创建人'), nameOf(c.createdBy || undefined))}
      {row(t('funcd.createdAt', '创建时间'), <span className="ms-mono">{(c.createdAt || '').slice(0, 19) || '—'}</span>)}
      {row(
        t('funcd.tags', '标签'),
        <span>
          {(c.tags || []).map((tag) => (
            <Tag key={tag} closable onClose={() => onSaveTags((c.tags || []).filter((x) => x !== tag))}>{tag}</Tag>
          ))}
          <Input
            size="small"
            style={{ width: 180 }}
            variant="borderless"
            placeholder={t('funcd.addTagPh', '添加标签，回车结束')}
            value={tagInput}
            onChange={(e) => setTagInput(e.target.value)}
            onPressEnter={addTag}
          />
        </span>,
      )}
    </div>
  )
}

/** Detail tab: prerequisite / steps (or text description) / remark / attachments; the edit button flips to inline edit. */
function DetailTab({
  c, projectId, onSave,
}: {
  c: FunctionalCase
  projectId: string
  onSave: (patch: { customFields?: Record<string, string>; steps?: CaseStep[] }) => void
}) {
  const { t } = useI18n()
  const cf = c.customFields || {}
  const stepModel = cf.stepModel === 'TEXT' ? 'TEXT' : 'STEP'
  // Inline edit drafts: prerequisite / steps / remark, saved in one PUT.
  const [editing, setEditing] = useState(false)
  const [draftPre, setDraftPre] = useState('')
  const [draftRemark, setDraftRemark] = useState('')
  const [draftSteps, setDraftSteps] = useState<CaseStep[]>([])
  const startEdit = () => {
    setDraftPre(cf['前置条件'] || '')
    setDraftRemark(cf['备注'] || '')
    setDraftSteps(c.steps?.length ? [...c.steps] : [])
    setEditing(true)
  }
  const saveEdit = () => {
    const customFields = { ...cf }
    if (draftPre.trim()) customFields['前置条件'] = draftPre.trim()
    else delete customFields['前置条件']
    if (draftRemark.trim()) customFields['备注'] = draftRemark.trim()
    else delete customFields['备注']
    onSave({ customFields, steps: draftSteps.filter((s) => s.step.trim() || s.expected.trim()) })
    setEditing(false)
  }
  const [files, setFiles] = useState<ProjectFile[]>([])
  const loadFiles = () =>
    api.projectFiles(projectId)
      .then((all) => setFiles(all.filter((f) => f.moduleId === attachmentModule(c.id))))
      .catch(() => setFiles([]))
  useEffect(() => { loadFiles() }, [c.id]) // eslint-disable-line react-hooks/exhaustive-deps

  const upload = (file: File) => {
    if (file.size > 50 * 1024 * 1024) {
      message.error(t('funcd.fileTooLarge', '文件大小不能超过 50MB'))
      return false
    }
    const reader = new FileReader()
    reader.onload = async () => {
      const b64 = String(reader.result).split(',')[1] || ''
      try {
        await api.uploadProjectFile({
          projectId,
          name: file.name,
          fileFormat: file.name.split('.').pop() || '',
          sizeBytes: file.size,
          contentBase64: b64,
          moduleId: attachmentModule(c.id),
        })
        message.success(t('funcd.uploaded', '附件已上传'))
        loadFiles()
      } catch (e) {
        message.error(e instanceof ApiError ? e.message : t('funcd.uploadFailed', '上传失败'))
      }
    }
    reader.readAsDataURL(file)
    return false
  }
  const download = async (f: ProjectFile) => {
    try {
      const r = await api.downloadProjectFile(f.id)
      const bytes = Uint8Array.from(atob(r.contentBase64), (ch) => ch.charCodeAt(0))
      const url = URL.createObjectURL(new Blob([bytes]))
      const a = document.createElement('a')
      a.href = url
      a.download = r.name
      a.click()
      URL.revokeObjectURL(url)
    } catch {
      message.error(t('funcd.downloadFailed', '下载失败'))
    }
  }

  const section = (title: ReactNode, body: ReactNode) => (
    <div style={{ marginBottom: 24 }}>
      <div style={{ fontWeight: 600, marginBottom: 10 }}>{title}</div>
      {body}
    </div>
  )
  const textBlock = (v?: string) => (
    <div style={{ whiteSpace: 'pre-wrap', color: v ? 'var(--text-1)' : 'var(--text-2)' }}>{v || '-'}</div>
  )

  const setStep = (i: number, patch: Partial<CaseStep>) =>
    setDraftSteps((ss) => ss.map((s, j) => (j === i ? { ...s, ...patch } : s)))

  return (
    <div style={{ position: 'relative', overflow: 'auto', padding: '4px 8px 24px' }}>
      <Button
        type="link"
        icon={<EditOutlined />}
        style={{ position: 'absolute', right: 0, top: 0 }}
        onClick={() => { if (!editing) startEdit() }}
      >
        {t('funcd.contentEdit', '内容编辑')}
      </Button>
      {section(
        t('func.prerequisite', '前置条件'),
        editing ? (
          <MarkdownEditor projectId={projectId} value={draftPre} onChange={setDraftPre} placeholder={t('a.pleaseInput', '请输入内容')} />
        ) : (
          textBlock(cf['前置条件'])
        ),
      )}
      {section(
        <span>
          {t('func.stepsDesc', '步骤描述')}
          <span style={{ margin: '0 10px', color: 'var(--border)' }}>|</span>
          <Dropdown
            menu={{
              items: [
                { key: 'STEP', label: t('func.stepsDesc', '步骤描述') },
                { key: 'TEXT', label: t('funcd.textDesc', '文本描述') },
              ],
              onClick: ({ key }) => onSave({ customFields: { ...cf, stepModel: key } }),
            }}
          >
            <a style={{ fontWeight: 600 }}>{t('funcd.changeType', '更改类型')} <DownOutlined style={{ fontSize: 10 }} /></a>
          </Dropdown>
        </span>,
        stepModel === 'TEXT' ? (
          textBlock(cf['文本描述'])
        ) : editing ? (
          <>
            <Table<CaseStep>
              rowKey={(_, i) => String(i)}
              size="middle"
              pagination={false}
              dataSource={draftSteps}
              locale={{ emptyText: <span style={{ color: 'var(--text-2)' }}>{t('g.empty', '暂无数据')}</span> }}
              columns={[
                { title: t('func.colIndex', '序号'), width: 80, render: (_, __, i) => i + 1 },
                {
                  title: t('func.colStep', '用例步骤'), dataIndex: 'step',
                  render: (v: string, _s, i) => <Input value={v} onChange={(e) => setStep(i, { step: e.target.value })} placeholder={t('a.pleaseInput', '请输入')} />,
                },
                {
                  title: t('func.colExpected', '预期结果'), dataIndex: 'expected',
                  render: (v: string, _s, i) => <Input value={v} onChange={(e) => setStep(i, { expected: e.target.value })} placeholder={t('a.pleaseInput', '请输入')} />,
                },
                {
                  title: t('req.action', '操作'), width: 80,
                  render: (_v, _s, i) => (
                    <Button type="text" size="small" danger icon={<DeleteOutlined />} onClick={() => setDraftSteps((ss) => ss.filter((_, j) => j !== i))} />
                  ),
                },
              ]}
            />
            <a style={{ display: 'inline-block', marginTop: 8 }} onClick={() => setDraftSteps((ss) => [...ss, { step: '', expected: '' }])}>
              <PlusOutlined /> {t('funcd.addStep', '添加步骤')}
            </a>
          </>
        ) : (
          <Table<CaseStep>
            rowKey={(_, i) => String(i)}
            size="middle"
            pagination={false}
            dataSource={c.steps || []}
            locale={{ emptyText: <span style={{ color: 'var(--text-2)' }}>{t('g.empty', '暂无数据')}</span> }}
            columns={[
              { title: t('func.colIndex', '序号'), width: 120, render: (_, __, i) => i + 1 },
              { title: t('func.colStep', '用例步骤'), dataIndex: 'step', render: (v: string) => v || '—' },
              { title: t('func.colExpected', '预期结果'), dataIndex: 'expected', render: (v: string) => v || '—' },
            ]}
          />
        ),
      )}
      {section(
        t('func.remark', '备注'),
        editing ? (
          <MarkdownEditor projectId={projectId} value={draftRemark} onChange={setDraftRemark} placeholder={t('a.pleaseInput', '请输入内容')} />
        ) : (
          textBlock(cf['备注'])
        ),
      )}
      {editing && (
        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8, marginBottom: 24 }}>
          <Button onClick={() => setEditing(false)}>{t('a.cancel', '取消')}</Button>
          <Button type="primary" onClick={saveEdit}>{t('a.save', '保存')}</Button>
        </div>
      )}
      {section(
        t('funcd.addAttachment', '添加附件'),
        <div>
          <Upload showUploadList={false} beforeUpload={upload}>
            <Button icon={<PlusOutlined />}>{t('funcd.addAttachment', '添加附件')}</Button>
          </Upload>
          <div style={{ color: 'var(--text-3)', marginTop: 8 }}>{t('funcd.attachmentHint', '支持任意类型文件，文件大小不超过 50MB')}</div>
          {files.length ? (
            <div style={{ marginTop: 12 }}>
              {files.map((f) => (
                <div key={f.id} style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '6px 0', borderBottom: '1px solid var(--border-soft)' }}>
                  <PaperClipOutlined style={{ color: 'var(--text-3)' }} />
                  <a onClick={() => download(f)}>{f.name}</a>
                  <span style={{ color: 'var(--text-3)', fontSize: 12 }}>{(f.sizeBytes / 1024).toFixed(1)} KB</span>
                  <span style={{ flex: 1 }} />
                  <Popconfirm title={t('funcd.confirmDeleteFile', '删除该附件?')} onConfirm={() => api.deleteProjectFile(f.id).then(loadFiles)}>
                    <Button type="text" size="small" danger icon={<DeleteOutlined />} />
                  </Popconfirm>
                </div>
              ))}
            </div>
          ) : (
            <div style={{ textAlign: 'center', padding: '48px 0', color: 'var(--text-3)' }}>
              <InboxOutlined style={{ fontSize: 32 }} />
              <div style={{ marginTop: 8 }}>{t('g.empty', '暂无数据')}</div>
            </div>
          )}
        </div>,
      )}
    </div>
  )
}

/** Requirements tab: linked requirements + add dialog (link existing by ID, or create then link). */
function RequirementTab({ caseId, projectId }: { caseId: string; projectId: string }) {
  const { t } = useI18n()
  const [links, setLinks] = useState<CaseRequirementLink[]>([])
  const [search, setSearch] = useState('')
  const [open, setOpen] = useState(false)
  const [fid, setFid] = useState('')
  const [ftitle, setFtitle] = useState('')
  const [furl, setFurl] = useState('')
  const load = () => api.caseRequirements(caseId).then(setLinks).catch(() => setLinks([]))
  useEffect(() => { load() }, [caseId]) // eslint-disable-line react-hooks/exhaustive-deps

  const submit = async (keepOpen: boolean) => {
    if (!ftitle.trim() && !fid.trim()) return message.warning(t('funcd.reqTitleRequired', '请输入需求标题'))
    try {
      let reqId = fid.trim()
      if (!reqId) {
        const desc = furl.trim() ? `${t('funcd.reqUrl', '需求地址')}: ${furl.trim()}` : undefined
        const r = await api.createRequirement({ projectId, title: ftitle.trim(), description: desc, acceptanceCriteria: [] })
        reqId = r.id
      }
      await api.linkRequirementCase({ requirementId: reqId, criterionIndex: 0, functionalCaseId: caseId, projectId })
      message.success(t('req.linked', '已关联'))
      setFid(''); setFtitle(''); setFurl('')
      if (!keepOpen) setOpen(false)
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('req.linkFailed', '关联失败'))
    }
  }

  const visible = links.filter((l) => !search || l.requirementTitle.includes(search))
  return (
    <div style={{ overflow: 'auto' }}>
      <div style={{ display: 'flex', marginBottom: 12 }}>
        <Button onClick={() => setOpen(true)}>{t('funcd.addRequirement', '添加需求')}</Button>
        <span style={{ flex: 1 }} />
        <Input
          style={{ width: 260 }}
          placeholder={t('funcd.searchByName', '通过名称搜索')}
          suffix={<SearchOutlined style={{ color: 'var(--text-3)' }} />}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          allowClear
        />
      </div>
      <Table<CaseRequirementLink>
        rowKey={(l) => `${l.requirementId}:${l.criterionIndex}`}
        size="middle"
        pagination={false}
        dataSource={visible}
        locale={{
          emptyText: (
            <span style={{ color: 'var(--text-2)' }}>
              {t('g.empty', '暂无数据')} <a onClick={() => setOpen(true)}>{t('funcd.addRequirement', '添加需求')}</a>
            </span>
          ),
        }}
        columns={[
          { title: 'ID', dataIndex: 'requirementId', width: 280, render: (v: string) => <span className="ms-mono">{v.slice(0, 8)}</span> },
          { title: t('funcd.reqName', '需求名称'), dataIndex: 'requirementTitle', render: (v: string) => v || '—' },
          { title: t('funcd.platform', '平台'), width: 160, render: () => '—' },
          {
            title: t('req.action', '操作'), width: 100,
            render: (_v, l) => (
              <Button
                type="link" size="small" danger
                onClick={() =>
                  api.unlinkRequirementCase({ requirementId: l.requirementId, criterionIndex: l.criterionIndex, functionalCaseId: caseId }).then(load)
                }
              >
                {t('func.unlink', '取消关联')}
              </Button>
            ),
          },
        ]}
      />
      <Modal
        open={open}
        title={t('funcd.addRequirement', '添加需求')}
        onCancel={() => setOpen(false)}
        destroyOnHidden
        footer={
          <>
            <Button onClick={() => setOpen(false)}>{t('a.cancel', '取消')}</Button>
            <Button onClick={() => submit(true)}>{t('funcd.saveAndContinue', '保存并继续添加')}</Button>
            <Button type="primary" onClick={() => submit(false)}>{t('a.create', '创建')}</Button>
          </>
        }
      >
        <div style={{ marginBottom: 16 }}>
          <div style={{ marginBottom: 6 }}>ID</div>
          <Input placeholder={t('funcd.reqIdPh', '请输入 ID')} value={fid} onChange={(e) => setFid(e.target.value)} />
        </div>
        <div style={{ marginBottom: 16 }}>
          <div style={{ marginBottom: 6 }}>{t('funcd.reqTitle', '需求标题')} <span style={{ color: 'var(--error)' }}>*</span></div>
          <Input placeholder={t('funcd.reqTitlePh', '请输入需求标题')} value={ftitle} onChange={(e) => setFtitle(e.target.value)} />
        </div>
        <div>
          <div style={{ marginBottom: 6 }}>{t('funcd.reqUrl', '需求地址')}</div>
          <Input placeholder={t('funcd.reqUrlPh', '请输入需求地址')} value={furl} onChange={(e) => setFurl(e.target.value)} />
        </div>
      </Modal>
    </div>
  )
}

/** Bugs tab: bugs linked to the case (direct) + link/create dialogs; the test-plan view is a placeholder. */
function BugTab({ caseId, projectId, nameOf }: { caseId: string; projectId: string; nameOf: (u?: string) => string }) {
  const { t } = useI18n()
  const [view, setView] = useState<'direct' | 'plan'>('direct')
  const [rows, setRows] = useState<CaseBugLink[]>([])
  const [search, setSearch] = useState('')
  const [linkOpen, setLinkOpen] = useState(false)
  const [newOpen, setNewOpen] = useState(false)
  const [bugList, setBugList] = useState<Bug[]>([])
  const [selBug, setSelBug] = useState<string>()
  const [newTitle, setNewTitle] = useState('')
  const load = () => api.caseBugs(caseId).then(setRows).catch(() => setRows([]))
  useEffect(() => { load() }, [caseId]) // eslint-disable-line react-hooks/exhaustive-deps

  const openLink = async () => {
    setLinkOpen(true)
    setSelBug(undefined)
    try { setBugList(await api.bugs(projectId)) } catch { setBugList([]) }
  }
  const doLink = async () => {
    if (!selBug) return message.warning(t('funcd.pickBug', '请选择缺陷'))
    try {
      await api.linkBugRelation(selBug, { kind: 'FUNCTIONAL_CASE', targetId: caseId })
      message.success(t('req.linked', '已关联'))
      setLinkOpen(false)
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('req.linkFailed', '关联失败'))
    }
  }
  const doCreate = async () => {
    if (!newTitle.trim()) return message.warning(t('funcd.bugTitleRequired', '请输入缺陷名称'))
    try {
      const b = await api.createBug({ projectId, title: newTitle.trim(), initialStatus: 'OPEN' })
      await api.linkBugRelation(b.id, { kind: 'FUNCTIONAL_CASE', targetId: caseId })
      message.success(t('funcd.bugCreated', '缺陷已创建并关联'))
      setNewOpen(false)
      setNewTitle('')
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('func.saveFailed', '保存失败'))
    }
  }

  const visible = rows.filter((r) => !search || r.title.includes(search))
  return (
    <div style={{ overflow: 'auto' }}>
      <div style={{ display: 'flex', gap: 12, marginBottom: 12, alignItems: 'center' }}>
        <Segmented
          value={view}
          onChange={(v) => setView(v as 'direct' | 'plan')}
          options={[
            { value: 'direct', label: t('funcd.directLink', '直接关联') },
            { value: 'plan', label: t('home.plan', '测试计划') },
          ]}
        />
        <Input
          style={{ width: 240 }}
          placeholder={t('funcd.searchByName', '通过名称搜索')}
          suffix={<SearchOutlined style={{ color: 'var(--text-3)' }} />}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          allowClear
        />
        <span style={{ flex: 1 }} />
        <Button type="primary" onClick={openLink}>{t('funcd.linkBug', '关联缺陷')}</Button>
        <Button onClick={() => setNewOpen(true)}>{t('funcd.newBug', '新建缺陷')}</Button>
      </div>
      {view === 'plan' ? (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('g.empty', '暂无数据')} style={{ marginTop: 48 }} />
      ) : (
        <Table<CaseBugLink>
          rowKey="bugId"
          size="middle"
          pagination={false}
          dataSource={visible}
          locale={{
            emptyText: (
              <span style={{ color: 'var(--text-2)' }}>
                {t('funcd.noDataPlease', '暂无数据，请')} <a onClick={openLink}>{t('funcd.linkBug', '关联缺陷')}</a>{' '}
                {t('funcd.or', '或')} <a onClick={() => setNewOpen(true)}>{t('funcd.newBug', '新建缺陷')}</a>
              </span>
            ),
          }}
          columns={[
            { title: 'ID', dataIndex: 'bugId', width: 120, render: (v: string) => <span className="ms-mono">{v.slice(0, 8)}</span> },
            { title: t('funcd.bugName', '缺陷名称'), dataIndex: 'title' },
            { title: t('funcd.bugStatus', '缺陷状态'), dataIndex: 'status', width: 140, render: (s: string) => <Tag>{s}</Tag> },
            { title: t('lv.createdBy', '创建人'), dataIndex: 'createdBy', width: 140, render: (u: string) => nameOf(u || undefined) },
            { title: t('funcd.handler', '处理人'), dataIndex: 'handler', width: 140, render: (u: string) => (u ? nameOf(u) : '—') },
            {
              title: t('req.action', '操作'), width: 110,
              render: (_v, r) => (
                <Button type="link" size="small" danger onClick={() => api.unlinkBugRelation(r.bugId, 'FUNCTIONAL_CASE', caseId).then(load)}>
                  {t('func.unlink', '取消关联')}
                </Button>
              ),
            },
          ]}
        />
      )}
      <Modal open={linkOpen} title={t('funcd.linkBug', '关联缺陷')} onCancel={() => setLinkOpen(false)} onOk={doLink} okText={t('a.confirm', '确定')} cancelText={t('a.cancel', '取消')} destroyOnHidden>
        <Select
          showSearch
          style={{ width: '100%' }}
          placeholder={t('funcd.pickBug', '请选择缺陷')}
          optionFilterProp="label"
          value={selBug}
          onChange={setSelBug}
          options={bugList.map((b) => ({ value: b.id, label: b.title || b.id }))}
          notFoundContent={t('funcd.noBugs', '项目暂无缺陷')}
        />
      </Modal>
      <Modal open={newOpen} title={t('funcd.newBug', '新建缺陷')} onCancel={() => setNewOpen(false)} onOk={doCreate} okText={t('a.create', '创建')} cancelText={t('a.cancel', '取消')} destroyOnHidden>
        <Input placeholder={t('funcd.bugTitlePh', '请输入缺陷名称')} value={newTitle} onChange={(e) => setNewTitle(e.target.value)} />
      </Modal>
    </div>
  )
}

/** Dependencies tab: pre/post case dependency edges over the project case pool. */
function DependencyTab({
  caseId, projectId, cases, nameOf,
}: {
  caseId: string
  projectId: string
  cases: FunctionalCase[]
  nameOf: (u?: string) => string
}) {
  const { t } = useI18n()
  const [direction, setDirection] = useState<'PRE' | 'POST'>('PRE')
  const [rows, setRows] = useState<CaseDependencyLink[]>([])
  const [search, setSearch] = useState('')
  const [open, setOpen] = useState(false)
  const [sel, setSel] = useState<string>()
  const load = () => api.caseDependencies(caseId, direction).then(setRows).catch(() => setRows([]))
  useEffect(() => { load() }, [caseId, direction]) // eslint-disable-line react-hooks/exhaustive-deps

  const addLabel = direction === 'PRE' ? t('funcd.addPreCase', '添加前置用例') : t('funcd.addPostCase', '添加后置用例')
  const doAdd = async () => {
    if (!sel) return message.warning(t('funcd.pickCase', '请选择用例'))
    try {
      await api.addCaseDependency(caseId, { targetCaseId: sel, direction, projectId })
      message.success(t('req.linked', '已关联'))
      setOpen(false)
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('req.linkFailed', '关联失败'))
    }
  }
  const visible = rows.filter((r) => !search || r.name.includes(search))
  return (
    <div style={{ overflow: 'auto' }}>
      <div style={{ display: 'flex', marginBottom: 12, alignItems: 'center' }}>
        <Button type="primary" onClick={() => { setSel(undefined); setOpen(true) }}>{addLabel}</Button>
        <span style={{ flex: 1 }} />
        <Segmented
          value={direction}
          onChange={(v) => setDirection(v as 'PRE' | 'POST')}
          options={[
            { value: 'PRE', label: t('funcd.preCase', '前置用例') },
            { value: 'POST', label: t('funcd.postCase', '后置用例') },
          ]}
          style={{ marginRight: 12 }}
        />
        <Input
          style={{ width: 240 }}
          placeholder={t('funcd.searchByName', '通过名称搜索')}
          suffix={<SearchOutlined style={{ color: 'var(--text-3)' }} />}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          allowClear
        />
      </div>
      <Table<CaseDependencyLink>
        rowKey="caseId"
        size="middle"
        pagination={false}
        dataSource={visible}
        locale={{
          emptyText: (
            <span style={{ color: 'var(--text-2)' }}>
              {t('funcd.noDataPlease', '暂无数据，请')} <a onClick={() => { setSel(undefined); setOpen(true) }}>{addLabel}</a>
            </span>
          ),
        }}
        columns={[
          { title: 'ID', dataIndex: 'num', width: 200, render: (v: number) => <span className="ms-mono">{v || '—'}</span> },
          { title: t('funcd.caseName2', '用例名称'), dataIndex: 'name' },
          { title: t('lv.createdBy', '创建人'), dataIndex: 'createdBy', width: 180, render: (u: string) => nameOf(u || undefined) },
          {
            title: t('req.action', '操作'), width: 110,
            render: (_v, r) => (
              <Button type="link" size="small" danger onClick={() => api.removeCaseDependency(caseId, r.caseId, direction).then(load)}>
                {t('funcd.removeDep', '取消依赖')}
              </Button>
            ),
          },
        ]}
      />
      <Modal open={open} title={addLabel} onCancel={() => setOpen(false)} onOk={doAdd} okText={t('a.confirm', '确定')} cancelText={t('a.cancel', '取消')} destroyOnHidden>
        <Select
          showSearch
          style={{ width: '100%' }}
          placeholder={t('funcd.pickCase', '请选择用例')}
          optionFilterProp="label"
          value={sel}
          onChange={setSel}
          options={cases.filter((x) => x.id !== caseId).map((x) => ({ value: x.id, label: `[${x.num || '—'}] ${x.name}` }))}
        />
      </Modal>
    </div>
  )
}

const REVIEW_STATUS: Record<string, string> = { PASS: '通过', REJECT: '不通过', PENDING: '进行中' }

/** Reviews tab: reviews containing the case + the case's verdict inside each. */
function ReviewTab({ caseId }: { caseId: string }) {
  const { t } = useI18n()
  const [rows, setRows] = useState<CaseReviewLink[]>([])
  const [search, setSearch] = useState('')
  useEffect(() => {
    api.caseReviewLinks(caseId).then(setRows).catch(() => setRows([]))
  }, [caseId])
  const visible = rows.filter((r) => !search || r.reviewId.includes(search))
  return (
    <div style={{ overflow: 'auto' }}>
      <div style={{ display: 'flex', marginBottom: 12 }}>
        <span style={{ fontWeight: 600 }}>{t('funcd.reviewList', '用例评审列表')}</span>
        <span style={{ flex: 1 }} />
        <Input
          style={{ width: 260 }}
          placeholder={t('funcd.searchByIdName', '通过 ID/名称搜索')}
          suffix={<SearchOutlined style={{ color: 'var(--text-3)' }} />}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          allowClear
        />
      </div>
      <Table<CaseReviewLink>
        rowKey="reviewId"
        size="middle"
        pagination={false}
        dataSource={visible}
        locale={{ emptyText: <span style={{ color: 'var(--text-2)' }}>{t('g.empty', '暂无数据')}</span> }}
        columns={[
          { title: 'ID', dataIndex: 'reviewId', width: 160, render: (v: string) => <span className="ms-mono">{v.slice(0, 8)}</span> },
          { title: t('funcd.reviewName', '评审名称'), dataIndex: 'reviewId', render: (v: string) => v },
          {
            title: t('funcd.reviewStatus', '评审状态'), width: 140,
            render: (_v, r) => <Tag color={r.status === 'PENDING' ? 'blue' : 'default'}>{r.status === 'PENDING' ? t('funcd.reviewing', '进行中') : t('funcd.reviewDone', '已完成')}</Tag>,
          },
          {
            title: t('funcd.reviewResult', '评审结果'), dataIndex: 'status', width: 140,
            render: (s: string) => (
              <Tag color={s === 'PASS' ? 'green' : s === 'REJECT' ? 'red' : 'blue'}>
                {t(`funcd.review.${s}`, REVIEW_STATUS[s] || s)}
              </Tag>
            ),
          },
          { title: t('funcd.reviewTime', '评审时间'), dataIndex: 'createdAt', width: 200, render: (v: string) => <span className="ms-mono">{v.slice(0, 19)}</span> },
        ]}
      />
    </div>
  )
}

const EXEC_STATUS: Record<string, string> = { PENDING: '未执行', PASS: '通过', FAIL: '失败', BLOCKED: '阻塞', SKIP: '跳过' }

/** Test plans tab: plans containing the case + execution outcome. */
function PlanTab({ caseId }: { caseId: string }) {
  const { t } = useI18n()
  const [rows, setRows] = useState<CasePlanLink[]>([])
  const [search, setSearch] = useState('')
  useEffect(() => {
    api.casePlanLinks(caseId).then(setRows).catch(() => setRows([]))
  }, [caseId])
  const visible = rows.filter((r) => !search || r.planName.includes(search) || r.planId.includes(search))
  return (
    <div style={{ overflow: 'auto' }}>
      <div style={{ display: 'flex', marginBottom: 12 }}>
        <span style={{ fontWeight: 600 }}>{t('funcd.planList', '测试计划列表')}</span>
        <span style={{ flex: 1 }} />
        <Input
          style={{ width: 280 }}
          placeholder={t('funcd.searchByIdNameTag', '通过 ID/名称/标签搜索')}
          suffix={<SearchOutlined style={{ color: 'var(--text-3)' }} />}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          allowClear
        />
      </div>
      <Table<CasePlanLink>
        rowKey="planId"
        size="middle"
        pagination={false}
        dataSource={visible}
        locale={{ emptyText: <span style={{ color: 'var(--text-2)' }}>{t('g.empty', '暂无数据')}</span> }}
        columns={[
          { title: 'ID', dataIndex: 'planId', width: 120, render: (v: string) => <span className="ms-mono">{v.slice(0, 8)}</span> },
          { title: t('funcd.planName', '计划名称'), dataIndex: 'planName' },
          { title: t('funcd.belongProject', '所属项目'), dataIndex: 'projectName', width: 160, render: (v: string) => v || '—' },
          {
            title: t('funcd.planStatus', '计划状态'), dataIndex: 'archived', width: 120,
            render: (a: boolean) => <Tag color={a ? 'default' : 'blue'}>{a ? t('funcd.archived', '已归档') : t('funcd.running', '进行中')}</Tag>,
          },
          {
            title: t('funcd.execResult', '执行结果'), dataIndex: 'execStatus', width: 120,
            render: (s: string) => (
              <Tag color={s === 'PASS' ? 'green' : s === 'FAIL' ? 'red' : 'default'}>{t(`funcd.exec.${s}`, EXEC_STATUS[s] || s)}</Tag>
            ),
          },
          { title: t('funcd.execTime', '执行时间'), dataIndex: 'executedAt', width: 200, render: (v: string) => (v ? <span className="ms-mono">{v.slice(0, 19)}</span> : '—') },
        ]}
      />
    </div>
  )
}

/** Comments tab: case comments (review/execution comment sources are separate flows, empty for now). */
function CommentTab({ caseId, nameOf }: { caseId: string; nameOf: (u?: string) => string }) {
  const { t } = useI18n()
  const [kind, setKind] = useState<'case' | 'review' | 'exec'>('case')
  const [rows, setRows] = useState<CommentItem[]>([])
  const load = () => api.comments('FUNCTIONAL_CASE', caseId).then(setRows).catch(() => setRows([]))
  useEffect(() => { load() }, [caseId]) // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    const h = () => load()
    window.addEventListener('case-comment-posted', h)
    return () => window.removeEventListener('case-comment-posted', h)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [caseId])
  return (
    <div style={{ overflow: 'auto' }}>
      <div style={{ display: 'flex', marginBottom: 12 }}>
        <span style={{ fontWeight: 600 }}>{t('funcd.commentList', '评论列表')}</span>
        <span style={{ flex: 1 }} />
        <Segmented
          value={kind}
          onChange={(v) => setKind(v as typeof kind)}
          options={[
            { value: 'case', label: t('funcd.caseComments', '用例评论') },
            { value: 'review', label: t('funcd.reviewComments', '评审评论') },
            { value: 'exec', label: t('funcd.execComments', '执行评论') },
          ]}
        />
      </div>
      {kind !== 'case' || !rows.length ? (
        <div style={{ textAlign: 'center', color: 'var(--text-3)', padding: '48px 0' }}>{t('g.empty', '暂无数据')}</div>
      ) : (
        rows.map((r) => (
          <div key={r.id} style={{ display: 'flex', gap: 12, padding: '12px 0', borderBottom: '1px solid var(--border-soft)' }}>
            <Avatar size="small" icon={<UserOutlined />} />
            <div style={{ flex: 1 }}>
              <div>
                <span style={{ fontWeight: 600 }}>{nameOf(r.author)}</span>
                <span style={{ color: 'var(--text-3)', marginLeft: 8, fontSize: 12 }} className="ms-mono">{r.createdAt.slice(0, 19)}</span>
              </div>
              <div style={{ marginTop: 4, whiteSpace: 'pre-wrap' }}>{r.content}</div>
            </div>
            <Popconfirm title={t('funcd.confirmDeleteComment', '删除该评论?')} onConfirm={() => api.deleteComment(r.id).then(load)}>
              <Button type="text" size="small" icon={<DeleteOutlined />} />
            </Popconfirm>
          </div>
        ))
      )}
    </div>
  )
}

const CHANGE_FIELD: Record<string, string> = {
  create: '创建用例',
  name: '名称',
  module: '所属模块',
  priority: '用例等级',
  status: '状态',
  tags: '标签',
  steps: '步骤描述',
}

/** Change history tab: field-level audit rows recorded by the backend on create/update. */
function HistoryTab({ caseId, nameOf }: { caseId: string; nameOf: (u?: string) => string }) {
  const { t } = useI18n()
  const [rows, setRows] = useState<CaseChange[]>([])
  useEffect(() => {
    api.caseChanges(caseId).then(setRows).catch(() => setRows([]))
  }, [caseId])
  const fieldLabel = (f: string) =>
    f.startsWith('field.') ? f.slice(6) : t(`funcd.field.${f}`, CHANGE_FIELD[f] || f)
  const cell = (v: string) =>
    v ? (
      <Tooltip title={<span style={{ whiteSpace: 'pre-wrap' }}>{v}</span>}>
        <span style={{ display: 'inline-block', maxWidth: 260, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', verticalAlign: 'bottom' }}>{v}</span>
      </Tooltip>
    ) : (
      '—'
    )
  return (
    <Table<CaseChange>
      rowKey={(_, i) => String(i)}
      size="middle"
      pagination={false}
      dataSource={rows}
      locale={{ emptyText: <span style={{ color: 'var(--text-2)' }}>{t('funcd.noChanges', '暂无变更记录')}</span> }}
      columns={[
        { title: t('funcd.changeField', '变更字段'), dataIndex: 'field', width: 160, render: fieldLabel },
        { title: t('funcd.oldValue', '旧值'), dataIndex: 'oldValue', render: cell },
        { title: t('funcd.newValue', '新值'), dataIndex: 'newValue', render: cell },
        { title: t('funcd.operator', '操作人'), dataIndex: 'actor', width: 140, render: (u: string) => nameOf(u || undefined) },
        { title: t('funcd.changeTime', '变更时间'), dataIndex: 'createdAt', width: 200, render: (v: string) => <span className="ms-mono">{v.slice(0, 19)}</span> },
      ]}
    />
  )
}

/** Bottom comment input: ⌘/Ctrl+Enter posts to the case's comment stream. */
function CommentBar({ caseId, onPosted }: { caseId: string; onPosted: () => void }) {
  const { t } = useI18n()
  const [value, setValue] = useState('')
  const posting = useRef(false)
  const post = async () => {
    const content = value.trim()
    if (!content || posting.current) return
    posting.current = true
    try {
      await api.addComment({ targetType: 'FUNCTIONAL_CASE', targetId: caseId, content })
      setValue('')
      window.dispatchEvent(new Event('case-comment-posted'))
      onPosted()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('funcd.commentFailed', '评论失败'))
    } finally {
      posting.current = false
    }
  }
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '12px 16px', borderTop: '1px solid var(--border-soft)' }}>
      <Avatar icon={<UserOutlined />} />
      <Input
        placeholder={t('funcd.commentPh', '请输入评论，⌘ + Enter 结束')}
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={(e) => {
          if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') post()
        }}
      />
    </div>
  )
}
