import { useEffect, useState } from 'react'
import { Button, Empty, Input, Table, Tag } from 'antd'
import { message } from '../feedback'
import { ReloadOutlined } from '@ant-design/icons'
import { api, ApiError, type McpTool } from '../api'
import { useI18n } from '../i18n'
import { PageBody, PageContainer, PageHeader } from '../components/Page'

// MCP tools: read-only list of the server's exposed JSON-RPC tools (tools/list).
export default function Mcp() {
  const { t } = useI18n()
  const [tools, setTools] = useState<McpTool[]>([])
  const [loading, setLoading] = useState(false)
  const [q, setQ] = useState('')

  const load = async () => {
    setLoading(true)
    try {
      const r = await api.mcpTools()
      setTools(r.result?.tools || [])
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : t('mcp.loadFailed', '加载 MCP 工具失败'))
    } finally {
      setLoading(false)
    }
  }
  useEffect(() => {
    load()
  }, [])

  const filtered = tools.filter((t) => t.name.toLowerCase().includes(q.toLowerCase()))

  return (
    <PageContainer>
      <PageHeader
        title={t('mcp.title', 'MCP 工具')}
        extra={
          <>
            <Input.Search placeholder={t('mcp.searchTool', '搜索工具名')} allowClear style={{ width: 240 }} onChange={(e) => setQ(e.target.value)} />
            <Button icon={<ReloadOutlined />} onClick={load} />
          </>
        }
      >
        <Tag color="blue">{tools.length}</Tag>
      </PageHeader>
      <PageBody>
        <Table<McpTool>
          rowKey="name"
          size="middle"
          loading={loading}
          dataSource={filtered}
          pagination={false}
          locale={{ emptyText: <Empty description={t('mcp.emptyTools', '无工具(server 未启用 MCP?)')} /> }}
          columns={[
            { title: t('mcp.colToolName', '工具名'), dataIndex: 'name', width: 280, render: (n: string) => <span className="ms-mono">{n}</span> },
            { title: t('mcp.colDesc', '说明'), dataIndex: 'description', render: (d?: string) => d || '—' },
          ]}
        />
      </PageBody>
    </PageContainer>
  )
}
