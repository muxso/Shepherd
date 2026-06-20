import { useEffect, useState } from 'react'
import { Button, Empty, Input, Table, Tag, Typography, message } from 'antd'
import { ReloadOutlined } from '@ant-design/icons'
import { api, ApiError, type McpTool } from '../api'

// MCP 工具:只读列出 server 暴露的 JSON-RPC 工具(tools/list)。
export default function Mcp() {
  const [tools, setTools] = useState<McpTool[]>([])
  const [loading, setLoading] = useState(false)
  const [q, setQ] = useState('')

  const load = async () => {
    setLoading(true)
    try {
      const r = await api.mcpTools()
      setTools(r.result?.tools || [])
    } catch (e) {
      message.error(e instanceof ApiError ? e.message : '加载 MCP 工具失败')
    } finally {
      setLoading(false)
    }
  }
  useEffect(() => {
    load()
  }, [])

  const filtered = tools.filter((t) => t.name.toLowerCase().includes(q.toLowerCase()))

  return (
    <div style={{ height: '100%', display: 'flex', flexDirection: 'column' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '14px 16px', background: '#fff', borderBottom: '1px solid #f0f0f0' }}>
        <Typography.Text strong style={{ fontSize: 15 }}>
          MCP 工具
        </Typography.Text>
        <Tag color="blue">{tools.length}</Tag>
        <div style={{ flex: 1 }} />
        <Input.Search placeholder="搜索工具名" allowClear style={{ width: 240 }} onChange={(e) => setQ(e.target.value)} />
        <Button icon={<ReloadOutlined />} onClick={load} />
      </div>
      <div style={{ flex: 1, overflow: 'auto', padding: 16 }}>
        <Table<McpTool>
          rowKey="name"
          size="middle"
          loading={loading}
          dataSource={filtered}
          pagination={false}
          locale={{ emptyText: <Empty description="无工具(server 未启用 MCP?)" /> }}
          columns={[
            { title: '工具名', dataIndex: 'name', width: 280, render: (n: string) => <span className="ms-mono">{n}</span> },
            { title: '说明', dataIndex: 'description', render: (d?: string) => d || '—' },
          ]}
        />
      </div>
    </div>
  )
}
