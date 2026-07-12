import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { Button, Dropdown, Input, Modal, Tooltip, Tree } from 'antd'
import { message, modal } from '../feedback'
import { FolderOutlined, InboxOutlined, MinusSquareOutlined, MoreOutlined, PlusOutlined, SearchOutlined } from '@ant-design/icons'
import { api, ApiError, type ApiModule } from '../api'
import { useI18n } from '../i18n'

// Return a module id plus all descendant ids (parent selection includes children, keeping counts consistent with the list).
export function collectSubtreeIds(modules: ApiModule[], moduleId: string): string[] {
  const childrenOf = (mid: string) => modules.filter((m) => (m.parentId || null) === mid)
  const walk = (mid: string): string[] => [mid, ...childrenOf(mid).flatMap((c) => walk(c.id))]
  return walk(moduleId)
}

// Whether a list item falls under the selected module key (ALL = everything, UNFILED = no module, <id> = that module's whole subtree).
export function inSelectedModule(modules: ApiModule[], selectedKey: string, moduleId: string): boolean {
  if (selectedKey === 'ALL') return true
  if (selectedKey === 'UNFILED') return !moduleId
  return collectSubtreeIds(modules, selectedKey).includes(moduleId)
}

// Shared left module-tree panel (scenarios, file management, ...): module name search +
// "All (N)" toolbar row (collapse all / new top-level module) + hierarchical tree
// (subtree counts + new sub-module / rename / delete).
// No outer width/border — callers wrap it in <ResizableSider> for drag-resizing.
export function ModuleTreePanel<T>({
  projectId,
  modules,
  items,
  getModuleId,
  selectedKey,
  onSelect,
  allLabel,
  unfiledLabel,
  moduleSearch,
  onModuleSearch,
  searchPlaceholder,
  onModulesChanged,
  header,
  footer,
  deleteModuleContent,
}: {
  projectId: string
  modules: ApiModule[]
  items: T[]
  getModuleId: (item: T) => string
  selectedKey: string
  onSelect: (key: string) => void
  allLabel: string
  unfiledLabel: string
  moduleSearch: string
  onModuleSearch: (v: string) => void
  searchPlaceholder?: string
  onModulesChanged: () => void
  header?: ReactNode
  footer?: ReactNode
  deleteModuleContent?: string
}) {
  const { t } = useI18n()
  const [expanded, setExpanded] = useState<string[]>(['ALL'])
  const [form, setForm] = useState<{ mode: 'create' | 'rename'; id?: string; parentId?: string | null; name?: string } | null>(null)

  const childModulesOf = (mid: string | null) => modules.filter((m) => (m.parentId || null) === mid)
  const directCount = (mid: string) => items.filter((it) => getModuleId(it) === mid).length
  const subtreeCount = (mid: string): number => directCount(mid) + childModulesOf(mid).reduce((n, c) => n + subtreeCount(c.id), 0)
  const unfiledCount = items.filter((it) => !getModuleId(it)).length
  const allExpandableKeys = useMemo(() => ['ALL', ...modules.map((m) => m.id)], [modules])

  const onModuleAction = (action: string, m: ApiModule) => {
    if (action === 'rename') setForm({ mode: 'rename', id: m.id, name: m.name })
    else if (action === 'sub') setForm({ mode: 'create', parentId: m.id })
    else if (action === 'delete')
      modal.confirm({
        title: `${t('apidef.deleteModuleTitle', '删除模块')}「${m.name}」?`,
        content: deleteModuleContent ?? t('apidef.deleteModuleContent', '其下内容将变为未规划(不会被删除)。'),
        okButtonProps: { danger: true },
        onOk: async () => {
          try {
            await api.deleteModule(m.id)
            message.success(t('apidef.deleted', '已删除'))
            if (selectedKey === m.id) onSelect('ALL')
            onModulesChanged()
          } catch (e) {
            message.error(e instanceof ApiError ? e.message : t('apidef.deleteFailed', '删除失败'))
          }
        },
      })
  }

  const treeData = useMemo(() => {
    const lc = (s: string) => s.toLowerCase()
    const matchName = (m: ApiModule) => !moduleSearch || lc(m.name).includes(lc(moduleSearch))
    const moduleNode = (m: ApiModule): any => {
      const subs = childModulesOf(m.id).map(moduleNode).filter(Boolean)
      if (moduleSearch && !matchName(m) && subs.length === 0) return null // hide when search misses
      return { key: m.id, title: <ModuleTitle name={m.name} count={subtreeCount(m.id)} onAction={(a) => onModuleAction(a, m)} />, children: subs.length ? subs : undefined }
    }
    const roots = childModulesOf(null).map(moduleNode).filter(Boolean)
    // Root row inlines the collapse-all / new-top-module icons on the same line as "All (N)".
    // Note: the root must not use Tree's icon slot — a separate icon element plus a 100%-wide
    // title pushes the title to the next line. Folding the folder icon into the title keeps
    // icon + name + tool buttons on one line.
    return [{
      key: 'ALL',
      title: (
        <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4, width: '100%', minWidth: 0 }}>
          <FolderOutlined style={{ color: 'var(--text-3)', flexShrink: 0 }} />
          <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{`${allLabel} (${items.length})`}</span>
          <Tooltip title={expanded.length ? t('apidef.collapseAll', '收起全部') : t('apidef.expandAll', '展开全部')}>
            <Button
              size="small"
              type="text"
              style={{ width: 22, minWidth: 22, padding: 0, flexShrink: 0 }}
              icon={<MinusSquareOutlined />}
              onClick={(e) => { e.stopPropagation(); setExpanded(expanded.length ? [] : allExpandableKeys) }}
            />
          </Tooltip>
          <Tooltip title={t('apidef.newTopModule', '新建顶层模块')}>
            <Button
              size="small"
              type="text"
              style={{ color: 'var(--success)', width: 22, minWidth: 22, padding: 0, flexShrink: 0 }}
              icon={<PlusOutlined />}
              onClick={(e) => { e.stopPropagation(); setForm({ mode: 'create', parentId: null }) }}
            />
          </Tooltip>
        </span>
      ),
      children: [
        { key: 'UNFILED', icon: <InboxOutlined />, title: `${unfiledLabel} (${unfiledCount})` },
        ...roots,
      ],
    }]
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [modules, items, moduleSearch, allLabel, unfiledLabel, expanded, allExpandableKeys])

  return (
    <>
      {header}
      <div style={{ padding: '10px 10px 6px' }}>
        <Input size="small" allowClear prefix={<SearchOutlined style={{ color: 'var(--text-3)' }} />} placeholder={searchPlaceholder ?? t('apidef.moduleSearch', '请输入模块名称搜索')} value={moduleSearch} onChange={(e) => onModuleSearch(e.target.value)} />
      </div>
      <div style={{ flex: 1, overflow: 'auto', padding: 8 }}>
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
      {footer}
      <ModuleFormModal state={form} projectId={projectId} onClose={() => setForm(null)} onDone={() => { setForm(null); onModulesChanged() }} />
    </>
  )
}

