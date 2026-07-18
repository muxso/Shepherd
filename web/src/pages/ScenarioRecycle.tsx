import { useEffect, useMemo, useState } from 'react'
import { Link } from 'react-router-dom'
import { Breadcrumb, Button, Empty, Input, Popconfirm, Space, Table, Tag } from 'antd'
import { ReloadOutlined, SearchOutlined } from '@ant-design/icons'
import type { ColumnsType } from 'antd/es/table'
import { message, modal } from '../feedback'
import { api, ApiError, type ApiModule, type Scenario } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'
import { priorityColor, statusColor, statusLabel } from '../components/tags'
import { ModuleTreePanel, inSelectedModule } from '../components/ModuleTreePanel'
import { ResizableSider } from '../components/Workspace'
import { SelectProjectEmpty } from '../components/Page'

/** Render server timestamp ("2026-06-21 12:34:56.78+00") as "2026-06-21 12:34:56"; empty/unparsable falls back to "—". */
function fmtTs(ts?: string): string {
  if (!ts) return '—'
  const m = ts.match(/^(\d{4}-\d{2}-\d{2})[ T](\d{2}:\d{2}:\d{2})/)
  return m ? `${m[1]} ${m[2]}` : '—'
}

// Scenario recycle bin: soft-deleted scenarios (steps kept, restore is lossless).
// Layout mirrors the scenarios page: left module tree + main list with batch bar.
export default function ScenarioRecycle() {
  const { t } = useI18n()
  const { projectId } = useApp()
  const [list, setList] = useState<Scenario[]>([])
  const [modules, setModules] = useState<ApiModule[]>([])
  const [loading, setLoading] = useState(false)
  const [search, setSearch] = useState('')
  const [moduleSearch, setModuleSearch] = useState('')
  const [selModule, setSelModule] = useState('ALL') // ALL | UNFILED | <moduleId>
  const [selectedIds, setSelectedIds] = useState<React.Key[]>([])
  const [busy, setBusy] = useState(false)
  const [pageSize, setPageSize] = useState(20)

  const load = async () => {
    if (!projectId) { setList([]); setModules([]); return }
    setLoading(true)
    try {
      const [ss, mm] = await Promise.all([
        // Old backends without the recycle endpoint 404: show an empty bin instead of an error.
        api.recycleScenarios(projectId).catch((e) => { if (e instanceof ApiError && e.status === 404) return []; throw e }),
        api.modules(projectId).catch(() => []),
      ])
      setList(Array.isArray(ss) ? ss : [])
      setModules(Array.isArray(mm) ? mm : [])
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('scenario.loadFailed', '加载场景失败'))
    } finally {
      setLoading(false)
    }
  }
  useEffect(() => {
    load()
    setSelectedIds([])
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId])

  const moduleOf = (s: Scenario) => (s.meta?.moduleId as string) || ''
  const moduleName = (mid: string) => modules.find((m) => m.id === mid)?.name

  const filtered = useMemo(() => {
    const q = search.toLowerCase()
    return list.filter((s) => {
      if (!inSelectedModule(modules, selModule, moduleOf(s))) return false
      if (!q) return true
      const tags = (s.meta?.tags as string[] | undefined) || []
      return s.name.toLowerCase().includes(q) || s.id.toLowerCase().includes(q) || String(s.num || '').includes(q) || tags.some((tg) => tg.toLowerCase().includes(q))
    })
  }, [list, search, selModule, modules])

  if (!projectId) return <SelectProjectEmpty />

  const restoreOne = async (s: Scenario) => {
    try {
      await api.restoreScenario(s.id)
      message.success(t('scenario.restored', '已恢复'))
      setSelectedIds((ids) => ids.filter((id) => id !== s.id))
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('scenario.restoreFailed', '恢复失败'))
    }
  }
  const purgeOne = async (s: Scenario) => {
    try {
      await api.purgeScenario(s.id)
      message.success(t('scenario.purged', '已彻底删除'))
      setSelectedIds((ids) => ids.filter((id) => id !== s.id))
      load()
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('scenario.deleteFailed', '删除失败'))
    }
  }
  const selectedRows = list.filter((s) => selectedIds.includes(s.id))
  const batchRestore = async () => {
    setBusy(true)
    try {
      for (const s of selectedRows) { try { await api.restoreScenario(s.id) } catch { /* partial failures surface via reload */ } }
      message.success(t('scenario.restored', '已恢复'))
      setSelectedIds([])
      load()
    } finally {
      setBusy(false)
    }
  }
  const batchPurge = () => {
    modal.confirm({
      title: `${t('scenario.batchPurgeConfirm', '确认彻底删除选中的场景?')} (${selectedRows.length})`,
      content: t('scenario.purgeConfirmBody', '彻底删除后不可恢复。'),
      okType: 'danger',
      okText: t('a.delete', '删除'),
      cancelText: t('a.cancel', '取消'),
      onOk: async () => {
        for (const s of selectedRows) { try { await api.purgeScenario(s.id) } catch { /* partial failures surface via reload */ } }
        setSelectedIds([])
        load()
      },
    })
  }

  const muted = (v?: string) => <span style={{ color: 'var(--text-3)' }}>{v || '—'}</span>
  const columns: ColumnsType<Scenario> = [
    { key: 'id', title: 'ID', dataIndex: 'num', width: 110, sorter: (a, b) => (a.num || 0) - (b.num || 0), render: (v: number, s) => <span className="ms-mono" style={{ color: 'var(--brand)', fontSize: 12 }}>{v || s.id.slice(0, 8)}</span> },
    { key: 'name', title: t('scenario.colSceneName', '场景名称'), dataIndex: 'name', ellipsis: true, sorter: (a, b) => a.name.localeCompare(b.name), render: (v: string) => <span style={{ fontWeight: 500 }}>{v}</span> },
    { key: 'priority', title: t('scenario.priority', '场景等级'), width: 110, render: (_v, s) => { const p = (s.meta?.priority as string) || 'P0'; return <span style={{ color: priorityColor(p) }}>● {p}</span> } },
    { key: 'status', title: t('scenario.colStatus', '状态'), dataIndex: 'status', width: 110, render: (s: string) => <Tag color={statusColor(s)}>{statusLabel(s, t)}</Tag> },
    { key: 'tags', title: t('scenario.tags', '标签'), width: 160, render: (_v, s) => { const tags = (s.meta?.tags as string[] | undefined) || []; return tags.length ? <Space size={[4, 4]} wrap>{tags.map((tg) => <Tag key={tg} style={{ margin: 0 }}>{tg}</Tag>)}</Space> : muted() } },
    { key: 'sceneEnv', title: t('scenario.colSceneEnv', '场景环境'), width: 130, render: (_v, s) => { const en = s.meta?.envName as string | undefined; return en ? <Tag color="blue" style={{ margin: 0 }}>{en}</Tag> : muted() } },
    { key: 'steps', title: t('scenario.colSteps', '步骤数'), width: 90, render: (_v, s) => <span>{s.steps?.length ?? 0}</span> },
    { key: 'module', title: t('scenario.colModule', '所属模块'), width: 140, ellipsis: true, render: (_v, s) => { const mid = moduleOf(s); return mid && moduleName(mid) ? <span>{moduleName(mid)}</span> : muted(t('scenario.unplanned', '未规划场景')) } },
    { key: 'createdAt', title: t('apidef.colCreatedAt', '创建时间'), width: 160, render: (_v, s) => <span style={{ color: 'var(--text-3)', fontSize: 12 }}>{fmtTs(s.createdAt)}</span> },
    { key: 'updatedAt', title: t('apidef.updatedAt', '更新时间'), width: 160, render: (_v, s) => <span style={{ color: 'var(--text-3)', fontSize: 12 }}>{fmtTs(s.updatedAt)}</span> },
    {
      key: 'action',
      title: t('apidef.colAction', '操作'),
      width: 150,
      fixed: 'right',
      render: (_v, s) => (
        <Space size={4} onClick={(e) => e.stopPropagation()}>
          <Button type="link" size="small" onClick={() => restoreOne(s)}>{t('scenario.restore', '恢复')}</Button>
          <Popconfirm
            title={t('scenario.purgeConfirmTitle', '彻底删除该场景?')}
            description={t('scenario.purgeConfirmBody', '彻底删除后不可恢复。')}
            okType="danger"
            okText={t('a.delete', '删除')}
            cancelText={t('a.cancel', '取消')}
            onConfirm={() => purgeOne(s)}
          >
            <Button type="link" size="small" danger>{t('scenario.purge', '彻底删除')}</Button>
          </Popconfirm>
        </Space>
      ),
    },
  ]

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', background: 'var(--panel)' }}>
      <div style={{ padding: '10px 16px', borderBottom: '1px solid var(--border-soft)' }}>
        <Breadcrumb
          items={[
            { title: <Link to="/api/scenario">{t('m.scenario', '场景')}</Link> },
            { title: t('scenario.recycleBin', '回收站') },
          ]}
        />
      </div>
      <div style={{ display: 'flex', flex: 1, minHeight: 0 }}>
        <ResizableSider defaultWidth={252} storageKey="ms.sider.scenarioRecycle">
          <ModuleTreePanel
            projectId={projectId}
            modules={modules}
            items={list}
            getModuleId={moduleOf}
            selectedKey={selModule}
            onSelect={setSelModule}
            allLabel={t('scenario.allScenarios', '全部场景')}
            unfiledLabel={t('scenario.unplanned', '未规划场景')}
            moduleSearch={moduleSearch}
            onModuleSearch={setModuleSearch}
            searchPlaceholder={t('scenario.moduleSearchPh', '请输入模块名称进行搜索')}
            onModulesChanged={load}
            deleteModuleContent={t('scenario.deleteModuleContent', '其下场景将变为未规划(不会删除场景)。')}
          />
        </ResizableSider>
        <div style={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '10px 12px', borderBottom: '1px solid var(--border-soft)' }}>
            <span style={{ fontWeight: 600 }}>{t('scenario.recycleList', '回收站列表')}</span>
            <div style={{ flex: 1 }} />
            <Input allowClear prefix={<SearchOutlined style={{ color: 'var(--text-3)' }} />} placeholder={t('scenario.searchByIdNameTag', '通过 ID/名称/标签搜索')} style={{ width: 260 }} value={search} onChange={(e) => setSearch(e.target.value)} />
            <Button icon={<ReloadOutlined />} onClick={load} />
          </div>
          <div style={{ flex: 1, overflow: 'auto', padding: 12 }}>
            <Table<Scenario>
              rowKey="id"
              size="middle"
              loading={loading}
              dataSource={filtered}
              columns={columns}
              scroll={{ x: 'max-content' }}
              rowSelection={{ type: 'checkbox', selectedRowKeys: selectedIds, onChange: setSelectedIds }}
              pagination={{ pageSize, size: 'small', showSizeChanger: true, pageSizeOptions: ['10', '20', '30', '50'], onShowSizeChange: (_, s) => setPageSize(s), showTotal: (total) => `${t('apidef.totalPrefix', '共')} ${total} ${t('scenario.unit', '条')}` }}
              locale={{ emptyText: <Empty description={t('scenario.recycleEmpty', '回收站为空')} /> }}
            />
          </div>
          {selectedIds.length > 0 && (
            <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '10px 16px', borderTop: '1px solid var(--border-soft)', background: 'var(--panel)' }}>
              <span>{t('scenario.selectedPrefix', '已选择')} {selectedIds.length} {t('scenario.unit', '条')}</span>
              <Button size="small" loading={busy} onClick={batchRestore}>{t('scenario.batchRestore', '批量恢复')}</Button>
              <Button size="small" danger onClick={batchPurge}>{t('scenario.batchPurge', '批量彻底删除')}</Button>
              <Button size="small" type="text" onClick={() => setSelectedIds([])}>{t('scenario.clearSel', '清空')}</Button>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
