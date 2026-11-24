import { ArrowRightOutlined } from '@ant-design/icons'
import { Button } from 'antd'
import type { Notice } from '../api'
import { useI18n } from '../i18n'

const SCENARIO_EVENTS = ['SCENARIO_RUN_SUCCEEDED', 'SCENARIO_RUN_FAILED']

function scenarioReportLink(notice: Notice) {
  const link = notice.content.split('\n').find((line) => line.startsWith('[查看完整测试报告]('))
  const url = link ? /\]\((https?:\/\/[^\s)]+)\)/.exec(link)?.[1] : undefined
  if (!url) return undefined
  try {
    const { pathname, search } = new URL(url)
    const params = new URLSearchParams(search)
    const unified = /^\/share\/report\/([^/]+)$/.exec(pathname)
    const id = pathname === '/ocr-report.html'
      ? params.get('reportId')
      : unified && params.get('access') === 'project' ? decodeURIComponent(unified[1]) : null
    if (!id) return undefined
    return `/share/report/${encodeURIComponent(id)}?access=project`
  } catch {
    return undefined
  }
}

export default function NoticeCard({ notice, projectName, time, canOpen, onOpen }: {
  notice: Notice
  projectName?: string
  time: string
  canOpen: boolean
  onOpen: (link?: string) => void
}) {
  const { t } = useI18n()
  const scenario = SCENARIO_EVENTS.includes(notice.eventType)
  const plan = notice.eventType === 'PLAN_RUN_FINISHED'
  const failed = notice.eventType.endsWith('_FAILED')
  const scenarioSummary = scenario
    ? /^\|[^|\n]+\|\s*(\d+)\s*\|\s*(\d+)\s*\|\s*(\d+)\s*\|\s*$/m.exec(notice.content)
    : null
  const planSummary = plan ? /^(\d+)\/(\d+) passed, (\d+) failed$/.exec(notice.content) : null
  const counts = scenarioSummary
    ? [scenarioSummary[1], scenarioSummary[2], scenarioSummary[3]]
    : planSummary ? [planSummary[2], planSummary[1], planSummary[3]] : null
  const href = scenario
    ? scenarioReportLink(notice)
    : plan ? `/share/report/${encodeURIComponent(notice.resourceId)}?access=project&type=plan` : undefined
  const details = scenario
    ? (notice.content.split('\n### 用例结果\n')[1] ?? '').split('\n').filter((line) => line.startsWith('- ')).map((line) => line.slice(2))
    : []
  const title = scenario ? notice.title.replace(/^场景执行(?:成功|失败)｜/, '') : notice.title
  const labels = [t('msg.reportTotal', '总数'), t('scenario.pass', '通过'), t('scenario.fail', '失败')]
  const content = counts || (scenario && /^report [\w-]+$/.test(notice.content)) ? '' : notice.content
  const eventLabel = scenario
    ? (failed ? t('msg.scenarioFailed', '场景执行失败') : t('msg.scenarioSucceeded', '场景执行成功'))
    : plan ? t('msg.planFinished', '测试计划执行完成') : t(`msg.evt.${notice.eventType}`, notice.eventType)

  return (
    <article style={{ background: 'var(--panel)', border: '1px solid var(--border-soft)', borderRadius: 8, padding: '12px 14px', marginBottom: 8 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, color: 'var(--text-3)', fontSize: 12 }}>
        {!notice.read && <span aria-label={t('msg.tab.unread', '未读')} style={{ width: 6, height: 6, borderRadius: '50%', background: 'var(--brand)' }} />}
        {(scenario || plan) && projectName && <strong>{projectName}</strong>}
        <span style={{ color: failed ? 'var(--error)' : scenario ? 'var(--success)' : undefined }}>{eventLabel}</span>
        {notice.atMention && <span>{t('msg.atMe', '@我')}</span>}
        <span style={{ marginInlineStart: 'auto', whiteSpace: 'nowrap' }}>{time}</span>
      </div>
      <div style={{ marginTop: 6, color: 'var(--text)', fontWeight: 600, overflowWrap: 'anywhere' }}>{title}</div>
      {counts && (
        <dl style={{ display: 'grid', gridTemplateColumns: 'repeat(3, minmax(0, 1fr))', gap: 8, margin: '10px 0 0', borderTop: '1px solid var(--border-soft)', paddingTop: 8, fontSize: 12 }}>
          {counts.map((count, index) => (
            <div key={labels[index]} style={{ display: 'flex', gap: 6 }}>
              <dt style={{ color: 'var(--text-3)' }}>{labels[index]}</dt>
              <dd style={{ margin: 0, fontVariantNumeric: 'tabular-nums', color: index === 2 && count !== '0' ? 'var(--error)' : 'var(--text)' }}>{count}</dd>
            </div>
          ))}
        </dl>
      )}
      {details.length > 0 && (
        <ul style={{ paddingInlineStart: 18, color: 'var(--text-2)', fontSize: 12, margin: '8px 0 0', overflowWrap: 'anywhere' }}>
          {details.map((detail, index) => <li key={index}>{detail}</li>)}
        </ul>
      )}
      {content && <div style={{ marginTop: 6, color: 'var(--text-2)', fontSize: 12, whiteSpace: 'pre-wrap', overflowWrap: 'anywhere' }}>{content}</div>}
      {(notice.operator || href || canOpen || !notice.read) && (
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginTop: 6 }}>
          <span style={{ flex: 1, color: 'var(--text-3)', fontSize: 12 }}>{notice.operator}</span>
          {(href || canOpen || !notice.read) && (
            <Button type="link" size="small" onClick={() => onOpen(href)}>
              {href || plan ? t('plan.viewReport', '查看报告') : canOpen ? t('msg.openDetails', '查看详情') : t('msg.markRead', '标为已读')}
              {(href || canOpen) && <ArrowRightOutlined />}
            </Button>
          )}
        </div>
      )}
    </article>
  )
}
