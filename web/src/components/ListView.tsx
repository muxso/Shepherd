// Shared list toolkit: views (saved filter/column snapshots in /api/api-view, keyed per page
// by config.kind) + filters (declarative fields: enum/text/tags/bool, all client-side;
// collapsible "advanced conditions" with contains/equals/empty operators) + column visibility
// + page size (set in the column-settings gear, saved with views).
// Usage: page declares fields/columns; useListView returns {toolbar, rows, columns, pagination}
// and the page feeds rows/columns/pagination to its own Table. Page-private state (module
// selection) can ride along in the view snapshot via `extra`.
import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { Alert, Button, Checkbox, Dropdown, Input, Popover, Segmented, Select, Space, Switch, Tag } from 'antd'
import EditDrawer from './EditDrawer'
import { DeleteOutlined, DownOutlined, EditOutlined, FilterOutlined, LinkOutlined, MinusCircleOutlined, MinusOutlined, PlusOutlined, RightOutlined, SettingOutlined, SearchOutlined } from '@ant-design/icons'
import type { ColumnType, TablePaginationConfig } from 'antd/es/table'
import { message } from '../feedback'
import { api, ApiError, type ApiView } from '../api'
import { useI18n } from '../i18n'

/** Filter field declaration. enum = single select; tags = multi select (any match); text = contains; bool = switch. */
export interface FilterField<T> {
  key: string
  label: string
  type: 'enum' | 'tags' | 'text' | 'bool'
  options?: { value: string; label: string }[]
  /** Value extractor: enum/text return string; tags return string[]; bool returns boolean. */
  get: (row: T) => string | string[] | boolean | undefined
  /** Only offered in "advanced conditions", not rendered in the declarative filter area (e.g. name/path fields that duplicate the search box). */
  advOnly?: boolean
}

/** Column declaration = antd column + stable key + display label (for the column panel) + default-hidden flag. */
export interface ListColumn<T> extends ColumnType<T> {
  key: string
  label: string
  defaultHidden?: boolean
}

/** Advanced condition: field (key of a declared text/enum field) + operator + value. */
export interface AdvCond {
  field: string
  op: 'contains' | 'notContains' | 'equals' | 'notEquals' | 'empty' | 'notEmpty'
  value: string
}

/** System view ("system views" group in the view dropdown), declared per page, e.g. "created by me" = createdBy equals current user. */
export interface SystemView<T> {
  key: string
  label: string
  pred: (row: T) => boolean
}

// Operator options store i18n key + fallback, resolved via t() at render time (module-level constant has no hook access).
const ADV_OPS: { value: AdvCond['op']; key: string; fallback: string }[] = [
  { value: 'contains', key: 'lv.opContains', fallback: '包含' },
  { value: 'notContains', key: 'lv.opNotContains', fallback: '不包含' },
  { value: 'equals', key: 'lv.opEquals', fallback: '等于' },
  { value: 'notEquals', key: 'lv.opNotEquals', fallback: '不等于' },
  { value: 'empty', key: 'lv.opEmpty', fallback: '为空' },
  { value: 'notEmpty', key: 'lv.opNotEmpty', fallback: '不为空' },
]

/** Column-header text search (antd filterDropdown); stacks with toolbar filters.
    Usage: { ...columnSearch((r) => r.name, t), ...other column props } */
export function columnSearch<T>(
  get: (row: T) => string,
  t: (k: string, d?: string) => string,
): Pick<ColumnType<T>, 'filterDropdown' | 'filterIcon' | 'onFilter'> {
  return {
    filterDropdown: ({ selectedKeys, setSelectedKeys, confirm, clearFilters }) => (
      <div style={{ padding: 8 }} onKeyDown={(e) => e.stopPropagation()}>
        <Input
          size="small"
          autoFocus
          placeholder={t('lv.colSearchPh', '输入关键字')}
          value={(selectedKeys[0] as string) || ''}
          onChange={(e) => setSelectedKeys(e.target.value ? [e.target.value] : [])}
          onPressEnter={() => confirm()}
          style={{ width: 180, marginBottom: 8, display: 'block' }}
        />
        <Space>
          <Button size="small" type="primary" onClick={() => confirm()}>{t('lv.filter', '筛选')}</Button>
          <Button size="small" onClick={() => { clearFilters?.(); confirm() }}>{t('lv.reset', '重置')}</Button>
        </Space>
      </div>
    ),
    filterIcon: (filtered) => <SearchOutlined style={{ color: filtered ? 'var(--brand)' : undefined }} />,
    onFilter: (value, record) => get(record).toLowerCase().includes(String(value).toLowerCase()),
  }
}

