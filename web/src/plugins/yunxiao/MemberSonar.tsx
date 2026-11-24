import { useI18n } from '../../i18n'
import { Empty, Typography } from 'antd'
import type { MemberMetric } from './api'
import { SonarNewCodeMetrics } from './SonarNewCodeMetrics'

export function MemberSonar({ member }: { member: MemberMetric }) {
  const { t } = useI18n()
  const sonar = member.sonar
  return <>
    <Typography.Title level={5}>{t("engineering.sonarNewCodeQuality")}</Typography.Title>
    {!sonar ? <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t("engineering.noAttributedSonarAnalysesThisMonth")} /> : <>
      <Typography.Paragraph type="secondary">{t('engineering.memberScanSummary', undefined, { total: sonar.pushes, passed: sonar.passed, evaluated: sonar.evaluated, shown: sonar.recent.length })}</Typography.Paragraph>
      {sonar.recent.map(analysis => <SonarNewCodeMetrics key={analysis.taskId} analysis={analysis} />)}
    </>}
  </>
}
