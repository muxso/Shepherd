import { useLayoutEffect, useRef } from 'react'
import { Button, Popover, Tooltip } from 'antd'
import { WarningFilled } from '@ant-design/icons'
import type { ActivityKind, ActivityMember, TeamActivityData } from './api'
import { activityRoles, datesFor, dayFacts, lineCount, memberTotals } from './activityModel'
import { attribution, memberLabel, type ActivityReview } from './activityReview'
import { dateTime } from './codeActivity'
import type { ActivitySelection } from './ActivityDetails'
export default function ActivityMatrix({ data, members, metric, view, expanded, reviews, select, associate }: { data: TeamActivityData; members: ActivityMember[]; metric: ActivityKind; view: string; expanded: boolean; reviews: Map<string, ActivityReview>; select: (value: ActivitySelection) => void; associate: () => void }) {
  const ref = useRef<HTMLTableElement>(null)
  useLayoutEffect(() => {
    const table = ref.current
    if (!table || !members.length) return
    const fit = () => {
      const available = window.innerHeight - table.getBoundingClientRect().top - (table.tHead?.getBoundingClientRect().height || 0) - 70
      table.style.setProperty('--activity-cell-height', `${Math.max(16, Math.min(25, Math.floor(available / members.length) - 2))}px`)
    }
    fit(); const observer = new ResizeObserver(fit); observer.observe(table.parentElement!)
    window.addEventListener('resize', fit)
    return () => { observer.disconnect(); window.removeEventListener('resize', fit) }
  }, [data, members.length, view])
  const dates = datesFor(data, view), roleInfo = activityRoles.find(r => r.value === data.role)!
  const totalHeaders = data.role === 'code' ? ['提交', '需求', 'Bug'] : data.role === 'product' ? ['新增', '交付', '需求'] : ['用例', 'Bug', '活动']
  const openMember = (person: ActivityMember) => {
    const last = person.lastActivityAt ? dateTime(person.lastActivityAt).slice(0, 10) : ''
    select(last >= data.from && last <= data.through ? { date: last, person: person.name } : { person: person.name, history: true })
  }
  return <div className="yx-activity-table-scroll"><table ref={ref} className={`yx-activity-matrix ${view === 'month' ? 'is-month' : ''} ${expanded ? 'is-expanded' : ''}`} aria-label="成员每日活动热力图">
    <thead><tr><th className="yx-activity-name">成员 / 原始账号</th>{dates.map(date => { const weekday = new Date(`${date}T00:00:00Z`).getUTCDay(); return <th key={date} className={weekday === 0 || weekday === 6 ? 'is-weekend' : ''}><button type="button" aria-label={`查看 ${date} 全员明细`} onClick={() => select({ date })}><time dateTime={date}>{date.slice(5).replace('-', '/')}</time><small>周{'日一二三四五六'[weekday]}</small></button></th> })}{totalHeaders.map((label, i) => <th key={label} className="yx-activity-total"><Tooltip title={i === 1 && data.role === 'code' ? '统计期关联需求，按需求 ID 去重' : label === 'Bug' && data.role === 'code' ? '统计期新增责任 Bug，按 Bug ID 去重' : '所选统计期累计'}>{label}</Tooltip></th>)}</tr></thead>
    <tbody>{members.map(person => {
      const review = reviews.get(person.name), label = memberLabel(person), state = attribution(person)
      return <tr key={person.name} className={review ? 'needs-review' : ''}><td className="yx-activity-name"><div className="yx-activity-name-content"><Tooltip title={<><strong>{label} · {person.group}</strong>{state === 'unmappedAccount' && <div>原始 Git 作者尚未关联到统一人员，提交与行数已计入。</div>}{state === 'unknownAuthor' && <div>缺少 Git 作者信息；这里是提交汇总，不计作一个人员。</div>}{review && <><div>{review.summary}</div>{review.reasons.map(reason => <div key={reason}>{reason}</div>)}</>}</>}><button type="button" className="yx-activity-member-name" aria-label={`${label}${review ? ' · 需核验' : ' · 最近记录'}`} onClick={() => openMember(person)}>{review && <WarningFilled />}<span>{label}</span></button></Tooltip>{state === 'unmappedAccount' && <Button type="link" size="small" className="yx-activity-associate" aria-label={`关联账号 ${label}`} onClick={associate}>关联</Button>}</div></td>
        {dates.map(date => {
          const facts = dayFacts(data, person.name, date, metric), count = facts.events.length, future = date > data.through, outside = date < data.from, weekend = [0, 6].includes(new Date(`${date}T00:00:00Z`).getUTCDay())
          const level = count > 9 ? 4 : count > 5 ? 3 : count > 2 ? 2 : count ? 1 : 0, time = facts.last ? dateTime(facts.last).slice(11, 16) : ''
          const tip = <div className="yx-activity-hover"><strong>{label} · {date}</strong><span>{count} 次{roleInfo.metrics.find(m => m.value === metric)?.label}{facts.last ? ` · 最后 ${dateTime(facts.last).slice(11)}` : ''}</span>{data.role === 'code' && <><span>新增 +{lineCount(facts.events, 'additions')} / 删除 −{lineCount(facts.events, 'deletions')} 行</span>{facts.bugs.length > 0 && <span>新增责任 Bug {facts.bugs.length}</span>}<span>{facts.requirements.length ? `关联 ${facts.requirements.length} 项需求` : '没有明确关联需求'}</span>{facts.requirements.slice(0, 3).map(r => <span key={r.externalId}>{r.serial || r.externalId} {r.title || ''}</span>)}{facts.requirements.length > 3 && <small>另 {facts.requirements.length - 3} 项，点击查看</small>}</>}</div>
          return <td key={date} className={weekend ? 'is-weekend' : ''}><Popover content={tip} trigger={['hover', 'focus']} mouseEnterDelay={0.15}><button type="button" disabled={future || outside} className={`yx-activity-cell level-${level} ${future ? 'is-future' : ''} ${outside ? 'is-outside' : ''}`} aria-label={`${label} ${date} ${count} 次活动${time ? `，最后 ${time}` : ''}${facts.bugs.length ? `，Bug ${facts.bugs.length}` : ''}`} onClick={() => select({ date, person: person.name })}>{future ? <span className="yx-activity-dot">·</span> : view === 'month' ? <span>{data.role === 'code' ? facts.bugs.length || '' : count || ''}</span> : <>{time && <time dateTime={facts.last}>{time}</time>}{facts.bugs.length > 0 && <span className="yx-activity-bug">Bug {facts.bugs.length}</span>}{!time && !facts.bugs.length && <span className="yx-activity-dot">·</span>}{expanded && data.role === 'code' && count > 0 && <><span className="yx-activity-lines">+{lineCount(facts.events, 'additions')} / −{lineCount(facts.events, 'deletions')}</span>{facts.requirements.length > 0 && <small>关联需求 {facts.requirements.length}</small>}</>}{expanded && data.role !== 'code' && count > 0 && <small>{count} 条记录</small>}</>}</button></Popover></td>
        })}
        {memberTotals(data, person).map((value, i) => <td key={i} className={`yx-activity-total ${totalHeaders[i] === 'Bug' && value > 0 ? 'has-bugs' : ''}`}>{totalHeaders[i] === 'Bug' && (!value || data.role === 'code' && !data.permissions.bugs) ? '—' : value.toLocaleString()}</td>)}
      </tr>
    })}</tbody>
  </table></div>
}
