import { useEffect, useMemo, useState } from 'react'
import { Button, Card, Checkbox, Col, Dropdown, Empty, Row, Spin, Statistic, Tooltip } from 'antd'
import {
  ApiOutlined,
  PartitionOutlined,
  ProfileOutlined,
  ScheduleOutlined,
  FileTextOutlined,
  BugOutlined,
  SettingOutlined,
  ArrowUpOutlined,
  ArrowDownOutlined,
  ThunderboltOutlined,
  SafetyCertificateOutlined,
} from '@ant-design/icons'
import { useNavigate } from 'react-router-dom'
import { api, type ApiCase, type ApiDefinition } from '../api'
import { useApp } from '../context'
import { regList } from '../registry'
import { useI18n } from '../i18n'
import Donut from '../components/Donut'
import GroupedBars, { type BarRow } from '../components/GroupedBars'

interface Counts {
  def: number
  scenario: number
  apiCase: number
  funcCase: number
  plan: number
  req: number
  bug: number
}

// 协议分段配色(轮转)。
const PROTO_COLORS = ['#7c3aed', '#1677ff', '#13c2c2', '#52c41a', '#fa8c16', '#eb2f96', '#f5222d', '#8a9099']

// 项目对比柱状图的资产系列(配色对齐资产分布环)。
const PROJECT_SERIES = [
  { key: 'def', label: '接口定义', color: '#7c3aed' },
  { key: 'scenario', label: '场景用例', color: '#1677ff' },
  { key: 'apiCase', label: '接口用例', color: '#13c2c2' },
  { key: 'funcCase', label: '功能用例', color: '#52c41a' },
]

// 卡片清单(完整版「卡片设置」对标 MeterSphere):每张卡可独立显隐 + 自由排序。
const ALL_CARDS = ['overview', 'projectBars', 'assets', 'apiStats', 'quality', 'shortcuts'] as const
type CardKey = (typeof ALL_CARDS)[number]
interface CardPref {
  key: CardKey
  shown: boolean
}
const CARDS_KEY = 'shepherd.home.cards.v4'

/** 读持久化偏好。缺省全显示、按 ALL_CARDS 顺序;新增卡默认显示并追加到末尾。 */
function loadPrefs(): CardPref[] {
  const def = (): CardPref[] => ALL_CARDS.map((k) => ({ key: k, shown: true }))
  try {
    const raw = localStorage.getItem(CARDS_KEY)
    if (raw) {
      const parsed = JSON.parse(raw) as CardPref[]
      const valid = parsed.filter((p) => (ALL_CARDS as readonly string[]).includes(p.key))
      const missing = ALL_CARDS.filter((k) => !valid.some((p) => p.key === k)).map((k) => ({ key: k, shown: true }))
      return [...valid, ...missing]
    }
  } catch {
    /* ignore */
  }
  return def()
}