interface ViewConfig {
  kind?: string
  search?: string
  filters?: Record<string, unknown>
  adv?: { logic: 'all' | 'any'; conds: AdvCond[] }
  hiddenCols?: string[]
  /** Rows per page; older views may carry it in extra instead (normalized below). */
  pageSize?: number
  /** Page-private snapshot (read/written via the extra hook), e.g. {moduleKey}. */
  extra?: Record<string, unknown>
}

/** Normalize the legacy API-definition view shape (top-level advConds/advLogic/moduleKey and extra.pageSize) so old saved views keep working. */
export function normalizeViewConfig(raw: unknown): ViewConfig {
  const c = (raw || {}) as ViewConfig & {
    advConds?: AdvCond[]
    advLogic?: 'all' | 'any'
    moduleKey?: string
  }
  const adv =
    c.adv ??
    (Array.isArray(c.advConds) || c.advLogic
      ? { logic: c.advLogic === 'any' ? ('any' as const) : ('all' as const), conds: Array.isArray(c.advConds) ? c.advConds : [] }
      : undefined)
  const extra = c.extra ?? (c.moduleKey !== undefined ? { moduleKey: c.moduleKey } : undefined)
  // pageSize lives top-level now; fall back to extra.pageSize written by the old API-definition page.
  const pageSize =
    typeof c.pageSize === 'number' ? c.pageSize : typeof extra?.pageSize === 'number' ? extra.pageSize : undefined
  return { kind: c.kind, search: c.search, filters: c.filters, adv, hiddenCols: c.hiddenCols, pageSize, extra }
}

/** Value-requiring operators with an empty value count as unfilled and are ignored. */
const effectiveConds = (conds: AdvCond[]) => conds.filter((c) => c.op === 'empty' || c.op === 'notEmpty' || c.value.trim())

/** Rows-per-page choices offered in the column-settings gear. */
const PAGE_SIZES = [10, 15, 20, 50, 100]
const DEFAULT_PAGE_SIZE = 15

