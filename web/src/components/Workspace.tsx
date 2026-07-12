import { createContext, useContext, useEffect, useRef, useState, type ReactNode } from 'react'
import { useSearchParams } from 'react-router-dom'
import { Button, Empty, Input, Space, Table, Tabs } from 'antd'
import type { ColumnsType, TableProps } from 'antd/es/table'
import { PlusOutlined, ReloadOutlined } from '@ant-design/icons'
import { useI18n } from '../i18n'

const LIST_KEY = '__list__'

// Extra-content slot (DOM node) on the right of the workspace tab bar. The active tab can
// createPortal its own toolbar (e.g. scenario's env/run/save) into the same row.
const ExtraSlotContext = createContext<HTMLElement | null>(null)
export function useWorkspaceExtraSlot() {
  return useContext(ExtraSlotContext)
}

// Deep link: read ?open=<id>, let the caller open the detail tab, then strip the param to avoid re-triggering.
export function useOpenParam(onOpen: (id: string) => void) {
  const [params, setParams] = useSearchParams()
  useEffect(() => {
    const id = params.get('open')
    if (id) {
      onOpen(id)
      const next = new URLSearchParams(params)
      next.delete('open')
      setParams(next, { replace: true })
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [params])
}

// Multi-tab workspace state: a permanent list tab + dynamically opened detail tabs.
export function useWorkTabs() {
  const [openIds, setOpenIds] = useState<string[]>([])
  const [activeKey, setActiveKey] = useState(LIST_KEY)
  const open = (id: string) => {
    setOpenIds((ids) => (ids.includes(id) ? ids : [...ids, id]))
    setActiveKey(id)
  }
  const close = (id: string) => {
    setOpenIds((ids) => {
      const next = ids.filter((x) => x !== id)
      setActiveKey((cur) => (cur === id ? next[next.length - 1] || LIST_KEY : cur))
      return next
    })
  }
  const reset = () => {
    setOpenIds([])
    setActiveKey(LIST_KEY)
  }
  return { openIds, activeKey, setActiveKey, open, close, reset, LIST_KEY }
}

export interface WorkTab {
  key: string
  label: ReactNode
  children: ReactNode
}

// Shared resizable left pane (tree/filter panels). A 6px hot zone on the right edge drags the
// width, clamped to [min, max]; with storageKey the width persists to localStorage.
export function ResizableSider({
  children,
  defaultWidth = 252,
  min = 200,
  max = 480,
  storageKey,
}: {
  children: ReactNode
  defaultWidth?: number
  min?: number
  max?: number
  storageKey?: string
}) {
  const [width, setWidth] = useState(() => {
    if (storageKey && typeof localStorage !== 'undefined') {
      const saved = Number(localStorage.getItem(storageKey))
      if (saved >= min && saved <= max) return saved
    }
    return defaultWidth
  })
  const drag = useRef<{ startX: number; startW: number } | null>(null)

  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      if (!drag.current) return
      setWidth(Math.min(max, Math.max(min, drag.current.startW + (e.clientX - drag.current.startX))))
    }
    const onUp = () => {
      if (!drag.current) return
      drag.current = null
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
      if (storageKey) localStorage.setItem(storageKey, String(width))
    }
    window.addEventListener('mousemove', onMove)
    window.addEventListener('mouseup', onUp)
    return () => {
      window.removeEventListener('mousemove', onMove)
      window.removeEventListener('mouseup', onUp)
    }
  }, [width, max, min, storageKey])

  return (
    <div style={{ position: 'relative', width, flexShrink: 0, background: 'var(--panel)', borderRight: '1px solid var(--border-soft)', display: 'flex', flexDirection: 'column' }}>
      {children}
      <div
        onMouseDown={(e) => { drag.current = { startX: e.clientX, startW: width }; document.body.style.cursor = 'col-resize'; document.body.style.userSelect = 'none' }}
        style={{ position: 'absolute', top: 0, right: -3, width: 6, height: '100%', cursor: 'col-resize', zIndex: 10 }}
      />
    </div>
  )
}

