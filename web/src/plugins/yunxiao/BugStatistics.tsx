import { Card, Col, Empty, Row, Statistic, Table } from 'antd'
import type { BugStatistics as Data } from './api'
export default function BugStatistics({ data }: { data?: Data }) {
  if (!data) return <Empty description="暂无缺陷统计" />
  return <section aria-label="云效缺陷统计">
    <Row gutter={[12,12]} style={{ marginBottom:16 }}>{[
      ['缺陷总数',data.total],['本期新增',data.new],['已关联需求',data.linked],['未关联需求',data.unlinked],
    ].map(([title,value]) => <Col xs={12} lg={6} key={String(title)}><Card size="small"><Statistic title={title} value={value} /></Card></Col>)}</Row>
    <Row gutter={[16,16]}>
      <Col xs={24} lg={12}><Card size="small" title="缺陷状态分布"><div className="yx-contributors">{data.statuses.map(item => <div key={item.status}>
        <div className="yx-person"><span>{item.status}</span><b>{item.count}</b></div>
        <div className="yx-person-track"><i style={{ width:`${data.total ? item.count/data.total*100 : 0}%` }} /></div>
      </div>)}</div></Card></Col>
      <Col xs={24} lg={12}><Card size="small" title="实际发现人"><Table size="small" rowKey="name" dataSource={data.reporters} pagination={{ pageSize:8,hideOnSinglePage:true }} columns={[
        { title:'发现人',dataIndex:'name' },{ title:'缺陷数',dataIndex:'count' },
      ]} /></Card></Col>
    </Row>
  </section>
}
