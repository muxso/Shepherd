import { ReadError } from '../../components/Page'
import { useI18n } from '../../i18n'
import { useEffect, useState } from 'react'
import { Card, Empty, Segmented, Select, Space, Spin, Table, Tabs, Tag, Typography } from 'antd'
import { ApiError } from '../../api'
import { devopsApi, type DevopsRow, type RequirementOutputData } from './api'
import RequirementOverview from './RequirementOverview'
import './requirement-output.css'
import './management.css'

interface Props { sourceId?: string; projectId?: string; reference?: string; revision?: number }
const when = (value: unknown) => typeof value === 'string' && value ? new Date(value).toLocaleString('zh-CN') : '—'
const names = (items: { name: string }[] = []) => items.map(item => item.name).join('、') || '未记录'
const lines = (value: number | null) => value == null ? '未知' : value.toLocaleString('zh-CN')

export default function RequirementOutput({ sourceId, projectId, reference: initialReference = '', revision = 0 }: Props) {
  const { t } = useI18n()
  const [reference, setReference] = useState(initialReference)
  const [overview, setOverview] = useState(!!sourceId && !initialReference)
  const [search, setSearch] = useState('')
  const [options, setOptions] = useState<{ value: string; label: string }[]>([])
  const [data, setData] = useState<RequirementOutputData | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<unknown>()
  const [days, setDays] = useState(initialReference || !sourceId ? 0 : 7)
  const [bugPage, setBugPage] = useState(1), [commitPage, setCommitPage] = useState(1)
  useEffect(() => { setReference(initialReference); setOverview(!!sourceId && !initialReference); setBugPage(1); setCommitPage(1) }, [initialReference, sourceId, projectId])
  useEffect(() => {
    if (!sourceId) return
    let live = true
    const timer = setTimeout(() => {
      devopsApi.records(sourceId, { kind: 'requirement', search, page: 1, pageSize: 20 }).then(result => {
        if (!live) return
        const next = result.items.map(item => ({ value: String(item.externalId), label: `${item.serial || ''} ${item.title || ''}`.trim() }))
        setOptions(next)
        if (!overview) setReference(current => current || next[0]?.value || '')
      }).catch(e => { if (live) setError(e) })
    }, 250)
    return () => { live = false; clearTimeout(timer) }
  }, [sourceId, search, overview])
  useEffect(() => {
    if (overview || !reference || (!sourceId && !projectId)) { setData(null); return }
    let live = true
    setLoading(true); setError(undefined); setData(null)
    devopsApi.requirementOutput({ ...(sourceId ? { sourceId } : { projectId: projectId! }), reference,
      ...(days ? { days } : {}), bugPage, commitPage, pageSize: 20 }).then(result => { if (live) setData(result) })
      .catch(e => { if (live && !(e instanceof ApiError && e.status === 404)) setError(e) })
      .finally(() => { if (live) setLoading(false) })
    return () => { live = false }
  }, [sourceId, projectId, reference, days, bugPage, commitPage, overview, revision])
  const output = data?.output
  const requirement = data?.requirement
  return <section aria-label={t("engineering.requirementActivityByRole")} className="yx-requirement-output">
    <div className="yx-requirement-toolbar">
      {sourceId && <Segmented aria-label={t("engineering.requirementDeliveryView")} value={overview ? 'overview' : 'detail'} onChange={value => setOverview(value === 'overview')} options={[{value:'overview',label:t("engineering.requirementOverview")},{value:'detail',label:t("engineering.singleRequirement")}]} />}
      {sourceId && <Select aria-label={t("engineering.selectARequirement")} showSearch filterOption={false} onSearch={setSearch}
        style={{ width: 480, maxWidth: '100%' }} placeholder={t("engineering.searchRequirementNumberOrTitle")} value={reference || undefined}
        options={options.some(item => item.value === reference) ? options : requirement ? [{ value: reference, label: `${requirement.serial || ''} ${requirement.title || ''}` }, ...options] : options}
        onChange={value => { setOverview(false); setReference(value); setBugPage(1); setCommitPage(1) }} />}
      <Space size={8}>{!sourceId && <Tag>{t("engineering.yunxiaoPlugin")}</Tag>}<Select aria-label={t("engineering.requirementDeliveryRange")} value={days} onChange={value => { setDays(value); setBugPage(1); setCommitPage(1) }}
        options={[{ value: 0, label: t("engineering.allCollectedRecords") }, ...[7,30,90,365].map(value => ({ value, label: t("engineering.lastV0Days", undefined, { v0: value }) }))]} /></Space>
    </div>
    {error != null && <ReadError error={error} title={t("engineering.couldNotLoadRequirementDelivery")} />}
    {overview && sourceId ? <RequirementOverview sourceId={sourceId} days={days} revision={revision} openRequirement={value => {setReference(value);setOverview(false);setBugPage(1);setCommitPage(1)}} /> : <Spin spinning={loading}>
      {!loading && !data && !error && <Empty description={reference ? t("engineering.noMatchingSyncedRequirement") : t("engineering.noSyncedActivityIsLinkedToThisRequirement")} />}
      {data && requirement && output && <>
        <div className="yx-role-flow">
          <Card size="small" title={t("engineering.productRequirements")}>
            <strong>{requirement.serial || requirement.externalId}</strong><p>{requirement.title}</p>
            <Tag>{String(requirement.status || t("engineering.unknownStatus"))}</Tag>
            <dl><dt>{t("engineering.createdBy")}</dt><dd>{requirement.creator || t("engineering.notRecorded")}</dd><dt>{t("engineering.currentOwner")}</dt><dd>{requirement.owner || t("engineering.notRecorded")}</dd></dl>
          </Card>
          <Card size="small" title={t("engineering.testingDefects")}>
            <div className="yx-role-number">{data.permissions.bugs ? output.bugTotal : '—'}<small>{t("engineering.linkedDefects")}</small></div>
            <dl><dt>{t("engineering.actualReporter")}</dt><dd>{data.permissions.bugs ? names(output.testers) : t("engineering.noPermissionToViewDefects")}</dd></dl>
            <Space size={[4,4]} wrap>{output.bugStates?.map(state => <Tag key={state.status}>{state.status} {state.count}</Tag>)}</Space>
          </Card>
          <Card size="small" title={t("engineering.engineeringCodeAndRepositories")}>
            <div className="yx-role-number">{output.commitTotal}<small>实际提交 · {output.repositories.length} 个仓库</small></div>
            <dl><dt>{t("engineering.codeAuthor")}</dt><dd>{names(output.developers)}</dd></dl>
            {output.repositories.slice(0,5).map(repo => <div className="yx-repo-share" key={repo.id}><span>{repo.name}</span><b>{repo.commits}</b>
              <div className="yx-repo-track"><i style={{ width: `${output.commitTotal ? repo.commits / output.commitTotal * 100 : 0}%` }} /></div></div>)}
            {output.repositories.length > 5 && <Typography.Text type="secondary">另 {output.repositories.length-5} 个仓库，见下方仓库列表</Typography.Text>}
          </Card>
        </div>
        <Tabs defaultActiveKey={data.permissions.bugs ? 'bugs' : 'repositories'} items={[
          { key: 'bugs', label: `关联缺陷 (${data.permissions.bugs ? output.bugTotal : '—'})`, disabled: !data.permissions.bugs,
            children: <Table<DevopsRow> size="small" rowKey={row => String(row.externalId)} dataSource={output.bugs || []} scroll={{ x: 850 }}
              pagination={{ current: data.bugPage, pageSize: data.pageSize, total: output.bugTotal || 0, showSizeChanger: false, onChange: setBugPage }} columns={[
                { title: t("engineering.number"), dataIndex: 'serial', width: 130 }, { title: t("engineering.defects"), dataIndex: 'title' }, { title: t("engineering.status"), dataIndex: 'status', width: 120 },
                { title: t("engineering.reporter"), dataIndex: 'creator', width: 110 }, { title: t("engineering.currentAssignee"), dataIndex: 'owner', width: 120 }, { title: t("engineering.createdAt"), dataIndex: 'createdAt', width: 180, render: when },
              ]} /> },
          { key: 'repositories', label: `关联仓库 (${output.repositories.length})`, children: <Table size="small" rowKey="id" pagination={false} dataSource={output.repositories} scroll={{ x: 750 }} columns={[
            { title: t("engineering.repository"), dataIndex: 'name' }, { title: t("engineering.commitCount"), dataIndex: 'commits', width: 90 }, { title: t("engineering.codeAuthor"), dataIndex: 'authors', render: (values: string[]) => values.join('、') },
            { title: t("engineering.knownLinesAdded"), dataIndex: 'additions', render: lines }, { title: t("engineering.knownLinesDeleted"), dataIndex: 'deletions', render: lines }, { title: t("engineering.commitsWithUnknownLineCounts"), dataIndex: 'unknownStats' },
          ]} /> },
          { key: 'commits', label: `关联代码 (${output.commitTotal})`, children: <Table<DevopsRow> size="small" rowKey={row => String(row.externalId)} dataSource={output.commits} scroll={{ x: 900 }}
            pagination={{ current: data.commitPage, pageSize: data.pageSize, total: output.commitTotal, showSizeChanger: false, onChange: setCommitPage }} columns={[
              { title: t("engineering.commit"), dataIndex: 'sha', width: 100, render: (value: string, row) => row.url?.startsWith('https://') ? <a href={row.url} target="_blank" rel="noreferrer">{value?.slice(0,8)}</a> : value?.slice(0,8) },
              { title: t("engineering.description"), dataIndex: 'title' }, { title: t("engineering.repository"), dataIndex: 'repo', width: 160 }, { title: t("engineering.actualAuthor"), dataIndex: 'author', width: 110 },
              { title: t("engineering.linesAddedDeleted"), render: (_: unknown, row) => `+${lines(row.additions ?? null)} / -${lines(row.deletions ?? null)}` }, { title: t("engineering.authorDate"), dataIndex: 'authoredAt', width: 180, render: when },
            ]} /> },
        ]} />
      </>}
    </Spin>}
  </section>
}
