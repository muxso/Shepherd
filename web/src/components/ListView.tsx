// 列表三件套的通用抽象:视图(保存的筛选/列快照,存 /api/api-view,按 config.kind 区分页面)
// + 筛选(声明式字段:枚举/文本/标签/布尔,全客户端过滤;可折叠「高级条件」支持 包含/等于/为空 等操作符组合)
// + 列设置(显隐)。
// 用法:页面声明 fields/columns,useListView 返回 {toolbar, rows, columns},
// 页面只管把 rows/columns 喂给自己的 Table;页面私有状态(模块选中/分页等)可经 extra 挂进视图快照。
import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { Button, Checkbox, Dropdown, Input, Popover, Segmented, Select, Space, Switch, Tag } from 'antd'
import { DeleteOutlined, DownOutlined, FilterOutlined, LinkOutlined, MinusOutlined, PlusOutlined, RightOutlined, SettingOutlined, EyeOutlined } from '@ant-design/icons'
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
  /** 只出现在「高级条件」的字段选择里,不渲染在声明式筛选区(如与搜索框重复的 名称/路径)。 */
  advOnly?: boolean
}

/** 列声明 = antd 列 + 稳定 key + 显示名(列设置面板用)+ 是否默认隐藏。 */
export interface ListColumn<T> extends ColumnType<T> {
  key: string
  label: string
  defaultHidden?: boolean
}

/** 高级条件:字段(text/enum 声明字段的 key)+ 操作符 + 值。 */
export interface AdvCond {
  field: string
  op: 'contains' | 'notContains' | 'equals' | 'notEquals' | 'empty' | 'notEmpty'
  value: string
}

// 操作符选项:存 i18n key + 中文回退,渲染时经 t() 解析(模块级常量拿不到 hook)。
const ADV_OPS: { value: AdvCond['op']; key: string; fallback: string }[] = [
  { value: 'contains', key: 'lv.opContains', fallback: '包含' },
  { value: 'notContains', key: 'lv.opNotContains', fallback: '不包含' },
  { value: 'equals', key: 'lv.opEquals', fallback: '等于' },
  { value: 'notEquals', key: 'lv.opNotEquals', fallback: '不等于' },
  { value: 'empty', key: 'lv.opEmpty', fallback: '为空' },
  { value: 'notEmpty', key: 'lv.opNotEmpty', fallback: '不为空' },
]

interface ViewConfig {
  kind?: string
  search?: string
  filters?: Record<string, unknown>
  adv?: { logic: 'all' | 'any'; conds: AdvCond[] }
  hiddenCols?: string[]
  /** 页面私有快照(经 extra 钩子存取),如 {moduleKey, pageSize}。 */
  extra?: Record<string, unknown>
}

/** 旧接口定义页的视图形状(顶层 advConds/advLogic/moduleKey/pageSize)→ 归一为当前形状,老视图继续可用。 */
export function normalizeViewConfig(raw: unknown): ViewConfig {
  const c = (raw || {}) as ViewConfig & {
    advConds?: AdvCond[]
    advLogic?: 'all' | 'any'
    moduleKey?: string
    pageSize?: number
  }
  const adv =
    c.adv ??
    (Array.isArray(c.advConds) || c.advLogic
      ? { logic: c.advLogic === 'any' ? ('any' as const) : ('all' as const), conds: Array.isArray(c.advConds) ? c.advConds : [] }
      : undefined)
  const extra =
    c.extra ??
    (c.moduleKey !== undefined || c.pageSize !== undefined
      ? {
          ...(c.moduleKey !== undefined ? { moduleKey: c.moduleKey } : {}),
          ...(c.pageSize !== undefined ? { pageSize: c.pageSize } : {}),
        }
      : undefined)
  return { kind: c.kind, search: c.search, filters: c.filters, adv, hiddenCols: c.hiddenCols, extra }
}

