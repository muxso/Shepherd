import { useEffect, useMemo, useState } from 'react'
import { Button, Checkbox, Empty, Input, Modal, Pagination, Popover, Segmented, Select, Table, Tag, Tooltip } from 'antd'
import type { ColumnsType } from 'antd/es/table'
import {
  FilterOutlined,
  FolderOutlined,
  ReloadOutlined,
  SearchOutlined,
  SettingOutlined,
} from '@ant-design/icons'
import { api, type ApiDefinition, type ApiModule } from '../../api'
import { methodColor, priorityColor } from '../tags'
import { useApp } from '../../context'
import { useI18n } from '../../i18n'

/** Which category a mind-map node belongs to (drives the picker tabs). */
export type PlanCatType = 'func' | 'api' | 'scenario'

type TabKey = 'API' | 'CASE' | 'SCENARIO'

/** Unified list row across definitions / cases / scenarios. */
interface Row {
  id: string
  num: string
  name: string
  method?: string
  path?: string
  tags?: string[]
  level?: string
  status?: string
  moduleId?: string | null
}

const PAGE_SIZES = [10, 20, 50, 100]

/**
 * Near-fullscreen link-cases dialog (MeterSphere style): module tree on the left,
 * tabbed list (API definitions / api cases / scenarios) with search + pagination
 * on the right. Selected ids feed the node's caseIds / scenarioIds.
 */
