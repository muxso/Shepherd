import { useI18n } from '../../i18n'
import { useEffect, useState } from 'react'
import { Alert, Button, Drawer, Empty, Input, Pagination, Segmented, Skeleton, Tag, Typography } from 'antd'
import { FileTextOutlined, LinkOutlined, SearchOutlined } from '@ant-design/icons'
import { ApiError, request } from '../../api'
import type { SonarAnalysis } from '../yunxiao/api'

type DiffFile = { path?: string; oldPath?: string; diff: string; binary?: boolean; clipped?: boolean; added?: boolean; deleted?: boolean }
type Diff = { revision: string; title?: string; parent?: string; status: 'ready' | 'no_parent'; files: DiffFile[]; truncated?: boolean }
type Issue = { key: string; rule: string; severity: string; type: string; status: string; message: string; file: string; line?: number }
type Issues = { status: 'ready' | 'historical'; items: Issue[]; total: number; page?: number }
export type EvidenceMode = 'diff' | 'issues'
const PAGE_SIZE = 25

const issueCategoryKeys = { BUG: 'reliabilityBugs', VULNERABILITY: 'securityVulnerabilities', CODE_SMELL: 'maintainabilityIssues' } as const
type IssueCategory = keyof typeof issueCategoryKeys | ''
const severityKeys: Record<string, string> = {
  BLOCKER: 'blocker', CRITICAL: 'critical', MAJOR: 'major', MINOR: 'minor', INFO: 'info',
  HIGH: 'high', MEDIUM: 'medium', LOW: 'low',
}
const statusKeys: Record<string, string> = {
  OPEN: 'open', CONFIRMED: 'confirmed', ACCEPTED: 'accepted', FIXED: 'fixed', FALSE_POSITIVE: 'falsePositive',
  RESOLVED: 'resolved', REOPENED: 'reopened', CLOSED: 'closed',
}
const issuePath = (issue: Issue) => issue.file + (issue.line == null ? '' : ':' + issue.line)
const filePath = (file: DiffFile, index: number) => file.path || file.oldPath || String(index + 1)

function diffRows(diff: string) {
  let oldLine: number | null = null
  let newLine: number | null = null
  return diff.replace(/\n$/, '').split('\n').map(text => {
    const match = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(text)
    if (match) {
      oldLine = Number(match[1])
      newLine = Number(match[2])
      return { kind: 'hunk', text }
    }
    if (oldLine == null || newLine == null || text.startsWith('\\')) return { kind: 'metadata', text }
    if (text.startsWith('+')) return { kind: 'added', text: text.slice(1), new: newLine++, marker: '+' }
    if (text.startsWith('-')) return { kind: 'removed', text: text.slice(1), old: oldLine++, marker: '−' }
    if (text.startsWith(' ')) return { kind: 'context', text: text.slice(1), old: oldLine++, new: newLine++ }
    return { kind: 'metadata', text }
  })
}

function DiffPreview({ file }: { file: DiffFile }) {
  const { t } = useI18n()
  const rows = diffRows(file.diff)
  const additions = rows.filter(line => line.kind === 'added').length
  const deletions = rows.filter(line => line.kind === 'removed').length
  return <>
    <div className="sonar-diff-summary">
      <span><span className="sonar-line-count-added">+{additions}</span> <span className="sonar-line-count-removed">−{deletions}</span></span>
      {file.clipped && <Typography.Text type="secondary">{t("engineering.onlyAPreviewOfThisFileIsShown")}</Typography.Text>}
    </div>
    <div className="sonar-inline-diff" aria-label={t("engineering.commitDiffContent")}>
      <div className="sonar-diff-columns" aria-hidden="true">
        <span>{t("engineering.old")}</span><span>{t("engineering.new")}</span><span /><span>{t("engineering.diff")}</span>
      </div>
      <div className="sonar-diff-lines">
        {rows.map((line, index) => <div key={index} className={'sonar-diff-line ' + line.kind}>
          {line.kind === 'hunk' || line.kind === 'metadata' ? <code>{line.text || ' '}</code> : <>
            <span aria-hidden="true">{line.old}</span><span aria-hidden="true">{line.new}</span>
            <span aria-hidden="true">{line.marker}</span><code>{line.text || ' '}</code>
          </>}
        </div>)}
      </div>
    </div>
  </>
}

