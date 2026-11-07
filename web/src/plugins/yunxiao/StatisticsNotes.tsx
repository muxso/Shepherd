import { Button, Popover } from 'antd'
import { QuestionCircleOutlined } from '@ant-design/icons'
import type { DevopsDashboard, DevopsSource } from './api'

/** A single, on-demand explanation shared by the plugin page and its workbench card. */
export default function StatisticsNotes({ info, data }: { info?: DevopsSource; data?: DevopsDashboard | null }) {
  const metrics = data?.productivity?.metrics
  const missing = ['requirements', 'delivered', 'testcases', 'bugs'].reduce((n, key) => n + (metrics?.[key]?.missingTime || 0), 0)
  return <Popover trigger="click" placement="bottomRight" title="统计口径" content={<div className="yx-statistics-notes">
    <p>按上海自然日统计，含今天；环比为前一个等长区间。悬浮趋势可查看当日记录。</p>
    <p>新增按创建时间及创建人；交付按当前已发布状态、状态变更时间及当前负责人。{missing} 项事件缺少时间，未计入趋势；未知创建人列为待归属。</p>
    <p>延期仅统计紧急、高、中优先级，排除待发布及完成终态；代码排除合并提交，区分组织提交与需求关联提交。关联提交中 {metrics?.unknownStats?.current || 0} 次行数未知。</p>
    <p>代码采集：{info?.code_since?.slice(0, 10) || '未知'} 至 {info?.code_until?.slice(0, 10) || '未知'}。历史以已采集记录为准，不推算导入前状态或历史经手人。</p>
    <p>来源：云效插件，单向同步，每小时更新。岗位是指标视角，不作为绩效评分。</p>
  </div>}>
    <Button type="text" size="small" icon={<QuestionCircleOutlined />}>统计口径</Button>
  </Popover>
}
