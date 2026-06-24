import { useState } from 'react'
import { Card, Form, Input, Button, Typography } from 'antd'
import { message } from '../feedback'
import { DeploymentUnitOutlined } from '@ant-design/icons'
import { api, ApiError, userStore } from '../api'
import { useApp } from '../context'
import { useI18n } from '../i18n'

export default function Login() {
  const { login } = useApp()
  const { t } = useI18n()
  const [loading, setLoading] = useState(false)

  const onFinish = async (v: { username: string; password: string }) => {
    setLoading(true)
    try {
      const { token } = await api.login(v.username, v.password)
      userStore.set(v.username) // 供个人中心 / 创建人列展示(后端暂无 /me)
      login(token)
      message.success(t('login.ok', '登录成功'))
    } catch (e) {
      message.error(e instanceof ApiError ? `${t('login.fail', '登录失败')}:${e.status}` : t('login.fail', '登录失败'))
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
        background: 'linear-gradient(135deg,#0b1f3a,var(--brand)22)',
      }}
    >
      <Card style={{ width: 360 }} variant="borderless">
        <div style={{ textAlign: 'center', marginBottom: 24 }}>
          <DeploymentUnitOutlined style={{ fontSize: 36, color: 'var(--brand)' }} />
          <Typography.Title level={3} style={{ margin: '8px 0 0' }}>
            Shepherd
          </Typography.Title>
          <Typography.Text type="secondary">{t('login.subtitle', '接口测试管理平台')}</Typography.Text>
        </div>
        <Form layout="vertical" onFinish={onFinish} initialValues={{ username: 'admin' }}>
          <Form.Item name="username" label={t('login.username', '用户名')} rules={[{ required: true }]}>
            <Input size="large" placeholder="admin" autoFocus />
          </Form.Item>
          <Form.Item name="password" label={t('login.password', '密码')} rules={[{ required: true }]}>
            <Input.Password size="large" placeholder={t('login.passwordPlaceholder', '请输入密码')} />
          </Form.Item>
          <Button type="primary" size="large" htmlType="submit" block loading={loading}>
            {t('login.submit', '登录')}
          </Button>
        </Form>
      </Card>
    </div>
  )
}
