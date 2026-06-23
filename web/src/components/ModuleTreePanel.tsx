import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { Button, Dropdown, Input, Modal, Tooltip, Tree } from 'antd'
import { message, modal } from '../feedback'
import { FolderOutlined, InboxOutlined, MinusSquareOutlined, MoreOutlined, PlusOutlined, SearchOutlined } from '@ant-design/icons'
import { api, ApiError, type ApiModule } from '../api'
import { useI18n } from '../i18n'

// 给定模块 id,返回其自身 + 所有后代 id(用于「选父含子」的列表过滤,使计数与列表一致)。
export function collectSubtreeIds(modules: ApiModule[], moduleId: string): string[] {
  const childrenOf = (mid: string) => modules.filter((m) => (m.parentId || null) === mid)
  const walk = (mid: string): string[] => [mid, ...childrenOf(mid).flatMap((c) => walk(c.id))]
  return walk(moduleId)
}

// 列表项是否落在当前选中的模块键下(ALL=全部,UNFILED=未归类,<id>=该模块整棵子树)。
export function inSelectedModule(modules: ApiModule[], selectedKey: string, moduleId: string): boolean {
  if (selectedKey === 'ALL') return true
  if (selectedKey === 'UNFILED') return !moduleId
  return collectSubtreeIds(modules, selectedKey).includes(moduleId)
}

// 统一的左侧模块树面板(场景 / 文件管理 等复用):模块名称搜索 + 「全部 (N)」工具条
// (收起全部 / 新建顶层模块)+ 层级模块树(子树计数 + 新建子模块 / 重命名 / 删除)。
// 不含外层宽度/边框 —— 由调用方用 <ResizableSider> 包裹(获得左右拖拽改宽)。
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
      if (moduleSearch && !matchName(m) && subs.length === 0) return null // 搜索无命中则隐藏
      return { key: m.id, title: <ModuleTitle name={m.name} count={subtreeCount(m.id)} onAction={(a) => onModuleAction(a, m)} />, children: subs.length ? subs : undefined }
    }
    const roots = childModulesOf(null).map(moduleNode).filter(Boolean)
    return [{
      key: 'ALL',
      icon: <FolderOutlined />,
      title: `${allLabel} (${items.length})`,
      children: [
        { key: 'UNFILED', icon: <InboxOutlined />, title: `${unfiledLabel} (${unfiledCount})` },
        ...roots,
      ],
    }]
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [modules, items, moduleSearch, allLabel, unfiledLabel])

  return (
    <>
      {header}
      <div style={{ padding: '10px 10px 6px' }}>
        <Input size="small" allowClear prefix={<SearchOutlined style={{ color: 'var(--text-3)' }} />} placeholder={searchPlaceholder ?? t('apidef.moduleSearch', '请输入模块名称搜索')} value={moduleSearch} onChange={(e) => onModuleSearch(e.target.value)} />
      </div>
      {/* 工具条:收起全部 / 新建顶层模块(「全部 (N)」由下方树根节点承载,不在此重复)。 */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 2, padding: '0 8px 8px', borderBottom: '1px solid var(--border-soft)' }}>
        <div style={{ flex: 1 }} />
        <Tooltip title={expanded.length ? t('apidef.collapseAll', '收起全部') : t('apidef.expandAll', '展开全部')}>
          <Button size="small" type="text" icon={<MinusSquareOutlined />} onClick={() => setExpanded(expanded.length ? [] : allExpandableKeys)} />
        </Tooltip>
        <Tooltip title={t('apidef.newTopModule', '新建顶层模块')}>
          <Button size="small" type="text" icon={<PlusOutlined />} style={{ color: '#52c41a' }} onClick={() => setForm({ mode: 'create', parentId: null })} />
        </Tooltip>
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

// 模块树子节点标题:文件夹图标 + 名称 + 子树计数 + 「...」菜单(新建子模块 / 重命名 / 删除)。
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

// 新建 / 重命名模块弹窗(复用项目级模块 API)。
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
