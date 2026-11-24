import { useI18n } from '../../i18n'
import { useLayoutEffect, useMemo, useRef } from 'react'
import { Button, Popover, Tooltip } from 'antd'
import { WarningFilled } from '@ant-design/icons'
import type { ActivityKind, ActivityMember, TeamActivityData } from './api'
import { activityRoles, datesFor, dayFacts, lineCount, memberTotals, passRateText } from './activityModel'
import { attribution, memberLabel, type ActivityReview } from './activityReview'
import { dateTime } from './codeActivity'
import type { ActivitySelection } from './ActivityDetails'
import { SonarNewCodeMetrics, sonarRates } from './SonarNewCodeMetrics'
export default function ActivityMatrix({ data, members, metric, view, expanded, colorByLines, reviews, select, associate }: { data: TeamActivityData; members: ActivityMember[]; metric: ActivityKind; view: string; expanded: boolean; colorByLines: boolean; reviews: Map<string, ActivityReview>; select: (value: ActivitySelection) => void; associate: () => void }) {
  const { t, lang } = useI18n()
  const weekdayFormat = new Intl.DateTimeFormat(lang, { timeZone: 'UTC', weekday: 'short' })
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
  const quality = useMemo(() => {
    const summaries = new Map<string, { sonarPassed: number; sonarEvaluated: number; regressionPassed: number; regressionEvaluated: number }>()
    const cells = new Map<string, { sonar?: NonNullable<TeamActivityData['sonarDaily']>[number]; regression?: NonNullable<TeamActivityData['regressionDaily']>[number] }>()
    const summary = (member: string) => {
      const value = summaries.get(member) ?? { sonarPassed: 0, sonarEvaluated: 0, regressionPassed: 0, regressionEvaluated: 0 }
      summaries.set(member, value)
      return value
    }
    for (const sonar of data.sonarDaily ?? []) {
      cells.set(JSON.stringify([sonar.member, sonar.date]), { sonar })
      const totals = summary(sonar.member)
      totals.sonarPassed += sonar.passed
      totals.sonarEvaluated += sonar.passed + sonar.failed
    }
    for (const regression of data.regressionDaily ?? []) {
      const key = JSON.stringify([regression.member, regression.date])
      cells.set(key, { ...cells.get(key), regression })
      const totals = summary(regression.member)
      totals.regressionPassed += regression.passed
      totals.regressionEvaluated += regression.evaluated
    }
    return { cells, summaries }
  }, [data.sonarDaily, data.regressionDaily])
  const totalHeaders = data.role === 'code' ? ['commit', 'requirements', 'Bug'] : data.role === 'product' ? ['added', 'delivered', 'requirements'] : ['testCases', 'Bug', 'activity']
  const openMember = (person: ActivityMember) => {
    const last = person.lastActivityAt ? dateTime(person.lastActivityAt).slice(0, 10) : ''
    select(last >= data.from && last <= data.through ? { date: last, person: person.name } : { person: person.name, history: true })
  }
  return <div className="yx-activity-table-scroll"><table ref={ref} className={`yx-activity-matrix ${view === 'month' ? 'is-month' : ''} ${expanded ? 'is-expanded' : ''}`} aria-label={t("engineering.dailyMemberActivityHeatmap")}>
    <thead><tr><th className="yx-activity-name">{t("engineering.memberOriginalAccount")}</th>{dates.map(date => { const weekday = new Date(`${date}T00:00:00Z`).getUTCDay(); return <th key={date} className={weekday === 0 || weekday === 6 ? 'is-weekend' : ''}><button type="button" aria-label={t("engineering.viewAllMembersOnV0", undefined, { v0: date })} onClick={() => select({ date })}><time dateTime={date}>{date.slice(5).replace('-', '/')}</time><small>{weekdayFormat.format(new Date(date + 'T00:00:00Z'))}</small></button></th> })}{totalHeaders.map((label, i) => <th key={label} className="yx-activity-total"><Tooltip title={i === 1 && data.role === 'code' ? t("engineering.uniqueLinkedRequirementsInTheReportingPeriod") : label === 'Bug' && data.role === 'code' ? t("engineering.uniqueNewlyAssignedDefectsInTheReportingPeriod") : t("engineering.totalForTheSelectedPeriod")}>{label === 'Bug' ? label : t('engineering.' + label)}</Tooltip></th>)}{data.role === 'code' && <th className="yx-activity-total"><Tooltip title={t("engineering.sonarNewCodeGatePassRateInTheReportingPeriod")}>Sonar</Tooltip></th>}{data.role === 'code' && <th className="yx-activity-total"><Tooltip title={t("engineering.passedTestRunsDividedByRunsWithResults")}>{t("engineering.regression")}</Tooltip></th>}</tr></thead>
    <tbody>{members.map(person => {
      const review = reviews.get(person.name), label = memberLabel(person), state = attribution(person)
      const qualityTotals = quality.summaries.get(person.name)
      return <tr key={person.name} className={review ? 'needs-review' : ''}><td className="yx-activity-name"><div className="yx-activity-name-content"><Tooltip title={<><strong>{label} · {person.group}</strong>{state === 'unmappedAccount' && <div>{t("engineering.theGitAuthorIsUnlinkedTheirCommitsAndLinesAreIncluded")}</div>}{state === 'unknownAuthor' && <div>{t("engineering.gitAuthorInformationIsMissingTheseCommitsAreGroupedHere")}</div>}{review && <><div>{review.summary}</div>{review.reasons.map(reason => <div key={reason}>{reason}</div>)}</>}</>}><button type="button" className="yx-activity-member-name" aria-label={`${label} · ${review ? t("engineering.reviewNeeded") : t("engineering.latestActivity")}`} onClick={() => openMember(person)}>{review && <WarningFilled />}<span>{label}</span></button></Tooltip>{state === 'unmappedAccount' && <Button type="link" size="small" className="yx-activity-associate" aria-label={t("engineering.linkAccountV0", undefined, { v0: label })} onClick={associate}>{t("engineering.link")}</Button>}</div></td>
        {dates.map(date => {
          const facts = dayFacts(data, person.name, date, metric), count = facts.events.length, future = date > data.through, outside = date < data.from, weekend = [0, 6].includes(new Date(`${date}T00:00:00Z`).getUTCDay())
          const { sonar, regression } = quality.cells.get(JSON.stringify([person.name, date])) ?? {}
          const showQuality = data.role === 'code' && !future && !outside
          const sonarRate = sonar ? passRateText(sonar.passed, sonar.passed + sonar.failed) : '—'
          const { coverage, duplication } = sonarRates(sonar?.recent?.[0])
          const hasScan = !!sonar?.recent?.length
          const regressionRate = regression ? passRateText(regression.passed, regression.evaluated) : '—'
          const qualityLabel = showQuality
            ? `，Sonar 门禁 ${sonarRate}${sonar ? `，扫描 ${sonar.pushes} 次` : ''}${hasScan ? `，覆盖率 ${coverage}，重复率 ${duplication}` : ''}，回归用例通过率 ${regressionRate}`
            : ''
          const knownLines = colorByLines ? facts.events.reduce((total, event) => total + (event.additions ?? 0) + (event.deletions ?? 0), 0) : 0
          const level = colorByLines ? knownLines >= 10_000 ? 4 : knownLines >= 2_000 ? 3 : knownLines >= 500 ? 2 : knownLines ? 1 : 0
            : count > 9 ? 4 : count > 5 ? 3 : count > 2 ? 2 : count ? 1 : 0
          const time = facts.last ? dateTime(facts.last).slice(11, 16) : ''
          const tip = <div className="yx-activity-hover"><strong>{label} · {date}</strong><span>{count} 次{roleInfo.metrics.find(m => m.value === metric)?.label}{facts.last ? t("engineering.lastAtV0", undefined, { v0: dateTime(facts.last).slice(11) }) : ''}</span>{data.role === 'code' && <><span>{t('engineering.linesChanged', undefined, { added: lineCount(facts.events, 'additions'), deleted: lineCount(facts.events, 'deletions') })}</span>{facts.bugs.length > 0 && <span>新增责任 Bug {facts.bugs.length}</span>}<span>{facts.requirements.length ? t("engineering.v0LinkedRequirements", undefined, { v0: facts.requirements.length }) : t("engineering.noExplicitlyLinkedRequirements")}</span>{facts.requirements.slice(0, 3).map(r => <span key={r.externalId}>{r.serial || r.externalId} {r.title || ''}</span>)}{facts.requirements.length > 3 && <small>另 {facts.requirements.length - 3} 项，点击查看</small>}</>}</div>
          const compact = view === 'month'
          return <td key={date} className={`yx-activity-day level-${future || outside ? 0 : level}${weekend ? ' is-weekend' : ''}`}>
            <Popover content={<>{tip}{showQuality && <div className="yx-activity-hover">
              <span>{sonar ? t("engineering.sonarGateV0V1V2ScansV3", undefined, { v0: sonarRate, v1: sonar.passed, v2: sonar.passed + sonar.failed, v3: sonar.pushes }) : t("engineering.sonarNoScans")}</span>
              <span>{regression ? t("engineering.regressionPassRateV0V1V2", undefined, { v0: regressionRate, v1: regression.passed, v2: regression.evaluated }) : t("engineering.regressionNoTestResults")}</span>
            </div>}{sonar?.recent?.[0] && <SonarNewCodeMetrics analysis={sonar.recent[0]} />}</>} trigger={['hover', 'focus']} mouseEnterDelay={0.15}>
              <button type="button" disabled={future || outside}
                className={`yx-activity-cell level-${level}${showQuality ? ' shows-quality' : ''}${future ? ' is-future' : ''}${outside ? ' is-outside' : ''}`}
                aria-label={`${label} ${date} ${t('engineering.activityCount', undefined, { count })}${time ? t("engineering.lastAtV0", undefined, { v0: time }) : ''}${facts.bugs.length ? `，Bug ${facts.bugs.length}` : ''}${qualityLabel}`}
                onClick={() => select({ date, person: person.name })}>
                {future || outside ? <span className="yx-activity-dot">·</span> : compact && data.role !== 'code' ? <span>{count || ''}</span> : <>
                  {(time || facts.bugs.length > 0) &&
                    <span className="yx-activity-cell-summary">
                      {data.role === 'code' && count > 0 && <span className="yx-activity-commit-count">{t('engineering.commitActivityCount', undefined, { count })}</span>}
                      {time && <time dateTime={facts.last}>{time}</time>}
                      {facts.bugs.length > 0 && <span className="yx-activity-bug">Bug {facts.bugs.length}</span>}
                    </span>}
                  {!time && !facts.bugs.length && !showQuality && <span className="yx-activity-dot">·</span>}
                  {showQuality && <span className="yx-activity-quality">
                    <span className={sonarRate === '—' ? 'is-missing' : sonar?.failed ? 'has-failure' : ''}>
                      <span>{compact ? 'S' : 'Sonar'}</span> <b>{sonarRate}</b>
                    </span>
                    <span className={regression && regression.evaluated > 0 ? regression.passed < regression.evaluated ? 'has-failure' : '' : 'is-missing'}>
                      <span>{compact ? t('engineering.regressionShort') : t('engineering.regression')}</span> <b>{regressionRate}</b>
                    </span>
                  </span>}
                  {showQuality && hasScan && <span className="yx-activity-quality" title={t("engineering.latestSonarScanForTheDay")}>
                    <span className={coverage === '—' ? 'is-missing' : ''}>{compact ? t('engineering.coverageShort') : t("engineering.coverage")} <b>{coverage}</b></span>
                    <span className={duplication === '—' ? 'is-missing' : ''}>{compact ? t('engineering.duplicationShort') : t("engineering.duplication")} <b>{duplication}</b></span>
                  </span>}
                  {data.role === 'code' && count > 0 && <span className="yx-activity-lines">
                    <span>+{lineCount(facts.events, 'additions')}</span>
                    <span>−{lineCount(facts.events, 'deletions')}</span>
                  </span>}
                  {expanded && data.role === 'code' && facts.requirements.length > 0 &&
                    <small>{compact ? t("engineering.requirements") : t("engineering.linkedRequirements")} {facts.requirements.length}</small>}
                  {expanded && data.role !== 'code' && count > 0 && <small>{t('engineering.v0Records', undefined, { v0: count })}</small>}
                </>}
              </button>
            </Popover>
          </td>
        })}
        {memberTotals(data, person).map((value, i) => <td key={i} className={`yx-activity-total ${totalHeaders[i] === 'Bug' && value > 0 ? 'has-bugs' : ''}`}>{totalHeaders[i] === 'Bug' && (!value || data.role === 'code' && !data.permissions.bugs) ? '—' : value.toLocaleString()}</td>)}
        {data.role === 'code' && <>
          <td className="yx-activity-total">{qualityTotals ? passRateText(qualityTotals.sonarPassed, qualityTotals.sonarEvaluated) : '—'}</td>
          <td className="yx-activity-total">{qualityTotals ? passRateText(qualityTotals.regressionPassed, qualityTotals.regressionEvaluated) : '—'}</td>
        </>}
      </tr>
    })}</tbody>
  </table></div>
}
