import { useMemo, useState } from 'react'
import { Button, Dropdown, Input, Modal, Space, Tooltip, Tree } from 'antd'
import {
  DownOutlined,
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
import type { RegItem } from '../../registry'
import {
  moduleOf,
  moduleSubtreeIds,
  planModuleAdd,
  planModuleRemove,
  planModuleRename,
  type PlanModule,
} from './planLocal'

// Left module-tree panel for test plans (modules are local, stored in localStorage):
// "New ▾" dropdown (new plan / new group) + module name search + tree.
// Root row ("all plans (n)") carries add-module/add-submodule actions; module nodes carry add-submodule/rename/delete.
// Outer width/border comes from the caller's <ResizableSider>.
export default function PlanModuleTree({
  projectId,
  items,
  modules,
  selectedKey,
  onSelect,
  onNewPlan,
  onNewGroup,
  onModulesChanged,
}: {
  projectId: string
  items: RegItem[]
  modules: PlanModule[]
  selectedKey: string
  onSelect: (key: string) => void
  onNewPlan: () => void
  onNewGroup: () => void
  /** Fires after module add/rename/delete; delete passes removed ids so the caller moves owned plans back to unfiled. */
  onModulesChanged: (modules: PlanModule[], removedIds?: string[]) => void
}) {
  const { t } = useI18n()
  const [search, setSearch] = useState('')
  const [expanded, setExpanded] = useState<string[]>(['ALL'])
  const [form, setForm] = useState<{ mode: 'create' | 'rename'; id?: string; parentId?: string | null } | null>(null)

  const childOf = (pid: string | null) => modules.filter((m) => (m.parentId || null) === pid)
  const subtreeCount = (mid: string) => {
    const ids = moduleSubtreeIds(modules, mid)
    return items.filter((it) => ids.includes(moduleOf(it))).length
  }
  const unfiledCount = items.filter((it) => !moduleOf(it)).length
  // "Add submodule" attaches under the currently selected module; unavailable when ALL/UNFILED is selected.
  const selectedModule = modules.find((m) => m.id === selectedKey)

  const onModuleAction = (action: string, m: PlanModule) => {
    if (action === 'rename') setForm({ mode: 'rename', id: m.id })
    else if (action === 'sub') setForm({ mode: 'create', parentId: m.id })
    else if (action === 'delete')
      modal.confirm({
        title: `${t('plan.deleteModuleTitle', '删除模块')}「${m.name}」?`,
        content: t('plan.deleteModuleContent', '其下计划将变为未规划(不会删除计划)。'),
        okButtonProps: { danger: true },
        onOk: () => {
          const { modules: next, removedIds } = planModuleRemove(projectId, m.id)
          if (removedIds.includes(selectedKey)) onSelect('ALL')
          onModulesChanged(next, removedIds)
        },
      })
  }

  const treeData = useMemo(() => {
    const lc = (s: string) => s.toLowerCase()
    const node = (m: PlanModule): any => {
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
              {t('plan.allTestPlans', '全部测试计划')} ({items.length})
            </span>
            <Tooltip title={t('plan.addModule', '加模块')}>
              <Button
                size="small"
                type="text"
                icon={<FolderAddOutlined />}
                style={{ color: 'var(--brand)' }}
                onClick={(e) => { e.stopPropagation(); setForm({ mode: 'create', parentId: null }) }}
              />
            </Tooltip>
            <Tooltip title={selectedModule ? `${t('plan.addSubModule', '加子模块')} · ${selectedModule.name}` : t('plan.addSubModuleHint', '选中一个模块后可加子模块')}>
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
          { key: 'UNFILED', icon: <InboxOutlined />, title: `${t('plan.unplanned', '未规划计划')} ${unfiledCount}` },
          ...childOf(null).map(node).filter(Boolean),
        ],
      },
    ]
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [modules, items, search, selectedKey, t])

  return (
    <>
      {/* Create entry: main button = new test plan; dropdown arrow offers new plan / new plan group. */}
      <div style={{ padding: '10px 10px 0' }}>
        <Space.Compact style={{ width: '100%' }}>
          <Button type="primary" icon={<PlusOutlined />} style={{ flex: 1 }} onClick={onNewPlan}>
            {t('plan.new', '新建')}
          </Button>
          <Dropdown
            trigger={['click']}
            menu={{
              items: [
                { key: 'plan', label: t('plan.newPlan', '新建测试计划') },
                { key: 'group', label: t('plan.newGroup', '新建计划组') },
              ],
              onClick: ({ key }) => (key === 'group' ? onNewGroup() : onNewPlan()),
            }}
          >
            <Button type="primary" icon={<DownOutlined />} />
          </Dropdown>
        </Space.Compact>
      </div>
      <div style={{ padding: '10px 10px 8px', borderBottom: '1px solid var(--border-soft)' }}>
        <Input
          size="small"
          allowClear
          prefix={<SearchOutlined style={{ color: 'var(--text-3)' }} />}
          placeholder={t('plan.moduleSearchPh', '请输入模块名称')}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
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
      <ModuleNameModal
        key={form ? `${form.mode}:${form.id ?? ''}:${form.parentId ?? ''}` : 'closed'}
        state={form}
        modules={modules}
        onClose={() => setForm(null)}
        onSubmit={(name) => {
          if (!form) return
          const next = form.mode === 'create' ? planModuleAdd(projectId, name, form.parentId ?? null) : planModuleRename(projectId, form.id!, name)
          setForm(null)
          onModulesChanged(next)
        }}
      />
    </>
  )
}

// Module node title: folder icon + name + subtree plan count + "..." menu (add submodule/rename/delete).
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
            { key: 'sub', label: t('plan.addSubModule', '加子模块') },
            { key: 'rename', label: t('plan.renameModule', '重命名') },
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

// Create/rename module modal (local only, no backend call).
function ModuleNameModal({
  state,
  modules,
  onClose,
  onSubmit,
}: {
  state: { mode: 'create' | 'rename'; id?: string } | null
  modules: PlanModule[]
  onClose: () => void
  onSubmit: (name: string) => void
}) {
  const { t } = useI18n()
  const initial = state?.mode === 'rename' ? modules.find((m) => m.id === state.id)?.name || '' : ''
  const [name, setName] = useState(initial)
  const submit = () => { const v = name.trim(); if (v) onSubmit(v) }
  if (!state) return null
  return (
    <Modal
      title={state.mode === 'create' ? t('plan.newModuleTitle', '新建模块') : t('plan.renameModuleTitle', '重命名模块')}
      open
      onCancel={onClose}
      onOk={submit}
      okButtonProps={{ disabled: !name.trim() }}
      destroyOnHidden
    >
      <Input
        placeholder={t('plan.moduleName', '模块名称')}
        value={name}
        onChange={(e) => setName(e.target.value)}
        onPressEnter={submit}
        autoFocus
      />
    </Modal>
  )
}
