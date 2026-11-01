import { useEffect, useState } from 'react'
import { Alert, Card, Empty, Select, Space, Spin, Table, Tabs, Tag, Typography } from 'antd'
import { ArrowRightOutlined } from '@ant-design/icons'
import { ApiError } from '../../api'
import { devopsApi, type DevopsRow, type RequirementOutputData } from './api'
import './requirement-output.css'

interface Props { sourceId?: string; projectId?: string; reference?: string }
const when = (value: unknown) => typeof value === 'string' && value ? new Date(value).toLocaleString('zh-CN') : '—'
const names = (items: { name: string }[] = []) => items.map(item => item.name).join('、') || '未记录'
const lines = (value: number | null) => value == null ? '未知' : value.toLocaleString('zh-CN')

export default function RequirementOutput({ sourceId, projectId, reference: initialReference = '' }: Props) {
  const [reference, setReference] = useState(initialReference)
  const [search, setSearch] = useState('')
  const [options, setOptions] = useState<{ value: string; label: string }[]>([])
  const [data, setData] = useState<RequirementOutputData | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [days, setDays] = useState(0)
  const [bugPage, setBugPage] = useState(1), [commitPage, setCommitPage] = useState(1)
  useEffect(() => { setReference(initialReference); setBugPage(1); setCommitPage(1) }, [initialReference, sourceId, projectId])
  useEffect(() => {
    if (!sourceId) return
    let live = true
    const timer = setTimeout(() => {
      devopsApi.records(sourceId, { kind: 'requirement', search, page: 1, pageSize: 20 }).then(result => {
        if (!live) return
        const next = result.items.map(item => ({ value: String(item.externalId), label: `${item.serial || ''} ${item.title || ''}`.trim() }))
        setOptions(next)
        setReference(current => current || next[0]?.value || '')
      }).catch(e => { if (live) setError(String(e.message || e)) })
    }, 250)
    return () => { live = false; clearTimeout(timer) }
  }, [sourceId, search])
  useEffect(() => {
    if (!reference || (!sourceId && !projectId)) { setData(null); return }
    let live = true
    setLoading(true); setError(''); setData(null)
    devopsApi.requirementOutput({ ...(sourceId ? { sourceId } : { projectId: projectId! }), reference,
      ...(days ? { days } : {}), bugPage, commitPage, pageSize: 20 }).then(result => { if (live) setData(result) })
      .catch(e => { if (live && !(e instanceof ApiError && e.status === 404)) setError(String(e.message || e)) })
      .finally(() => { if (live) setLoading(false) })
    return () => { live = false }
  }, [sourceId, projectId, reference, days, bugPage, commitPage])
  const output = data?.output
  const requirement = data?.requirement
  return <section aria-label="需求岗位产出" className="yx-requirement-output">
    <div className="yx-requirement-toolbar">
      {sourceId && <Select aria-label="选择产出需求" showSearch filterOption={false} onSearch={setSearch}
        style={{ width: 480, maxWidth: '100%' }} placeholder="搜索需求编号或标题" value={reference || undefined}
        options={options.some(item => item.value === reference) ? options : requirement ? [{ value: reference, label: `${requirement.serial || ''} ${requirement.title || ''}` }, ...options] : options}
        onChange={value => { setReference(value); setBugPage(1); setCommitPage(1) }} />}
      <Space size={8}>{!sourceId && <Tag>云效插件</Tag>}<Select aria-label="需求产出范围" value={days} onChange={value => { setDays(value); setBugPage(1); setCommitPage(1) }}
        options={[{ value: 0, label: '全部已采集' }, ...[7,30,90,365].map(value => ({ value, label: `最近 ${value} 天` }))]} /></Space>
    </div>
    {error && <Alert type="error" message="需求产出读取失败" description={error} />}
    <Spin spinning={loading}>
      {!loading && !data && !error && <Empty description={reference ? '未找到对应的同步需求' : '当前需求尚未关联同步产出'} />}
      {data && requirement && output && <>
        <div className="yx-role-flow">
          <Card size="small" title="产品 · 需求">
            <strong>{requirement.serial || requirement.externalId}</strong><p>{requirement.title}</p>
            <Tag>{String(requirement.status || '未记录状态')}</Tag>
            <dl><dt>创建人</dt><dd>{requirement.creator || '未记录'}</dd><dt>当前负责人</dt><dd>{requirement.owner || '未记录'}</dd></dl>
          </Card>
          <ArrowRightOutlined className="yx-role-arrow" />
          <Card size="small" title="测试 · 缺陷">
            <div className="yx-role-number">{data.permissions.bugs ? output.bugTotal : '—'}<small>关联缺陷</small></div>
            <dl><dt>实际发现人</dt><dd>{data.permissions.bugs ? names(output.testers) : '无缺陷读取权限'}</dd></dl>
            <Space size={[4,4]} wrap>{output.bugStates?.map(state => <Tag key={state.status}>{state.status} {state.count}</Tag>)}</Space>
          </Card>
          <ArrowRightOutlined className="yx-role-arrow" />
          <Card size="small" title="研发 · 代码与仓库">
            <div className="yx-role-number">{output.commitTotal}<small>实际提交 · {output.repositories.length} 个仓库</small></div>
            <dl><dt>代码作者</dt><dd>{names(output.developers)}</dd></dl>
            {output.repositories.slice(0,5).map(repo => <div className="yx-repo-share" key={repo.id}><span>{repo.name}</span><b>{repo.commits}</b>
              <div className="yx-repo-track"><i style={{ width: `${output.commitTotal ? repo.commits / output.commitTotal * 100 : 0}%` }} /></div></div>)}
            {output.repositories.length > 5 && <Typography.Text type="secondary">另 {output.repositories.length-5} 个仓库，见下方仓库列表</Typography.Text>}
          </Card>
        </div>
        <Tabs defaultActiveKey={data.permissions.bugs ? 'bugs' : 'repositories'} items={[
          { key: 'bugs', label: `关联缺陷 (${data.permissions.bugs ? output.bugTotal : '—'})`, disabled: !data.permissions.bugs,
            children: <Table<DevopsRow> size="small" rowKey={row => String(row.externalId)} dataSource={output.bugs || []} scroll={{ x: 850 }}
              pagination={{ current: data.bugPage, pageSize: data.pageSize, total: output.bugTotal || 0, showSizeChanger: false, onChange: setBugPage }} columns={[
                { title: '编号', dataIndex: 'serial', width: 130 }, { title: '缺陷', dataIndex: 'title' }, { title: '状态', dataIndex: 'status', width: 120 },
                { title: '发现人', dataIndex: 'creator', width: 110 }, { title: '当前处理人', dataIndex: 'owner', width: 120 }, { title: '创建时间', dataIndex: 'createdAt', width: 180, render: when },
              ]} /> },
          { key: 'repositories', label: `关联仓库 (${output.repositories.length})`, children: <Table size="small" rowKey="id" pagination={false} dataSource={output.repositories} scroll={{ x: 750 }} columns={[
            { title: '仓库', dataIndex: 'name' }, { title: '提交数', dataIndex: 'commits', width: 90 }, { title: '代码作者', dataIndex: 'authors', render: (values: string[]) => values.join('、') },
            { title: '已知新增行', dataIndex: 'additions', render: lines }, { title: '已知删除行', dataIndex: 'deletions', render: lines }, { title: '行数未知提交', dataIndex: 'unknownStats' },
          ]} /> },
          { key: 'commits', label: `关联代码 (${output.commitTotal})`, children: <Table<DevopsRow> size="small" rowKey={row => String(row.externalId)} dataSource={output.commits} scroll={{ x: 900 }}
            pagination={{ current: data.commitPage, pageSize: data.pageSize, total: output.commitTotal, showSizeChanger: false, onChange: setCommitPage }} columns={[
              { title: '提交', dataIndex: 'sha', width: 100, render: (value: string, row) => row.url?.startsWith('https://') ? <a href={row.url} target="_blank" rel="noreferrer">{value?.slice(0,8)}</a> : value?.slice(0,8) },
              { title: '说明', dataIndex: 'title' }, { title: '仓库', dataIndex: 'repo', width: 160 }, { title: '实际作者', dataIndex: 'author', width: 110 },
              { title: '增 / 删行', render: (_: unknown, row) => `+${lines(row.additions ?? null)} / -${lines(row.deletions ?? null)}` }, { title: '作者时间', dataIndex: 'authoredAt', width: 180, render: when },
            ]} /> },
        ]} />
        <Typography.Text type="secondary" className="yx-output-coverage">代码采集范围：{data.source.code_since?.slice(0,10) || '未知'} 至 {data.source.code_until?.slice(0,10) || '未知'}；按实际创建人、提交作者和明确关联归属。</Typography.Text>
      </>}
    </Spin>
  </section>
}
