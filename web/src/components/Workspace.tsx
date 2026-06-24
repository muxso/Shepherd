import { createContext, useContext, useEffect, useRef, useState, type ReactNode } from 'react'
import { useSearchParams } from 'react-router-dom'
import { Button, Empty, Input, Space, Table, Tabs } from 'antd'
import type { ColumnsType } from 'antd/es/table'
import { PlusOutlined, ReloadOutlined } from '@ant-design/icons'
import { useI18n } from '../i18n'

const LIST_KEY = '__list__'

// 工作区 Tab 栏右侧的额外内容插槽(DOM 节点)。活动 Tab 可通过 createPortal
// 把自己的工具栏(如场景的「环境/服务端执行/保存」)投射到 Tab 栏同一行右侧。
const ExtraSlotContext = createContext<HTMLElement | null>(null)
export function useWorkspaceExtraSlot() {
  return useContext(ExtraSlotContext)
}

// 深链:读取 ?open=<id>,交给调用方打开对应详情 tab,然后清除该参数(避免重复触发)。
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

// 多 Tab 工作区状态:首个常驻列表 tab + 动态打开的详情 tab。
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

// 统一的可拖拽左栏(树/筛选面板)。右缘提供 6px 拖拽热区,左右拖拽改宽,
// 宽度在 [min, max] 内夹取;带 storageKey 时记忆到 localStorage(刷新保留)。
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

// 统一三栏:左(树/筛选,可选)| 中右合一为多 Tab(列表常驻 + 详情多开)。
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
  // Tab 栏右侧插槽:活动 Tab 通过 useWorkspaceExtraSlot()+createPortal 投射工具栏。
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

// 统一列表:工具栏(新建/自定义动作 + 搜索 + 刷新)+ 表格 + 分页。
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
        {extraActions}
        <div style={{ flex: 1 }} />
        {onSearch && <Input.Search placeholder={searchPlaceholder ?? t('a.search', '搜索')} allowClear style={{ width: 240 }} onChange={(e) => onSearch(e.target.value)} />}
        {onRefresh && <Button icon={<ReloadOutlined />} onClick={onRefresh} />}
      </div>
      <div style={{ flex: 1, overflow: 'auto', padding: 12 }}>
        <Table<T>
          rowKey={rowKey}
          size="middle"
          loading={loading}
          dataSource={data}
          columns={columns}
          onRow={onRowClick ? (r) => ({ onClick: () => onRowClick(r), style: { cursor: 'pointer' } }) : undefined}
          pagination={{ pageSize: 15, size: 'small', showTotal: (n) => t('ws.total', '共 {n} 条').replace('{n}', String(n)) }}
          locale={{ emptyText: <Empty description={emptyText ?? t('common.empty', '暂无数据')} /> }}
        />
      </div>
    </div>
  )
}

// 左栏统一头部(标题 + 右侧动作)。
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
