import { useEffect, useState } from 'react'
import { Alert, Button, Card, Space, Spin, Tag, Typography } from 'antd'
import { Link } from 'react-router-dom'
import { usePlugins } from '../plugins'
import { useApp } from '../context'
import { scopedPath } from '../scope'
import { environmentPluginApi, type Descriptor } from '../plugins/environment/api'
import { environmentGroups } from '../plugins/environment/catalog'
import '../plugins/environment/catalog.css'

export default function Plugins() {
  const { items, loading, error } = usePlugins()
  const { projectId } = useApp()
  const environment = items.find(plugin => plugin.id === 'environment')
  const [descriptors, setDescriptors] = useState<Descriptor[]>([])
  const [capabilityError, setCapabilityError] = useState('')
  const [capabilitiesLoading, setCapabilitiesLoading] = useState(false)
  useEffect(() => {
    if (!environment?.installed && !environment?.enabled) return
    let live = true
    setCapabilitiesLoading(true); setCapabilityError('')
    environmentPluginApi.capabilities().then(result => { if (live) setDescriptors(result.items) })
      .catch(e => { if (live) setCapabilityError(String(e.message || e)) })
      .finally(() => { if (live) setCapabilitiesLoading(false) })
    return () => { live = false }
  }, [environment?.installed, environment?.enabled])
  return <div style={{ padding: 24, height: '100%', overflowY: 'auto', boxSizing: 'border-box' }}>
    <Typography.Title level={3}>插件</Typography.Title>
    {loading ? <Spin /> : error ? <Alert type="error" message={error} /> : <div className="plugin-catalog-groups">
      <section aria-label="数据同步">
        <Typography.Title level={4}>数据同步与集成</Typography.Title>
        <div className="plugin-catalog-grid">{items.filter(plugin => plugin.id !== 'environment').map(plugin =>
          <Card size="small" key={plugin.id} title={plugin.name} extra={<Tag color={plugin.enabled ? 'green' : 'default'}>{plugin.enabled ? '已启用' : plugin.installed ? '未启用' : '未安装'}</Tag>}>
            <Typography.Text type="secondary">{plugin.description}</Typography.Text>
            {plugin.enabled ? <Link to={plugin.entry}><Button>打开</Button></Link> : <Typography.Text>请由管理员配置 Shepherd 的插件后重启服务。</Typography.Text>}
          </Card>)}</div>
      </section>
      <div><Typography.Title level={4} style={{ marginBottom: 4 }}>环境配置</Typography.Title>
        <Typography.Text type="secondary">按用途查看全部能力；可用能力由当前服务加载的插件决定。</Typography.Text>
        {environment && <Tag style={{ marginLeft: 12 }} color={environment.enabled ? 'green' : 'default'}>{environment.enabled ? '运行时已启用' : environment.installed ? '运行时未启用' : '运行时未安装'}</Tag>}
      </div>
      {capabilityError && <Alert type="error" message="插件能力读取失败" description={capabilityError} />}
      {environmentGroups.map(group => <section aria-label={group.name} key={group.key}>
        <Typography.Title level={5} style={{ marginTop: 0 }}>{group.name}</Typography.Title>
        <div className="plugin-catalog-grid">{group.items.map(item => {
          const providers = descriptors.filter(descriptor => descriptor.section === item.section)
          const available = environment?.enabled && providers.length > 0
          const status = capabilitiesLoading ? '加载中' : capabilityError ? '状态未知' : !environment?.enabled ? environment?.installed ? '未启用' : '未安装' : providers.length ? '可用' : '待支持'
          const section = item.section === 'display' ? '' : `?section=${encodeURIComponent(item.section)}`
          return <Card size="small" key={item.section} title={item.name} extra={<Tag color={available ? 'green' : 'default'}>{status}</Tag>}>
            <Typography.Text type="secondary">{item.description}</Typography.Text>
            {providers.length > 0 && <Space wrap size={4}>{providers.map(provider => <Tag key={provider.id}>{provider.name}</Tag>)}</Space>}
            {available && projectId ? <Link to={`${scopedPath('/environment', projectId)}${section || '?settings=display'}`}><Button size="small">配置</Button></Link>
              : <Typography.Text type="secondary">{available ? '请先选择项目' : capabilityError ? '请刷新后重试' : '能力可用后在环境中配置'}</Typography.Text>}
          </Card>
        })}</div>
      </section>)}
    </div>}
  </div>
}
