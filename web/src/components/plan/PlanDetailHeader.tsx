import { useEffect, useState } from 'react'
import { Button, Dropdown, Empty, Space, Table, Tag } from 'antd'
import {
  ClockCircleOutlined,
  CopyOutlined,
  EditOutlined,
  FileMarkdownOutlined,
  MoreOutlined,
  ReloadOutlined,
  StarFilled,
  StarOutlined,
} from '@ant-design/icons'
import { message } from '../../feedback'
import { api, ApiError, type PlanStats } from '../../api'
import { useI18n } from '../../i18n'

const FOLLOW_KEY = 'shepherd.planFollow'

function followSet(): Set<string> {
  try {
    return new Set(JSON.parse(localStorage.getItem(FOLLOW_KEY) || '[]') as string[])
  } catch {
    return new Set()
  }
}

/** Plan detail header: status + id + name + progress, and edit/report/copy/follow/more actions. */
export default function PlanDetailHeader({
  planId,
  projectId,
  name,
  stats,
  onEdit,
  onReport,
  onSchedule,
  onRefresh,
}: {
  planId: string
  projectId: string
  name: string
  stats: PlanStats | null
  onEdit: () => void
  onReport: () => void
  onSchedule: () => void
  onRefresh: () => void
}) {
  const { t } = useI18n()
  const [followed, setFollowed] = useState(() => followSet().has(planId))
  const [copying, setCopying] = useState(false)

  const total = stats?.total ?? 0
  const exec = Math.round((stats?.executeRate ?? 0) * total)
  const passPct = Math.round((stats?.passRate ?? 0) * 100)
  const status: 'NOT_STARTED' | 'RUNNING' | 'DONE' = !total || exec <= 0 ? 'NOT_STARTED' : exec >= total ? 'DONE' : 'RUNNING'
  const statusMeta = {
    NOT_STARTED: { label: t('plan.statusNotStarted', '未开始'), color: 'default' },
    RUNNING: { label: t('plan.statusRunning', '进行中'), color: 'processing' },
    DONE: { label: t('plan.statusDone', '已完成'), color: 'success' },
  }[status]

  const toggleFollow = () => {
    const s = followSet()
    if (s.has(planId)) s.delete(planId)
    else s.add(planId)
    localStorage.setItem(FOLLOW_KEY, JSON.stringify([...s]))
    setFollowed(s.has(planId))
  }

  // Duplicate: new plan with the same planning doc (link sync recreates node-linked cases).
  const copy = async () => {
    setCopying(true)
    try {
      const created = await api.createPlan({ projectId, name: `${name}_copy` })
      const detail = await api.planDetail(planId)
      if (detail.planning) await api.savePlanPlanning(created.id, detail.planning)
      // The copy shows up in the server list on the next reload.
      message.success(t('plan.copied', '已复制计划'))
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('plan.copyFail', '复制失败'))
    } finally {
      setCopying(false)
    }
  }

  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '10px 16px', borderBottom: '1px solid var(--border-soft)' }}>
      <Tag color={statusMeta.color} style={{ marginInlineEnd: 0 }}>{statusMeta.label}</Tag>
      <span className="ms-mono" style={{ color: 'var(--text-3)', fontSize: 12 }}>[{planId.slice(0, 8)}]</span>
      <span style={{ fontWeight: 600, fontSize: 15, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{name}</span>
      <span style={{ display: 'inline-flex', alignItems: 'center', gap: 8, marginLeft: 8, color: 'var(--text-2)', fontSize: 12, whiteSpace: 'nowrap' }}>
        {t('plan.executed', '已执行')} {exec}/{total} · {t('plan.colPassRate', '通过率')} {passPct}%
        <span style={{ width: 80, height: 6, background: 'var(--panel-2)', border: '1px solid var(--border-soft)', borderRadius: 3, overflow: 'hidden', display: 'inline-block' }}>
          <span style={{ display: 'block', width: `${passPct}%`, height: '100%', background: 'var(--success)' }} />
        </span>
      </span>
      <div style={{ flex: 1 }} />
      <Space size={6}>
        <Button size="small" icon={<EditOutlined />} onClick={onEdit}>{t('a.edit', '编辑')}</Button>
        <Button size="small" icon={<FileMarkdownOutlined />} onClick={onReport}>{t('plan.genReport', '生成报告')}</Button>
        <Button size="small" icon={<CopyOutlined />} loading={copying} onClick={copy}>{t('plan.copy', '复制')}</Button>
        <Button
          size="small"
          icon={followed ? <StarFilled style={{ color: 'var(--warning)' }} /> : <StarOutlined />}
          onClick={toggleFollow}
        >
          {followed ? t('plan.followed', '已关注') : t('plan.follow', '关注')}
        </Button>
        <Dropdown
          menu={{
            items: [
              { key: 'schedule', icon: <ClockCircleOutlined />, label: t('plan.schedule', '定时'), onClick: onSchedule },
              { key: 'refresh', icon: <ReloadOutlined />, label: t('plan.refresh', '刷新'), onClick: onRefresh },
            ],
          }}
        >
          <Button size="small" icon={<MoreOutlined />}>{t('plan.more', '更多')}</Button>
        </Dropdown>
      </Space>
    </div>
  )
}

interface PlanRunRow {
  id: string
  status: string
  total: number
  passRate: number
  executeRate: number
  triggeredAt: string
}

/** 执行历史 tab: scheduled/manual run records of the plan. */
export function PlanRunsTable({ planId }: { planId: string }) {
  const { t } = useI18n()
  const [rows, setRows] = useState<PlanRunRow[]>([])
  const [loading, setLoading] = useState(false)
  useEffect(() => {
    setLoading(true)
    api.planRuns(planId)
      .then((r) => setRows(r as PlanRunRow[]))
      .catch(() => setRows([]))
      .finally(() => setLoading(false))
  }, [planId])
  return (
    <Table<PlanRunRow>
      rowKey="id"
      size="small"
      loading={loading}
      dataSource={rows}
      pagination={false}
      locale={{ emptyText: <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('plan.noRuns', '暂无执行记录')} /> }}
      columns={[
        { title: t('plan.runTime', '触发时间'), dataIndex: 'triggeredAt', width: 200, render: (v: string) => (v ? new Date(v).toLocaleString() : '—') },
        { title: t('plan.colStatus', '状态'), dataIndex: 'status', width: 110, render: (s: string) => <Tag color={s === 'COMPLETED' ? 'success' : 'processing'}>{s}</Tag> },
        { title: t('plan.totalCases', '用例总数'), dataIndex: 'total', width: 100 },
        { title: t('plan.colPassRate', '通过率'), dataIndex: 'passRate', width: 100, render: (v: number) => `${Math.round((v ?? 0) * 100)}%` },
        { title: t('plan.executeRate', '执行完成率'), dataIndex: 'executeRate', width: 110, render: (v: number) => `${Math.round((v ?? 0) * 100)}%` },
      ]}
    />
  )
}
