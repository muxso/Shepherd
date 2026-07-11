// 列表三件套的通用抽象:视图(保存的筛选/列快照,存 /api/api-view,按 config.kind 区分页面)
// + 筛选(声明式字段:枚举/文本/标签/布尔,全客户端过滤)+ 列设置(显隐)。
// 用法:页面声明 fields/columns,useListView 返回 {toolbar, rows, columns},
// 页面只管把 rows/columns 喂给自己的 Table。接口定义页的同类逻辑后续也可迁移到这里。
import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { Button, Checkbox, Dropdown, Input, Popover, Select, Space, Switch, Tag } from 'antd'
import { DeleteOutlined, FilterOutlined, LinkOutlined, SettingOutlined, EyeOutlined } from '@ant-design/icons'
import type { ColumnType } from 'antd/es/table'
import { message } from '../feedback'
import { api, ApiError, type ApiView } from '../api'
import { useI18n } from '../i18n'

/** 筛选字段声明。enum=单选下拉;tags=多选(命中任一);text=包含匹配;bool=开关。 */
export interface FilterField<T> {
  key: string
  label: string
  type: 'enum' | 'tags' | 'text' | 'bool'
  options?: { value: string; label: string }[]
  /** 从行取值:enum/text 返回 string;tags 返回 string[];bool 返回 boolean。 */
  get: (row: T) => string | string[] | boolean | undefined
}

/** 列声明 = antd 列 + 稳定 key + 显示名(列设置面板用)+ 是否默认隐藏。 */
export interface ListColumn<T> extends ColumnType<T> {
  key: string
  label: string
  defaultHidden?: boolean
}

interface ViewConfig {
  kind: string
  search: string
  filters: Record<string, unknown>
  hiddenCols: string[]
}