/** 需要值的操作符,值为空 → 条件视为未填,忽略。 */
const effectiveConds = (conds: AdvCond[]) => conds.filter((c) => c.op === 'empty' || c.op === 'notEmpty' || c.value.trim())

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
  /** 页面私有状态随视图存/取:保存时 get() 存入 config.extra,应用视图时 apply(extra) 还原。 */
  extra?: { get: () => Record<string, unknown>; apply: (v: Record<string, unknown>) => void }
  /** 视图归属判定(默认严格等于 kind);接口定义页需兼容老视图的 kind 缺省。 */
  matchKind?: (k: string | undefined) => boolean
}): { toolbar: ReactNode; rows: T[]; columns: ColumnType<T>[] } {
  const { t } = useI18n()
  const [search, setSearch] = useState('')
  const [filters, setFilters] = useState<Record<string, unknown>>({})
  const [hiddenCols, setHiddenCols] = useState<string[]>(columns.filter((c) => c.defaultHidden).map((c) => c.key))
  const [advLogic, setAdvLogic] = useState<'all' | 'any'>('all')
  const [advConds, setAdvConds] = useState<AdvCond[]>([])
  const [advOpen, setAdvOpen] = useState(false)
  const [views, setViews] = useState<ApiView[]>([])
  const [viewName, setViewName] = useState('')
  const [shared, setShared] = useState(false)

  // 高级条件可选字段:声明式字段里的 text/enum(复用其 label/get;enum 的值输入用其 options 下拉)。
  const advFields = fields.filter((f) => f.type === 'text' || f.type === 'enum')

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
    if (c.extra && extra) extra.apply(c.extra)
  }

  // 深链 ?view=<id>:视图列表就绪后命中即应用,然后清参数避免重复触发。
  useEffect(() => {
    const id = new URLSearchParams(window.location.search).get('view')
    if (!id || views.length === 0) return
    const v = views.find((x) => x.id === id)
    if (v) {
      applyConfig(v.config)
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
      if (!f) return true // 字段声明已不存在的旧条件:不参与过滤
      const a = String(f.get(r) ?? '').toLowerCase()
      const v = c.value.trim().toLowerCase()
      switch (c.op) {
        case 'contains': return a.includes(v)
        case 'notContains': return !a.includes(v)
        case 'equals': return a === v
        case 'notEquals': return a !== v
        case 'empty': return a === ''
        case 'notEmpty': return a !== ''
      }
    }
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
      // 声明式筛选与高级条件是 AND 关系;高级条件内部按 且(all)/或(any) 组合。
      if (conds.length > 0) {
        const ok = advLogic === 'all' ? conds.every((c) => condMatch(r, c)) : conds.some((c) => condMatch(r, c))
        if (!ok) return false
      }
      return true
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rows, search, filters, fields, searchOf, advConds, advLogic])

  const saveView = async () => {
    const name = viewName.trim()
    if (!name) return message.warning(t('lv.nameRequired', '请输入视图名称'))
    const config: ViewConfig = {
      kind,
      search,
      filters,
      adv: { logic: advLogic, conds: advConds },
      hiddenCols,
      ...(extra ? { extra: extra.get() } : {}),
    }
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

  // 高级条件区(筛选气泡内可折叠):行 = [字段][操作符][值] + 增删,组合方式 且/或。
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
                ) : fieldDef?.type === 'enum' ? (
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
    </div>
  )

  const viewMenu = {
    items: [
      ...views.map((v) => ({
        key: v.id,
        label: (
          <Space>
            <span onClick={() => applyConfig(v.config)}>{v.name}</span>
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
      <Dropdown menu={viewMenu} trigger={['click']}>
        <Button size="small" icon={<EyeOutlined />}>{t('lv.views', '视图')}{views.length > 0 ? ` (${views.length})` : ''}</Button>
      </Dropdown>
      <Popover content={filterPanel} trigger="click" placement="bottomRight">
        <Button size="small" icon={<FilterOutlined />}>
          {t('lv.filter', '筛选')}{activeFilterCount > 0 ? ` (${activeFilterCount})` : ''}
        </Button>
      </Popover>
      <Popover content={columnPanel} trigger="click" placement="bottomRight">
        <Button size="small" icon={<SettingOutlined />} />
      </Popover>
    </Space>
  )

  return { toolbar, rows: filtered, columns: columns.filter((c) => !hiddenCols.includes(c.key)) }
}
