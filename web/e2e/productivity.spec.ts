import { test, expect, type Page } from '@playwright/test'

test.use({ viewport: { width: 1440, height: 1080 } })
const sources = ['alpha', 'beta'].map(id => ({ id: `source-${id}`, name: id, project_id: id, last_success_at: '2026-11-01T01:00:00Z', code_since: '2026-10-01', code_until: '2026-11-01' }))
function dashboard(project: string, days: number) {
  const metrics = Object.fromEntries(['requirements', 'delivered', 'testcases', 'bugs', 'linkedCommits', 'commits', 'additions', 'deletions', 'unknownStats'].map((key, i) => [key, {
    current: project === 'alpha' ? 12 + i : 101 + i, previous: i ? 10 : 0, missingTime: 2, unattributed: 1,
    trend: Array.from({ length: days }, (_, day) => ({ date: `2026-10-${String(day + 1).padStart(2, '0')}`, value: day % 5 })),
    contributors: [{ name: `${project} 创建人`, count: project === 'alpha' ? 12 + i : 101 + i }],
  }]))
  return { authors: [], totals: { requirements: 80, delivered: 13, delayed: 2 }, productivity: { from: '2026-10-03', through: '2026-11-01', timezone: 'Asia/Shanghai', metrics } }
}
async function fixture(page: Page, options: { enabled?: boolean; failed?: boolean; linked?: boolean } = {}) {
  await page.addInitScript(() => {
    localStorage.setItem('shepherd.token', 'output-test')
    localStorage.setItem('shepherd.theme', 'light')
    localStorage.setItem('shepherd.projectId', 'alpha')
    localStorage.setItem('shepherd.home.cards.v7', '[]')
  })
  const requests: string[] = []
  await page.route('**/*', async route => {
    if (!['fetch', 'xhr'].includes(route.request().resourceType())) return route.continue()
    const url = new URL(route.request().url()), path = url.pathname
    let body: unknown = { items: [], total: 0 }
    if (path === '/organization') body = { items: [{ id: 'org' }] }
    if (path === '/project') body = { items: ['alpha', 'beta'].map(id => ({ id, name: id, organizationId: 'org' })) }
    if (path === '/api/plugins') body = { items: [{ id: 'yunxiao', name: 'Yunxiao', enabled: options.enabled !== false }] }
    if (path === '/api/devops/sources') body = { items: options.linked === false ? [] : sources }
    if (path.includes('/dashboard')) {
      requests.push(url.pathname + url.search)
      if (options.failed) return route.fulfill({ status: 503, body: 'Statistics temporarily unavailable' })
      body = dashboard(path.includes('alpha') ? 'alpha' : 'beta', Number(url.searchParams.get('days')))
    }
    if (['/api/definition', '/api/scenario', '/functional-case', '/api/exec-trend'].includes(path)) body = []
    if (['/delivery/collab-stats', '/api/case-exec-summary'].includes(path)) body = null
    await route.fulfill({ json: body })
  })
  return requests
}

test('workbench offers role metrics, daily trends and interval comparison', async ({ page }) => {
  const requests = await fixture(page)
  await page.goto('/home')
  const panel = page.getByRole('region', { name: '岗位产出统计' })
  await expect(panel.getByRole('button', { name: /新增需求/ })).toContainText('12')
  await expect(panel.getByText('较上期新增 12')).toBeVisible()
  await expect(panel.getByRole('meter')).toBeVisible()
  await panel.getByText('测试', { exact: true }).click()
  await expect(panel.getByRole('button', { name: /新增用例/ })).toContainText('14')
  await expect(panel.getByRole('button', { name: /发现缺陷/ })).toContainText('15')
  await panel.getByRole('button', { name: /发现缺陷/ }).click()
  await expect(panel.getByText('发现缺陷趋势')).toBeVisible()
  await panel.getByText('研发', { exact: true }).click()
  await expect(panel.getByRole('button', { name: /需求关联提交/ })).toContainText('16')
  await panel.getByRole('button', { name: '统计口径' }).click()
  await expect(page.getByText(/次行数未知/)).toBeVisible()
  await panel.getByRole('heading', { name: '岗位产出' }).click()
  await panel.getByRole('combobox', { name: '产出统计区间' }).press('ArrowDown')
  await page.locator('.ant-select-item-option').filter({ hasText: '最近 7 天' }).click()
  await expect.poll(() => requests.at(-1)).toContain('days=7')
  await expect(panel.locator('.yx-block-column')).toHaveCount(7)
  await page.getByRole('heading', { name: '岗位产出', exact: true }).click()
  await page.screenshot({ path: '/tmp/shepherd-productivity-desktop.png', fullPage: true })
})

