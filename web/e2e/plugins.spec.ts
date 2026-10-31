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
      await expect(page.getByRole('heading', { name: '云效研发统计' })).toBeVisible()
      await expect.poll(() => syncRequests).toBeGreaterThan(0)
      await expect(page.getByRole('menuitem', { name: '云效研发统计' })).toBeVisible()
    } else {
      await expect(page.getByText('插件未启用', { exact: true })).toBeVisible()
      await expect(page.getByRole('menuitem', { name: '云效研发统计' })).toHaveCount(0)
      expect(syncRequests).toBe(0)
    }
  })
}