export function useListView<T>({
  kind,
  projectId,
  searchLabel,
  searchOf,
  fields,
  columns,
  rows,
  extra,
  matchKind,
  systemViews = [],
}: {
  /** Page identifier for view ownership (config.kind), e.g. 'requirement' / 'bug'. */
  kind: string
  projectId: string
  searchLabel?: string
  /** Text the search box matches against (usually title/name). */
  searchOf: (row: T) => string
  fields: FilterField<T>[]
  columns: ListColumn<T>[]
  rows: T[]
  /** Page-private state saved with views (e.g. moduleKey): get() is stored into config.extra on save; apply(extra) restores it when a view is applied. */
  extra?: { get: () => Record<string, unknown>; apply: (v: Record<string, unknown>) => void }
  /** View ownership test (defaults to strict equality with kind); the API-definition page must accept legacy views with kind unset. */
  matchKind?: (k: string | undefined) => boolean
  /** System views (built-in data sets beyond "all data", e.g. "created by me"); omit for "all data" only. */
  systemViews?: SystemView<T>[]
}): { toolbar: ReactNode; rows: T[]; columns: ColumnType<T>[]; pagination: TablePaginationConfig } {
  const { t } = useI18n()
  const [search, setSearch] = useState('')
  const [filters, setFilters] = useState<Record<string, unknown>>({})
  const [hiddenCols, setHiddenCols] = useState<string[]>(columns.filter((c) => c.defaultHidden).map((c) => c.key))
  const [pageSize, setPageSize] = useState(DEFAULT_PAGE_SIZE)
  const [advLogic, setAdvLogic] = useState<'all' | 'any'>('all')
  const [advConds, setAdvConds] = useState<AdvCond[]>([])
  const [advOpen, setAdvOpen] = useState(false)
  const [views, setViews] = useState<ApiView[]>([])
  // Active view: system view ('sys:<key>', all data = 'sys:all') or saved view ('view:<id>').
  const [activeKey, setActiveKey] = useState('sys:all')
  // View editor modal: create (id empty) or edit; conds is a draft, written to advConds only on save-and-apply.
  const [editor, setEditor] = useState<{
    id?: string
    name: string
    nameEditing: boolean
    logic: 'all' | 'any'
    conds: AdvCond[]
    shared: boolean
  } | null>(null)

  // Fields available to advanced conditions: text/enum/tags (reusing their label/get; enum/tags
  // value input uses their options dropdown; tags values are arrays, joined with spaces into
  // text for contains/empty checks).
  const advFields = fields.filter((f) => f.type === 'text' || f.type === 'enum' || f.type === 'tags')

  const isKindMatch = (k: string | undefined) => (matchKind ? matchKind(k) : k === kind)
  const loadViews = () =>
    api
      .views(projectId)
      .then((vs) => setViews(vs.filter((v) => isKindMatch((v.config as unknown as ViewConfig)?.kind))))
      .catch(() => setViews([]))
  useEffect(() => {
    loadViews()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId, kind])

  const applyConfig = (raw: unknown) => {
    const c = normalizeViewConfig(raw)
    setSearch(c.search || '')
    setFilters(c.filters || {})
    setHiddenCols(Array.isArray(c.hiddenCols) ? c.hiddenCols : [])
    const conds = Array.isArray(c.adv?.conds) ? c.adv.conds : []
    setAdvLogic(c.adv?.logic === 'any' ? 'any' : 'all')
    setAdvConds(conds)
    setAdvOpen(conds.length > 0)
    setPageSize(typeof c.pageSize === 'number' && c.pageSize > 0 ? c.pageSize : DEFAULT_PAGE_SIZE)
    if (c.extra && extra) extra.apply(c.extra)
  }

  // Switching to a system view resets to a clean data set: clear search/filters/conditions, restore default column visibility.
  const resetConfig = () => {
    setSearch('')
    setFilters({})
    setAdvConds([])
    setAdvLogic('all')
    setAdvOpen(false)
    setHiddenCols(columns.filter((c) => c.defaultHidden).map((c) => c.key))
    setPageSize(DEFAULT_PAGE_SIZE)
  }

  const selectSystem = (key: string) => {
    setActiveKey(`sys:${key}`)
    resetConfig()
  }
  const selectView = (v: ApiView) => {
    setActiveKey(`view:${v.id}`)
    applyConfig(v.config)
  }

  // Deep link ?view=<id>: apply once the view list is ready, then strip the param to avoid re-triggering.
  useEffect(() => {
    const id = new URLSearchParams(window.location.search).get('view')
    if (!id || views.length === 0) return
    const v = views.find((x) => x.id === id)
    if (v) {
      selectView(v)
      const url = new URL(window.location.href)
      url.searchParams.delete('view')
      window.history.replaceState(null, '', url.toString())
      message.success(`${t('lv.applied', '已应用视图')}「${v.name}」`)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [views])

  const effConds = effectiveConds(advConds)
  const activeFilterCount =
    Object.values(filters).filter((v) => (Array.isArray(v) ? v.length > 0 : v !== undefined && v !== '' && v !== false)).length +
    effConds.length

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase()
    const conds = effectiveConds(advConds)
    const condMatch = (r: T, c: AdvCond): boolean => {
      const f = advFields.find((x) => x.key === c.field)
      if (!f) return true // stale condition whose field is no longer declared: skip it
      const raw = f.get(r)
      const a = (Array.isArray(raw) ? raw.join(' ') : String(raw ?? '')).toLowerCase()
      const v = c.value.trim().toLowerCase()
      // contains/notContains: value splits on spaces into keywords; any keyword hit counts as contains.
      const kws = v.split(/\s+/).filter(Boolean)
      switch (c.op) {
        case 'contains': return kws.length === 0 || kws.some((k) => a.includes(k))
        case 'notContains': return !kws.some((k) => a.includes(k))
        case 'equals': return a === v
        case 'notEquals': return a !== v
        case 'empty': return a === ''
        case 'notEmpty': return a !== ''
      }
    }
    const sysPred = activeKey.startsWith('sys:') && activeKey !== 'sys:all'
      ? systemViews.find((sv) => `sys:${sv.key}` === activeKey)?.pred
      : undefined
    return rows.filter((r) => {
      if (sysPred && !sysPred(r)) return false
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
      // Declarative filters AND advanced conditions; within advanced conditions, all/any per advLogic.
      if (conds.length > 0) {
        const ok = advLogic === 'all' ? conds.every((c) => condMatch(r, c)) : conds.some((c) => condMatch(r, c))
        if (!ok) return false
      }
      return true
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rows, search, filters, fields, searchOf, advConds, advLogic, activeKey, systemViews])

  // New view: default name = localized "unnamed view" + 001/002…, skipping names already taken.
  const openNewEditor = () => {
    const base = t('lv.unnamedView', '未命名视图')
    const names = new Set(views.map((v) => v.name))
    let n = 1
    while (names.has(`${base}${String(n).padStart(3, '0')}`)) n++
    setEditor({
      name: `${base}${String(n).padStart(3, '0')}`,
      nameEditing: false,
      logic: 'all',
      conds: advFields.length > 0 ? [{ field: advFields[0].key, op: 'contains', value: '' }] : [],
      shared: false,
    })
  }

  const openEditEditor = (v: ApiView) => {
    const c = normalizeViewConfig(v.config)
    setEditor({
      id: v.id,
      name: v.name,
      nameEditing: false,
      logic: c.adv?.logic === 'any' ? 'any' : 'all',
      conds: c.adv?.conds?.length ? c.adv.conds : advFields.length > 0 ? [{ field: advFields[0].key, op: 'contains', value: '' }] : [],
      shared: v.shared,
    })
  }

  // Save and apply: create stores conditions + current page size; edit keeps the original snapshot's other keys (search/filters/columns/extra) and swaps conditions/page size.
  const saveEditor = async () => {
    if (!editor) return
    const name = editor.name.trim()
    if (!name) return message.warning(t('lv.nameRequired', '请输入视图名称'))
    const orig = editor.id ? views.find((v) => v.id === editor.id) : undefined
    const config: ViewConfig = {
      ...(orig ? normalizeViewConfig(orig.config) : {}),
      kind,
      adv: { logic: editor.logic, conds: editor.conds },
      pageSize,
    }
    try {
      const saved = editor.id
        ? await api.updateView(editor.id, { name, config, shared: editor.shared })
        : await api.createView({ projectId, name, config, shared: editor.shared })
      setEditor(null)
      await loadViews()
      setActiveKey(`view:${saved.id}`)
      applyConfig(config)
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
      if (activeKey === `view:${v.id}`) selectSystem('all')
      message.success(t('lv.deleted', '视图已删除'))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('lv.deleteFailed', '删除失败'))
    }
  }

  // Advanced-conditions area (collapsible inside the filter popover): rows of [field][op][value] with add/remove, combined via all/any.
  const advPanel = advFields.length > 0 && (
    <div style={{ borderTop: '1px solid var(--border-soft)', marginTop: 10, paddingTop: 8 }}>
      <div
        onClick={() => setAdvOpen((v) => !v)}
        style={{ cursor: 'pointer', fontSize: 12, color: 'var(--text-2)', marginBottom: advOpen ? 8 : 0, userSelect: 'none' }}
      >
        {advOpen ? <DownOutlined style={{ fontSize: 10 }} /> : <RightOutlined style={{ fontSize: 10 }} />}{' '}
        {t('lv.advCond', '高级条件')}
        {effConds.length > 0 ? ` (${effConds.length})` : ''}
      </div>
      {advOpen && (
        <>
          <Segmented
            size="small"
            value={advLogic}
            onChange={(v) => setAdvLogic(v as 'all' | 'any')}
            options={[
              { value: 'all', label: t('lv.logicAll', '且') },
              { value: 'any', label: t('lv.logicAny', '或') },
            ]}
            style={{ marginBottom: 8 }}
          />
          {advConds.map((c, i) => {
            const set = (p: Partial<AdvCond>) => setAdvConds((cs) => cs.map((x, idx) => (idx === i ? { ...x, ...p } : x)))
            const fieldDef = advFields.find((f) => f.key === c.field)
            const noValue = c.op === 'empty' || c.op === 'notEmpty'
            return (
              <Space.Compact key={i} style={{ width: '100%', marginBottom: 6 }}>
                <Select
                  size="small"
                  style={{ width: 100 }}
                  value={c.field}
                  onChange={(v) => set({ field: v, value: '' })}
                  options={advFields.map((f) => ({ value: f.key, label: f.label }))}
                />
                <Select
                  size="small"
                  style={{ width: 88 }}
                  value={c.op}
                  onChange={(v) => set({ op: v })}
                  options={ADV_OPS.map((o) => ({ value: o.value, label: t(o.key, o.fallback) }))}
                />
                {noValue ? (
                  <Input size="small" disabled value="" placeholder="—" style={{ flex: 1, minWidth: 0 }} />
                ) : fieldDef?.type === 'enum' || fieldDef?.type === 'tags' ? (
                  <Select
                    size="small"
                    allowClear
                    style={{ flex: 1, minWidth: 0 }}
                    value={c.value || undefined}
                    onChange={(v) => set({ value: v ?? '' })}
                    options={fieldDef.options}
                    placeholder={t('lv.condValuePh', '值')}
                  />
                ) : (
                  <Input
                    size="small"
                    value={c.value}
                    onChange={(e) => set({ value: e.target.value })}
                    placeholder={t('lv.condValuePh', '值')}
                    style={{ flex: 1, minWidth: 0 }}
                  />
                )}
                <Button size="small" icon={<MinusOutlined />} onClick={() => setAdvConds((cs) => cs.filter((_, idx) => idx !== i))} />
              </Space.Compact>
            )
          })}
          <Button
            size="small"
            type="link"
            icon={<PlusOutlined />}
            style={{ paddingLeft: 0 }}
            onClick={() => setAdvConds((cs) => [...cs, { field: advFields[0].key, op: 'contains', value: '' }])}
          >
            {t('lv.addCond', '添加条件')}
          </Button>
        </>
      )}
    </div>
  )

  const filterPanel = (
    <div style={{ width: 320 }}>
      {fields.filter((f) => !f.advOnly).map((f) => (
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
      {advPanel}
      <Button size="small" block style={{ marginTop: 8 }} onClick={() => { setFilters({}); setAdvConds([]) }}>
        {t('lv.clearFilters', '清空筛选')}
      </Button>
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
      {/* Page size shares the gear panel so sizing has a single entry point (table pager hides its size changer). */}
      <div style={{ borderTop: '1px solid var(--border-soft)', marginTop: 8, paddingTop: 8, display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <span style={{ fontSize: 12, color: 'var(--text-2)' }}>{t('lv.pageSize', '每页条数')}</span>
        <Select
          size="small"
          style={{ width: 80 }}
          value={pageSize}
          onChange={setPageSize}
          options={PAGE_SIZES.map((n) => ({ value: n, label: String(n) }))}
        />
      </div>
    </div>
  )

  // View dropdown: system views (all data + page-declared data sets) / my views (inline edit/share/delete) / new view at the bottom.
  const viewMenu = {
    selectable: true,
    selectedKeys: [activeKey],
    items: [
      {
        type: 'group' as const,
        key: 'g-sys',
        label: t('lv.systemViews', '系统视图'),
        children: [
          { key: 'sys:all', label: t('lv.allData', '全部数据') },
          ...systemViews.map((sv) => ({ key: `sys:${sv.key}`, label: sv.label })),
        ],
      },
      ...(views.length > 0
        ? [
            {
              type: 'group' as const,
              key: 'g-mine',
              label: t('lv.myViews', '我的视图'),
              children: views.map((v) => ({
                key: `view:${v.id}`,
                label: (
                  <div style={{ display: 'flex', alignItems: 'center', gap: 6, minWidth: 180 }}>
                    <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{v.name}</span>
                    {v.shared && <Tag style={{ margin: 0 }}>{t('lv.shared', '共享')}</Tag>}
                    <EditOutlined onClick={(e) => { e.stopPropagation(); openEditEditor(v) }} />
                    <LinkOutlined onClick={(e) => { e.stopPropagation(); shareView(v) }} />
                    <DeleteOutlined onClick={(e) => { e.stopPropagation(); removeView(v) }} />
                  </div>
                ),
              })),
            },
          ]
        : []),
      { type: 'divider' as const, key: 'd' },
      {
        key: '__new',
        label: (
          <Space>
            <PlusOutlined />
            {t('lv.newView', '新建视图')}
          </Space>
        ),
      },
    ],
    onClick: ({ key }: { key: string }) => {
      if (key === '__new') return openNewEditor()
      if (key === 'sys:all') return selectSystem('all')
      if (key.startsWith('sys:')) return selectSystem(key.slice(4))
      if (key.startsWith('view:')) {
        const v = views.find((x) => x.id === key.slice(5))
        if (v) selectView(v)
      }
    },
  }

  const activeName = activeKey === 'sys:all'
    ? t('lv.allData', '全部数据')
    : activeKey.startsWith('sys:')
      ? systemViews.find((sv) => `sys:${sv.key}` === activeKey)?.label || t('lv.allData', '全部数据')
      : views.find((v) => `view:${v.id}` === activeKey)?.name || t('lv.allData', '全部数据')

  // View editor modal: renamable title; condition rows = searchable field + operator + value; footer has share toggle + cancel/save.
  const editorModal = editor && (
    <EditDrawer
      open
      width={640}
      onCancel={() => setEditor(null)}
      title={
        editor.nameEditing ? (
          <Input
            size="small"
            autoFocus
            defaultValue={editor.name}
            style={{ width: 240 }}
            onBlur={(e) => setEditor((s) => (s ? { ...s, name: e.target.value, nameEditing: false } : s))}
            onPressEnter={(e) => {
              const v = (e.target as HTMLInputElement).value
              setEditor((s) => (s ? { ...s, name: v, nameEditing: false } : s))
            }}
          />
        ) : (
          <Space>
            {editor.name}
            <EditOutlined
              style={{ color: 'var(--text-3)', fontSize: 13, cursor: 'pointer' }}
              onClick={() => setEditor((s) => (s ? { ...s, nameEditing: true } : s))}
            />
          </Space>
        )
      }
      footer={
        <div style={{ display: 'flex', alignItems: 'center' }}>
          <Checkbox
            checked={editor.shared}
            onChange={(e) => setEditor((s) => (s ? { ...s, shared: e.target.checked } : s))}
          >
            {t('lv.shared', '共享')}
          </Checkbox>
          <div style={{ flex: 1 }} />
          <Space>
            <Button onClick={() => setEditor(null)}>{t('a.cancel', '取消')}</Button>
            <Button type="primary" onClick={saveEditor}>{t('lv.save', '保存')}</Button>
          </Space>
        </div>
      }
    >
      <Alert
        type="info"
        showIcon
        closable
        message={t('lv.viewHint', '视图保存下方条件组合;应用视图即切换到该数据集')}
        style={{ marginBottom: 12 }}
      />
      <Space style={{ marginBottom: 10 }}>
        <span style={{ color: 'var(--text-2)' }}>{t('lv.matchCond', '符合以下条件')}</span>
        <Select
          size="small"
          style={{ width: 72 }}
          value={editor.logic}
          onChange={(v) => setEditor((s) => (s ? { ...s, logic: v } : s))}
          options={[
            { value: 'all', label: t('lv.matchAll', '所有') },
            { value: 'any', label: t('lv.matchAny', '任一') },
          ]}
        />
      </Space>
      {editor.conds.map((c, i) => {
        const set = (p: Partial<AdvCond>) =>
          setEditor((s) => (s ? { ...s, conds: s.conds.map((x, idx) => (idx === i ? { ...x, ...p } : x)) } : s))
        const fieldDef = advFields.find((f) => f.key === c.field)
        const noValue = c.op === 'empty' || c.op === 'notEmpty'
        return (
          <div key={i} style={{ display: 'flex', gap: 8, marginBottom: 8 }}>
            <Select
              showSearch
              optionFilterProp="label"
              style={{ width: 150 }}
              value={c.field || undefined}
              placeholder={t('lv.fieldPh', '请选择')}
              onChange={(v) => set({ field: v, value: '' })}
              options={advFields.map((f) => ({ value: f.key, label: f.label }))}
            />
            <Select
              style={{ width: 96 }}
              value={c.op}
              onChange={(v) => set({ op: v })}
              options={ADV_OPS.map((o) => ({ value: o.value, label: t(o.key, o.fallback) }))}
            />
            {noValue ? (
              <Input disabled value="" placeholder="—" style={{ flex: 1, minWidth: 0 }} />
            ) : fieldDef?.type === 'enum' || fieldDef?.type === 'tags' ? (
              <Select
                allowClear
                style={{ flex: 1, minWidth: 0 }}
                value={c.value || undefined}
                onChange={(v) => set({ value: v ?? '' })}
                options={fieldDef.options}
                placeholder={t('lv.condValuePh', '值')}
              />
            ) : (
              <Input
                value={c.value}
                onChange={(e) => set({ value: e.target.value })}
                placeholder={t('lv.kwPh', '关键字之间以空格进行分隔')}
                style={{ flex: 1, minWidth: 0 }}
              />
            )}
            <Button
              type="text"
              icon={<MinusCircleOutlined />}
              disabled={editor.conds.length <= 1}
              onClick={() => setEditor((s) => (s ? { ...s, conds: s.conds.filter((_, idx) => idx !== i) } : s))}
            />
          </div>
        )
      })}
      <Button
        type="link"
        icon={<PlusOutlined />}
        style={{ paddingLeft: 0 }}
        disabled={advFields.length === 0}
        onClick={() =>
          setEditor((s) => (s ? { ...s, conds: [...s.conds, { field: advFields[0].key, op: 'contains', value: '' }] } : s))
        }
      >
        {t('lv.addCond', '添加条件')}
      </Button>
    </EditDrawer>
  )

  // Toolbar controls use the default size (32px) to match page primary buttons.
  const toolbar = (
    <Space size={8} wrap>
      <Input.Search
        allowClear
        style={{ width: 220 }}
        placeholder={searchLabel || t('lv.searchPh', '搜索')}
        value={search}
        onChange={(e) => setSearch(e.target.value)}
      />
      <Dropdown menu={viewMenu} trigger={['click']}>
        <Button>
          <span style={{ color: 'var(--text-3)' }}>{t('lv.views', '视图')}</span>
          {activeName}
          <DownOutlined style={{ fontSize: 10 }} />
        </Button>
      </Dropdown>
      {editorModal}
      <Popover content={filterPanel} trigger="click" placement="bottomRight">
        <Button icon={<FilterOutlined />}>
          {t('lv.filter', '筛选')}{activeFilterCount > 0 ? ` (${activeFilterCount})` : ''}
        </Button>
      </Popover>
      <Popover content={columnPanel} trigger="click" placement="bottomRight">
        <Button icon={<SettingOutlined />} />
      </Popover>
    </Space>
  )

  // Header search/filter: auto-attached when a column key matches a filter field key
  // (text → search box; enum/tags → multi-select funnel), stacking with toolbar filters.
  // Columns that bring their own filterDropdown/filters are left untouched.
  const withHeaderFilter = (c: ListColumn<T>): ColumnType<T> => {
    if (c.filterDropdown || c.filters || c.onFilter) return c
    const f = fields.find((x) => x.key === c.key)
    if (!f) return c
    if (f.type === 'text') return { ...c, ...columnSearch((r) => String(f.get(r) ?? ''), t) }
    if ((f.type === 'enum' || f.type === 'tags') && f.options?.length)
      return {
        ...c,
        filters: f.options.map((o) => ({ text: o.label, value: o.value })),
        filterSearch: f.options.length > 8,
        onFilter: (v, r) => {
          const got = f.get(r)
          return Array.isArray(got) ? got.includes(String(v)) : got === v
        },
      }
    return c
  }

  // Ready to spread onto antd Table `pagination`; pageSize is controlled here so onChange must sync it back.
  const pagination: TablePaginationConfig = {
    pageSize,
    size: 'small',
    showSizeChanger: false,
    onChange: (_page, size) => { if (size && size !== pageSize) setPageSize(size) },
  }

  return { toolbar, rows: filtered, columns: columns.filter((c) => !hiddenCols.includes(c.key)).map(withHeaderFilter), pagination }
}
