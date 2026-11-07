import { Card } from 'antd'
import type { MemberMetric } from './api'
import { ManagementBars, type BarItem } from './ManagementCharts'
import './management.css'
export default function MemberManagementOverview({ members, select }: { members: MemberMetric[]; select: (member: MemberMetric) => void }) {
  const rows = (value: (p: MemberMetric) => number | null, note: (p: MemberMetric) => string): BarItem[] => members.map(p => ({ name: p.name, value: value(p), note: note(p) })).sort((a, b) => (b.value ?? -1) - (a.value ?? -1))
  const open = (name: string) => { const member = members.find(p => p.name === name); if (member) select(member) }
  return <div aria-label="成员指标管理总览">
    <div className="yx-management-scope"><span>统计对象 <strong>{members.length}</strong> · 存在延期记录 <strong>{members.filter(p => p.overdueAtEnd > 0).length}</strong> · 缺陷历史待核验 <strong>{members.filter(p => (p.reopenUnknownBugs || 0) > 0).length}</strong></span><span>点击成员查看来源明细</span></div>
    <div className="yx-management-grid-layout">
      <Card size="small" title="交付需求参与"><ManagementBars items={rows(p => p.deliveredRequirements, p => `新增 ${p.newRequirements}；交付按需求参与人统计，多人参与不能相加为团队交付总量`)} unit=" 项" onSelect={open} /></Card>
      <Card size="small" title="延期信号 · 保留样本数"><ManagementBars items={rows(p => p.delayRate, p => `延期 ${p.overdueAtEnd} / 有效到期 ${p.dueRequirements - p.unknownDelivery}；待发布不纳入延期`)} unit="%" onSelect={open} /></Card>
      <Card size="small" title="交付需求中的缺陷占比"><ManagementBars items={rows(p => p.bugRate, p => `含 Bug 需求 ${p.bugAffectedRequirements} / 已交付需求 ${p.deliveredRequirements}，不等同个人缺陷责任`)} unit="%" onSelect={open} /></Card>
      <Card size="small" title="源责任缺陷重开占比"><ManagementBars items={rows(p => (p.reopenUnknownBugs || 0) > 0 ? null : p.reopenRate ?? null, p => `重开 ${p.reopenedBugs || 0} / 已处理 ${p.processedBugs || 0}；历史待核验 ${p.reopenUnknownBugs || 0}`)} unit="%" onSelect={open} /></Card>
    </div>
    <p className="yx-management-notes">各图用于定位需要复核的记录。比例保留原始分母，缺少数据时显示待评价；不将不同岗位的数量和比例合成为总分。完整表格、月度对比与雷达可在“查看明细与雷达”中展开。</p>
  </div>
}
