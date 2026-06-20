import { useState } from 'react'
import { Card, Form, Input, Button, Typography, message } from 'antd'
import { DeploymentUnitOutlined } from '@ant-design/icons'
import { api, ApiError } from '../api'
import { useApp } from '../context'

export default function Login() {
  const { login } = useApp()
  const [loading, setLoading] = useState(false)

  const onFinish = async (v: { username: string; password: string }) => {
    setLoading(true)
    try {
      const { token } = await api.login(v.username, v.password)
      login(token)
      message.success('登录成功')
    } catch (e) {
      message.error(e instanceof ApiError ? `登录失败:${e.status}` : '登录失败')
    } finally {
      setLoading(false)
    }
  }

  return (
    <div
      style={{
        height: '100vh',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        background: 'linear-gradient(135deg,#0b1f3a,#1664ff22)',
      }}
    >
      <Card style={{ width: 360 }} variant="borderless">
        <div style={{ textAlign: 'center', marginBottom: 24 }}>
          <DeploymentUnitOutlined style={{ fontSize: 36, color: '#1664ff' }} />
          <Typography.Title level={3} style={{ margin: '8px 0 0' }}>
            Shepherd
          </Typography.Title>
          <Typography.Text type="secondary">接口测试管理平台</Typography.Text>
        </div>
        <Form layout="vertical" onFinish={onFinish} initialValues={{ username: 'admin' }}>
          <Form.Item name="username" label="用户名" rules={[{ required: true }]}>
            <Input size="large" placeholder="admin" autoFocus />
          </Form.Item>
          <Form.Item name="password" label="密码" rules={[{ required: true }]}>
            <Input.Password size="large" placeholder="请输入密码" />
          </Form.Item>
          <Button type="primary" size="large" htmlType="submit" block loading={loading}>
            登录
          </Button>
        </Form>
      </Card>
    </div>
  )
}