export function useListView<T>({
  kind,
  projectId,
  searchLabel,
  searchOf,
  fields,
  columns,
  rows,
}: {
  /** 视图归属页面标识(config.kind),如 'requirement' / 'bug'。 */
  kind: string
  projectId: string
  searchLabel?: string
  /** 搜索框匹配的文本(通常是标题/名称)。 */
  searchOf: (row: T) => string
  fields: FilterField<T>[]
  columns: ListColumn<T>[]
  rows: T[]
}): { toolbar: ReactNode; rows: T[]; columns: ColumnType<T>[] } {
  const { t } = useI18n()
  const [search, setSearch] = useState('')
  const [filters, setFilters] = useState<Record<string, unknown>>({})
  const [hiddenCols, setHiddenCols] = useState<string[]>(columns.filter((c) => c.defaultHidden).map((c) => c.key))
  const [views, setViews] = useState<ApiView[]>([])
  const [viewName, setViewName] = useState('')
  const [shared, setShared] = useState(false)

  const loadViews = () =>
    api
      .views(projectId)
      .then((vs) => setViews(vs.filter((v) => (v.config as unknown as ViewConfig)?.kind === kind)))
      .catch(() => setViews([]))
  useEffect(() => {
    loadViews()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId, kind])

  const applyConfig = (c: ViewConfig) => {
    setSearch(c.search || '')
    setFilters(c.filters || {})
    setHiddenCols(Array.isArray(c.hiddenCols) ? c.hiddenCols : [])
  }

  // 深链 ?view=<id>:视图列表就绪后命中即应用,然后清参数避免重复触发。
  useEffect(() => {
    const id = new URLSearchParams(window.location.search).get('view')
    if (!id || views.length === 0) return
    const v = views.find((x) => x.id === id)
    if (v) {
      applyConfig(v.config as unknown as ViewConfig)
      const url = new URL(window.location.href)
      url.searchParams.delete('view')
      window.history.replaceState(null, '', url.toString())
      message.success(`${t('lv.applied', '已应用视图')}「${v.name}」`)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [views])

  const activeFilterCount = Object.values(filters).filter((v) =>
    Array.isArray(v) ? v.length > 0 : v !== undefined && v !== '' && v !== false,
  ).length

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase()
    return rows.filter((r) => {
      if (q && !searchOf(r).toLowerCase().includes(q)) return false
      for (const f of fields) {
        const want = filters[f.key]
        if (want === undefined || want === '' || want === false || (Array.isArray(want) && want.length === 0)) continue
        const got = f.get(r)
        if (f.type === 'enum' && got !== want) return false
        if (f.type === 'text' && !String(got ?? '').toLowerCase().includes(String(want).toLowerCase())) return false
        if (f.type === 'bool' && got !== true) return false
        if (f.type === 'tags') {
          const gs = Array.isArray(got) ? got : []
          if (!(want as string[]).some((w) => gs.includes(w))) return false
        }
      }
      return true
    })
  }, [rows, search, filters, fields, searchOf])

  const saveView = async () => {
    const name = viewName.trim()
    if (!name) return message.warning(t('lv.nameRequired', '请输入视图名称'))
    const config: ViewConfig = { kind, search, filters, hiddenCols }
    try {
      await api.createView({ projectId, name, config, shared })
      setViewName('')
      loadViews()
      message.success(t('lv.saved', '视图已保存'))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('lv.saveFailed', '保存失败'))
    }
  }

  const shareView = (v: ApiView) => {
    const url = `${window.location.origin}${window.location.pathname}?view=${encodeURIComponent(v.id)}`
    navigator.clipboard.writeText(url).then(
      () => message.success(t('lv.linkCopied', '分享链接已复制')),
      () => message.info(url),
    )
  }

  const removeView = async (v: ApiView) => {
    try {
      await api.deleteView(v.id)
      setViews((vs) => vs.filter((x) => x.id !== v.id))
      message.success(t('lv.deleted', '视图已删除'))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('lv.deleteFailed', '删除失败'))
    }
  }

  const filterPanel = (
    <div style={{ width: 260 }}>
      {fields.map((f) => (
        <div key={f.key} style={{ marginBottom: 10 }}>
          <div style={{ fontSize: 12, color: 'var(--text-2)', marginBottom: 4 }}>{f.label}</div>
          {f.type === 'enum' && (
            <Select
              allowClear
              size="small"
              style={{ width: '100%' }}
              value={(filters[f.key] as string) || undefined}
              onChange={(v) => setFilters((s) => ({ ...s, [f.key]: v }))}
              options={f.options}
            />
          )}
          {f.type === 'tags' && (
            <Select
              mode="multiple"
              allowClear
              size="small"
              style={{ width: '100%' }}
              value={(filters[f.key] as string[]) || []}
              onChange={(v) => setFilters((s) => ({ ...s, [f.key]: v }))}
              options={f.options}
            />
          )}
          {f.type === 'text' && (
            <Input
              size="small"
              allowClear
              value={(filters[f.key] as string) || ''}
              onChange={(e) => setFilters((s) => ({ ...s, [f.key]: e.target.value }))}
            />
          )}
          {f.type === 'bool' && (
            <Switch
              size="small"
              checked={filters[f.key] === true}
              onChange={(v) => setFilters((s) => ({ ...s, [f.key]: v }))}
            />
          )}
        </div>
      ))}
      <Button size="small" block onClick={() => setFilters({})}>{t('lv.clearFilters', '清空筛选')}</Button>
    </div>
  )

  const columnPanel = (
    <div style={{ width: 200 }}>
      {columns.map((c) => (
        <div key={c.key} style={{ padding: '3px 0' }}>
          <Checkbox
            checked={!hiddenCols.includes(c.key)}
            onChange={(e) =>
              setHiddenCols((h) => (e.target.checked ? h.filter((k) => k !== c.key) : [...h, c.key]))
            }
          >
            {c.label}
          </Checkbox>
        </div>
      ))}
    </div>
  )

  const viewMenu = {
    items: [
      ...views.map((v) => ({
        key: v.id,
        label: (
          <Space>
            <span onClick={() => applyConfig(v.config as unknown as ViewConfig)}>{v.name}</span>
            {v.shared && <Tag style={{ margin: 0 }}>{t('lv.shared', '共享')}</Tag>}
            <LinkOutlined onClick={(e) => { e.stopPropagation(); shareView(v) }} />
            <DeleteOutlined onClick={(e) => { e.stopPropagation(); removeView(v) }} />
          </Space>
        ),
      })),
      { type: 'divider' as const, key: 'd' },
      {
        key: '__save',
        label: (
          <div onClick={(e) => e.stopPropagation()} style={{ display: 'flex', gap: 6, alignItems: 'center' }}>
            <Input
              size="small"
              style={{ width: 130 }}
              placeholder={t('lv.namePh', '视图名称')}
              value={viewName}
              onChange={(e) => setViewName(e.target.value)}
              onPressEnter={saveView}
            />
            <Checkbox checked={shared} onChange={(e) => setShared(e.target.checked)}>
              {t('lv.shared', '共享')}
            </Checkbox>
            <Button size="small" type="primary" onClick={saveView}>{t('lv.save', '保存')}</Button>
          </div>
        ),
      },
    ],
  }

  const toolbar = (
    <Space size={8} wrap>
      <Input.Search
        allowClear
        size="small"
        style={{ width: 220 }}
        placeholder={searchLabel || t('lv.searchPh', '搜索')}
        value={search}
        onChange={(e) => setSearch(e.target.value)}
      />
      <Popover content={filterPanel} trigger="click" placement="bottomLeft">
        <Button size="small" icon={<FilterOutlined />}>
          {t('lv.filter', '筛选')}{activeFilterCount > 0 ? ` (${activeFilterCount})` : ''}
        </Button>
      </Popover>
      <Popover content={columnPanel} trigger="click" placement="bottomLeft">
        <Button size="small" icon={<SettingOutlined />}>{t('lv.columns', '列')}</Button>
      </Popover>
      <Dropdown menu={viewMenu} trigger={['click']}>
        <Button size="small" icon={<EyeOutlined />}>{t('lv.views', '视图')}{views.length > 0 ? ` (${views.length})` : ''}</Button>
      </Dropdown>
    </Space>
  )

  return { toolbar, rows: filtered, columns: columns.filter((c) => !hiddenCols.includes(c.key)) }
}
