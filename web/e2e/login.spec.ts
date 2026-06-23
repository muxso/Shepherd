import { test, expect } from '@playwright/test'

// 真实登录流程(不走 fixture 注入,测登录 UI 本身)。
test('登录:admin/密码 → 进入应用', async ({ page }) => {
  await page.goto('/login')
  await page.getByPlaceholder('admin').fill('admin')
  await page.locator('input[type="password"]').fill(process.env.E2E_PASSWORD || 's3cret')
  await page.locator('button[type="submit"]').click()
  // 成功 → 离开登录页,侧栏可见。
  await expect(page).not.toHaveURL(/\/login/)
  await expect(page.getByText('需求', { exact: true }).first()).toBeVisible()
})

test('登录:错误密码被拒', async ({ page }) => {
  await page.goto('/login')
  await page.getByPlaceholder('admin').fill('admin')
  await page.locator('input[type="password"]').fill('wrong-password-xxx')
  await page.locator('button[type="submit"]').click()
  await expect(page).toHaveURL(/\/login/)
})
