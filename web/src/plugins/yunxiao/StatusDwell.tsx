import { ReadError } from '../../components/Page'
import { useI18n } from '../../i18n'
import { useEffect, useState } from 'react'
import { Button, Card, DatePicker, Empty, Input, Segmented, Select, Space, Table, Tag, Tooltip, Typography } from 'antd'
import dayjs from 'dayjs'
import 'dayjs/locale/zh-cn'
import monthLocale from 'antd/es/date-picker/locale/zh_CN'
import { shanghaiMonth } from './memberMonth'
import { devopsApi, type StatusDwellData, type StatusDwellStage } from './api'

const shanghaiTime = new Intl.DateTimeFormat('zh-CN', {
  timeZone: 'Asia/Shanghai', dateStyle: 'short', timeStyle: 'short', hourCycle: 'h23',
})

function duration(seconds: number) {
  const whole = Math.floor(seconds)
  const parts = [
    [Math.floor(whole / 86400), '天'],
    [Math.floor(whole / 3600) % 24, '小时'],
    [Math.floor(whole / 60) % 60, '分钟'],
    [whole % 60, '秒'],
  ] as const
  return parts.filter(([value]) => value > 0).map(([value, unit]) => `${value} ${unit}`).join(' ') || '不足 1 秒'
}

const stageTones = [100, 55, 79, 37, 90, 47, 68, 28]
const stageColors = stageTones.map(tone => `color-mix(in srgb, var(--brand) ${tone}%, var(--panel))`)

function stageTint(color: string, strength: number) {
  return `color-mix(in srgb, ${color} ${strength}%, transparent)`
}
const increments = [1, 5, 10, 30, 60, 300, 600, 1800, 3600, 7200, 21600, 43200, 86400, 259200, 604800, 1382400]
const pendingStatuses = ['待开发', '待测试', '待发布']
const shareFormat = new Intl.NumberFormat('zh-CN', { style: 'percent', maximumFractionDigits: 1 })

function stageTotals(stages: StatusDwellStage[]) {
  const totals = new Map<string, number>()
  for (const stage of stages) totals.set(stage.status, (totals.get(stage.status) ?? 0) + stage.seconds)
  return Array.from(totals, ([status, seconds], index) => ({
    status, seconds, color: stageColors[index % stageColors.length],
  }))
}

function StageMiniChart({ stages }: { stages: StatusDwellStage[] }) {
  const colors = new Map(stageTotals(stages).map(stage => [stage.status, stage.color]))
  const max = Math.max(...stages.map(stage => stage.seconds))
  const step = increments.find(value => value * 2 >= max) ?? Math.ceil(max / 172800) * 86400
  return <div role="group" aria-label="阶段停留柱状图" style={{ display: 'flex', maxWidth: 490, paddingBlock: 3 }}>
    <div aria-hidden="true" style={{ flex: '0 0 auto', height: 49, display: 'flex', flexDirection: 'column', justifyContent: 'space-between', alignItems: 'end', paddingRight: 6, fontSize: 10, color: 'var(--text-3)', lineHeight: 1 }}>
      <span>{duration(step * 2)}</span><span>{duration(step)}</span><span>0</span>
    </div>
    <div style={{ flex: 1, minWidth: 0, overflowX: 'auto' }}>
      <div style={{ position: 'relative', display: 'flex', minWidth: '100%', width: 'max-content', borderLeft: '1px solid var(--border-soft)' }}>
        <div aria-hidden="true" style={{
          position: 'absolute', top: 4, left: 0, right: 0, height: 41, pointerEvents: 'none',
          borderBlock: '1px solid var(--border-soft)',
        }}><div style={{ position: 'absolute', top: '50%', width: '100%', borderTop: '1px dashed var(--border-soft)' }} /></div>
        {stages.map((stage, index) => {
          const label = `${index + 1}. ${stage.status} · ${duration(stage.seconds)}`
          const current = stage.phase === 'current'
          const color = colors.get(stage.status) ?? 'var(--brand)'
          return <Tooltip key={index} title={current ? `${label} · 同步时仍在此阶段` : label} trigger={['hover', 'focus']}>
            <div tabIndex={0} aria-label={label} style={{ flex: '0 0 59px', textAlign: 'center', cursor: 'default', zIndex: 1 }}>
              <div aria-hidden="true" style={{ height: 45, display: 'flex', alignItems: 'flex-end', justifyContent: 'center' }}>
                <div style={{
                  width: 19, height: Math.max(1, stage.seconds / (step * 2) * 41), borderRadius: '3px 3px 0 0',
                  background: current ? stageTint(color, 12) :
                    `linear-gradient(180deg, color-mix(in srgb, ${color} 85%, var(--panel)), ${color})`,
                  border: current ? `1px dashed ${color}` : undefined,
                }} />
              </div>
              <div aria-hidden="true" style={{ overflow: 'hidden', whiteSpace: 'nowrap', textOverflow: 'ellipsis', fontSize: 11, marginTop: 3 }}>
                {stage.status}
              </div>
            </div>
          </Tooltip>
        })}
      </div>
    </div>
  </div>
}

