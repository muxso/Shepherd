import { useState, type ReactNode } from 'react'
import { Layout, Menu, Select, Button, Space, Typography, Tooltip } from 'antd'
import {
  ApiOutlined,
  PartitionOutlined,
  LogoutOutlined,
  DeploymentUnitOutlined,
  PlusOutlined,
} from '@ant-design/icons'
import { useLocation, useNavigate } from 'react-router-dom'
import { useApp } from '../context'
import NewProjectModal from './NewProjectModal'

const { Header, Sider, Content } = Layout

export default function AppShell({ children }: { children: ReactNode }) {
  const { projects, projectId, setProjectId, logout } = useApp()
  const nav = useNavigate()
  const loc = useLocation()
  const [newProjOpen, setNewProjOpen] = useState(false)

  const menuItems = [
    { key: '/api/definition', icon: <ApiOutlined />, label: '接口定义' },
    { key: '/api/scenario', icon: <PartitionOutlined />, label: '场景用例' },
  ]

  return (
    <Layout style={{ height: '100vh' }}>
      <Header
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 16,
          background: '#001529',
          paddingInline: 20,
        }}
      >
        <Space style={{ color: '#fff', fontSize: 18, fontWeight: 700 }}>
          <DeploymentUnitOutlined style={{ color: '#1664ff' }} />
          Shepherd
          <Typography.Text style={{ color: '#8a9099', fontSize: 13, fontWeight: 400 }}>接口测试</Typography.Text>
        </Space>
        <div style={{ flex: 1 }} />
        <Space>
          <span style={{ color: '#8a9099' }}>项目</span>
          <Select
            style={{ width: 220 }}
            value={projectId || undefined}
            placeholder="选择项目"
            onChange={setProjectId}
            options={projects.map((p) => ({ value: p.id, label: p.name }))}
            notFoundContent="暂无项目"
          />
          <Tooltip title="新建项目">
            <Button type="text" icon={<PlusOutlined />} style={{ color: '#fff' }} onClick={() => setNewProjOpen(true)} />
          </Tooltip>
          <Tooltip title="退出登录">
            <Button type="text" icon={<LogoutOutlined />} style={{ color: '#fff' }} onClick={logout} />
          </Tooltip>
        </Space>
      </Header>
      <Layout>
        <Sider width={168} theme="light" style={{ borderRight: '1px solid #f0f0f0' }}>
          <Menu
            mode="inline"
            selectedKeys={[loc.pathname]}
            items={menuItems}
            onClick={(e) => nav(e.key)}
            style={{ height: '100%', borderInlineEnd: 'none' }}
          />
        </Sider>
        <Content style={{ background: '#f5f6f8', overflow: 'hidden' }}>{children}</Content>
      </Layout>
      <NewProjectModal open={newProjOpen} onClose={() => setNewProjOpen(false)} />
    </Layout>
  )
}
