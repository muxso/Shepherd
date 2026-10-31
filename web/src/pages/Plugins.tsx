import { Alert, Button, Card, Space, Spin, Tag, Typography } from 'antd'
import { Link } from 'react-router-dom'
import { usePlugins } from '../plugins'

export default function Plugins() {
  const { items, loading, error } = usePlugins()
  return <div style={{ padding: 24, overflowY: 'auto' }}>
    <Typography.Title level={3}>插件</Typography.Title>
    {loading ? <Spin /> : error ? <Alert type="error" message={error} /> : items.map(plugin =>
      <Card key={plugin.id} title={plugin.name} extra={<Tag color={plugin.enabled ? 'green' : 'default'}>{plugin.enabled ? '已启用' : plugin.installed ? '未启用' : '未安装'}</Tag>}>
        <Space direction="vertical">
          <Typography.Paragraph>{plugin.description}</Typography.Paragraph>
          {plugin.enabled
            ? <Link to={plugin.entry}><Button>打开</Button></Link>
            : <Typography.Text>请由管理员配置 Shepherd 的插件后重启服务。</Typography.Text>}
        </Space>
      </Card>)}
  </div>
}