// 首页工作台:当前项目测试资产概览。对标 MeterSphere 首页(可自定义卡片显隐与排序)。
export default function Home() {
  const { projectId, projects } = useApp()
  const { t } = useI18n()
  const navigate = useNavigate()
  const [c, setC] = useState<Counts | null>(null)
  const [defs, setDefs] = useState<ApiDefinition[]>([])
  const [cases, setCases] = useState<ApiCase[]>([])
  const [projRows, setProjRows] = useState<BarRow[]>([])
  const [loading, setLoading] = useState(false)
  const [prefs, setPrefs] = useState<CardPref[]>(loadPrefs)

  const savePrefs = (next: CardPref[]) => {
    setPrefs(next)
    localStorage.setItem(CARDS_KEY, JSON.stringify(next))
  }
  const toggle = (key: CardKey, shown: boolean) => savePrefs(prefs.map((p) => (p.key === key ? { ...p, shown } : p)))
  const move = (idx: number, dir: -1 | 1) => {
    const j = idx + dir
    if (j < 0 || j >= prefs.length) return
    const next = [...prefs]
    ;[next[idx], next[j]] = [next[j], next[idx]]
    savePrefs(next)
  }

  useEffect(() => {
    if (!projectId) {
      setC(null)
      return
    }
    setLoading(true)
    Promise.all([
      api.definitions(projectId).catch(() => [] as ApiDefinition[]),
      api.scenarios(projectId).then((d) => d.length).catch(() => 0),
      api.projectCases(projectId).then((p) => ({ total: p.total ?? p.items.length, items: p.items ?? [] })).catch(() => ({ total: 0, items: [] as ApiCase[] })),
      api.functionalCases(projectId).then((d) => d.length).catch(() => 0),
    ])
      .then(([defList, scenario, casePage, funcCase]) => {
        const dl = Array.isArray(defList) ? defList : []
        setDefs(dl)
        setCases(casePage.items)
        setC({
          def: dl.length,
          scenario,
          apiCase: casePage.total,
          funcCase,
          plan: regList('plan', projectId).length,
          req: regList('requirement', projectId).length,
          bug: regList('bug', projectId).length,
        })
      })
      .finally(() => setLoading(false))
  }, [projectId])

  // 接口协议分布 + 覆盖率(真实数据:定义按 protocol 分组;有用例引用的定义=已覆盖)。
  const protocolSegs = useMemo(() => {
    const m = new Map<string, number>()
    defs.forEach((d) => { const k = (d.protocol || 'HTTP').toUpperCase(); m.set(k, (m.get(k) ?? 0) + 1) })
    return [...m.entries()].sort((a, b) => b[1] - a[1]).map(([label, value], i) => ({ label, value, color: PROTO_COLORS[i % PROTO_COLORS.length] }))
  }, [defs])
  const coveredDefs = useMemo(() => {
    const ref = new Set(cases.map((x) => x.apiDefinitionId))
    return defs.filter((d) => ref.has(d.id)).length
  }, [defs, cases])

  // 项目对比:对每个项目并发拉取四类资产计数(真实数据,与当前选中项目无关)。
  useEffect(() => {
    if (!projects.length) { setProjRows([]); return }
    let alive = true
    Promise.all(
      projects.map(async (p) => {
        const [def, scenario, apiCase, funcCase] = await Promise.all([
          api.definitions(p.id).then((d) => d.length).catch(() => 0),
          api.scenarios(p.id).then((d) => d.length).catch(() => 0),
          api.projectCases(p.id).then((x) => x.total ?? x.items.length).catch(() => 0),
          api.functionalCases(p.id).then((d) => d.length).catch(() => 0),
        ])
        return { name: p.name, values: { def, scenario, apiCase, funcCase } } as BarRow
      }),
    ).then((rows) => { if (alive) setProjRows(rows) })
    return () => { alive = false }
  }, [projects])

  const cardTitle: Record<CardKey, string> = {
    overview: t('home.title', '项目概览'),
    projectBars: t('home.projectCompare', '项目资产对比'),
    assets: t('home.assetDist', '测试资产分布'),
    apiStats: t('home.apiStats', '接口数'),
    quality: t('home.quality', '质量概览'),
    shortcuts: t('home.shortcuts', '快捷入口'),
  }

  const cards = useMemo(
    () => [
      { key: 'def', label: t('home.def', '接口定义'), value: c?.def ?? 0, icon: <ApiOutlined />, color: '#7c3aed' },
      { key: 'scenario', label: t('home.scenario', '场景用例'), value: c?.scenario ?? 0, icon: <PartitionOutlined />, color: '#1677ff' },
      { key: 'apiCase', label: t('home.apiCase', '接口用例'), value: c?.apiCase ?? 0, icon: <ProfileOutlined />, color: '#13c2c2' },
      { key: 'funcCase', label: t('home.funcCase', '功能用例'), value: c?.funcCase ?? 0, icon: <ProfileOutlined />, color: '#52c41a' },
      { key: 'plan', label: t('home.plan', '测试计划'), value: c?.plan ?? 0, icon: <ScheduleOutlined />, color: '#fa8c16' },
      { key: 'req', label: t('home.req', '需求'), value: c?.req ?? 0, icon: <FileTextOutlined />, color: '#eb2f96' },
      { key: 'bug', label: t('home.bug', '缺陷'), value: c?.bug ?? 0, icon: <BugOutlined />, color: '#f5222d' },
    ],
    [c, t],
  )

  const donutSegs = [
    { label: t('home.def', '接口定义'), value: c?.def ?? 0, color: '#7c3aed' },
    { label: t('home.scenario', '场景用例'), value: c?.scenario ?? 0, color: '#1677ff' },
    { label: t('home.apiCase', '接口用例'), value: c?.apiCase ?? 0, color: '#13c2c2' },
    { label: t('home.funcCase', '功能用例'), value: c?.funcCase ?? 0, color: '#52c41a' },
  ]
  const totalAssets = donutSegs.reduce((s, x) => s + x.value, 0)
  const totalCases = (c?.apiCase ?? 0) + (c?.funcCase ?? 0)
  const bugRate = totalCases ? ((c?.bug ?? 0) * 100) / totalCases : 0

  if (!projectId)
    return (
      <div style={{ padding: 48 }}>
        <Empty description={t('common.selectProject', '请先在顶部选择项目')} />
      </div>
    )

  const renderCard = (key: CardKey): React.ReactNode => {
    switch (key) {
      case 'overview':
        return (
          <Card title={cardTitle.overview} size="small" style={{ marginBottom: 16 }}>
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
        )
      case 'assets':
        return (
          <Card title={cardTitle.assets} size="small" style={{ marginBottom: 16 }}>
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
        )
      case 'projectBars': {
        // 资产量降序,仅取前 12,避免项目过多时 x 轴标签重叠。
        const TOP = 12
        const ranked = projRows
          .map((r) => ({ r, total: Object.values(r.values).reduce((s, v) => s + v, 0) }))
          .filter((x) => x.total > 0)
          .sort((a, b) => b.total - a.total)
        const shown = ranked.slice(0, TOP).map((x) => x.r)
        return (
          <Card
            title={<span><ApiOutlined style={{ color: '#1677ff', marginRight: 6 }} />{cardTitle.projectBars}</span>}
            extra={ranked.length > TOP ? <span style={{ fontSize: 12, color: '#8a9099' }}>{t('home.topN', '资产量前 {n}').replace('{n}', String(TOP))}</span> : undefined}
            size="small"
            style={{ marginBottom: 16 }}
          >
            {shown.length === 0 ? (
              <Empty description={t('common.empty', '暂无数据')} />
            ) : (
              <GroupedBars
                series={PROJECT_SERIES.map((s) => ({ ...s, label: t(`home.${s.key}`, s.label) }))}
                rows={shown}
              />
            )}
          </Card>
        )
      }
      case 'apiStats': {
        const totalDefs = defs.length
        const uncovered = totalDefs - coveredDefs
        const coverRate = totalDefs ? (coveredDefs * 100) / totalDefs : 0
        return (
          <Card title={<span><ApiOutlined style={{ color: '#7c3aed', marginRight: 6 }} />{cardTitle.apiStats}</span>} size="small" style={{ marginBottom: 16 }}>
            {totalDefs === 0 ? (
              <Empty description={t('common.empty', '暂无数据')} />
            ) : (
              <Row gutter={[24, 16]} align="middle">
                {/* 协议分布:总数环 + 逐协议占比 */}
                <Col xs={24} lg={14}>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 24 }}>
                    <Donut segments={protocolSegs} size={132} thickness={18} centerLabel={t('home.apiTotal', '接口总数')} />
                    <div style={{ flex: 1, minWidth: 0 }}>
                      {protocolSegs.map((s) => (
                        <div key={s.label} style={{ display: 'flex', alignItems: 'center', padding: '5px 0', fontSize: 13 }}>
                          <span style={{ width: 10, height: 10, borderRadius: 2, background: s.color, marginRight: 8 }} />
                          <span style={{ flex: 1, color: '#5b6470' }}>{s.label}</span>
                          <b>{s.value}</b>
                          <span style={{ width: 56, textAlign: 'right', color: '#8a9099' }}>{((s.value * 100) / totalDefs).toFixed(1)}%</span>
                        </div>
                      ))}
                    </div>
                  </div>
                </Col>
                {/* 用例覆盖率:已覆盖 / 未覆盖 */}
                <Col xs={24} lg={10}>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 24 }}>
                    <Donut
                      segments={[
                        { label: t('home.covered', '已覆盖'), value: coveredDefs, color: '#52c41a' },
                        { label: t('home.uncovered', '未覆盖'), value: uncovered, color: '#e8eaed' },
                      ]}
                      size={132}
                      thickness={18}
                      centerLabel={t('home.coverRate', '覆盖率')}
                    />
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div style={{ fontSize: 22, fontWeight: 700, color: coverRate >= 60 ? '#52c41a' : coverRate >= 30 ? '#fa8c16' : '#f5222d' }}>{coverRate.toFixed(1)}%</div>
                      <div style={{ color: '#8a9099', fontSize: 12, marginBottom: 10 }}>{t('home.coverRateHint', '有用例引用的接口占比')}</div>
                      <div style={{ display: 'flex', alignItems: 'center', padding: '5px 0', fontSize: 13 }}>
                        <span style={{ width: 10, height: 10, borderRadius: 2, background: '#52c41a', marginRight: 8 }} />
                        <span style={{ flex: 1, color: '#5b6470' }}>{t('home.covered', '已覆盖')}</span>
                        <b>{coveredDefs}</b>
                      </div>
                      <div style={{ display: 'flex', alignItems: 'center', padding: '5px 0', fontSize: 13 }}>
                        <span style={{ width: 10, height: 10, borderRadius: 2, background: '#d9d9d9', marginRight: 8 }} />
                        <span style={{ flex: 1, color: '#5b6470' }}>{t('home.uncovered', '未覆盖')}</span>
                        <b>{uncovered}</b>
                      </div>
                    </div>
                  </div>
                </Col>
              </Row>
            )}
          </Card>
        )
      }
      case 'quality':
        return (
          <Card
            title={
              <span>
                <SafetyCertificateOutlined style={{ color: '#52c41a', marginRight: 6 }} />
                {cardTitle.quality}
              </span>
            }
            size="small"
            style={{ marginBottom: 16 }}
          >
            <Row gutter={[16, 16]}>
              <Col xs={12} sm={6}>
                <Statistic title={t('home.req', '需求')} value={c?.req ?? 0} valueStyle={{ color: '#eb2f96' }} />
              </Col>
              <Col xs={12} sm={6}>
                <Statistic title={t('home.bug', '缺陷')} value={c?.bug ?? 0} valueStyle={{ color: '#f5222d' }} />
              </Col>
              <Col xs={12} sm={6}>
                <Statistic title={t('home.plan', '测试计划')} value={c?.plan ?? 0} valueStyle={{ color: '#fa8c16' }} />
              </Col>
              <Col xs={12} sm={6}>
                <Tooltip title={t('home.bugRateHint', '缺陷数 / 用例总数')}>
                  <Statistic
                    title={t('home.bugRate', '缺陷率')}
                    value={bugRate}
                    precision={1}
                    suffix="%"
                    valueStyle={{ color: bugRate > 20 ? '#f5222d' : '#52c41a' }}
                  />
                </Tooltip>
              </Col>
            </Row>
          </Card>
        )
      case 'shortcuts':
        return (
          <Card
            title={
              <span>
                <ThunderboltOutlined style={{ color: '#fa8c16', marginRight: 6 }} />
                {cardTitle.shortcuts}
              </span>
            }
            size="small"
            style={{ marginBottom: 16 }}
          >
            <div style={{ display: 'flex', flexWrap: 'wrap', gap: 10 }}>
              <Button icon={<ApiOutlined />} onClick={() => navigate('/api/definition')}>{t('home.def', '接口定义')}</Button>
              <Button icon={<PartitionOutlined />} onClick={() => navigate('/api/scenario')}>{t('home.scenario', '场景用例')}</Button>
              <Button icon={<ProfileOutlined />} onClick={() => navigate('/functional-case')}>{t('home.funcCase', '功能用例')}</Button>
              <Button icon={<ScheduleOutlined />} onClick={() => navigate('/test-plan')}>{t('home.plan', '测试计划')}</Button>
              <Button icon={<FileTextOutlined />} onClick={() => navigate('/requirement')}>{t('home.req', '需求')}</Button>
              <Button icon={<BugOutlined />} onClick={() => navigate('/bug')}>{t('home.bug', '缺陷')}</Button>
            </div>
          </Card>
        )
    }
  }

  return (
    <div style={{ padding: 16, height: '100%', overflow: 'auto' }}>
      <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 12 }}>
        <Dropdown
          trigger={['click']}
          popupRender={() => (
            <Card size="small" styles={{ body: { padding: 8 } }} style={{ width: 260, boxShadow: '0 6px 16px rgba(0,0,0,.12)' }}>
              <div style={{ fontSize: 12, color: '#8a9099', padding: '2px 6px 8px' }}>{t('home.cardSettingsHint', '勾选显示,箭头调整顺序')}</div>
              {prefs.map((p, i) => (
                <div key={p.key} style={{ display: 'flex', alignItems: 'center', gap: 6, padding: '4px 6px' }}>
                  <Checkbox checked={p.shown} onChange={(e) => toggle(p.key, e.target.checked)} style={{ flex: 1 }}>
                    {cardTitle[p.key]}
                  </Checkbox>
                  <Button type="text" size="small" icon={<ArrowUpOutlined />} disabled={i === 0} onClick={() => move(i, -1)} />
                  <Button type="text" size="small" icon={<ArrowDownOutlined />} disabled={i === prefs.length - 1} onClick={() => move(i, 1)} />
                </div>
              ))}
            </Card>
          )}
        >
          <Button size="small" icon={<SettingOutlined />}>{t('home.cardSettings', '卡片设置')}</Button>
        </Dropdown>
      </div>
      <Spin spinning={loading}>
        {prefs.filter((p) => p.shown).map((p) => <div key={p.key}>{renderCard(p.key)}</div>)}
        {prefs.every((p) => !p.shown) && <Empty description={t('home.allHidden', '所有卡片已隐藏,点击右上角「卡片设置」开启')} />}
      </Spin>
    </div>
  )
}