function IssueList({ issues, page, onPage, category }: { issues: Issues; page: number; onPage: (page: number) => void; category: IssueCategory }) {
  const { t } = useI18n()
  if (!issues.items.length) return <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={category ? t("engineering.noIssuesInThisCategory") : t("engineering.noIssuesForThisAnalysis")} />
  return <>
    <div className="sonar-evidence-section-heading">
      <strong>{t("engineering.issueList")}</strong><Typography.Text type="secondary">{t('engineering.issueCount', undefined, { count: issues.total })}</Typography.Text>
    </div>
    <ul className="sonar-issue-list" aria-label={t("engineering.sonarqubeIssueList")}>
      {issues.items.map(issue => <li key={issue.key}>
        <div className="sonar-issue-heading">
          <strong>{issue.message}</strong>
          {issue.severity && <Tag className={'sonar-issue-severity ' + (
            ['BLOCKER', 'CRITICAL', 'HIGH'].includes(issue.severity) ? 'is-high' :
              ['MAJOR', 'MEDIUM'].includes(issue.severity) ? 'is-medium' : ''
          )}>{severityKeys[issue.severity] ? t('engineering.' + severityKeys[issue.severity]) : issue.severity}</Tag>}
        </div>
        <div className="sonar-issue-location"><FileTextOutlined aria-hidden="true" /><code>{issuePath(issue) || t("engineering.unknownFile")}</code></div>
        <div className="sonar-issue-meta">
          {issue.type && <span>{t('engineering.' + (issueCategoryKeys[issue.type as keyof typeof issueCategoryKeys] ?? 'otherType'))}</span>}
          {issue.rule && <span>{t("engineering.rule")} <code>{issue.rule}</code></span>}
          {issue.status && <span>{statusKeys[issue.status] ? t('engineering.' + statusKeys[issue.status]) : issue.status}</span>}
        </div>
      </li>)}
    </ul>
    <div className="sonar-issue-pagination">
      {issues.total > 2500 && <Typography.Text type="secondary">{t("engineering.showingTheFirst2500Issues")}</Typography.Text>}
      <Pagination size="small" current={page} pageSize={PAGE_SIZE} total={Math.min(issues.total, 2500)}
        showSizeChanger={false} onChange={onPage} />
    </div>
  </>
}

function failureMessage(error: unknown, mode: EvidenceMode, t: ReturnType<typeof useI18n>['t']) {
  if (error instanceof ApiError) {
    if (error.status === 403) return mode === 'diff' ? t("engineering.permissionToViewCommitDiffsIsRequired") : t("engineering.permissionToViewCodeQualityIsRequired")
    if (error.status === 413) return t("engineering.theDiffIsTooLargeToDisplayHereViewTheFullDiffInCodeup")
    if (error.status === 404) return t("engineering.theSourceCouldNotFindThisRecord")
    if (error.status === 503) return t("engineering.anAdministratorMustConfigureTheApiForThisSource")
  }
  return t("engineering.couldNotLoadDetailsPleaseTryAgain")
}