// Child node title: folder icon + name + subtree count + "..." menu (new sub-module / rename / delete).
function ModuleTitle({ name, count, onAction }: { name: string; count?: number; onAction: (a: string) => void }) {
  const { t } = useI18n()
  return (
    <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6, width: '100%', minWidth: 0 }}>
      <FolderOutlined style={{ color: 'var(--text-3)', flexShrink: 0 }} />
      <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{name}</span>
      {count != null && <span style={{ color: 'var(--text-3)', fontSize: 12, flexShrink: 0 }}>{count}</span>}
      <Dropdown
        trigger={['click']}
        menu={{
          items: [
            { key: 'sub', label: t('apidef.newSubModule', '新建子模块') },
            { key: 'rename', label: t('apidef.rename', '重命名') },
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

// Create / rename module modal (reuses the project-level module API).
function ModuleFormModal({ state, projectId, onClose, onDone }: { state: { mode: 'create' | 'rename'; id?: string; parentId?: string | null; name?: string } | null; projectId: string; onClose: () => void; onDone: () => void }) {
  const { t } = useI18n()
  const [name, setName] = useState('')
  useEffect(() => setName(state?.name || ''), [state])
  if (!state) return null
  const submit = async () => {
    if (!name.trim()) return
    try {
      if (state.mode === 'create') await api.createModule({ projectId, parentId: state.parentId ?? null, name: name.trim() })
      else await api.renameModule(state.id!, name.trim())
      message.success(t('apidef.saved', '已保存'))
      onDone()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('apidef.saveFailed', '保存失败'))
    }
  }
  return (
    <Modal title={state.mode === 'create' ? t('apidef.newModule', '新建模块') : t('apidef.renameModule', '重命名模块')} open onCancel={onClose} onOk={submit} destroyOnHidden>
      <Input placeholder={t('apidef.moduleName', '模块名')} value={name} onChange={(e) => setName(e.target.value)} onPressEnter={submit} autoFocus />
    </Modal>
  )
}