test('home statistics follow the selected project without showing another source', async ({ page }) => {
  const requests = await fixture(page)
  await page.goto('/home')
  const panel = page.getByRole('region', { name: '岗位产出统计' })
  await expect(panel.getByRole('button', { name: /新增需求/ })).toContainText('12')
  await page.getByRole('combobox', { name: '项目切换' }).press('ArrowDown')
  await page.locator('.ant-select-item-option').filter({ hasText: 'beta' }).click()
  await expect(page).toHaveURL(/\/home$/)
  await expect(panel.getByRole('button', { name: /新增需求/ })).toContainText('101')
  await expect(panel.getByText('alpha 创建人')).toHaveCount(0)
  expect(requests.at(-1)).toContain('source-beta')
  await page.setViewportSize({ width: 700, height: 1000 })
  await expect(panel.getByRole('button', { name: /新增需求/ })).toBeVisible()
  expect(await panel.evaluate(el => el.scrollWidth <= el.clientWidth)).toBe(true)
  await page.screenshot({ path: '/tmp/shepherd-productivity-mobile.png', fullPage: true })
})

for (const state of ['disabled', 'unlinked', 'failed'] as const) {
  test(`workbench handles ${state} plugin statistics without inventing values`, async ({ page }) => {
    const requests = await fixture(page, { enabled: state !== 'disabled', linked: state !== 'unlinked', failed: state === 'failed' })
    const pluginCalls: string[] = []
    page.on('request', request => { if (new URL(request.url()).pathname.startsWith('/api/devops/')) pluginCalls.push(request.url()) })
    await page.goto('/home')
    if (state === 'disabled' || state === 'unlinked') {
      await expect(page.getByText('编辑', { exact: true })).toBeVisible()
      await expect(page.getByRole('region', { name: '岗位产出统计' })).toHaveCount(0)
      expect(requests).toHaveLength(0)
      if (state === 'disabled') expect(pluginCalls).toHaveLength(0)
    } else {
      await expect(page.getByText('岗位产出读取失败', { exact: true })).toBeVisible()
      await expect(page.getByRole('button', { name: /新增需求/ })).toHaveCount(0)
    }
  })
}

test('template categories and actions share one compact toolbar', async ({ page }) => {
  await fixture(page)
  await page.goto('/p/alpha/project/templates')
  const tab = page.getByRole('tab', { name: '需求', exact: true })
  const add = page.getByRole('button', { name: /添加字段/ })
  await expect(add).toBeVisible()
  const tabBox = await tab.boundingBox(), addBox = await add.boundingBox()
  expect(Math.abs(tabBox!.y + tabBox!.height / 2 - addBox!.y - addBox!.height / 2)).toBeLessThan(8)
  await page.getByRole('tab', { name: '缺陷', exact: true }).click()
  await expect(page.getByRole('tab', { name: '缺陷', exact: true })).toHaveAttribute('aria-selected', 'true')
  await page.screenshot({ path: '/tmp/shepherd-template-compact.png', fullPage: true })
  await add.click()
  await expect(page.getByRole('textbox', { name: '字段名' })).toBeVisible()
})

