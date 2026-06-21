import { useState, type ReactNode } from 'react'
import { Button, Empty, Input, Space, Table, Tabs } from 'antd'
import type { ColumnsType } from 'antd/es/table'
import { PlusOutlined, ReloadOutlined } from '@ant-design/icons'

const LIST_KEY = '__list__'

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

// 统一三栏:左(树/筛选,可选)| 中右合一为多 Tab(列表常驻 + 详情多开)。
export function Workspace({
  left,
  leftWidth = 220,
  listLabel = '列表',
  listContent,
  tabs,
  activeKey,
  onChange,
  onClose,
}: {
  left?: ReactNode
  leftWidth?: number
  listLabel?: string
  listContent: ReactNode
  tabs: WorkTab[]
  activeKey: string
  onChange: (k: string) => void
  onClose: (k: string) => void
}) {
  const items = [
    { key: LIST_KEY, label: listLabel, closable: false, children: listContent },
    ...tabs.map((t) => ({ key: t.key, label: t.label, children: t.children })),
  ]
  return (
    <div style={{ display: 'flex', height: '100%' }}>
      {left !== undefined && (
        <div style={{ width: leftWidth, background: '#fff', borderRight: '1px solid #f0f0f0', display: 'flex', flexDirection: 'column' }}>
          {left}
        </div>
      )}
      <div style={{ flex: 1, minWidth: 0, background: '#fff' }}>
        <Tabs
          type="editable-card"
          hideAdd
          activeKey={activeKey}
          onChange={onChange}
          onEdit={(key, action) => action === 'remove' && onClose(String(key))}
          items={items}
          style={{ height: '100%' }}
          className="ms-worktabs"
        />
      </div>
    </div>
  )
}

// 统一列表:工具栏(新建/自定义动作 + 搜索 + 刷新)+ 表格 + 分页。
export function WorkList<T extends object>({
  onNew,
  newLabel = '新建',
  extraActions,
  onSearch,
  searchPlaceholder = '搜索',
  onRefresh,
  columns,
  data,
  loading,
  rowKey = 'id',
  onRowClick,
  emptyText = '暂无数据',
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
  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '10px 12px', borderBottom: '1px solid #f0f0f0' }}>
        {onNew && (
          <Button type="primary" icon={<PlusOutlined />} onClick={onNew}>
            {newLabel}
          </Button>
        )}
        {extraActions}
        <div style={{ flex: 1 }} />
        {onSearch && <Input.Search placeholder={searchPlaceholder} allowClear style={{ width: 240 }} onChange={(e) => onSearch(e.target.value)} />}
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
          pagination={{ pageSize: 15, size: 'small', showTotal: (t) => `共 ${t} 条` }}
          locale={{ emptyText: <Empty description={emptyText} /> }}
        />
      </div>
    </div>
  )
}

// 左栏统一头部(标题 + 右侧动作)。
export function PaneHeader({ title, action }: { title: string; action?: ReactNode }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', padding: '10px 12px', borderBottom: '1px solid #f5f5f5' }}>
      <span style={{ fontWeight: 600 }}>{title}</span>
      <div style={{ flex: 1 }} />
      {action}
    </div>
  )
}

export { Space }
