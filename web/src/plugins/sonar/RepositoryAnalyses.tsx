import { useI18n } from '../../i18n'
import { useMemo, useState } from 'react'
import { Button, Card, Empty, Progress, Select, Space, Tag, Typography } from 'antd'
import { BranchesOutlined, CodeOutlined, ExportOutlined } from '@ant-design/icons'
import SonarQuality from '../yunxiao/SonarQuality'
import CommitEvidence, { type EvidenceMode } from './CommitEvidence'
import type { SonarAnalysis } from '../yunxiao/api'

const branchOf = (row: SonarAnalysis) => row.branch || row.push?.branch || ''
const repositoryName = (row: SonarAnalysis) => row.repositoryName || row.projectKey || row.repositoryId

function commitLink(row: SonarAnalysis) {
  if (!row.repositoryUrl || !/^[a-f0-9]{40}(?:[a-f0-9]{24})?$/i.test(row.revision)) return undefined
  try {
    const url = new URL(row.repositoryUrl)
    if (url.origin !== 'https://codeup.aliyun.com' || url.username || url.password) return undefined
    return url.origin + url.pathname.replace(/\/+$/, '') + '/commit/' + row.revision
  } catch {
    return undefined
  }
}

function sonarIssuesLink(row: SonarAnalysis) {
  if (!row.sonarUrl || !row.projectKey) return undefined
  try {
    const url = new URL(row.sonarUrl)
    if (!['https:', 'http:'].includes(url.protocol) || url.username || url.password) return undefined
    url.pathname = url.pathname.replace(/\/+$/, '') + '/project/issues'
    url.search = ''
    url.hash = ''
    url.searchParams.set('id', row.projectKey)
    const branch = branchOf(row)
    if (branch && row.push?.branch && !row.projectKey.includes(':branch:') && branch !== 'main' && branch !== 'master') {
      url.searchParams.set('branch', branch)
    }
    return url.href
  } catch {
    return undefined
  }
}

function IssueSnapshot({ row }: { row: SonarAnalysis }) {
  const { t } = useI18n()
  const overall = row.overallQuality
  const quality = row.analysisQuality
  const newIssues = quality?.newIssues ?? row.newIssues
  const values = [
    { name: t("engineering.overallIssues"), value: overall?.issues, color: 'var(--text-2)' },
    { name: t("engineering.newIssues"), value: newIssues, color: 'var(--brand)' },
    { name: t("engineering.acceptedIssuesOnNewCode"), value: quality?.acceptedIssues, color: 'var(--brand)' },
    { name: t("engineering.securityHotspotsOnNewCode"), value: quality?.securityHotspots, color: 'var(--warning)' },
  ]
  const max = Math.max(1, ...values.map(item => item.value ?? 0))
  return <div className="sonar-commit-issues" aria-label={t("engineering.commitIssueSnapshot")}>
    {values.map(({ name, value, color }) => <div key={name}>
      <div className="sonar-commit-issue-caption"><span>{name}</span><strong>{value ?? '—'}</strong></div>
      {value != null && <Progress size="small" percent={100 * value / max} showInfo={false} strokeColor={color} trailColor="var(--border-soft)" />}
    </div>)}
  </div>
}

export default function RepositoryAnalyses({ items, loading, repository, source, onRepositoryChange }: {
  items: SonarAnalysis[]
  source: string
  loading: boolean
  repository?: string
  onRepositoryChange: (repository?: string) => void
}) {
  const { t, lang } = useI18n()
  const shanghai = new Intl.DateTimeFormat(lang, { timeZone: 'Asia/Shanghai', dateStyle: 'short', timeStyle: 'short' })
  const [evidence, setEvidence] = useState<EvidenceMode>()
  const [selection, setSelection] = useState<{ repo: string; branch?: string; revision?: string; task?: string }>()
  const grouped = useMemo(() => {
    const byRepository = new Map<string, SonarAnalysis[]>()
    for (const row of items) {
      const rows = byRepository.get(row.repositoryId)
      if (rows) rows.push(row)
      else byRepository.set(row.repositoryId, [row])
    }
    return byRepository
  }, [items])
  const repo = repository && grouped.has(repository) ? repository : items[0]?.repositoryId
  const selected = selection?.repo === repo ? selection : undefined
  const repositoryRows = (repo && grouped.get(repo)) || []
  const repositories = Array.from(grouped.entries(), ([id, rows]) => ({
    value: id, label: repositoryName(rows.find(row => row.repositoryName) || rows[0]),
  }))
  const branches = [...new Set(repositoryRows.map(branchOf))].map(branch => ({ value: branch, label: branch || t("engineering.unknownBranch") }))
  const branch = branches.some(option => option.value === selected?.branch) ? selected?.branch : branches[0]?.value
  const branchRows = repositoryRows.filter(row => branchOf(row) === branch)
  const revisions = [...new Set(branchRows.map(row => row.revision))]
  const revision = selected?.revision && revisions.includes(selected.revision) ? selected.revision : revisions[0]
  const scans = branchRows.filter(row => row.revision === revision)
  const snapshot = scans.find(row => row.taskId === selected?.task) || scans[0]
  const codeLink = snapshot && commitLink(snapshot)
  const issuesLink = snapshot && sonarIssuesLink(snapshot)

  return <Card title={t("engineering.repositoriesAndCommits")} size="small" loading={loading}>
    {!repo || !snapshot ? <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t("engineering.noMatchingScans")} /> : <>
      <div className="sonar-commit-controls">
        <Select aria-label={t("engineering.browseRepositories")} value={repo} options={repositories} showSearch optionFilterProp="label" onChange={value => {
          onRepositoryChange(value)
          setSelection({ repo: value })
        }} />
        <Select aria-label={t("engineering.browseBranches")} value={branch} options={branches} prefix={<BranchesOutlined />} showSearch optionFilterProp="label"
          onChange={value => setSelection({ repo, branch: value })} />
        <Select aria-label={t("engineering.browseCommits")} value={revision} showSearch optionFilterProp="label"
          options={revisions.map(id => ({ value: id, label: id.slice(0, 12) }))}
          onChange={value => setSelection({ repo, branch, revision: value })} />
        {scans.length > 1 && <Select aria-label={t("engineering.browseAnalysisRuns")} value={snapshot.taskId}
          options={scans.map(scan => ({ value: scan.taskId, label: shanghai.format(new Date(scan.analyzedAt)) }))}
          onChange={value => setSelection({ repo, branch, revision, task: value })} />}
      </div>
      <div className="sonar-commit-header">
        <Space wrap>
          <Typography.Text strong>{repositoryName(snapshot)}</Typography.Text>
          <Typography.Text code copyable>{snapshot.revision}</Typography.Text>
          <Tag>{t('engineering.analysisCount', undefined, { count: scans.length })}</Tag>
        </Space>
        <Space wrap>
          {snapshot.scope === 'sonar_repository' && source && <>
            <Button size="small" icon={<CodeOutlined />} onClick={() => setEvidence('diff')}>{t("engineering.viewCommitDiff")}</Button>
            <Button size="small" icon={<ExportOutlined />} onClick={() => setEvidence('issues')}>{t("engineering.sonarqubeIssues")}</Button>
          </>}
        </Space>
      </div>
      <IssueSnapshot row={snapshot} />
      {evidence && <CommitEvidence key={snapshot.taskId + evidence} mode={evidence} source={source} row={snapshot}
        onClose={() => setEvidence(undefined)} codeUrl={codeLink} issuesUrl={issuesLink} />}
      <SonarQuality items={scans} loading={false} title={t("engineering.analysisDetailsForThisCommit")} />
    </>}
  </Card>
}
