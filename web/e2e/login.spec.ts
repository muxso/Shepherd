import { test, expect } from '@playwright/test'
import { LoginPage } from './pages/LoginPage'

const PW = process.env.E2E_PASSWORD || 's3cret'

// Exercise login through the UI.
test('登录:admin/密码 → 进入应用', async ({ page }) => {
  const login = new LoginPage(page)
  await login.goto()
  await login.login('admin', PW)
  await expect(page).not.toHaveURL(/\/login/)
  await expect(page.getByText('需求', { exact: true }).first()).toBeVisible()
})

test('登录:错误密码被拒', async ({ page }) => {
  const login = new LoginPage(page)
  await login.goto()
  await login.login('admin', 'wrong-password-xxx')
  await expect(page).toHaveURL(/\/login/)
})