export default function PlanCasePicker({
  open,
  projectId,
  catType,
  caseIds,
  scenarioIds,
  onClose,
  onOk,
}: {
  open: boolean
  projectId: string
  catType: PlanCatType
  caseIds: string[]
  scenarioIds: string[]
  onClose: () => void
  onOk: (caseIds: string[], scenarioIds: string[], names: Record<string, string>) => void
}) {
  const { t } = useI18n()
  const { projects } = useApp()
  const projectName = projects.find((p) => p.id === projectId)?.name || ''

  const [tab, setTab] = useState<TabKey>('API')
  const [modules, setModules] = useState<ApiModule[]>([])
  const [defRows, setDefRows] = useState<Row[]>([])
  const [caseRows, setCaseRows] = useState<Row[]>([])
  const [scenRows, setScenRows] = useState<Row[]>([])
  const [sel, setSel] = useState<Set<string>>(new Set())
  const [selScen, setSelScen] = useState<Set<string>>(new Set())
  const [moduleKey, setModuleKey] = useState('ALL')
  const [modKw, setModKw] = useState('')
  const [kw, setKw] = useState('')
  const [page, setPage] = useState(1)
  const [pageSize, setPageSize] = useState(50)
  const [hiddenCols, setHiddenCols] = useState<Set<string>>(new Set())
  const [loading, setLoading] = useState(false)

  const load = () => {
    if (catType === 'func') return
    setLoading(true)
    const jobs: Promise<unknown>[] = [
      api.modules(projectId).then(setModules).catch(() => setModules([])),
      api
        .definitions(projectId)
        .then((defs: ApiDefinition[]) => {
          setDefRows(
            defs.map((d) => ({
              id: d.id,
              num: d.num != null ? String(d.num) : d.id.slice(0, 8),
              name: d.name,
              method: d.protocol === 'HTTP' || !d.protocol ? d.method : d.protocol,
              path: d.path,
              moduleId: d.moduleId ?? null,
            })),
          )
          return defs
        })
        .then((defs) =>
          api.projectCasesAll(projectId).then((cases) => {
            const modOf = new Map(defs.map((d) => [d.id, d.moduleId ?? null]))
            setCaseRows(
              cases.map((c) => ({
                id: c.id,
                num: c.id.slice(0, 8),
                name: c.name,
                method: c.method,
                path: c.url,
                tags: c.tags,
                level: c.priority,
                moduleId: modOf.get(c.apiDefinitionId) ?? null,
              })),
            )
          }),
        )
        .catch(() => {
          setDefRows([])
          setCaseRows([])
        }),
    ]
    if (catType === 'scenario')
      jobs.push(
        api
          .scenarios(projectId)
          .then((list) =>
            setScenRows(
              list.map((s) => ({
                id: s.id,
                num: s.num != null ? String(s.num) : s.id.slice(0, 8),
                name: s.name,
                level: (s.meta?.level as string) || '',
                status: s.status,
              })),
            ),
          )
          .catch(() => setScenRows([])),
      )
    Promise.allSettled(jobs).then(() => setLoading(false))
  }

  // Reset selection + data whenever the dialog opens for a node.
  useEffect(() => {
    if (!open) return
    setSel(new Set(caseIds))
    setSelScen(new Set(scenarioIds))
    setTab(catType === 'scenario' ? 'SCENARIO' : 'API')
    setModuleKey('ALL')
    setModKw('')
    setKw('')
    setPage(1)
    load()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  const rowsAll = tab === 'API' ? defRows : tab === 'CASE' ? caseRows : scenRows
  const isScenTab = tab === 'SCENARIO'
  const selSet = isScenTab ? selScen : sel
  const setSelSet = isScenTab ? setSelScen : setSel

  // module id -> all subtree ids (select a folder = include its descendants)
  const subtreeOf = useMemo(() => {
    const kids = new Map<string, string[]>()
    modules.forEach((m) => {
      const p = m.parentId || ''
      kids.set(p, [...(kids.get(p) || []), m.id])
    })
    const map = new Map<string, Set<string>>()
    const walk = (id: string): string[] => {
      const list = [id, ...(kids.get(id) || []).flatMap(walk)]
      map.set(id, new Set(list))
      return list
    }
    modules.filter((m) => !m.parentId).forEach((m) => walk(m.id))
    // Orphans (parent deleted) still resolve.
    modules.forEach((m) => {
      if (!map.has(m.id)) walk(m.id)
    })
    return map
  }, [modules])

  const inModule = (r: Row) => {
    if (moduleKey === 'ALL' || isScenTab) return true
    return !!r.moduleId && (subtreeOf.get(moduleKey)?.has(r.moduleId) ?? false)
  }
  const matchKw = (r: Row) => {
    const q = kw.trim().toLowerCase()
    if (!q) return true
    return (
      r.num.toLowerCase().includes(q) ||
      r.name.toLowerCase().includes(q) ||
      (r.path || '').toLowerCase().includes(q) ||
      (r.tags || []).some((x) => x.toLowerCase().includes(q))
    )
  }
  const filtered = rowsAll.filter((r) => inModule(r) && matchKw(r))
  const pageRows = filtered.slice((page - 1) * pageSize, page * pageSize)

  // Per-module "selected/total" counts for the current tab.
  const moduleCount = (id: string) => {
    const set = subtreeOf.get(id)
    const rows = rowsAll.filter((r) => !!r.moduleId && (set?.has(r.moduleId) ?? false))
    const selCnt = rows.filter((r) => selSet.has(r.id)).length
    return `${selCnt}/${rows.length}`
  }

  const toggle = (id: string, checked: boolean) =>
    setSelSet((prev) => {
      const next = new Set(prev)
      if (checked) next.add(id)
      else next.delete(id)
      return next
    })
  const toggleMany = (ids: string[], checked: boolean) =>
    setSelSet((prev) => {
      const next = new Set(prev)
      ids.forEach((id) => (checked ? next.add(id) : next.delete(id)))
      return next
    })

  const selectedTotal = sel.size + selScen.size

  const ok = () => {
    const names: Record<string, string> = {}
    ;[...defRows, ...caseRows].forEach((r) => {
      if (sel.has(r.id)) names[r.id] = r.name
    })
    scenRows.forEach((r) => {
      if (selScen.has(r.id)) names[r.id] = r.name
    })
    onOk([...sel], [...selScen], names)
  }

  // Optional columns toggleable from the header gear.
  const optionalCols: { key: string; label: string }[] = isScenTab
    ? [
        { key: 'level', label: t('plan.mm.colLevel', '等级') },
        { key: 'status', label: t('plan.mm.colStatus', '状态') },
      ]
    : [
        { key: 'path', label: t('plan.mm.colPath', '路径') },
        { key: 'tags', label: t('plan.mm.colTags', '标签') },
      ]
  const colSettings = (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
      {optionalCols.map((c) => (
        <Checkbox
          key={c.key}
          checked={!hiddenCols.has(c.key)}
          onChange={(e) =>
            setHiddenCols((prev) => {
              const next = new Set(prev)
              if (e.target.checked) next.delete(c.key)
              else next.add(c.key)
              return next
            })
          }
        >
          {c.label}
        </Checkbox>
      ))}
    </div>
  )

  const columns: ColumnsType<Row> = [
    {
      title: t('plan.mm.colId', 'ID'),
      dataIndex: 'num',
      width: 110,
      render: (v: string) => <span style={{ color: 'var(--brand)' }}>{v}</span>,
    },
    {
      title: isScenTab || tab === 'CASE' ? t('plan.mm.colName', '名称') : t('plan.mm.colApiName', '接口名称'),
      dataIndex: 'name',
      ellipsis: true,
    },
  ]
  if (!isScenTab)
    columns.push({
      title: t('plan.mm.colMethod', '请求类型'),
      dataIndex: 'method',
      width: 110,
      render: (m: string) => (
        <Tag color={methodColor(m || '')} style={{ margin: 0, fontWeight: 600 }}>
          {m || '—'}
        </Tag>
      ),
    })
  if (!isScenTab && !hiddenCols.has('path'))
    columns.push({ title: t('plan.mm.colPath', '路径'), dataIndex: 'path', ellipsis: true, render: (v: string) => v || '-' })
  if (tab === 'CASE' && !hiddenCols.has('level'))
    columns.push({
      title: t('plan.mm.colLevel', '等级'),
      dataIndex: 'level',
      width: 80,
      render: (v: string) => (v ? <span style={{ color: priorityColor(v) }}>{v}</span> : '-'),
    })
  if (isScenTab && !hiddenCols.has('level'))
    columns.push({
      title: t('plan.mm.colLevel', '等级'),
      dataIndex: 'level',
      width: 90,
      render: (v: string) => (v ? <span style={{ color: priorityColor(v) }}>{v}</span> : '-'),
    })
  if (isScenTab && !hiddenCols.has('status'))
    columns.push({ title: t('plan.mm.colStatus', '状态'), dataIndex: 'status', width: 110, render: (v: string) => v || '-' })
  if (!isScenTab && !hiddenCols.has('tags'))
    columns.push({
      title: t('plan.mm.colTags', '标签'),
      dataIndex: 'tags',
      width: 140,
      render: (tags?: string[]) =>
        tags?.length ? tags.map((x) => <Tag key={x} style={{ marginRight: 4 }}>{x}</Tag>) : '-',
    })
  columns.push({
    title: (
      <Popover content={colSettings} title={t('plan.mm.columnSettings', '列设置')} trigger="click" placement="bottomRight">
        <SettingOutlined style={{ cursor: 'pointer', color: 'var(--text-2)' }} />
      </Popover>
    ),
    key: '__settings',
    width: 44,
    render: () => null,
  })

  // Left module tree rows (flat list with depth indent; API/CASE tabs only).
  const treeRows = useMemo(() => {
    const kids = new Map<string, ApiModule[]>()
    modules.forEach((m) => {
      const p = m.parentId || ''
      kids.set(p, [...(kids.get(p) || []), m])
    })
    const out: { m: ApiModule; depth: number }[] = []
    const walk = (parent: string, depth: number) =>
      (kids.get(parent) || []).forEach((m) => {
        out.push({ m, depth })
        walk(m.id, depth + 1)
      })
    walk('', 0)
    const q = modKw.trim().toLowerCase()
    return q ? out.filter(({ m }) => m.name.toLowerCase().includes(q)) : out
  }, [modules, modKw])

  const rootLabel = isScenTab ? t('plan.mm.allScenarios', '全部场景') : t('plan.mm.allApis', '全部接口')

  const leftPanel = (
    <div
      style={{
        width: 250,
        flexShrink: 0,
        display: 'flex',
        flexDirection: 'column',
        gap: 8,
        paddingRight: 12,
        borderRight: '1px solid var(--border-soft)',
      }}
    >
      <div
        onClick={() => {
          setModuleKey('ALL')
          setPage(1)
        }}
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 6,
          padding: '4px 8px',
          borderRadius: 6,
          cursor: 'pointer',
          background: moduleKey === 'ALL' ? 'var(--brand-soft)' : 'transparent',
          color: moduleKey === 'ALL' ? 'var(--brand)' : 'var(--text)',
          fontWeight: 500,
        }}
      >
        <FolderOutlined style={{ color: 'var(--brand)' }} />
        <span>
          {rootLabel} ({rowsAll.length})
        </span>
      </div>
      <Input
        size="small"
        allowClear
        prefix={<SearchOutlined style={{ color: 'var(--text-3)' }} />}
        placeholder={t('plan.mm.moduleSearch', '请输入模块名称')}
        value={modKw}
        onChange={(e) => setModKw(e.target.value)}
      />
      <div style={{ flex: 1, overflow: 'auto' }}>
        {!isScenTab &&
          treeRows.map(({ m, depth }) => (
            <div
              key={m.id}
              onClick={() => {
                setModuleKey(m.id)
                setPage(1)
              }}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 6,
                padding: '4px 8px',
                paddingLeft: 8 + depth * 14,
                borderRadius: 6,
                cursor: 'pointer',
                background: moduleKey === m.id ? 'var(--brand-soft)' : 'transparent',
                color: moduleKey === m.id ? 'var(--brand)' : 'var(--text)',
              }}
            >
              <FolderOutlined style={{ color: 'var(--text-3)', flexShrink: 0 }} />
              <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', fontSize: 13 }}>
                {m.name}
              </span>
              <span style={{ fontSize: 12, color: 'var(--text-3)' }}>{moduleCount(m.id)}</span>
            </div>
          ))}
      </div>
    </div>
  )

  const tabOptions =
    catType === 'scenario'
      ? [
          { value: 'SCENARIO', label: 'SCENARIO' },
          { value: 'CASE', label: 'CASE' },
        ]
      : [
          { value: 'API', label: 'API' },
          { value: 'CASE', label: 'CASE' },
        ]

  const body =
    catType === 'func' ? (
      <div style={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
        <Empty description={t('plan.mm.noFuncCases', '暂无功能用例数据')} />
      </div>
    ) : (
      <>
        {leftPanel}
        <div style={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', gap: 8, paddingLeft: 12 }}>
          {/* Toolbar: tab toggle + search / view / filter / refresh */}
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <Segmented
              size="small"
              value={tab}
              onChange={(v) => {
                setTab(v as TabKey)
                setPage(1)
              }}
              options={tabOptions}
            />
            <div style={{ flex: 1 }} />
            <Input
              size="small"
              allowClear
              style={{ width: 240 }}
              prefix={<SearchOutlined style={{ color: 'var(--text-3)' }} />}
              placeholder={t('plan.mm.pickSearch', '通过 ID/名称/标签/路径搜索')}
              value={kw}
              onChange={(e) => {
                setKw(e.target.value)
                setPage(1)
              }}
            />
            <span style={{ fontSize: 12, color: 'var(--text-2)', display: 'flex', alignItems: 'center', gap: 4 }}>
              {t('plan.mm.view', '视图')}
              <Select
                size="small"
                style={{ width: 100 }}
                value="all"
                options={[{ value: 'all', label: t('plan.mm.allData', '全部数据') }]}
              />
            </span>
            <Button size="small" icon={<FilterOutlined />}>
              {t('plan.mm.filter', '筛选')}
            </Button>
            <Tooltip title={t('plan.mm.refresh', '刷新')}>
              <Button size="small" icon={<ReloadOutlined />} onClick={load} />
            </Tooltip>
          </div>
          <div style={{ flex: 1, minHeight: 0, overflow: 'auto' }}>
            <Table<Row>
              rowKey="id"
              size="small"
              loading={loading}
              dataSource={pageRows}
              columns={columns}
              pagination={false}
              rowSelection={{
                selectedRowKeys: [...selSet],
                onSelect: (r, checked) => toggle(r.id, checked),
                onSelectAll: (checked, _rows, changed) => toggleMany(changed.map((r) => r.id), checked),
              }}
              onRow={(r) => ({
                onClick: () => toggle(r.id, !selSet.has(r.id)),
                style: { cursor: 'pointer' },
              })}
            />
          </div>
          {/* Bottom bar: selection summary + pagination */}
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              paddingTop: 8,
              borderTop: '1px solid var(--border-soft)',
            }}
          >
            <span style={{ fontSize: 13, color: 'var(--text-2)', display: 'flex', alignItems: 'center', gap: 12 }}>
              <span>
                {t('plan.mm.selectedNData', '已选 {n} 项数据').split('{n}')[0]}
                <span style={{ color: 'var(--brand)', margin: '0 2px' }}>{selectedTotal}</span>
                {t('plan.mm.selectedNData', '已选 {n} 项数据').split('{n}')[1] || ''}
              </span>
              <a
                onClick={() => {
                  setSel(new Set())
                  setSelScen(new Set())
                }}
                style={{ color: 'var(--brand)' }}
              >
                {t('plan.mm.clear', '清空')}
              </a>
            </span>
            <span style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 13, color: 'var(--text-2)' }}>
              <span>{t('plan.mm.totalN', '共 {n} 条').replace('{n}', String(filtered.length))}</span>
              <Select
                size="small"
                style={{ width: 100 }}
                value={pageSize}
                onChange={(v) => {
                  setPageSize(v)
                  setPage(1)
                }}
                options={PAGE_SIZES.map((n) => ({ value: n, label: t('plan.mm.perPage', '{n} 条/页').replace('{n}', String(n)) }))}
              />
              <Pagination
                size="small"
                current={page}
                pageSize={pageSize}
                total={filtered.length}
                showSizeChanger={false}
                onChange={setPage}
              />
            </span>
          </div>
        </div>
      </>
    )

  return (
    <Modal
      open={open}
      onCancel={onClose}
      width="min(1280px, 94vw)"
      style={{ top: 24 }}
      destroyOnHidden
      title={
        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
          <span>{t('plan.mm.pickTitle', '关联用例')}</span>
          {projectName && (
            <span
              style={{
                fontSize: 12,
                fontWeight: 400,
                padding: '2px 12px',
                border: '1px solid var(--border)',
                borderRadius: 6,
                color: 'var(--text-2)',
              }}
            >
              {projectName}
            </span>
          )}
        </div>
      }
      footer={
        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
          <Button onClick={onClose}>{t('a.cancel', '取消')}</Button>
          <Button type="primary" onClick={ok}>
            {t('plan.mm.link', '关联')}
          </Button>
        </div>
      }
    >
      <div style={{ display: 'flex', height: 'calc(100vh - 240px)', minHeight: 400 }}>{body}</div>
    </Modal>
  )
}