function StageShare({ stages, seconds }: { stages: StatusDwellStage[]; seconds: number }) {
  let offset = 0
  const slices = stageTotals(stages).map(stage => {
    const start = offset
    offset += stage.seconds / seconds * 100
    return { ...stage, stop: `${stage.color} ${start}% ${offset}%` }
  })
  return <Tooltip trigger={['hover', 'focus']} title={<div>
    {slices.map(stage => <div key={stage.status} style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
      <span aria-hidden="true" style={{ width: 8, height: 8, borderRadius: '50%', background: stage.color }} />
      {stage.status} · {shareFormat.format(stage.seconds / seconds)} · {duration(stage.seconds)}
    </div>)}
  </div>}>
    <div tabIndex={0} aria-label={`月内合计 ${duration(seconds)}。阶段占比 ${slices.map(stage => `${stage.status} ${shareFormat.format(stage.seconds / seconds)}`).join('，')}`}
      style={{ display: 'flex', alignItems: 'center', gap: 8, justifyContent: 'flex-start' }}>
      <div aria-hidden="true" style={{
        flex: '0 0 34px', height: 34, borderRadius: '50%',
        background: `linear-gradient(145deg, rgb(255 255 255 / 10%), transparent 62%), conic-gradient(${slices.map(stage => stage.stop).join(', ')})`,
        maskImage: 'radial-gradient(transparent 43%, #000 46%)',
      }} />
      <span style={{ fontWeight: 600, fontVariantNumeric: 'tabular-nums' }}>{duration(seconds)}</span>
    </div>
  </Tooltip>
}

