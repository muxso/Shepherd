import { useI18n } from '../../i18n'
import type { SonarAnalysis, SonarCondition } from './api'
import './sonar-new-code.css'

const integer = new Intl.NumberFormat('en', { maximumFractionDigits: 0 })
const short = new Intl.NumberFormat('en', { notation: 'compact', maximumFractionDigits: 1 })
const count = (value?: number | null) => value == null ? '—' : integer.format(value)
const percent = (value?: number | null) => value == null ? '—' : `${value.toFixed(1)}%`
export function sonarRates(analysis?: SonarAnalysis) {
  return {
    coverage: percent(analysis?.analysisQuality?.coverage ?? analysis?.newCoverage),
    duplication: percent(analysis?.analysisQuality?.duplication ?? analysis?.newDuplication),
  }
}
const operators: Record<string, string> = { LT: '≥', GT: '≤', NE: '=', EQ: '≠', LTE: '>', GTE: '<', LESS_THAN: '≥', GREATER_THAN: '≤', NOT_EQUALS: '=', EQUALS: '≠' }

function required(label: string, condition?: SonarCondition, percentage = false) {
  if (condition?.threshold == null || condition.threshold === '') return null
  const value = Number(condition.threshold)
  const comparator = operators[condition.comparator ?? '']
  const op = !percentage && value === 0 && comparator === '≤' ? '=' : comparator
  return Number.isFinite(value) && op ? `${label} ${op} ${percentage ? percent(value) : count(value)}` : null
}

export function SonarNewCodeMetrics({ analysis }: { analysis: SonarAnalysis }) {
  const { t } = useI18n()
  const requiredLabel = t('sonar.required', '门禁要求')
  const failedLabel = t('sonar.failed', '未达标')
  const q = analysis.analysisQuality
  const code = analysis.newCode
  const conditions = q?.conditions
  const linesToCover = q?.linesToCover ?? code?.linesToCover
  const lines = q?.lines ?? code?.lines
  const branch = analysis.branch || analysis.push?.branch
  const overall = analysis.overallQuality
  const hasNewCode = [q?.newIssues ?? analysis.newIssues, q?.acceptedIssues ?? code?.acceptedIssues,
    q?.coverage ?? analysis.newCoverage, q?.duplication ?? analysis.newDuplication,
    q?.securityHotspots ?? code?.securityHotspots, lines, linesToCover].some(value => value != null)
  return <section className="yx-sonar-analysis" aria-label="Sonar 新代码质量">
    <header><strong>{analysis.repositoryName || analysis.projectKey || analysis.repositoryId}{branch ? ` / ${branch}` : ''}</strong>
      <code>{analysis.revision.slice(0, 12)}</code>
      <small>{analysis.gate === 'OK' ? '门禁通过' : analysis.gate === 'ERROR' ? '门禁未通过' : '门禁待评价'}</small>
      {analysis.push?.baselineVerified === false && <small>Sonar 项目基线 · Push 基线待核验</small>}
    </header>
    {!hasNewCode && <p className="yx-sonar-data-note">本次分析没有可用的新代码指标。</p>}
    {hasNewCode && <dl className="yx-sonar-metrics">
      <div><dt>{t('sonar.newIssues', '新增问题')} {conditions?.new_violations?.status === 'ERROR' && <strong className="yx-sonar-condition-failed">{failedLabel}</strong>}</dt><dd>{count(q?.newIssues ?? analysis.newIssues)}</dd>
        {required(requiredLabel, conditions?.new_violations) && <small>{required(requiredLabel, conditions?.new_violations)}</small>}</div>
      <div><dt>{t('sonar.acceptedIssues', '已接受问题')}</dt><dd>{count(q?.acceptedIssues ?? code?.acceptedIssues)}</dd>
        <small>{t('sonar.acceptedHint', '已确认且暂不修复的问题')}</small></div>
      <div><dt>{t('sonar.coverage', '覆盖率')} {conditions?.new_coverage?.status === 'ERROR' && <strong className="yx-sonar-condition-failed">{failedLabel}</strong>}</dt>
        <dd>{percent(q?.coverage ?? analysis.newCoverage)}</dd>
        {required(requiredLabel, conditions?.new_coverage, true) && <small>{required(requiredLabel, conditions?.new_coverage, true)}</small>}
        <small>{t('sonar.linesToCover', '待覆盖新代码行数')} <b>{linesToCover == null ? '—' : short.format(linesToCover)}</b></small></div>
      <div><dt>{t('sonar.duplication', '重复率')} {conditions?.new_duplicated_lines_density?.status === 'ERROR' && <strong className="yx-sonar-condition-failed">{failedLabel}</strong>}</dt>
        <dd>{percent(q?.duplication ?? analysis.newDuplication)}</dd>
        {required(requiredLabel, conditions?.new_duplicated_lines_density, true) && <small>{required(requiredLabel, conditions?.new_duplicated_lines_density, true)}</small>}
        <small>{t('sonar.newLines', '新代码行数')} <b>{lines == null ? '—' : short.format(lines).toLowerCase()}</b></small></div>
      <div><dt>{t('sonar.hotspots', '安全热点')} <small>{t('sonar.deprecated', '已弃用')}</small></dt><dd>{count(q?.securityHotspots ?? code?.securityHotspots)}</dd></div>
    </dl>}
    {overall && !hasNewCode && <>
      <h4 className="yx-sonar-data-note">整体代码质量</h4>
      <dl className="yx-sonar-metrics">
        <div><dt>问题总数</dt><dd>{count(overall.issues)}</dd></div>
        <div><dt>已接受问题</dt><dd>{count(overall.acceptedIssues)}</dd></div>
        <div><dt>覆盖率</dt><dd>{percent(overall.coverage)}</dd></div>
        <div><dt>重复率</dt><dd>{percent(overall.duplication)}</dd></div>
        <div><dt>安全热点</dt><dd>{count(overall.securityHotspots)}</dd></div>
      </dl>
    </>}
  </section>
}
