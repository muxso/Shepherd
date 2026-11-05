import { test, expect } from '@playwright/test'

test('reference cards retain full CJK and long Latin names inside their bounds in both layouts', async ({ page }, testInfo) => {
  test.setTimeout(90_000)
  const names = ['里程OCR E2E 非法Base64输入拒绝', '里程OCR E2E 缺少全部凭据', '里程OCR E2E 拒绝覆盖模型', '里程OCR E2E 未知租户隔离', 'LongUnbrokenRepositoryAndScenarioName'.repeat(5), '跨午夜提交校验😀'.repeat(8)]
  const definition = { id: 'definition', projectId: 'project', name: '里程OCR E2E 里程识别接口及租户权限验证', protocol: 'HTTP', method: 'POST', path: '/ocr', status: 'COMPLETED', tags: [] }
  await page.addInitScript(() => { localStorage.setItem('shepherd.token', 'graph-test'); localStorage.setItem('shepherd.projectId', 'project'); localStorage.setItem('shepherd.theme', 'light') })
  await page.route('**/*', async route => {
    if (!['fetch', 'xhr'].includes(route.request().resourceType())) return route.continue()
    const path = new URL(route.request().url()).pathname
    let body: unknown = { items: [], total: 0 }
    if (path === '/organization') body = { items: [{ id: 'org' }] }
    if (path === '/project') body = { items: [{ id: 'project', name: 'OCR', organizationId: 'org' }] }
    if (path === '/api/module') body = []
    if (path === '/api/definition') body = [definition]
    if (path === '/api/definition/definition') body = definition
    if (path.endsWith('/references')) body = { cases: names.map((name, i) => ({ id: `case-${i}`, name })), scenarios: [{ id: 'scenario', name: '跨系统场景与异常链路验证' }] }
    await route.fulfill({ json: body })
  })
  await page.goto('/p/project/api/definition?open=definition', {waitUntil:'domcontentloaded'})
  await page.getByRole('tab', { name: '引用关系', exact: true }).click({timeout:20000})
  const nodes = page.locator('[data-reference-node]')
  await expect(nodes).toHaveCount(7)
  for (const [index, name] of names.entries()) {
    const node = page.locator(`[data-reference-node="case-${index}"]`)
    expect(await node.locator('text').textContent()).toBe(name)
  }
  const checkBounds = async () => {
    const results = await nodes.evaluateAll(elements => elements.map(el => {
      const rect = el.querySelector('rect:last-of-type') as SVGRectElement
      const box = rect.getBBox()
      const lines = [...el.querySelectorAll('tspan')].map(line => line.getBBox())
      const viewport = el.ownerSVGElement!.getBoundingClientRect()
      const screen = rect.getBoundingClientRect()
      return { id:el.getAttribute('data-reference-node'), box:{x:box.x,y:box.y,width:box.width,height:box.height}, lines:lines.map(b=>({x:b.x,y:b.y,width:b.width,height:b.height})), contained: lines.every(b => b.x >= box.x && b.y >= box.y && b.x + b.width <= box.x + box.width && b.y + b.height <= box.y + box.height), fitted: screen.left >= viewport.left && screen.right <= viewport.right && screen.top >= viewport.top && screen.bottom <= viewport.bottom }
    }))
    expect(results.filter(r => !r.contained)).toEqual([])
    expect(results.every(r => r.fitted)).toBe(true)
  }
  await checkBounds()
  await page.screenshot({ path: testInfo.outputPath('reference-full-names.png') })
  await page.getByText('分组', { exact: true }).click()
  await checkBounds()
  await page.getByText('放射', { exact: true }).click()
  await page.setViewportSize({ width: 390, height: 844 })
  const fit = page.getByRole('button').filter({ has: page.locator('.anticon-expand') })
  await fit.focus()
  await fit.press('Enter')
  await checkBounds()
})