export default function StatusDwell({ sourceId, revision }: { sourceId: string; revision: number }) {
  const { t } = useI18n()
  const [kind, setKind] = useState<'requirement' | 'bug'>('requirement')
  const [month, setMonth] = useState('')
  const [search, setSearch] = useState('')
  const [page, setPage] = useState(1)
  const [sortStage, setSortStage] = useState('')
  const [currentStatus, setCurrentStatus] = useState('')
  const [sortOrder, setSortOrder] = useState<'asc' | 'desc'>('desc')
  const [data, setData] = useState<StatusDwellData>()
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<unknown>()

  useEffect(() => {
    let active = true
    setLoading(true)
    setError(undefined)
    devopsApi.statusDwell(sourceId, kind, month, search, page, sortStage, sortOrder, currentStatus).then(result => {
      if (active) setData(result)
    }).catch((err: Error) => {
      if (active) {
        setData(undefined)
        setError(err)
      }
    }).finally(() => {
      if (active) setLoading(false)
    })
    return () => { active = false }
  }, [sourceId, kind, month, search, page, sortStage, sortOrder, currentStatus, revision])

  const selectedMonth = month || data?.month
  const statusCounts = data?.currentStatusCounts
  const statusOptions = Array.from(new Set([
    ...(kind === 'requirement' ? pendingStatuses : []),
    ...Object.keys(statusCounts ?? {}),
  ]))
  const filterStatus = (status: string) => { setCurrentStatus(status); setPage(1) }
  return <Card title={kind === 'requirement' ? t("engineering.requirementTimeByStage") : t("engineering.defectTimeByStage")}>
    <Space direction="vertical" size="middle" style={{ width: '100%' }}>
      <Space wrap>
        <Segmented<"requirement" | "bug"> aria-label={t("engineering.workItemType")} value={kind} onChange={value => { setKind(value); setSortStage(''); filterStatus('') }} options={[
          { value: 'requirement', label: t("engineering.requirements") }, { value: 'bug', label: t("engineering.defects") },
        ]} />
        <label>{t("engineering.reportingMonth")}{' '}
          <DatePicker aria-label={t("engineering.reportingMonth")} picker="month" locale={monthLocale}
            value={selectedMonth ? dayjs(`${selectedMonth}-01`).locale('zh-cn') : null}
            format="YYYY年MM月" inputReadOnly allowClear={false} style={{ width: 160 }}
            disabledDate={date => date.format('YYYY-MM') > shanghaiMonth()}
            onChange={date => { if (date) { setMonth(date.format('YYYY-MM')); setSortStage(''); setPage(1) } }} />
        </label>
        <Input.Search aria-label={t("engineering.searchNumberOrTitle")} placeholder={t("engineering.searchNumberOrTitle")} allowClear
          onSearch={value => { setSearch(value); setPage(1) }} />
        <Select aria-label={t("engineering.currentStatus")} placeholder={t("engineering.currentStatus")} allowClear showSearch
          value={currentStatus || undefined} style={{ minWidth: 146 }}
          options={statusOptions.map(status => ({ value: status, label: status }))}
          onChange={value => filterStatus(value || '')} />
        <Select aria-label={t("engineering.sortByStageDuration")} placeholder={t("engineering.sortByStageDuration")} allowClear showSearch
          value={sortStage || undefined} style={{ minWidth: 158 }}
          options={(data?.stages ?? []).map(status => ({ value: status, label: status }))}
          onChange={value => { setSortStage(value || ''); setPage(1) }} />
        {sortStage && <Select aria-label={t("engineering.stageDurationSortDirection")} value={sortOrder}
          options={[{ value: 'desc', label: t("engineering.longestFirst") }, { value: 'asc', label: t("engineering.shortestFirst") }]}
          onChange={value => { setSortOrder(value); setPage(1) }} />}
      </Space>
      {kind === 'requirement' && <Space wrap size={[8, 6]} style={{
        width: '100%', padding: '9px 12px', background: 'linear-gradient(105deg, color-mix(in srgb, var(--brand) 8%, transparent), transparent 82%)',
        border: '1px solid var(--border-soft)', borderRadius: 8,
      }}>
        <Typography.Text type="secondary">{t("engineering.projectWideStatus")}</Typography.Text>
        {pendingStatuses.map(status => <Button key={status} size="small"
          type={currentStatus === status ? 'primary' : 'default'} aria-pressed={currentStatus === status}
          disabled={!statusCounts}
          onClick={() => filterStatus(currentStatus === status ? '' : status)}>
          {status} {statusCounts ? (statusCounts[status] ?? 0).toLocaleString('zh-CN') : '—'}
        </Button>)}
      </Space>}
      {error ? <ReadError error={error} title={t("engineering.statusDwellReadFailed", "状态停留时间读取失败")} /> :
        <Table<StatusDwellData['workItems'][number]>
          rowKey="externalId" loading={loading} dataSource={data?.workItems ?? []} size="small" scroll={{ x: 920 }}
          pagination={{ current: page, pageSize: 15, total: data?.total ?? 0, showSizeChanger: false, onChange: setPage }}
          locale={{ emptyText: <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={loading ? t("engineering.calculatingTimeInStatus") : t("engineering.noStageRecordsForTheSelectedMonth")} /> }}
          expandable={{
            rowExpandable: item => item.stages.length > 0,
            expandedRowRender: item => {
              const max = Math.max(1, ...item.stages.map(stage => stage.seconds))
              return <Table<StatusDwellStage>
                rowKey={(stage) => `${stage.enteredAt}-${stage.status}`} dataSource={item.stages}
                size="small" pagination={false} scroll={{ x: 560 }}
                columns={[
                  { title: t("engineering.yunxiaoStage"), dataIndex: 'status', width: 150 },
                  { title: t("engineering.entered"), render: (_, stage) => shanghaiTime.format(new Date(stage.enteredAt)) },
                  { title: t("engineering.left"), render: (_, stage) => stage.exitedAt ? shanghaiTime.format(new Date(stage.exitedAt)) : t("engineering.stillInThisStageAtSync") },
                  {
                    title: t("engineering.timeThisMonth"), dataIndex: 'seconds', width: 220,
                    render: (seconds: number) => <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
                      <div aria-hidden="true" className="yx-management-bar-track" style={{ flex: 1 }}>
                        <i style={{ width: `${seconds / max * 100}%` }} />
                      </div>
                      <span>{duration(seconds)}</span>
                    </div>,
                  },
                ]} />
            },
          }}
          columns={[
            { title: t("engineering.number"), width: 135, render: (_, item) => item.serial || item.externalId },
            { title: t("engineering.title"), width: 280, render: (_, item) => <Space size={[6, 4]} wrap>
              <span>{item.title || '—'}</span>
              {item.currentStatus && <Tooltip title={t("engineering.latestSyncedYunxiaoStatus")}>
                <Tag style={{ color: 'var(--brand)', background: 'var(--brand-soft)', borderColor: 'transparent' }}>
                  {item.currentStatus}
                </Tag>
              </Tooltip>}
            </Space> },
            {
              title: t("engineering.stagesAndTimeThisMonth"),
              render: (_, item) => item.stages.length
                ? <StageMiniChart stages={item.stages} />
                : <Typography.Text type="secondary">{t("engineering.noStageRecordsThisMonth")}</Typography.Text>,
            },
            { title: t("engineering.monthlyTotalAndStageShare"), width: 190,
              render: (_, item) => item.seconds ? <StageShare stages={item.stages} seconds={item.seconds} /> : '—' },
          ]}
        />}
    </Space>
  </Card>
}