export default function CommitEvidence({ source, row, mode, onClose, codeUrl, issuesUrl }: {
  source: string
  row: SonarAnalysis
  mode: EvidenceMode
  onClose: () => void
  codeUrl?: string
  issuesUrl?: string
}) {
  const { t } = useI18n()
  const [data, setData] = useState<Diff | Issues>()
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(true)
  const [page, setPage] = useState(1)
  const [category, setCategory] = useState<IssueCategory>('')
  const [fileIndex, setFileIndex] = useState(0)
  const [fileQuery, setFileQuery] = useState('')

  useEffect(() => {
    let current = true
    const path = '/api/sonar/sources/' + encodeURIComponent(source) + '/analyses/' +
      encodeURIComponent(row.taskId) + '/' + mode + (mode === 'issues' ? '?page=' + page + (category ? '&category=' + category : '') : '')
    setLoading(true)
    setError('')
    request<Diff | Issues>('GET', path).then(result => { if (current) setData(result) })
      .catch(cause => {
        if (!current) return
        setData(undefined)
        setError(failureMessage(cause, mode, t))
      }).finally(() => { if (current) setLoading(false) })
    return () => { current = false }
  }, [source, row.taskId, mode, page, category, t])

  const diff = data && 'files' in data ? data : null
  const file = diff?.files[fileIndex]
  const fileEntries = (diff?.files ?? []).map((file, index) => ({ file, index, path: filePath(file, index) }))
    .filter(entry => entry.path.toLowerCase().includes(fileQuery.trim().toLowerCase()))
  const issues = data && 'items' in data ? data : null
  const loadingState = <div role="status" aria-busy="true" aria-label={t('common.loading', '加载中')}>
    <Skeleton title paragraph={{ rows: 7 }} />
  </div>
  const external = mode === 'diff' ? codeUrl : issuesUrl

  return <Drawer open width="min(1120px, 100vw)" onClose={onClose}
    className="sonar-evidence-drawer"
    title={<div className="sonar-evidence-title"><span>{mode === 'diff' ? t("engineering.commitDiff") : t("engineering.sonarqubeIssues")}</span>
      <span className="sonar-evidence-subtitle">
        {row.repositoryName || row.projectKey || row.repositoryId}<Typography.Text code>{row.revision.slice(0, 12)}</Typography.Text>
      </span></div>}
    extra={external && <Button size="small" icon={<LinkOutlined />} href={external} target="_blank" rel="noreferrer">{mode === 'diff' ? t("engineering.openInCodeup") : t("engineering.openInSonarqube")}</Button>}>
    {loading && issues?.status !== 'ready' && loadingState}
    <div>
      {error && <Alert type="warning" showIcon message={error} />}
      {diff && <>
        <div className="sonar-evidence-commit-summary">
          <Typography.Title level={5}>{diff.title || t("engineering.changedFiles")}</Typography.Title>
          {diff.parent && <Typography.Text type="secondary">{t('engineering.compareParent', undefined, { revision: diff.parent.slice(0, 12) })}</Typography.Text>}
        </div>
        {diff.status === 'no_parent' && <Alert type="info" showIcon message={t("engineering.theInitialCommitHasNoParentToCompare")} />}
        {diff.truncated && <Alert type="info" showIcon message={t("engineering.thisDiffIsLargeViewTheFullDiffInCodeup")} />}
        {diff.files.length > 0 && <div className="sonar-diff-layout">
          <aside className="sonar-diff-files" aria-label={t("engineering.changedFiles")}>
            <div className="sonar-evidence-section-heading"><strong>{t("engineering.changedFiles")}</strong><Typography.Text type="secondary">{t('engineering.fileCount', undefined, { count: diff.files.length })}</Typography.Text></div>
            <Input aria-label={t("engineering.searchChangedFiles")} placeholder={t("engineering.searchFilePaths")} prefix={<SearchOutlined />} size="small" allowClear
              value={fileQuery} onChange={event => setFileQuery(event.target.value)} />
            <ul>
              {fileEntries.map(entry => <li key={entry.index}>
                <button type="button" aria-current={entry.index === fileIndex ? 'true' : undefined} title={entry.path}
                  onClick={() => setFileIndex(entry.index)}>
                  <FileTextOutlined aria-hidden="true" />
                  <span>{entry.path}</span>
                  {entry.file.added && <span className="sonar-line-count-added" aria-label={t("engineering.added")}>+</span>}
                  {entry.file.deleted && <span className="sonar-line-count-removed" aria-label={t("engineering.deleted")}>−</span>}
                </button>
              </li>)}
              {!fileEntries.length && <li className="sonar-file-empty">{t("engineering.noMatchingFiles")}</li>}
            </ul>
          </aside>
          {file && <section className="sonar-inline-file" aria-label={t("engineering.fileDiff")}>
            <div className="sonar-inline-file-heading">
              <code>{filePath(file, fileIndex)}</code>
              {file.added && <Tag className="sonar-file-status is-added">{t("engineering.added")}</Tag>}
              {file.deleted && <Tag className="sonar-file-status is-removed">{t("engineering.deleted")}</Tag>}
              {file.oldPath && file.path && file.oldPath !== file.path && <Typography.Text type="secondary">{t('engineering.originalPath', undefined, { path: file.oldPath })}</Typography.Text>}
            </div>
            {file.binary ? <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t("engineering.binaryFileChange")} /> : file.diff
              ? <DiffPreview file={file} />
              : <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t("engineering.noTextDiffToDisplay")} />}
          </section>}
        </div>}
        {diff.status === 'ready' && !diff.files.length && <Empty description={t("engineering.noChangedFiles")} />}
      </>}
      {issues?.status === 'historical' && <Alert type="info" showIcon
        message={t("engineering.historicalMetricsAreAvailableInTheCommitSnapshotSonarqubeProvidesTheCurrentIssueListForTheProjectOrBranch")} />}
      {issues?.status === 'ready' && <>
        <div className="sonar-issue-category">
          <Segmented<IssueCategory> aria-label={t("engineering.issueCategory")} value={category} options={([{ value: '', label: t('engineering.all') }, ...Object.entries(issueCategoryKeys).map(([value, key]) => ({ value: value as IssueCategory, label: t('engineering.' + key) }))])}
            onChange={value => {
              setCategory(value)
              setPage(1)
            }} />
        </div>
        {loading ? loadingState : <IssueList issues={issues} page={page} onPage={setPage} category={category} />}
      </>}
    </div>
  </Drawer>
}
