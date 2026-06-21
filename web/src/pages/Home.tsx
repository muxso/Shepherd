import { useEffect, useState } from 'react'
import { Card, Col, Empty, Row, Spin, Statistic } from 'antd'
import {
  ApiOutlined,
  PartitionOutlined,
  ProfileOutlined,
  ScheduleOutlined,
  FileTextOutlined,
  BugOutlined,
} from '@ant-design/icons'
import { api } from '../api'
import { useApp } from '../context'
import { regList } from '../registry'
import { useI18n } from '../i18n'
import Donut from '../components/Donut'

interface Counts {
  def: number
  scenario: number
  apiCase: number
  funcCase: number
  plan: number
  req: number
  bug: number
}

// 首页工作台:当前项目测试资产概览(计数卡片 + 资产分布环形图)。对标 MeterSphere 首页。
export default function Home() {
  const { projectId } = useApp()
  const { t } = useI18n()
  const [c, setC] = useState<Counts | null>(null)
  const [loading, setLoading] = useState(false)

  useEffect(() => {
    if (!projectId) {
      setC(null)
      return
    }
    setLoading(true)
    Promise.all([
      api.definitions(projectId).then((d) => d.length).catch(() => 0),
      api.scenarios(projectId).then((d) => d.length).catch(() => 0),
      api.projectCases(projectId).then((p) => p.total ?? p.items.length).catch(() => 0),
      api.functionalCases(projectId).then((d) => d.length).catch(() => 0),
    ])
      .then(([def, scenario, apiCase, funcCase]) =>
        setC({
          def,
          scenario,
          apiCase,
          funcCase,
          plan: regList('plan', projectId).length,
          req: regList('requirement', projectId).length,
          bug: regList('bug', projectId).length,
        }),
      )
      .finally(() => setLoading(false))
  }, [projectId])

  if (!projectId)
    return (
      <div style={{ padding: 48 }}>
        <Empty description={t('common.selectProject', '请先在顶部选择项目')} />
      </div>
    )

  const cards: { key: string; label: string; value: number; icon: React.ReactNode; color: string }[] = [
    { key: 'def', label: t('home.def', '接口定义'), value: c?.def ?? 0, icon: <ApiOutlined />, color: '#7c3aed' },
    { key: 'scenario', label: t('home.scenario', '场景用例'), value: c?.scenario ?? 0, icon: <PartitionOutlined />, color: '#1677ff' },
    { key: 'apiCase', label: t('home.apiCase', '接口用例'), value: c?.apiCase ?? 0, icon: <ProfileOutlined />, color: '#13c2c2' },
    { key: 'funcCase', label: t('home.funcCase', '功能用例'), value: c?.funcCase ?? 0, icon: <ProfileOutlined />, color: '#52c41a' },
    { key: 'plan', label: t('home.plan', '测试计划'), value: c?.plan ?? 0, icon: <ScheduleOutlined />, color: '#fa8c16' },
    { key: 'req', label: t('home.req', '需求'), value: c?.req ?? 0, icon: <FileTextOutlined />, color: '#eb2f96' },
    { key: 'bug', label: t('home.bug', '缺陷'), value: c?.bug ?? 0, icon: <BugOutlined />, color: '#f5222d' },
  ]

  const donutSegs = [
    { label: t('home.def', '接口定义'), value: c?.def ?? 0, color: '#7c3aed' },
    { label: t('home.scenario', '场景用例'), value: c?.scenario ?? 0, color: '#1677ff' },
    { label: t('home.apiCase', '接口用例'), value: c?.apiCase ?? 0, color: '#13c2c2' },
    { label: t('home.funcCase', '功能用例'), value: c?.funcCase ?? 0, color: '#52c41a' },
  ]
  const totalAssets = donutSegs.reduce((s, x) => s + x.value, 0)

  return (
    <div style={{ padding: 16, height: '100%', overflow: 'auto' }}>
      <Spin spinning={loading}>
        <Card title={t('home.title', '项目概览')} size="small" style={{ marginBottom: 16 }}>
          <Row gutter={[16, 16]}>
            {cards.map((card) => (
              <Col key={card.key} xs={12} sm={8} md={6} lg={6} xl={3}>
                <Card size="small" styles={{ body: { padding: '14px 16px' } }}>
                  <Statistic
                    title={
                      <span style={{ color: '#5b6470' }}>
                        <span style={{ color: card.color, marginRight: 6 }}>{card.icon}</span>
                        {card.label}
                      </span>
                    }
                    value={card.value}
                    valueStyle={{ color: card.color, fontWeight: 700 }}
                  />
                </Card>
              </Col>
            ))}
          </Row>
        </Card>

        <Card title={t('home.assetDist', '测试资产分布')} size="small">
          {totalAssets === 0 ? (
            <Empty description={t('common.empty', '暂无数据')} />
          ) : (
            <div style={{ display: 'flex', alignItems: 'center', gap: 32, padding: '8px 0' }}>
              <Donut segments={donutSegs} size={140} thickness={20} />
              <div style={{ flex: 1, maxWidth: 360 }}>
                {donutSegs.map((s) => (
                  <div key={s.label} style={{ display: 'flex', alignItems: 'center', padding: '6px 0', fontSize: 13 }}>
                    <span style={{ width: 10, height: 10, borderRadius: 2, background: s.color, marginRight: 8 }} />
                    <span style={{ flex: 1, color: '#5b6470' }}>{s.label}</span>
                    <b>{s.value}</b>
                    <span style={{ width: 56, textAlign: 'right', color: '#8a9099' }}>
                      {totalAssets ? ((s.value * 100) / totalAssets).toFixed(1) : '0'}%
                    </span>
                  </div>
                ))}
              </div>
            </div>
          )}
        </Card>
      </Spin>
    </div>
  )
}
