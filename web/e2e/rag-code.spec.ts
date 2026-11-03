import { test, expect } from '@playwright/test'

test('large code knowledge library uses server pages and search resets the page', async ({ page }, testInfo) => {
  const calls: URL[] = []
  await page.addInitScript(() => {
    localStorage.setItem('shepherd.token', 'rag-test')
    localStorage.setItem('shepherd.projectId', 'project')
    sessionStorage.setItem('rag_intro_seen', '1')
  })
  await page.route('**/*', async route => {
    if (!['fetch', 'xhr'].includes(route.request().resourceType())) return route.continue()
    const url = new URL(route.request().url())
    let body: unknown = { items: [], total: 0 }
    if (url.pathname === '/organization') body = { items: [{ id: 'org' }] }
    if (url.pathname === '/project') body = { items: [{ id: 'project', name: 'Code knowledge', organizationId: 'org' }] }
    if (url.pathname === '/rag/stats') body = { documents: 85000, chunks: 200000 }
    if (url.pathname === '/rag/visibility-group') body = []
    if (url.pathname === '/rag/document') {
      calls.push(url)
      const n = Number(url.searchParams.get('page') || 1)
      body = { items: [{ id: `doc-${n}`, title: `sample/src/OrderService.java @abc page ${n}`, sourceType: 'code', visibilityGroups: [], updatedAt: 0 }], total: 85000, current: n, pageSize: 20 }
    }
    await route.fulfill({ json: body })
  })
  await page.goto('/p/project/chat')
  await page.getByRole('button', { name: '知识库管理' }).click()
  const drawer = page.getByRole('dialog')
  await expect(drawer.getByText('sample/src/OrderService.java @abc page 1')).toBeVisible()
  await drawer.getByTitle('2', { exact: true }).click()
  await expect(drawer.getByText('sample/src/OrderService.java @abc page 2')).toBeVisible()
  const search = drawer.getByPlaceholder('搜索仓库、路径或文档标题')
  await search.fill('OrderService')
  await search.press('Enter')
  await expect.poll(() => calls.at(-1)?.searchParams.get('search')).toBe('OrderService')
  await expect.poll(() => calls.at(-1)?.searchParams.get('page')).toBe('1')
  await expect(drawer.locator('tbody tr.ant-table-row')).toHaveCount(1)
  await page.screenshot({ path: testInfo.outputPath('rag-pagination.png') })
})
