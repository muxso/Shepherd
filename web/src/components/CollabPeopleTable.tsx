import { useEffect, useMemo, useState } from 'react'
import { Alert, Button, Input, Table, Tabs, Tag } from 'antd'
import { api, type CollabPersonRow } from '../api'

type PersonSummary = {
  personId: string; personName: string; roles: Set<string>; requirements: number
  delivered: number; aiTasks: number; humanTasks: number; points: number; requirementShare: number
}
const amount = (n: number) => Number(n.toFixed(2))
const statusName: Record<string, string> = { DRAFT: '待处理', BASELINED: '进行中', DELIVERED: '已交付', ARCHIVED: '已归档' }

export function CollabPeopleTable({ projectId, requirementId }: { projectId: string; requirementId?: string }) {
  const [rows, setRows] = useState<CollabPersonRow[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState(false)
  const [query, setQuery] = useState('')
  const [retry, setRetry] = useState(0)
  useEffect(() => {
    let active = true
    setRows([]); setLoading(true); setError(false)
    api.collabPeople(projectId, requirementId).then(r => { if (active) setRows(r) })
      .catch(() => { if (active) setError(true) })
      .finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [projectId, requirementId, retry])
  const filtered = useMemo(() => {
    const q = query.trim().toLocaleLowerCase()
    return rows.filter(r => !q || `${r.title} ${r.personName} ${r.roles.join(' ')}`.toLocaleLowerCase().includes(q))
  }, [rows, query])
  const people = useMemo(() => {
    const index = new Map<string, PersonSummary>()
    for (const r of filtered) {
      if (!r.personId) continue
      const p = index.get(r.personId) ?? { personId: r.personId, personName: r.personName, roles: new Set<string>(), requirements: 0, delivered: 0, aiTasks: 0, humanTasks: 0, points: 0, requirementShare: 0 }
      r.roles.forEach(role => p.roles.add(role))
      p.requirements++; p.delivered += Number(r.delivered); p.aiTasks += r.aiTasks
      p.humanTasks += r.humanTasks; p.points += r.points; p.requirementShare += r.requirementShare
      index.set(r.personId, p)
    }
    return [...index.values()].sort((a, b) => b.delivered - a.delivered || b.requirements - a.requirements || a.personId.localeCompare(b.personId))
  }, [filtered])
  const roles = (values: string[]) => values.length ? values.map(r => <Tag key={r}>{r}</Tag>) : '—'
  const sharedColumns = [
    { title: '需求折算交付量', dataIndex: 'requirementShare', width: 140, render: amount },
    { title: '人工已验收任务', dataIndex: 'humanTasks', width: 135 },
    { title: 'AI 已验收任务', dataIndex: 'aiTasks', width: 125 },
    { title: '任务工作量（点）', dataIndex: 'points', width: 145 },
  ]
  return <div data-testid="collab-people" style={{ marginTop: 20 }}>
    <div style={{ fontWeight: 600, marginBottom: 8 }}>需求参与与协同人效</div>
    <Alert type="info" showIcon style={{ marginBottom: 12 }} message="参与人数与交付量分别统计" description="角色取自云效负责人、参与人、开发者、测试人员及协同人字段；关注人不计入。同一人在同一需求中合并角色。未拆分的已交付需求在已登记人员间均分 1 个交付量，仅作数量折算，不代表工时或点数；已有任务按负责人及实际验收记录归属。未分配任务不归给其他参与人。" />
    {error ? <Alert type="error" message="人员统计加载失败" action={<Button onClick={() => setRetry(n => n + 1)}>重试</Button>} /> : <>
      <Input.Search allowClear placeholder="搜索需求、人员或角色" value={query} onChange={e => setQuery(e.target.value)} style={{ maxWidth: 420, marginBottom: 8 }} />
      <div style={{ color: 'var(--text-3)', fontSize: 12 }}>{new Set(filtered.map(r => r.requirementId)).size} 个需求 · {people.length} 位已登记人员（参与人数可重复，交付折算量不重复）</div>
      <Tabs defaultActiveKey={requirementId ? 'requirements' : 'people'} items={[
        { key: 'people', label: '人员汇总', children: <Table<PersonSummary> size="small" loading={loading} rowKey="personId" dataSource={people} scroll={{ x: 1180 }} pagination={{ pageSize: 10, showSizeChanger: false }} columns={[
          { title: '人员', dataIndex: 'personName', width: 120, fixed: 'left' },
          { title: '参与角色', width: 230, render: (_, r) => roles([...r.roles]) },
          { title: '参与需求', dataIndex: 'requirements', width: 100 },
          { title: '已交付需求参与', dataIndex: 'delivered', width: 130 },
          ...sharedColumns,
        ]} /> },
        { key: 'requirements', label: '需求人员明细', children: <Table<CollabPersonRow> size="small" loading={loading} rowKey={r => `${r.requirementId}:${r.personId}`} dataSource={filtered} scroll={{ x: requirementId ? 1040 : 1360 }} pagination={{ pageSize: 10, showSizeChanger: false, showTotal: n => `共 ${n} 条人员明细` }} columns={[
          ...(!requirementId ? [{ title: '需求', dataIndex: 'title', width: 320, ellipsis: true, fixed: 'left' as const }] : []),
          { title: '需求状态', dataIndex: 'status', width: 110, render: (s: string) => statusName[s] ?? s },
          { title: '人员', dataIndex: 'personName', width: 120 },
          { title: '参与角色', width: 230, render: (_, r) => roles(r.roles) },
          ...sharedColumns,
        ]} /> },
      ]} />
    </>}
  </div>
}
