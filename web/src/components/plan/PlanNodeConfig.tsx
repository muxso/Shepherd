import { useEffect, useState } from 'react'
import { Button, Modal, Radio, Select, Switch, Table, Tabs, Tag } from 'antd'
import type { Environment, PlanningNode, PlanningNodeConfig, ResourcePool } from '../../api'
import { api } from '../../api'
import { useI18n } from '../../i18n'

export interface CaseOption {
  id: string
  name: string
}

/** Right slide-in config panel for a mind-map node: case links + execution config. */
export default function PlanNodeConfig({
  node,
  projectId,
  envs,
  pools,
  onSave,
  onClose,
}: {
  node: PlanningNode
  projectId: string
  envs: Environment[]
  pools: ResourcePool[]
  /** Save the edited config + selections back into the tree (names feed the backend link sync). */
  onSave: (patch: {
    config: PlanningNodeConfig
    caseIds: string[]
    scenarioIds: string[]
    names: Record<string, string>
  }) => void
  onClose: () => void
}) {
  const { t } = useI18n()
  const [config, setConfig] = useState<PlanningNodeConfig>({})
  const [caseIds, setCaseIds] = useState<string[]>([])
  const [scenarioIds, setScenarioIds] = useState<string[]>([])
  const [names, setNames] = useState<Record<string, string>>({})
  const [pickerOpen, setPickerOpen] = useState(false)

  // Re-seed local state whenever the target node changes.
  useEffect(() => {
    setConfig({ inherit: true, mode: 'serial', ...(node.config || {}) })
    setCaseIds(node.caseIds || [])
    setScenarioIds(node.scenarioIds || [])
    setNames({})
  }, [node])

  const selected = caseIds.length + scenarioIds.length
  const inherit = config.inherit !== false
  const set = (patch: PlanningNodeConfig) => setConfig((c) => ({ ...c, ...patch }))

  const row = (label: string, control: React.ReactNode) => (
    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', margin: '14px 0' }}>
      <span style={{ color: 'var(--text-2)', fontSize: 13 }}>{label}</span>
      {control}
    </div>
  )

  return (
    <div
      style={{
        position: 'absolute',
        top: 0,
        right: 0,
        bottom: 0,
        width: 340,
        background: 'var(--panel)',
        borderLeft: '1px solid var(--border-soft)',
        boxShadow: '-6px 0 16px rgba(0,0,0,0.08)',
        display: 'flex',
        flexDirection: 'column',
        zIndex: 5,
      }}
    >
      <div style={{ padding: '12px 16px', borderBottom: '1px solid var(--border-soft)', fontWeight: 600 }}>
        {node.name} · {t('plan.mm.nodeConfig', '配置')}
      </div>
      <div style={{ flex: 1, overflow: 'auto', padding: '4px 16px' }}>
        <div style={{ margin: '14px 0' }}>
          <div style={{ color: 'var(--text-2)', fontSize: 13, marginBottom: 8 }}>{t('plan.mm.selectCases', '选择用例')}</div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
            <Tag color="processing">{t('plan.mm.selectedN', '已选 {n} 条').replace('{n}', String(selected))}</Tag>
            <Button size="small" type="primary" ghost onClick={() => setPickerOpen(true)}>
              {t('plan.mm.linkCases', '关联用例')}
            </Button>
            <Button size="small" disabled={!selected} onClick={() => { setCaseIds([]); setScenarioIds([]) }}>
              {t('plan.mm.clearCases', '清空已选用例')}
            </Button>
          </div>
        </div>
        {row(
          t('plan.mm.inherit', '继承上级配置'),
          <Switch size="small" checked={inherit} onChange={(v) => set({ inherit: v })} />,
        )}
        {row(
          t('plan.mm.pool', '资源池'),
          <Select
            size="small"
            style={{ width: 180 }}
            allowClear
            disabled={inherit}
            value={config.poolId || undefined}
            onChange={(v) => set({ poolId: v })}
            options={pools.map((p) => ({ value: p.id, label: p.name }))}
          />,
        )}
        {row(
          t('plan.mm.env', '环境'),
          <Select
            size="small"
            style={{ width: 180 }}
            allowClear
            disabled={inherit}
            value={config.envId || undefined}
            onChange={(v) => set({ envId: v })}
            options={envs.map((e) => ({ value: e.id, label: e.name }))}
          />,
        )}
        {row(
          t('plan.mm.execMode', '执行方式'),
          <Radio.Group
            size="small"
            disabled={inherit}
            value={config.mode || 'serial'}
            onChange={(e) => set({ mode: e.target.value })}
            options={[
              { value: 'serial', label: t('plan.mm.serialFull', '串行') },
              { value: 'parallel', label: t('plan.mm.parallelFull', '并行') },
            ]}
            optionType="button"
          />,
        )}
        {row(
          t('plan.mm.stopOnFail', '失败停止'),
          <Switch size="small" disabled={inherit} checked={!!config.stopOnFail} onChange={(v) => set({ stopOnFail: v })} />,
        )}
        {row(
          t('plan.mm.retry', '失败重试'),
          <Switch size="small" disabled={inherit} checked={!!config.retry} onChange={(v) => set({ retry: v })} />,
        )}
      </div>
      <div style={{ padding: '10px 16px', borderTop: '1px solid var(--border-soft)', display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
        <Button size="small" onClick={onClose}>{t('a.cancel', '取消')}</Button>
        <Button size="small" type="primary" onClick={() => onSave({ config, caseIds, scenarioIds, names })}>
          {t('lv.save', '保存')}
        </Button>
      </div>
      <CasePickerModal
        open={pickerOpen}
        projectId={projectId}
        caseIds={caseIds}
        scenarioIds={scenarioIds}
        onClose={() => setPickerOpen(false)}
        onOk={(c, s, n) => {
          setCaseIds(c)
          setScenarioIds(s)
          setNames((prev) => ({ ...prev, ...n }))
          setPickerOpen(false)
        }}
      />
    </div>
  )
}

/** Case/scenario picker: link api-case ids and scenario ids to the node. */
function CasePickerModal({
  open,
  projectId,
  caseIds,
  scenarioIds,
  onClose,
  onOk,
}: {
  open: boolean
  projectId: string
  caseIds: string[]
  scenarioIds: string[]
  onClose: () => void
  onOk: (caseIds: string[], scenarioIds: string[], names: Record<string, string>) => void
}) {
  const { t } = useI18n()
  const [cases, setCases] = useState<CaseOption[]>([])
  const [scenarios, setScenarios] = useState<CaseOption[]>([])
  const [selCases, setSelCases] = useState<string[]>([])
  const [selScenarios, setSelScenarios] = useState<string[]>([])

  useEffect(() => {
    if (!open) return
    setSelCases(caseIds)
    setSelScenarios(scenarioIds)
    api.projectCases(projectId)
      .then((p) => setCases(p.items.map((c) => ({ id: c.id, name: `${c.method} ${c.name}` }))))
      .catch(() => setCases([]))
    api.scenarios(projectId)
      .then((list) => setScenarios(list.map((s) => ({ id: s.id, name: s.name }))))
      .catch(() => setScenarios([]))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  const table = (rows: CaseOption[], sel: string[], setSel: (v: string[]) => void) => (
    <Table<CaseOption>
      rowKey="id"
      size="small"
      dataSource={rows}
      pagination={{ pageSize: 8, size: 'small' }}
      rowSelection={{ selectedRowKeys: sel, onChange: (ks) => setSel(ks.map(String)) }}
      columns={[{ title: t('plan.caseName', '用例名'), dataIndex: 'name', ellipsis: true }]}
      onRow={(r) => ({
        onClick: () => setSel(sel.includes(r.id) ? sel.filter((k) => k !== r.id) : [...sel, r.id]),
        style: { cursor: 'pointer' },
      })}
    />
  )

  const ok = () => {
    const names: Record<string, string> = {}
    cases.filter((c) => selCases.includes(c.id)).forEach((c) => (names[c.id] = c.name))
    scenarios.filter((s) => selScenarios.includes(s.id)).forEach((s) => (names[s.id] = s.name))
    onOk(selCases, selScenarios, names)
  }

  return (
    <Modal title={t('plan.mm.pickTitle', '关联用例')} open={open} onCancel={onClose} onOk={ok} width={640} destroyOnHidden>
      <Tabs
        size="small"
        items={[
          { key: 'api', label: t('plan.mm.pickApiTab', '接口用例'), children: table(cases, selCases, setSelCases) },
          { key: 'scenario', label: t('plan.mm.pickScenarioTab', '场景用例'), children: table(scenarios, selScenarios, setSelScenarios) },
        ]}
      />
    </Modal>
  )
}
