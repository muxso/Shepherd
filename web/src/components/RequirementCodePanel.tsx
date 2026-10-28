import { useEffect, useMemo, useState } from 'react'
import { Alert, Button, Empty, Input, Table } from 'antd'
import { api, type RequirementCode, type RequirementCommit } from '../api'

const lines = (n: number | null) => n == null ? '未知' : n
export function RequirementCodePanel({ projectId, requirementId }: { projectId: string; requirementId: string }) {
  const [data, setData] = useState<RequirementCode | null>(null)
  const [error, setError] = useState(false)
  const [loading, setLoading] = useState(true)
  const [query, setQuery] = useState('')
  const [retry, setRetry] = useState(0)
  useEffect(() => {
    let active = true
    setLoading(true); setError(false); setData(null)
    api.requirementCode(projectId, requirementId).then(d => { if (active) setData(d) })
      .catch(() => { if (active) setError(true) }).finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [projectId, requirementId, retry])
  const commits = useMemo(() => {
    const q = query.trim().toLocaleLowerCase()
    return (data?.commits ?? []).filter(c => !q || `${c.repo} ${c.author} ${c.sha} ${c.title}`.toLocaleLowerCase().includes(q))
  }, [data, query])
  const authors = useMemo(() => {
    const grouped = new Map<string, RequirementCommit[]>()
    for (const c of commits) grouped.set(c.author, [...(grouped.get(c.author) ?? []), c])
    return [...grouped].map(([author, rows]) => ({ author, count: rows.length, repos: [...new Set(rows.map(c => c.repo))].join('、'),
      additions: rows.some(c => c.additions == null) ? null : rows.reduce((n, c) => n + c.additions!, 0),
      deletions: rows.some(c => c.deletions == null) ? null : rows.reduce((n, c) => n + c.deletions!, 0),
    }))
  }, [commits])
  return <div data-testid="requirement-code" style={{ marginTop: 24 }}>
    <div style={{ fontWeight: 600, marginBottom: 8 }}>关联仓库与代码贡献</div>
    <div style={{ color: 'var(--text-2)', fontSize: 12, marginBottom: 12 }}>
      仅统计云效明确关联到本需求的提交，按实际作者归属，排除合并提交。登记为参与人或测试人员不代表提交过代码；代码行数不代表工时。
      {data?.codeSince && <div>已采集范围：{data.codeSince} 至 {data.codeUntil ?? '未知'}</div>}
    </div>
    {error ? <Alert type="error" message="关联代码加载失败" action={<Button onClick={() => setRetry(n => n + 1)}>重试</Button>} /> : <>
      <Input.Search allowClear placeholder="搜索仓库、作者或提交" value={query} onChange={e => setQuery(e.target.value)} style={{ maxWidth: 420, marginBottom: 12 }} />
      <div style={{ marginBottom: 8 }}>{commits.length} 个提交 · {new Set(commits.map(c => c.repoId || c.repo)).size} 个仓库 · {authors.length} 位实际作者</div>
      <Table size="small" rowKey="author" loading={loading} dataSource={authors} pagination={false} scroll={{ x: 760 }} columns={[
        { title: '实际作者', dataIndex: 'author', width: 120, render: (s: string) => s || '未知作者' },
        { title: '关联仓库', dataIndex: 'repos', ellipsis: true },
        { title: '提交数', dataIndex: 'count', width: 90 },
        { title: '新增行', dataIndex: 'additions', width: 100, render: lines },
        { title: '删除行', dataIndex: 'deletions', width: 100, render: lines },
      ]} locale={{ emptyText: loading ? '加载中' : <Empty description="当前已采集范围内暂无明确关联的提交，不表示该需求没有代码变更" /> }} />
      {commits.length > 0 && <Table<RequirementCommit> size="small" rowKey={c => `${c.repoId}:${c.sha}`} dataSource={commits} pagination={{ pageSize: 10 }} scroll={{ x: 1050 }} style={{ marginTop: 12 }} columns={[
        { title: '仓库', dataIndex: 'repo', width: 180 },
        { title: '提交', dataIndex: 'sha', width: 110, render: (sha: string, c) => /^https?:\/\//i.test(c.url) ? <a href={c.url} target="_blank" rel="noopener noreferrer">{sha.slice(0, 10)}</a> : <code>{sha.slice(0, 10)}</code> },
        { title: '说明', dataIndex: 'title', ellipsis: true },
        { title: '实际作者', dataIndex: 'author', width: 110 },
        { title: '作者时间', dataIndex: 'authoredAt', width: 180, render: (v: string) => v ? new Date(v).toLocaleString() : '未知' },
        { title: '新增 / 删除', width: 130, render: (_, c) => `${c.additions == null ? '未知' : '+' + c.additions} / ${c.deletions == null ? '未知' : '-' + c.deletions}` },
      ]} />}
    </>}
  </div>
}