// Shared layout: optional left pane (tree/filter) | multi-tab area (permanent list + multiple detail tabs).
export function Workspace({
  left,
  leftWidth = 220,
  siderKey,
  listLabel,
  listContent,
  tabs,
  activeKey,
  onChange,
  onClose,
}: {
  left?: ReactNode
  leftWidth?: number
  siderKey?: string
  listLabel?: string
  listContent: ReactNode
  tabs: WorkTab[]
  activeKey: string
  onChange: (k: string) => void
  onClose: (k: string) => void
}) {
  const { t } = useI18n()
  // Right-side slot: the active tab portals its toolbar in via useWorkspaceExtraSlot() + createPortal.
  const [slotEl, setSlotEl] = useState<HTMLDivElement | null>(null)
  const items = [
    { key: LIST_KEY, label: listLabel ?? t('ws.list', '列表'), closable: false, children: listContent },
    ...tabs.map((tab) => ({ key: tab.key, label: tab.label, children: tab.children })),
  ]
  return (
    <ExtraSlotContext.Provider value={slotEl}>
      <div style={{ display: 'flex', height: '100%' }}>
        {left !== undefined && (
          <ResizableSider defaultWidth={leftWidth} storageKey={siderKey}>
            {left}
          </ResizableSider>
        )}
        <div style={{ flex: 1, minWidth: 0, background: 'var(--panel)' }}>
          <Tabs
            type="editable-card"
            hideAdd
            activeKey={activeKey}
            onChange={onChange}
            onEdit={(key, action) => action === 'remove' && onClose(String(key))}
            items={items}
            style={{ height: '100%' }}
            className="ms-worktabs"
            tabBarExtraContent={{ right: <div ref={setSlotEl} style={{ display: 'flex', alignItems: 'center', gap: 8, paddingRight: 12 }} /> }}
          />
        </div>
      </div>
    </ExtraSlotContext.Provider>
  )
}

// Shared list: toolbar (new/custom actions + search + refresh) + table + pagination.
export function WorkList<T extends object>({
  onNew,
  newLabel,
  extraActions,
  onSearch,
  searchPlaceholder,
  onRefresh,
  columns,
  data,
  loading,
  rowKey = 'id',
  onRowClick,
  emptyText,
  expandable,
}: {
  onNew?: () => void
  newLabel?: string
  extraActions?: ReactNode
  onSearch?: (q: string) => void
  searchPlaceholder?: string
  onRefresh?: () => void
  columns: ColumnsType<T>
  data: T[]
  loading?: boolean
  rowKey?: string
  onRowClick?: (record: T) => void
  emptyText?: string
  /** Inline expand preview (passed through to antd expandable); clicking the expand icon does not trigger row click. */
  expandable?: TableProps<T>['expandable']
}) {
  const { t } = useI18n()
  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '10px 12px', borderBottom: '1px solid var(--border-soft)' }}>
        {onNew && (
          <Button type="primary" icon={<PlusOutlined />} onClick={onNew}>
            {newLabel ?? t('a.new', '新建')}
          </Button>
        )}
        <div style={{ flex: 1 }} />
        {onSearch && <Input.Search placeholder={searchPlaceholder ?? t('a.search', '搜索')} allowClear style={{ width: 240 }} onChange={(e) => onSearch(e.target.value)} />}
        {/* Utility actions (search/views/filter/columns) align right, matching the scenario page layout */}
        {extraActions}
        {onRefresh && <Button icon={<ReloadOutlined />} onClick={onRefresh} />}
      </div>
      <div style={{ flex: 1, overflow: 'auto', padding: 12 }}>
        <Table<T>
          rowKey={rowKey}
          size="middle"
          loading={loading}
          dataSource={data}
          columns={columns}
          expandable={expandable}
          onRow={onRowClick ? (r) => ({
            onClick: (e) => {
              // Expand-icon clicks only expand; don't open the detail.
              if ((e.target as Element).closest?.('.ant-table-row-expand-icon')) return
              onRowClick(r)
            },
            style: { cursor: 'pointer' },
          }) : undefined}
          pagination={{ pageSize: 15, size: 'small', showTotal: (n) => t('ws.total', '共 {n} 条').replace('{n}', String(n)) }}
          locale={{ emptyText: <Empty description={emptyText ?? t('common.empty', '暂无数据')} /> }}
        />
      </div>
    </div>
  )
}

// Shared left-pane header (title + right-side action).
export function PaneHeader({ title, action }: { title: string; action?: ReactNode }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', padding: '10px 12px', borderBottom: '1px solid var(--border-soft)' }}>
      <span style={{ fontWeight: 600 }}>{title}</span>
      <div style={{ flex: 1 }} />
      {action}
    </div>
  )
}

export { Space }
