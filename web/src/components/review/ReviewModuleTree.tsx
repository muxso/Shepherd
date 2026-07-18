import { useMemo, useState } from 'react'
import { Button, Dropdown, Input, Tooltip, Tree } from 'antd'
import EditDrawer from '../EditDrawer'
import {
  FolderAddOutlined,
  FolderOutlined,
  InboxOutlined,
  MoreOutlined,
  PlusOutlined,
  SearchOutlined,
  SubnodeOutlined,
} from '@ant-design/icons'
import { modal } from '../../feedback'
import { useI18n } from '../../i18n'
import type { CaseReviewSummary } from '../../api'
import {
  inReviewModule,
  moduleSubtreeIds,
  reviewModuleAdd,
  reviewModuleRemove,
  reviewModuleRename,
  type ReviewModule,
} from './reviewLocal'

// Left module-tree panel for case reviews (modules are local, reviews carry moduleId backend-side):
// "新建" primary button + module name search + tree with "全部评审 (N)" root / per-module counts / 未规划评审.
export default function ReviewModuleTree({
  projectId,
  items,
  modules,
  selectedKey,
  onSelect,
  onNewReview,
  onModulesChanged,
}: {
  projectId: string
  items: CaseReviewSummary[]
  modules: ReviewModule[]
  selectedKey: string
  onSelect: (key: string) => void
  onNewReview: () => void
  /** Fires after module add/rename/delete; delete passes removed ids (owned reviews become unfiled visually). */
  onModulesChanged: (modules: ReviewModule[], removedIds?: string[]) => void
}) {
  const { t } = useI18n()
  const [search, setSearch] = useState('')
  const [expanded, setExpanded] = useState<string[]>(['ALL'])
  const [form, setForm] = useState<{ mode: 'create' | 'rename'; id?: string; parentId?: string | null } | null>(null)

  const childOf = (pid: string | null) => modules.filter((m) => (m.parentId || null) === pid)
  const subtreeCount = (mid: string) => {
    const ids = moduleSubtreeIds(modules, mid)
    return items.filter((it) => it.moduleId && ids.includes(it.moduleId)).length
  }
  const unfiledCount = items.filter((it) => inReviewModule(modules, 'UNFILED', it.moduleId)).length
  const selectedModule = modules.find((m) => m.id === selectedKey)

  const onModuleAction = (action: string, m: ReviewModule) => {
    if (action === 'rename') setForm({ mode: 'rename', id: m.id })
    else if (action === 'sub') setForm({ mode: 'create', parentId: m.id })
    else if (action === 'delete')
      modal.confirm({
        title: `${t('review.deleteModuleTitle', '删除模块')}「${m.name}」?`,
        content: t('review.deleteModuleContent', '其下评审将归入未规划评审(不会删除评审)。'),
        okButtonProps: { danger: true },
        onOk: () => {
          const { modules: next, removedIds } = reviewModuleRemove(projectId, m.id)
          if (removedIds.includes(selectedKey)) onSelect('ALL')
          onModulesChanged(next, removedIds)
        },
      })
  }

  const treeData = useMemo(() => {
    const lc = (s: string) => s.toLowerCase()
    const node = (m: ReviewModule): any => {
      const subs = childOf(m.id).map(node).filter(Boolean)
      if (search && !lc(m.name).includes(lc(search)) && subs.length === 0) return null // hide when search misses
      return {
        key: m.id,
        title: <ModuleTitle name={m.name} count={subtreeCount(m.id)} onAction={(a) => onModuleAction(a, m)} />,
        children: subs.length ? subs : undefined,
      }
    }
    return [
      {
        key: 'ALL',
        icon: <FolderOutlined />,
        title: (
          <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4, width: '100%', minWidth: 0 }}>
            <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
              {t('review.allReviews', '全部评审')} ({items.length})
            </span>
            <Tooltip title={t('review.addModule', '加模块')}>
              <Button
                size="small"
                type="text"
                icon={<FolderAddOutlined />}
                style={{ color: 'var(--brand)' }}
                onClick={(e) => { e.stopPropagation(); setForm({ mode: 'create', parentId: null }) }}
              />
            </Tooltip>
            <Tooltip title={selectedModule ? `${t('review.addSubModule', '加子模块')} · ${selectedModule.name}` : t('review.addSubModuleHint', '选中一个模块后可加子模块')}>
              <Button
                size="small"
                type="text"
                icon={<SubnodeOutlined />}
                disabled={!selectedModule}
                style={selectedModule ? { color: 'var(--brand)' } : undefined}
                onClick={(e) => { e.stopPropagation(); if (selectedModule) setForm({ mode: 'create', parentId: selectedModule.id }) }}
              />
            </Tooltip>
          </span>
        ),
        children: [
          { key: 'UNFILED', icon: <InboxOutlined />, title: `${t('review.moduleUnfiled', '未规划评审')} ${unfiledCount}` },
          ...childOf(null).map(node).filter(Boolean),
        ],
      },
    ]
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [modules, items, search, selectedKey, t])

  return (
    <>
      <div style={{ padding: '10px 10px 0', display: 'flex', gap: 8 }}>
        <Input
          size="small"
          allowClear
          prefix={<SearchOutlined style={{ color: 'var(--text-3)' }} />}
          placeholder={t('review.moduleSearchPh', '请输入模块名称')}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          style={{ flex: 1 }}
        />
        <Button type="primary" size="small" icon={<PlusOutlined />} onClick={onNewReview}>
          {t('review.new', '新建')}
        </Button>
      </div>
      <div style={{ flex: 1, overflow: 'auto', padding: 8, marginTop: 6, borderTop: '1px solid var(--border-soft)' }}>
        <Tree
          showIcon
          blockNode
          expandedKeys={expanded}
          onExpand={(keys) => setExpanded(keys.map(String))}
          selectedKeys={[selectedKey]}
          treeData={treeData}
          onSelect={(keys) => { const k = String(keys[0] ?? ''); if (k) onSelect(k) }}
        />
      </div>
      <ModuleNameDrawer
        key={form ? `${form.mode}:${form.id ?? ''}:${form.parentId ?? ''}` : 'closed'}
        state={form}
        modules={modules}
        onClose={() => setForm(null)}
        onSubmit={(name) => {
          if (!form) return
          const next = form.mode === 'create' ? reviewModuleAdd(projectId, name, form.parentId ?? null) : reviewModuleRename(projectId, form.id!, name)
          setForm(null)
          onModulesChanged(next)
        }}
      />
    </>
  )
}

