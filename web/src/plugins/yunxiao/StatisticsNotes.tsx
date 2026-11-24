import { useI18n } from '../../i18n'
import { Button, Popover } from 'antd'
import { QuestionCircleOutlined } from '@ant-design/icons'
import type { DevopsDashboard, DevopsSource } from './api'

/** A single, on-demand explanation shared by the plugin page and its workbench card. */
export default function StatisticsNotes({ info, data }: { info?: DevopsSource; data?: DevopsDashboard | null }) {
  const { t } = useI18n()
  const metrics = data?.productivity?.metrics
  const missing = ['requirements', 'delivered', 'testcases', 'bugs'].reduce((n, key) => n + (metrics?.[key]?.missingTime || 0), 0)
  return <Popover trigger="click" placement="bottomRight" title={t("engineering.metricDefinitions")} content={<div className="yx-statistics-notes">
    <p>{t("engineering.usesShanghaiCalendarDaysIncludingTodayComparisonsUseThePreviousPeriodOfTheSameLengthHoverOverATrendToViewDailyRecords")}</p>
    <p>{t("engineering.newItemsAreAttributedByCreationDateAndCreatorDeliveryUsesCurrentReleaseStatusStatusChangeDateAndCurrentOwner")} {t('engineering.eventsWithoutDate', undefined, { count: missing })}</p>
    <p>{t('engineering.overdueAndCommitScope', undefined, { count: metrics?.unknownStats?.current || 0 })}</p>
    <p>{t('engineering.codeCollectionRange', undefined, {
      since: info?.code_since?.slice(0, 10) || t('engineering.unknown'),
      through: info?.code_until?.slice(0, 10) || t('engineering.unknown'),
    })}</p>
    <p>{t("engineering.sourceYunxiaoPluginSyncedHourlyRolesAreReportingPerspectivesAndAreNotPerformanceScores")}</p>
  </div>}>
    <Button type="text" size="small" icon={<QuestionCircleOutlined />}>{t("engineering.metricDefinitions")}</Button>
  </Popover>
}
