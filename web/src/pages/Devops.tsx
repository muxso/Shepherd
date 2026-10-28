import { useEffect, useState } from 'react'
import { Alert, Button, Card, Col, Input, Row, Select, Space, Statistic, Switch, Table, Tabs, Tag, Typography } from 'antd'
import { ReloadOutlined } from '@ant-design/icons'
import { devopsApi, type DevopsDashboard, type DevopsRow, type DevopsSource } from '../api'
const kinds = [{ key: 'requirement', label: '需求' }, { key: 'bug', label: '缺陷' }, { key: 'sprint', label: '迭代' }, { key: 'commit', label: '实际提交' }, { key: 'history', label: '状态变化' }, { key: 'runs', label: '同步记录' }]
const when = (v?: string) => v ? new Date(v).toLocaleString('zh-CN', { hour12: false }) : '—'
const text = (v: unknown) => v == null || v === '' ? '—' : String(v)
export default function Devops() {
  const [sources, setSources] = useState<DevopsSource[]>([]), [source, setSource] = useState('')
  const [dashboard, setDashboard] = useState<DevopsDashboard | null>(null), [rows, setRows] = useState<DevopsRow[]>([])
  const [total, setTotal] = useState(0), [kind, setKind] = useState('requirement'), [days, setDays] = useState(7)
  const [group, setGroup] = useState(''), [search, setSearch] = useState(''), [delayed, setDelayed] = useState(false)
  const [page, setPage] = useState(1), [revision, setRevision] = useState(0), [loading, setLoading] = useState(false), [error, setError] = useState('')
  useEffect(() => {
    let live = true
    devopsApi.sources().then(d => { if (live) { setSources(d.items); setSource(s => s || d.items[0]?.id || '') } }).catch(e => { if (live) setError(String(e.message || e)) })
    return () => { live = false }
  }, [revision])
  useEffect(() => {
    if (!source) return
    let live = true
    setDashboard(null)
    devopsApi.dashboard(source, days).then(d => { if (live) setDashboard(d) }).catch(e => { if (live) setError(String(e.message || e)) })
    return () => { live = false }
  }, [source, days, revision])
  useEffect(() => {
    if (!source) return
    let live = true
    setLoading(true); setError('')
    devopsApi.records(source, { kind, days, group, search, page, pageSize: 30, delayedOnly: kind === 'requirement' && delayed }).then(d => {
      if (live) { setRows(d.items); setTotal(d.total) }
    }).catch(e => { if (live) { setRows([]); setTotal(0); setError(String(e.message || e)) } }).finally(() => { if (live) setLoading(false) })
    return () => { live = false }
  }, [source, kind, days, group, search, page, delayed, revision])
  const groups = Array.from(new Set(dashboard?.authors.map(x => x.group) || []))
  const info = sources.find(s => s.id === source)
  const code = dashboard?.authors.reduce((a, p) => ({ commits: a.commits + p.commits, linked: a.linked + p.linkedCommits }), { commits: 0, linked: 0 })
  const columns = kind === 'commit' ? [
    { title: '提交', dataIndex: 'sha', width: 115, render: (v: string, r: DevopsRow) => r.url?.startsWith('https://') ? <a href={r.url} target="_blank" rel="noreferrer">{v?.slice(0, 10)}</a> : v?.slice(0, 10) },
    { title: '说明', dataIndex: 'title' }, { title: '实际作者', dataIndex: 'author', width: 120 }, { title: '分组', dataIndex: 'group', width: 140 }, { title: '仓库', dataIndex: 'repo' },
    { title: '作者时间', dataIndex: 'authoredAt', width: 175, render: when },
    { title: '增 / 删行', key: 'lines', width: 125, render: (_: unknown, r: DevopsRow) => r.additions == null ? '行数未知' : `+${r.additions} / -${r.deletions}` },
    { title: '需求关联', dataIndex: 'requirementIds', render: (v?: string[]) => v?.length ? v.join('、') : <Tag>未关联</Tag> },
  ] : kind === 'history' ? [
    { title: '类型', dataIndex: 'kind' }, { title: '外部 ID', dataIndex: 'external_id' }, { title: '原状态', dataIndex: 'old_status' }, { title: '新状态', dataIndex: 'new_status' },
    { title: '云效状态时间', dataIndex: 'source_changed_at', render: when }, { title: '同步观察时间', dataIndex: 'observed_at', render: when },
  ] : kind === 'runs' ? [
    { title: '状态', dataIndex: 'status', render: (v: string) => <Tag color={v === 'SUCCESS' ? 'green' : v === 'FAILED' ? 'red' : 'blue'}>{v}</Tag> },
    { title: '开始', dataIndex: 'started_at', render: when }, { title: '结束', dataIndex: 'finished_at', render: when },
    { title: '同步数量', dataIndex: 'counts', render: (v: unknown) => JSON.stringify(v) }, { title: '异常', dataIndex: 'error' },
  ] : [
    { title: '云效编号 / ID', key: 'serial', width: 155, render: (_: unknown, r: DevopsRow) => text(r.serial || r.externalId) },
    { title: '标题', dataIndex: 'title', width: 400 }, { title: '状态', dataIndex: 'status', width: 110 },
    ...(kind === 'sprint' ? [{ title: '开始', dataIndex: 'startAt', render: when }, { title: '结束', dataIndex: 'endAt', render: when }] : [
      ...(kind === 'bug' ? [{ title: '关联需求', dataIndex: 'parentSerial', width: 140, render: text }] : []),
      { title: '优先级', dataIndex: 'priority', width: 90, render: (v: string) => <Tag color={v === '紧急' ? 'red' : v === '高' ? 'orange' : undefined}>{text(v)}</Tag> },
      { title: '负责人', dataIndex: 'owner', width: 110, render: text }, { title: '分组', dataIndex: 'group', width: 130 },
      { title: '迭代', dataIndex: 'sprint', width: 140, render: text }, { title: '截止日', dataIndex: 'due', width: 120, render: text },
      { title: '状态变更', dataIndex: 'statusChangedAt', width: 175, render: when },
    ]),
  ]
  return <div style={{ padding: 24 }}>
    <Space style={{ width: '100%', justifyContent: 'space-between', marginBottom: 16 }} wrap>
      <div><Typography.Title level={3} style={{ margin: 0 }}>云效研发统计</Typography.Title><Typography.Text type="secondary">需求、缺陷、迭代与实际提交 · 外部来源数据</Typography.Text></div>
      <Space wrap>
        <Select aria-label="云效项目" style={{ minWidth: 220 }} value={source || undefined} onChange={v => { setSource(v); setPage(1); setGroup('') }} options={sources.map(s => ({ value: s.id, label: s.name }))} placeholder="尚未配置数据源" />
        <Select aria-label="统计区间" value={days} onChange={v => { setDays(v); setPage(1) }} options={[7, 14, 30, 90].map(v => ({ value: v, label: `最近 ${v} 天` }))} />
        <Button icon={<ReloadOutlined />} onClick={() => setRevision(x => x + 1)}>刷新数据</Button>
      </Space>
    </Space>
    {error && <Alert showIcon type="error" message={error} style={{ marginBottom: 16 }} />}
    <Alert type="info" showIcon style={{ marginBottom: 16 }} message={info?.last_success_at ? `最近成功同步：${when(info.last_success_at)} · 每小时自动更新` : '等待首次同步完成；本页不会触发 AI 交付或修改云效数据。'} description={<>延期只统计紧急、高、中；代码只统计实际作者，排除合并提交。代码采集覆盖：{when(info?.code_since || undefined)} 至 {when(info?.code_until || undefined)}。历史指标只使用已采集的数据。</>} />
    <Row gutter={[12, 12]} style={{ marginBottom: 16 }}>
      {[
        ['需求总数', dashboard?.totals.requirements], ['本期已发布', dashboard?.totals.delivered], ['当前延期', dashboard?.totals.delayed], ['缺陷总数', dashboard?.totals.bugs], ['迭代数', dashboard?.totals.sprints], ['关联需求提交 / 组织提交', code ? `${code.linked} / ${code.commits}` : '—'],
      ].map(([title, value]) => <Col xs={12} lg={4} key={String(title)}><Card size="small"><Statistic title={title} value={value ?? '—'} /></Card></Col>)}
    </Row>
    <Card title="代码作者与组织分组" size="small" style={{ marginBottom: 16 }}>
      <Typography.Paragraph type="secondary">{dashboard?.codeScope}。英文别名统一映射；无法确认的身份保留为“待归属”。</Typography.Paragraph>
      <Table size="small" rowKey={r => `${r.name}:${r.group}`} dataSource={dashboard?.authors || []} pagination={{ pageSize: 8, hideOnSinglePage: true }} columns={[
        { title: '实际作者', dataIndex: 'name' }, { title: '分组', dataIndex: 'group' }, { title: '组织提交', dataIndex: 'commits' }, { title: '关联需求提交', dataIndex: 'linkedCommits' }, { title: '新增行', dataIndex: 'additions', render: text }, { title: '删除行', dataIndex: 'deletions', render: text }, { title: '行数未知提交', dataIndex: 'unknownStats' },
      ]} />
    </Card>
    <Card size="small">
      <Tabs activeKey={kind} items={kinds} onChange={v => { setKind(v); setPage(1); setSearch(''); setGroup(''); setDelayed(false) }} />
      {!['history', 'runs'].includes(kind) && <Space wrap style={{ marginBottom: 16 }}>
        <Input.Search aria-label="搜索云效记录" placeholder="编号、标题、负责人或作者" allowClear onSearch={v => { setSearch(v); setPage(1) }} style={{ width: 320 }} />
        {kind !== 'sprint' && <Select aria-label="组织分组" style={{ width: 180 }} value={group} onChange={v => { setGroup(v); setPage(1) }} options={[{ value: '', label: '全部分组' }, ...Array.from(new Set([...groups, '上海研发组', '产品运营一组', '产品运营二组', '沈阳服务端', '沈阳大前端', '质量交付组', 'AI算法组', '运维组', '待归属'])).map(g => ({ value: g, label: g }))]} />}
        {kind === 'requirement' && <Space><Switch checked={delayed} onChange={v => { setDelayed(v); setPage(1) }} />只看延期（紧急 / 高 / 中）</Space>}
      </Space>}
      <Table<DevopsRow> rowKey={r => String(r.externalId || r.id)} loading={loading} dataSource={rows} columns={columns} scroll={{ x: 1300 }} pagination={{ current: page, pageSize: 30, total, showSizeChanger: false, showTotal: n => `${n} 条`, onChange: setPage }} />
      {kind === 'history' && <Typography.Paragraph type="secondary" style={{ marginTop: 12 }}>{dashboard?.historyScope}</Typography.Paragraph>}
    </Card>
  </div>
}