// Module node title: folder icon + name + subtree review count + "..." menu (add submodule/rename/delete).
function ModuleTitle({ name, count, onAction }: { name: string; count: number; onAction: (a: string) => void }) {
  const { t } = useI18n()
  return (
    <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6, width: '100%', minWidth: 0 }}>
      <FolderOutlined style={{ color: 'var(--text-3)', flexShrink: 0 }} />
      <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{name}</span>
      <span style={{ color: 'var(--text-3)', fontSize: 12, flexShrink: 0 }}>{count}</span>
      <Dropdown
        trigger={['click']}
        menu={{
          items: [
            { key: 'sub', label: t('review.addSubModule', '加子模块') },
            { key: 'rename', label: t('review.renameModule', '重命名') },
            { type: 'divider' as const },
            { key: 'delete', label: t('a.delete', '删除'), danger: true },
          ],
          onClick: ({ key, domEvent }) => { domEvent.stopPropagation(); onAction(key) },
        }}
      >
        <MoreOutlined onClick={(e) => e.stopPropagation()} style={{ padding: '0 4px', color: 'var(--text-3)' }} />
      </Dropdown>
    </span>
  )
}

// Create/rename module drawer (local only, no backend call).
function ModuleNameDrawer({
  state,
  modules,
  onClose,
  onSubmit,
}: {
  state: { mode: 'create' | 'rename'; id?: string } | null
  modules: ReviewModule[]
  onClose: () => void
  onSubmit: (name: string) => void
}) {
  const { t } = useI18n()
  const initial = state?.mode === 'rename' ? modules.find((m) => m.id === state.id)?.name || '' : ''
  const [name, setName] = useState(initial)
  const submit = () => { const v = name.trim(); if (v) onSubmit(v) }
  if (!state) return null
  return (
    <EditDrawer
      title={state.mode === 'create' ? t('review.newModuleTitle', '新建模块') : t('review.renameModuleTitle', '重命名模块')}
      open
      onCancel={onClose}
      onOk={submit}
      okButtonProps={{ disabled: !name.trim() }}
    >
      <Input
        placeholder={t('review.moduleName', '模块名称')}
        value={name}
        onChange={(e) => setName(e.target.value)}
        onPressEnter={submit}
        autoFocus
      />
    </EditDrawer>
  )
}
