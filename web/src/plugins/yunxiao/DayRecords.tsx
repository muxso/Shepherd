import { useEffect, useState } from 'react'
import { Button, Empty, Spin, Typography } from 'antd'
import { Link } from 'react-router-dom'
import { devopsApi, type DevopsRow } from './api'

export const metricKinds: Record<string, string> = {
  requirements: 'requirement', delivered: 'requirement', testcases: 'testcase', bugs: 'bug',
  commits: 'commit', linkedCommits: 'commit', additions: 'commit', deletions: 'commit', unknownStats: 'commit',
}
export type DayPage = { items: DevopsRow[]; total: number }
export type DayCache = Map<string, Promise<DayPage>>
export default function DayRecords({ source, date, metric, cache }: { source: string; date: string; metric: string; cache: DayCache }) {
  const [data, setData] = useState<DayPage | null>(null)
  const [error, setError] = useState('')
  useEffect(() => {
    let live = true
    const key = `${source}:${metric}:${date}`
    let pending = cache.get(key)
    if (!pending) {
      pending = devopsApi.records(source, { kind: metricKinds[metric], metric, date, page: 1, pageSize: 5 })
      cache.set(key, pending)
    }
    pending.then(result => { if (live) setData(result) }).catch(() => { cache.delete(key); if (live) setError('明细读取失败，请重新悬浮重试') })
    return () => { live = false }
  }, [source, date, metric, cache])
  const query = new URLSearchParams({ source, kind: metricKinds[metric], metric, date })
  return <div className="yx-day-records" aria-label="当日产出明细">
    {!data && !error && <Spin size="small" />}
    {error && <Typography.Text type="danger">{error}</Typography.Text>}
    {data && (data.items.length ? <>
      <ul>{data.items.map(row => <li key={String(row.externalId || row.id)}>
        <span className="yx-day-serial">{row.serial || row.sha?.slice(0, 8) || row.externalId}</span>
        <span>{row.title || '未命名记录'}</span>
        {row.requirements?.length ? <div className="yx-day-links">关联需求：{row.requirements.map(item => [item.serial || item.externalId, item.title || '名称未采集'].join(' ')).join('；')}</div> : null}
      </li>)}</ul>
      <Link to={`/devops?${query}`}><Button size="small" type="link">查看全部 {data.total} 条记录</Button></Link>
    </> : <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="当日无记录" />)}
  </div>
}
