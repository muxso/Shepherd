import { test, expect, type Page } from '@playwright/test'

type Preference = { mode: 'light' | 'dark'; primaryColor: string }
async function fixture(page: Page) {
  const saved = new Map<string, Preference>(), writes: Preference[] = []
  await page.addInitScript(() => {
    if (!localStorage.getItem('shepherd.token')) {
      localStorage.setItem('shepherd.token', 'theme-user-a')
      localStorage.setItem('shepherd.userId', 'a')
      localStorage.setItem('shepherd.user', 'Account A')
      localStorage.setItem('shepherd.theme', 'light')
    }
  })
  await page.route('**/*', async route => {
    if (!['fetch', 'xhr'].includes(route.request().resourceType())) return route.continue()
    const path = new URL(route.request().url()).pathname
    const account = route.request().headers().authorization?.endsWith('theme-user-b') ? 'b' : 'a'
    let body: unknown = { items: [], total: 0 }
    if (path === '/auth/theme') {
      if (route.request().method() === 'PUT') {
        const preference = route.request().postDataJSON() as Preference
        saved.set(account, preference); writes.push(preference)
      }
      body = { userId: account, theme: saved.get(account) || null }
    }
    if (path === '/auth/me') body = { id: account, userId: account, username: `Account ${account.toUpperCase()}`, roles: [], permissions: [] }
    if (path === '/organization') body = { items: [{ id: 'org', name: 'Organization' }] }
    if (path === '/project') body = { items: [{ id: 'project', name: 'Project', organizationId: 'org' }] }
    if (path === '/api/plugins') body = { items: [] }
    await route.fulfill({ json: body })
  })
  return { saved, writes }
}
async function appearance(page: Page) {
  await page.locator('.ant-avatar').first().click()
  await page.getByText('个人中心', { exact: true }).click()
  await page.getByRole('button', { name: '主题与外观', exact: true }).click()
  return page.getByRole('region', { name: '个人主题设置' })
}

test('presets preview without writes; custom themes persist and survive reload', async ({ page }) => {
  const { writes } = await fixture(page)
  await page.goto('/home')
  await expect(page.locator('html')).toHaveAttribute('data-primary-color', '#4D9F92')
  let panel = await appearance(page)
  for (const name of ['青松绿', '海盐蓝', '湖水青', '鸢尾紫', '暖杏橙']) {
    await panel.getByRole('button', { name, exact: true }).click()
    await expect(panel.getByRole('button', { name, exact: true })).toHaveAttribute('aria-pressed', 'true')
  }
  expect(writes).toHaveLength(0)
  await expect(page.locator('html')).toHaveAttribute('data-primary-color', '#4D9F92')
  await panel.getByLabel('自定义主题色', { exact: true }).fill('#zzyyxx')
  await expect(panel.getByRole('button', { name: '保存并应用主题' })).toBeDisabled()
  await panel.getByLabel('自定义主题色', { exact: true }).fill('#ecf8e9')
  await panel.getByText('深色', { exact: true }).click()
  await panel.getByRole('button', { name: '保存并应用主题' }).click()
  await expect(page.locator('html')).toHaveAttribute('data-primary-color', '#ECF8E9')
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark')
  expect(writes).toEqual([{ mode: 'dark', primaryColor: '#ECF8E9' }])
  await page.reload()
  await expect(page.locator('html')).toHaveAttribute('data-primary-color', '#ECF8E9')
  panel = await appearance(page)
  await expect(panel.getByLabel('自定义主题色', { exact: true })).toHaveValue('#ECF8E9')
  await panel.getByRole('button', { name: '恢复默认绿色' }).click()
  await panel.getByRole('button', { name: '保存并应用主题' }).click()
  await expect(page.locator('html')).toHaveAttribute('data-primary-color', '#4D9F92')
})

test('account changes do not inherit another account theme or a stale response', async ({ page }) => {
  const { saved } = await fixture(page)
  saved.set('a', { mode: 'light', primaryColor: '#8064AD' })
  saved.set('b', { mode: 'dark', primaryColor: '#328C96' })
  await page.goto('/home')
  await expect(page.locator('html')).toHaveAttribute('data-primary-color', '#8064AD')
  let pending: (() => Promise<void>) | undefined
  await page.route('**/auth/theme', async route => {
    if (route.request().headers().authorization?.endsWith('theme-user-a')) {
      pending = () => route.fulfill({ json: { userId: 'a', theme: { mode: 'light', primaryColor: '#B67842' } } })
      return
    }
    await route.fallback()
  })
  await page.evaluate(() => window.dispatchEvent(new Event('shepherd-session-change')))
  await expect.poll(() => !!pending).toBe(true)
  await page.evaluate(() => {
    localStorage.setItem('shepherd.token', 'theme-user-b')
    localStorage.setItem('shepherd.userId', 'b')
    window.dispatchEvent(new Event('shepherd-session-change'))
  })
  await expect(page.locator('html')).toHaveAttribute('data-primary-color', '#328C96')
  await pending!()
  await expect(page.locator('html')).toHaveAttribute('data-primary-color', '#328C96')
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark')
})

test('light and dark custom colors remain readable and settings fit a narrow screen', async ({ page }) => {
  await fixture(page)
  await page.setViewportSize({ width: 700, height: 1000 })
  await page.goto('/home')
  const panel = await appearance(page)
  for (const mode of ['浅色', '深色']) {
    await panel.getByText(mode, { exact: true }).click()
    await panel.getByLabel('自定义主题色', { exact: true }).fill(mode === '浅色' ? '#FFFFFF' : '#000000')
    await panel.getByRole('button', { name: '保存并应用主题' }).click()
    await expect(page.locator('html')).toHaveAttribute('data-primary-color', mode === '浅色' ? '#FFFFFF' : '#000000')
    const ratio = await page.evaluate(() => {
      const color = getComputedStyle(document.documentElement).getPropertyValue('--brand').trim()
      const channels = [1, 3, 5].map(i => parseInt(color.slice(i, i + 2), 16) / 255)
      const luminance = channels.map(s => s <= .04045 ? s / 12.92 : ((s + .055) / 1.055) ** 2.4).reduce((sum, v, i) => sum + v * [.2126, .7152, .0722][i], 0)
      const bg = document.documentElement.dataset.theme === 'light' ? 1 : .01372
      return (Math.max(luminance, bg) + .05) / (Math.min(luminance, bg) + .05)
    })
    expect(ratio).toBeGreaterThanOrEqual(4.49)
  }
  const bounds = await panel.evaluate(el => ({ width: el.clientWidth, scroll: el.scrollWidth }))
  expect(bounds.scroll).toBeLessThanOrEqual(bounds.width + 1)
  await page.screenshot({ path: '/tmp/shepherd-personal-theme-narrow.png', fullPage: true, animations: 'disabled' })
})