test('case execution and pass rings display percentages and fit half-width cards', async ({ page }) => {
  await fixture(page, { enabled: false })
  await page.addInitScript(() => localStorage.setItem('shepherd.home.cards.v7', JSON.stringify([{ key: 'caseStats', size: 'half' }])))
  await page.route('**/api/case?**', route => route.fulfill({ json: { items: [], total: 378 } }))
  await page.route('**/api/case-exec-summary?**', route => route.fulfill({ json: { executedCases: 302, executions: 1442, passed: 1257 } }))
  await page.goto('/home')
  const execution = page.getByRole('figure', { name: '执行率 79.9%' })
  const passed = page.getByRole('figure', { name: '通过率 87.2%' })
  await expect(execution.locator('svg').getByText('79.9%', { exact: true })).toBeVisible()
  await expect(passed.locator('svg').getByText('87.2%', { exact: true })).toBeVisible()
  for (const width of [1920, 1280, 700]) {
    await page.setViewportSize({ width, height: 1000 })
    for (const figure of [execution, passed]) {
      const inside = await figure.evaluate(el => {
        const card = el.closest('.ant-card')!.getBoundingClientRect()
        return [...el.querySelectorAll('svg, b')].every(item => { const bounds = item.getBoundingClientRect(); return bounds.left >= card.left && bounds.right <= card.right })
      })
      expect(inside).toBe(true)
    }
  }
  await page.screenshot({ path: '/tmp/shepherd-case-rates.png', fullPage: true })
})

test('1330 requirements use bounded coverage summaries and no per-row requests', async ({ page }) => {
  await fixture(page, { enabled: false })
  const requirements = Array.from({ length: 1330 }, (_, index) => ({ id: `req-${index}`, title: `Requirement ${index}`, status: 'DRAFT', acceptanceCriteria: ['first', 'second'] }))
  let summaries = 0, perRow = 0
  await page.route('**/requirement?**', route => {
    const current = Number(new URL(route.request().url()).searchParams.get('current'))
    return route.fulfill({ json: { items: requirements.slice((current - 1) * 200, current * 200), total: 1330, totalPages: 7, current } })
  })
  await page.route('**/requirement/**/functional-coverage', route => { perRow++; return route.fulfill({ json: [] }) })
  await page.route('**/requirement/functional-coverage?**', route => { summaries++; return route.fulfill({ json: { 'req-0': [0] } }) })
  await page.goto('/p/alpha/requirement')
  await expect(page.getByText(/^Requirement 0/)).toBeVisible()
  await expect(page.getByText('50%', { exact: true }).first()).toBeVisible()
  // React StrictMode may replay mount effects in the Vite dev server; production mounts once.
  expect(summaries).toBeGreaterThan(0)
  expect(summaries).toBeLessThanOrEqual(2)
  expect(perRow).toBe(0)
})


test('Yunxiao output follows the page source and interval controls', async ({ page }) => {
  await fixture(page)
  await page.goto('/devops')
  const panel = page.getByRole('region', { name: '岗位产出统计' })
  await expect(panel.locator('.yx-block-column')).toHaveCount(7)
  await page.getByRole('combobox', { name: '统计区间', exact: true }).press('ArrowDown')
  await page.locator('.ant-select-item-option').filter({ hasText: '最近 30 天' }).click()
  await expect(panel.locator('.yx-block-column')).toHaveCount(30)
  await page.getByRole('combobox', { name: '云效项目', exact: true }).press('ArrowDown')
  await page.locator('.ant-select-item-option').filter({ hasText: 'beta' }).click()
  await expect(panel.getByRole('button', { name: /新增需求/ })).toContainText('101')
  await expect(panel.getByText('alpha 创建人')).toHaveCount(0)
})


