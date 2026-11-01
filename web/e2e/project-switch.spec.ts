import { test, expect, type Page } from '@playwright/test'

test.use({ viewport: { width: 1920, height: 1080 } })

async function selectProject(page: Page, name: string) {
  await page.getByRole('combobox', { name: '项目切换' }).press('ArrowDown')
  await page.locator('.ant-select-item-option').filter({ hasText: name }).click()
}

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem('shepherd.token', 'project-switch-test')
    localStorage.setItem('shepherd.projectId', 'alpha')
  })
  await page.route('**/*', async route => {
    if (!['fetch', 'xhr'].includes(route.request().resourceType())) return route.continue()
    const url = new URL(route.request().url())
    let body: unknown = { items: [], total: 0 }
    if (url.pathname === '/organization') body = { items: [{ id: 'org', name: 'Organization' }] }
    if (url.pathname === '/project') body = { items: [
      { id: 'alpha', name: '资产数字化', organizationId: 'org' },
      { id: 'beta', name: '云效 · 需求池', organizationId: 'org' },
    ] }
    if (url.pathname === '/api/environment') body = [{
      id: `${url.searchParams.get('projectId')}-env`, projectId: url.searchParams.get('projectId'),
      name: `${url.searchParams.get('projectId')} environment`, baseUrl: 'https://example.com', variables: {},
    }]
    await route.fulfill({ json: body })
  })
})

test('switching after SPA navigation uses the current route and resets project state', async ({ page }) => {
  await page.goto('/system/plugins')
  await expect(page.getByRole('combobox', { name: '项目切换' })).toBeVisible()
  // Use real client navigation: a reload would conceal the stale callback regression.
  await page.getByText('项目', { exact: true }).click()
  await page.getByRole('menuitem', { name: /环境/ }).click()
  await expect(page).toHaveURL(/\/p\/alpha\/environment$/)
  await page.getByText('alpha environment', { exact: true }).click()
  await selectProject(page, '云效 · 需求池')
  await expect(page).toHaveURL(/\/p\/beta\/environment$/)
  await expect(page.getByText('beta environment', { exact: true })).toBeVisible()
  await expect(page.getByText('alpha environment', { exact: true })).toHaveCount(0)
  expect(await page.evaluate(() => localStorage.getItem('shepherd.projectId'))).toBe('beta')
  await page.goBack()
  await expect(page).toHaveURL(/\/p\/alpha\/environment$/)
  await expect(page.getByText('alpha environment', { exact: true })).toBeVisible()
})

test('switching clears old detail/view IDs but keeps unrelated query settings', async ({ page }) => {
  await page.goto('/p/alpha/environment?open=old-detail&view=old-view&mode=list')
  await expect(page.getByText('alpha environment', { exact: true })).toBeVisible()
  await selectProject(page, '云效 · 需求池')
  await expect(page).toHaveURL(/\/p\/beta\/environment\?mode=list$/)
  await page.reload()
  await expect(page.getByText('beta environment', { exact: true })).toBeVisible()
})

test('switching on a global page updates the default without changing the page', async ({ page }) => {
  await page.goto('/system/plugins')
  await selectProject(page, '云效 · 需求池')
  await expect(page).toHaveURL(/\/system\/plugins$/)
  await page.getByText('项目', { exact: true }).click()
  await page.getByRole('menuitem', { name: /环境/ }).click()
  await expect(page).toHaveURL(/\/p\/beta\/environment$/)
  await expect(page.getByText('beta environment', { exact: true })).toBeVisible()
})
