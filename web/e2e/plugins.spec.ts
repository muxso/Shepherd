import { test, expect } from '@playwright/test'

for (const enabled of [false, true]) {
  test(`Yunxiao plugin ${enabled ? 'enabled' : 'disabled'} controls page access`, async ({ page }) => {
    await page.addInitScript(() => localStorage.setItem('shepherd.token', 'plugin-test'))
    let syncRequests = 0
    await page.route('**/*', async route => {
      if (!['fetch', 'xhr'].includes(route.request().resourceType())) return route.continue()
      const path = new URL(route.request().url()).pathname
      if (path.startsWith('/api/devops/')) syncRequests++
      const body = path === '/api/plugins'
        ? { items: [{ id: 'yunxiao', name: '云效同步', description: '单向同步到 Shepherd', entry: '/devops', installed: true, enabled }] }
        : { items: [], total: 0 }
      await route.fulfill({ json: body })
    })
    await page.goto('/system/plugins')
    await expect(page.getByText('云效同步', { exact: true })).toBeVisible()
    await expect(page.getByText(enabled ? '已启用' : '未启用', { exact: true })).toBeVisible()
    await page.goto('/devops')
    if (enabled) {
      await expect(page.getByRole('tab', { name: '概览', exact: true })).toBeVisible()
      await expect.poll(() => syncRequests).toBeGreaterThan(0)
      await expect(page.getByRole('menuitem', { name: '云效研发统计' })).toBeVisible()
    } else {
      await expect(page.getByText('插件未启用', { exact: true })).toBeVisible()
      await expect(page.getByRole('menuitem', { name: '云效研发统计' })).toHaveCount(0)
      expect(syncRequests).toBe(0)
    }
  })
}

test('environment capabilities are all visible in categorized plugin cards', async ({ page }) => {
  await page.addInitScript(() => { localStorage.setItem('shepherd.token', 'catalog-test'); localStorage.setItem('shepherd.projectId', 'project') })
  await page.route('**/*', async route => {
    if (!['fetch', 'xhr'].includes(route.request().resourceType())) return route.continue()
    const path = new URL(route.request().url()).pathname
    const body = path === '/api/plugins' ? { items: [{ id: 'environment', name: '环境配置', installed: true, enabled: true }] }
      : path === '/api/environment-plugins/capabilities' ? { enabled: true, items: [{ id: 'postgres', name: 'PostgreSQL', section: 'database' }] }
      : { items: [], total: 0 }
    await route.fulfill({ json: body })
  })
  await page.goto('/system/plugins')
  for (const name of ['数据连接', '网络与协议', '处理与校验', '界面扩展']) await expect(page.getByRole('region', { name, exact: true })).toBeVisible()
  for (const name of ['数据库', 'HOST', '前置', '后置', '断言', 'SSH配置', 'AMQP配置', 'Redis 配置', 'TCP配置', 'MongoDB 配置', 'GRPC配置', '显示设置']) {
    await expect(page.locator('.ant-card-head-title').filter({ hasText: new RegExp(`^${name}$`) })).toBeVisible()
  }
  const database = page.locator('.ant-card').filter({ has: page.locator('.ant-card-head-title').getByText('数据库', { exact: true }) })
  await expect(database.getByText('可用', { exact: true })).toBeVisible()
  await expect(database.getByRole('link')).toHaveAttribute('href', '/p/project/environment?section=database')
  const amqp = page.locator('.ant-card').filter({ has: page.locator('.ant-card-head-title').getByText('AMQP配置', { exact: true }) })
  await expect(amqp.getByText('待支持', { exact: true })).toBeVisible()
  await expect(amqp.getByRole('link')).toHaveCount(0)
  await page.screenshot({ path: '/tmp/shepherd-plugin-catalog.png', fullPage: true })
})
