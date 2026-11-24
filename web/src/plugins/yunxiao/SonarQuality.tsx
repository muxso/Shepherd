import { useI18n } from '../../i18n'
import { Card, Table, Tag } from 'antd'
import type { SonarAnalysis } from './api'
import { SonarNewCodeMetrics } from './SonarNewCodeMetrics'

const percent = (value: number | null) => value == null ? '—' : `${value}%`

export default function SonarQuality({ items, loading, title }: { items: SonarAnalysis[]; loading: boolean; title: string }) {
  const { t, lang } = useI18n()
  const analysisDate = new Intl.DateTimeFormat(lang, { dateStyle: 'short', timeStyle: 'short', timeZone: 'Asia/Shanghai' })
  const gates = { OK: t("engineering.passed"), ERROR: t("engineering.failed"), WARN: t("engineering.warning"), NONE: t("engineering.noGateResult") }
  return <Card title={title} size="small">
    <Table
      rowKey={row => `${row.scope}:${row.taskId}`}
      size="small"
      loading={loading}
      dataSource={items}
      locale={{ emptyText: t("engineering.noQualityReports") }}
      scroll={{ x: 1100 }}
      expandable={{
        rowExpandable: row => row.scope === 'sonar_repository',
        expandedRowRender: row => <SonarNewCodeMetrics analysis={row} />,
      }}
      columns={[
        { title: t("engineering.reportSource"), render: (_, row) => row.scope === 'sonar_repository' ? 'SonarQube' : t("engineering.ciPrCheck") },
        { title: t("engineering.repositoryBranch"), render: (_, row) => {
          const branch = row.branch || row.push?.branch
          return <div><strong>{row.repositoryName || row.projectKey || row.repositoryId}</strong>
            <div style={{ color: 'var(--text-3)', fontSize: 12 }}>{branch || `ID ${row.repositoryId}`}</div>
          </div>
        } },
        { title: t("engineering.person"), render: (_, row) => {
          const name = row.personName || (row.scope === 'ci_pull_request' ? row.author : row.push?.author || row.commitAuthor)
          const role = row.scope === 'ci_pull_request' ? t("engineering.prAuthor") : row.push?.author ? t("engineering.pushAttribution") : t("engineering.commitAuthor")
          return name ? <div>{name}<div style={{ color: 'var(--text-3)', fontSize: 12 }}>{role}</div></div> : t("engineering.unassigned")
        } },
        { title: 'PR', dataIndex: 'pullRequest', render: value => value || '—' },
        { title: t("engineering.commit"), dataIndex: 'revision', render: (value: string) => value.slice(0, 10) },
        { title: t("engineering.qualityGate"), dataIndex: 'gate', render: (value: SonarAnalysis['gate']) => <Tag color={value === 'ERROR' ? 'red' : value === 'OK' ? 'green' : undefined}>{gates[value]}</Tag> },
        { title: t("engineering.newIssues"), render: (_, row) => row.analysisQuality?.newIssues ?? row.newIssues ?? '—' },
        { title: t("engineering.newCodeCoverage"), render: (_, row) => percent(row.analysisQuality?.coverage ?? row.newCoverage) },
        { title: t("engineering.newCodeDuplication"), render: (_, row) => percent(row.analysisQuality?.duplication ?? row.newDuplication) },
        { title: t("engineering.analysisTime"), dataIndex: 'analyzedAt', render: (value: string) => {
          const timestamp = Date.parse(value)
          return Number.isFinite(timestamp) ? analysisDate.format(timestamp) : '—'
        } },
      ]}
    />
  </Card>
}
