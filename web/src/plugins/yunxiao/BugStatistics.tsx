import { ReadError } from '../../components/Page'
import { useI18n } from '../../i18n'
import { useEffect, useState } from 'react'
import { useLocation,useNavigate } from 'react-router-dom'
import { Card, DatePicker, Empty, Select, Spin, Tag, Typography } from 'antd'
import dayjs from 'dayjs'
import locale from 'antd/es/date-picker/locale/zh_CN'
import { devopsApi, type FilteredBugStatistics } from './api'
import { ManagementBars, ManagementTrend } from './ManagementCharts'
import './management.css'
const roleLabels = { responsible: 'defectOwner', reporter: 'actualReporter', assignee: 'currentAssignee' }
type Role = keyof typeof roleLabels
export default function BugStatistics({ sourceId, revision }: { sourceId: string; revision: number }) {
  const { t } = useI18n()
  const location=useLocation(),navigate=useNavigate(),query=new URLSearchParams(location.search)
  const initialRole=query.get('bugRole') as Role
  const [range, setRange] = useState<[string, string] | null>(()=>query.get('bugFrom') && query.get('bugThrough') ? [query.get('bugFrom')!,query.get('bugThrough')!] : null), [days, setDays] = useState(Math.max(1,Math.min(366,Number(query.get('days')) || 7))), [role, setRole] = useState<Role>(initialRole in roleLabels ? initialRole : 'responsible'), [person, setPerson] = useState(query.get('bugPerson') || ''), [group, setGroup] = useState(query.get('bugGroup') || '')
  const remember=(values:Record<string,string>)=>{const q=new URLSearchParams(location.search);q.set('view','bugs');q.set('source',sourceId);for(const [key,value] of Object.entries(values)){if(value)q.set(key,value);else q.delete(key)}navigate({pathname:location.pathname,search:q.toString()},{replace:true})}
  const choosePerson=(value:string)=>{setPerson(value);remember({bugPerson:value})}
  useEffect(() => {
    const q = new URLSearchParams(location.search), nextRole = q.get('bugRole') as Role
    const nextRange: [string, string] | null = q.get('bugFrom') && q.get('bugThrough') ? [q.get('bugFrom')!, q.get('bugThrough')!] : null
    setRange(value => JSON.stringify(value) === JSON.stringify(nextRange) ? value : nextRange)
    setDays(Math.max(1, Math.min(366, Number(q.get('days')) || 7)))
    setRole(nextRole in roleLabels ? nextRole : 'responsible'); setPerson(q.get('bugPerson') || ''); setGroup(q.get('bugGroup') || '')
  }, [location.search])

  const [result, setResult] = useState<{ key: string; data: FilteredBugStatistics } | null>(null), [error, setError] = useState<unknown>(), [loading, setLoading] = useState(false)
  const key = JSON.stringify([sourceId, range, days, role, person, group, revision]), data = result?.key === key ? result.data : undefined
  const options = result?.data
  useEffect(() => {
    let live = true; setLoading(true); setError(undefined)
    const timer = window.setTimeout(() => {
      devopsApi.bugStatistics(sourceId, { ...(range ? { from: range[0], through: range[1] } : { days: String(days) }), personRole: role, ...(person ? { person } : {}), ...(group ? { group } : {}) }).then(data => { if (live) setResult({ key, data }) }).catch(e => { if (live) setError(e) }).finally(() => { if (live) setLoading(false) })
    }, 120)
    return () => { live = false; window.clearTimeout(timer) }
  }, [key, sourceId, range, days, role, person, group])
  const count = (field: 'total' | 'unassigned' | 'linked' | 'unlinked') => data?.[field]?.toLocaleString() ?? '—'
  return <section aria-label={t("engineering.yunxiaoDefectStatistics")}>
    <div className="yx-management-filters">
      <Select aria-label={t("engineering.defectDateRange")} value={range ? 'custom' : String(days)} style={{ width: 150 }} onChange={value => { if (value !== 'custom') { setDays(Number(value)); setRange(null);remember({days:value,bugFrom:'',bugThrough:''}) } }} options={[7, 30, 90, 365].map(value => ({ value: String(value), label: t("engineering.lastV0Days", undefined, { v0: value }) })).concat(range ? [{ value: 'custom', label: t("engineering.customRange") }] : [])} />
      <DatePicker.RangePicker aria-label={t("engineering.defectCreationDate")} locale={locale} value={range ? [dayjs(range[0]), dayjs(range[1])] : null} disabledDate={d => d.isAfter(dayjs(), 'day')} onChange={value => { const dates:[string,string] | null=value?.[0] && value[1] ? [value[0].format('YYYY-MM-DD'),value[1].format('YYYY-MM-DD')] : null;setRange(dates);remember({bugFrom:dates?.[0] || '',bugThrough:dates?.[1] || ''}) }} />
      <Select aria-label={t("engineering.defectAttribution")} value={role} onChange={value => { setRole(value); setPerson(''); setGroup('');remember({bugRole:value,bugPerson:'',bugGroup:''}) }} style={{ width: 150 }} options={Object.entries(roleLabels).map(([value, label]) => ({ value, label: t('engineering.' + label) }))} />
      <Select aria-label={t("engineering.defectMember")} placeholder={t("engineering.allMembers")} allowClear showSearch value={person || undefined} onChange={value => choosePerson(value || '')} style={{ width: 160 }} options={[...new Set([...(options?.personOptions || []), ...(person ? [person] : [])])].map(value => ({ value, label: value }))} />
      <Select aria-label={t("engineering.defectGroup")} placeholder={t("engineering.allGroups")} allowClear value={group || undefined} onChange={value => {setGroup(value || '');remember({bugGroup:value || ''})}} style={{ width: 170 }} options={[...new Set([...(options?.groups || []), ...(group ? [group] : [])])].map(value => ({ value, label: value }))} />
    </div>
    {error != null && <ReadError error={error} title={t("engineering.couldNotLoadDefectStatistics")} />}
    <Spin spinning={loading}>
      <div className="yx-management-scope"><span>{data ? t('engineering.defectDateScope', undefined, { from: data.from, through: data.through }) : t("engineering.groupedByDefectCreationDate")}</span><span>{t('engineering.defectAttributionValue', undefined, { role: t('engineering.' + roleLabels[role]) })}</span></div>
      {!!data?.missingTime && <Typography.Text type="secondary">
        {t('engineering.defectsWithoutDate', '另有 {count} 条源缺陷缺少创建时间，未计入统计。', { count: data.missingTime })}
      </Typography.Text>}
      <div className="yx-management-summary">{[[t("engineering.selectedDefects"), count('total'), t("engineering.uniqueDefects")], [t("engineering.unassignedOwnership"), count('unassigned'), '源缺陷责任人尚未登记'], [t("engineering.linkedToRequirements"), count('linked'), '同数据源内明确关联'], [t("engineering.noLinkedRequirements"), count('unlinked'), t("engineering.reviewMissingLinksFirst")]].map(([label, value, note]) => <article key={label}><span>{label}</span><strong>{value}</strong><small>{note}</small></article>)}</div>
      {data && <>
        <div className="yx-management-grid-layout">
          <Card size="small" title={t("engineering.defectIntakeTrends")}><ManagementTrend title="所选范围新增缺陷趋势" series={[{ key: 'bugs', label: t("engineering.newDefects"), color: '#d69660', points: data.trend || [] }]} /></Card>
          <Card size="small" title={t("engineering.currentStatusDistribution")} extra={<Tag>{t("engineering.currentSnapshot")}</Tag>}><ManagementBars items={data.statuses.map(s => ({ name: s.status, value: s.count }))} unit=" 个" /></Card>
          <Card size="small" title={t('engineering.defectDistributionByRole', undefined, { role: t('engineering.' + roleLabels[role]) })}><ManagementBars items={data.people.map(p => ({ name: p.name, value: p.count }))} unit=" 个" onSelect={choosePerson} /></Card>
          <Card size="small" title={t("engineering.priorityDistribution")}><ManagementBars items={(data.priorities || []).map(p => ({ name: p.name, value: p.count }))} unit=" 个" /></Card>
        </div>
        {!data.total && <Empty description={t("engineering.noMatchingDefectsAdjustTheDatesPeopleOrGroups")} />}
      </>}
    </Spin>
  </section>
}