test('trend popovers load concrete records on demand, cache them and link to filtered details', async ({ page }) => {
  await fixture(page)
  const lookups: string[] = []
  await page.route('**/api/devops/sources/*/records?**', route => {
    const url = new URL(route.request().url()), metric = url.searchParams.get('metric')
    if (!metric) return route.fulfill({ json: { items: [], total: 0 } })
    lookups.push(metric)
    return route.fulfill({ json: { total: 12, items: [{ externalId: 'req-42', serial: 'REQ-42', title: metric === 'testcases' ? '登录校验用例' : metric === 'linkedCommits' ? '实现登录交付' : '登录需求交付', requirements: metric === 'linkedCommits' ? [{ externalId: 'linked', serial: 'REQ-18', title: '关联登录需求' }] : [] }] } })
  })
  await page.goto('/devops')
  const panel = page.getByRole('region', { name: '岗位产出统计' })
  await expect(panel.locator('.yx-block-column')).toHaveCount(7)
  expect(lookups).toHaveLength(0)
  const bar = panel.locator('.yx-block-column').nth(1)
  await bar.hover()
  const popup = page.getByLabel('当日产出明细')
  await expect(popup.getByText('登录需求交付')).toBeVisible()
  await expect(popup.getByText('REQ-42')).toBeVisible()
  expect(lookups).toEqual(['requirements'])
  await panel.getByRole('heading', { name: '岗位产出' }).hover()
  await bar.hover()
  expect(lookups).toEqual(['requirements'])
  await panel.getByRole('button', { name: /已交付需求/ }).click()
  await expect(panel.getByText('已交付需求趋势', { exact: true })).toBeVisible()
  expect(lookups).toEqual(['requirements'])
  await bar.hover()
  await expect.poll(() => lookups.at(-1)).toBe('delivered')
  await expect(popup.getByText('登录需求交付')).toBeVisible()
  await panel.getByText('测试', { exact: true }).click()
  await bar.hover()
  await expect(popup.getByText('登录校验用例')).toBeVisible()
  await panel.getByText('研发', { exact: true }).click()
  await bar.hover()
  await expect(popup.getByText(/关联登录需求/)).toBeVisible()
  await popup.getByRole('link', { name: '查看全部 12 条记录' }).click()
  await expect(page.getByRole('tab', { name: '明细记录', exact: true })).toHaveAttribute('aria-selected', 'true')
  await expect(page).toHaveURL(/metric=linkedCommits&date=2026-10-02/)
  await expect(page.getByRole('cell', { name: '实现登录交付', exact: true })).toBeVisible()
})

for (const mode of ['light','dark']) {
  test(`plugin uses shared layout, brand colors and a single compact notes entry in ${mode} mode`, async ({ page }) => {
    const requests = await fixture(page)
    await page.addInitScript(mode => localStorage.setItem('shepherd.theme', mode), mode)
    await page.goto('/devops')
    const panel = page.getByRole('region', { name: '岗位产出统计' })
    await expect(panel.getByRole('button', { name: /新增需求/ })).toBeVisible()
    await expect(page.getByRole('heading', { name: '云效研发统计' })).toHaveCount(0)
    await expect(page.getByLabel('云效项目').first()).toBeVisible()
    await expect(page.getByRole('button', { name: '统计口径' })).toHaveCount(1)
    await expect(page.locator('.ant-alert-info')).toHaveCount(0)
    expect(requests.filter(path => path.includes('dashboard'))).toHaveLength(1)
    for (const role of ['产品','测试','研发']) {
      await panel.getByText(role, { exact: true }).click()
      const consistent = await panel.locator('.yx-block-column .filled').first().evaluate(el => {
        const swatch = document.createElement('span'); swatch.style.color = 'var(--brand)'; document.body.append(swatch)
        const expected = getComputedStyle(swatch).color; swatch.remove()
        return getComputedStyle(el).backgroundColor === expected
      })
      expect(consistent).toBe(true)
    }
    await page.screenshot({ path: `/tmp/shepherd-devops-${mode}.png`, fullPage: true })
  })
}
