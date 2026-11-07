import { test, expect, type Page } from '@playwright/test'
import { dateKey, monthGrid } from '../src/plugins/yunxiao/codeActivity'
import { sourceHref } from '../src/plugins/yunxiao/activityModel'
import type { ActivityEvent, ActivityMemberSetting, TeamActivityData } from '../src/plugins/yunxiao/api'
const source = { id: 'source-a', name: '需求池', project_id: 'project', cloud_org_id: 'org', cloud_project_id: 'cloud-project', last_success_at: '2026-11-07T20:33:00+08:00', code_since: '2026-10-01T00:00:00+08:00', code_until: '2026-11-07T20:31:00+08:00' }
const requirement = { externalId: 'r1', serial: 'REQ-1', title: '金额边界校验', url: 'https://devops.aliyun.com/projex/project/cloud-project/req/r1' }
const base: ActivityEvent = { id: 'commit:a', externalId: 'repo:a', actor: 'Alice', authorRaw: 'git-a', group: '服务端', kind: 'commit', date: '2026-11-06', at: '2026-11-06T15:35:00+08:00', title: 'validate amount', serial: null, status: null, repoId: 'repo', repo: 'service-api', sha: 'a', additions: 60, deletions: 7, linked: true, requirements: [requirement], url: 'https://codeup.aliyun.com/org/service-api/commit/a', repositoryUrl: 'https://codeup.aliyun.com/org/service-api' }
const events: ActivityEvent[] = [base, { ...base, id: 'commit:b', externalId: 'console:b', repoId: 'console', repo: 'console-web', at: '2026-11-06T21:14:00+08:00', sha: 'b', additions: 4, deletions: 5, requirements: [{ externalId: 'r2', serial: 'REQ-2', title: '订单交互调整', url: null }], url: null, repositoryUrl: null }, { ...base, id: 'commit:c', externalId: 'repo:c', actor: 'Bob', authorRaw: 'Bob', group: '前端', date: '2026-11-07', at: '2026-11-07T17:44:00+08:00', sha: 'c' }]
function data(role: 'code' | 'product' | 'test' = 'code', can = true): TeamActivityData {
  const list: ActivityEvent[] = role === 'code' ? events : role === 'product' ? [{ ...base, actor: 'Real product creator', kind: 'requirementCreated', title: '真实需求', repo: null, repoId: null, sha: null }] : [{ ...base, actor: 'Real reporter', kind: 'bugReported', title: '真实缺陷', repo: null, repoId: null, sha: null }, { ...base, id: 'case:1', actor: 'Real case creator', kind: 'testcaseCreated', title: '真实用例', repo: null, repoId: null, sha: null }]
  return { source, role, days: 7, from: '2026-11-01', through: '2026-11-07', start: '2026-11-01T00:00:00+08:00', endExclusive: '2026-11-08T00:00:00+08:00', timezone: 'Asia/Shanghai', observedAt: source.last_success_at,
    events: list.map(e => ({ ...e, requirements: can ? e.requirements : [] })),
    members: [...new Set(list.map(e => e.actor))].map(name => ({ name, group: list.find(e => e.actor === name)!.group, inRoster: true, eventCount: list.filter(e => e.actor === name).length, lastActivityAt: list.filter(e => e.actor === name).map(e => e.at).sort().at(-1)!, status: 'active' as 'active' | 'noRecords' | 'unverified' })).concat(role === 'code' ? [{ name: 'No activity', group: '服务端', inRoster: true, eventCount: 0, lastActivityAt: '2026-10-20T15:00:00+08:00', status: 'noRecords' as const }] : []),
    bugs: role === 'code' && can ? [{ id: 'bug-1', serial: 'BUG-1', title: '边界缺陷', status: '已解决', at: '2026-11-06T10:00:00+08:00', date: '2026-11-06', responsibles: ['Alice'], requirements: [requirement], url: 'https://devops.aliyun.com/projex/project/cloud-project/bug/bug-1' }] : [],
    coverage: { complete: true, rosterConfigured: true, unattributedEvents: 0, testcasesCollected: true }, permissions: { requirements: can, bugs: can, testcases: true } }
}
async function fixture(page: Page, options: { can?: boolean; emptyRoles?: boolean; update?: boolean } = {}) {
  const requests: { method: string; path: string; body?: unknown; url: URL }[] = []
  const roster: ActivityMemberSetting[] = [{ name: 'Alice', group: '服务端', roles: ['code'], enabled: true }]
  await page.addInitScript(() => { localStorage.setItem('shepherd.token', 'test'); localStorage.setItem('shepherd.projectId', 'project'); localStorage.setItem('shepherd.theme', 'light') })
  await page.route('**/*', async route => {
    if (!['fetch', 'xhr'].includes(route.request().resourceType())) return route.continue()
    const url = new URL(route.request().url()), path = url.pathname, method = route.request().method()
    const body = route.request().postDataJSON()
    requests.push({ method, path, body, url })
    let response: unknown = { items: [], total: 0 }
    if (path === '/organization') response = { items: [{ id: 'org' }] }
    if (path === '/project') response = { items: [{ id: 'project', name: '项目', organizationId: 'org' }] }
    if (path === '/api/plugins') response = { items: [{ id: 'yunxiao', enabled: true, name: '云效同步' }] }
    if (path === '/api/devops/sources') response = { items: [source] }
    if (path.endsWith('/team-activity')) {
      const role = url.searchParams.get('role') as 'code' | 'product' | 'test'
      const value = data(role, options.can ?? true)
      value.days = Number(url.searchParams.get('days')); if (options.emptyRoles && role !== 'code') { value.events = []; value.members = []; value.bugs = [] }
      response = value
    }
    if (path.endsWith('/author-identities')) response = method === 'PUT' ? body : { identities: [{ name: 'Alice', group: '服务端', aliases: ['git-a'] }], permissions: { update: options.update ?? true } }
    if (path.endsWith('/activity-members')) response = method === 'PUT' ? body : { members: roster, permissions: { update: options.update ?? true } }
    if (path.endsWith('/dashboard')) response = { source, authors: [], totals: {} }
    await route.fulfill({ json: response })
  })
  return requests
}
test('calm heatmap preserves Shanghai times, source links, multiple requirements and distinct totals', async ({ page }, info) => {
  const errors: string[] = []; page.on('pageerror', e => errors.push(e.message))
  await fixture(page)
  await page.goto('/devops?view=authors&source=source-a')
  const matrix = page.getByRole('table', { name: '成员每日活动热力图' })
  await expect(matrix.locator('tbody tr')).toHaveCount(3)
  await expect(matrix.locator('th.is-weekend')).toHaveCount(2)
  const cell = page.getByRole('button', { name: 'Alice 2026-11-06 2 次活动，最后 21:14，Bug 1', exact: true })
  await expect(cell).toContainText('21:14'); await expect(cell).toContainText('Bug 1')
  await expect(matrix.getByText('Bug 0', { exact: true })).toHaveCount(0)
  const alice = matrix.locator('tbody tr').filter({ has: page.locator('.yx-activity-name', { hasText: 'Alice' }) })
  await expect(alice.locator('.yx-activity-total')).toHaveText(['2', '2', '1'])
  await cell.click()
  const drawer = page.getByRole('dialog', { name: '2026-11-06 · Alice' })
  await expect(drawer.getByText('当天关联需求 · 2 项')).toBeVisible()
  await expect(drawer.getByRole('link', { name: 'service-api', exact: true })).toHaveAttribute('href', 'https://codeup.aliyun.com/org/service-api')
  const req = drawer.getByRole('link', { name: 'REQ-1 金额边界校验', exact: true }); await expect(req).toHaveAttribute('href', requirement.url); await expect(req).toHaveAttribute('target', '_blank'); await expect(req).toHaveAttribute('rel', 'noopener noreferrer')
  await expect(drawer.getByRole('link', { name: 'BUG-1 边界缺陷', exact: true })).toHaveAttribute('target', '_blank')
  await drawer.getByText('查看当天提交', { exact: true }).click()
  await expect(drawer.getByRole('link', { name: 'a validate amount', exact: true })).toHaveAttribute('href', base.url!)
  await drawer.getByRole('button', { name: '关闭', exact: true }).click()
  await page.screenshot({ path: info.outputPath('code-authors-desktop.png'), fullPage: true, animations: 'disabled' })
  expect(errors).toEqual([])
})
test('custom inactivity window and role switches use the API and never synthesize names', async ({ page }) => {
  const calls = await fixture(page, { emptyRoles: true })
  await page.goto('/devops?view=authors&source=source-a')
  await expect(page.getByRole('table', { name: '成员每日活动热力图' })).toBeVisible()
  await page.getByRole('spinbutton', { name: '最近天数' }).fill('14')
  await expect.poll(() => calls.filter(c => c.path.endsWith('/team-activity')).at(-1)?.url.searchParams.get('days')).toBe('14')
  await page.getByRole('button', { name: '查看未记录产出人员（1）' }).click()
  await expect(page.getByRole('row').filter({ hasText: 'No activity' })).toContainText('最近 14 天未记录产出')
  await page.getByText('产品 · 需求', { exact: true }).click()
  await expect(page.getByText('所选范围暂无已同步的真实人员或活动', { exact: true })).toBeVisible()
  await page.getByText('测试 · 质量', { exact: true }).click()
  await expect(page.getByText('所选范围暂无已同步的真实人员或活动', { exact: true })).toBeVisible()
  await expect(page.getByText(/产品经理 [A-D]|测试工程师 [A-D]/)).toHaveCount(0)
})
test('product and testing render real source actors and their own metrics', async ({ page }) => {
  await fixture(page)
  await page.goto('/devops?view=authors&source=source-a&role=product')
  await expect(page.getByRole('cell', { name: 'Real product creator', exact: true })).toBeVisible()
  await page.getByText('测试 · 质量', { exact: true }).click()
  await expect(page.getByRole('cell', { name: 'Real reporter', exact: true })).toBeVisible()
  await expect(page.getByRole('cell', { name: 'Real case creator', exact: true })).toBeVisible()
  await page.getByLabel('活动指标', { exact: true }).click(); await page.locator('.ant-select-dropdown').getByTitle('新增用例', { exact: true }).click()
  await expect(page.getByRole('button', { name: 'Real case creator 2026-11-06 1 次活动，最后 15:35', exact: true })).toBeVisible()
})
test('alias associations and roster changes persist through explicit configuration APIs', async ({ page }) => {
  const calls = await fixture(page)
  await page.goto('/devops?view=authors&source=source-a')
  await page.getByRole('button', { name: '人员管理 ⌄' }).click(); await page.getByRole('menuitem', { name: '账号归并管理' }).click()
  const aliases = page.getByRole('dialog', { name: '提交账号归并管理' })
  await aliases.getByRole('button', { name: '添加人员归并' }).click()
  await aliases.getByLabel('统一姓名', { exact: true }).last().fill('Bob')
  await aliases.getByLabel('其他提交账号', { exact: true }).last().fill('git-b'); await aliases.getByLabel('其他提交账号', { exact: true }).last().press('Enter')
  await aliases.getByRole('button', { name: '保存配置' }).click()
  await expect.poll(() => calls.filter(c => c.method === 'PUT' && c.path.endsWith('/author-identities')).at(-1)?.body).toMatchObject({ identities: [{ name: 'Alice', aliases: ['git-a'] }, { name: 'Bob', aliases: ['git-b'] }] })
  await page.getByRole('button', { name: '人员管理 ⌄' }).click(); await page.getByRole('menuitem', { name: '统计人员名单' }).click()
  const roster = page.getByRole('dialog', { name: '统计人员名单' })
  await expect(roster.getByLabel('成员姓名', { exact: true })).toHaveValue('Alice')
  await roster.getByRole('button', { name: '保存名单' }).click()
  await expect.poll(() => calls.filter(c => c.method === 'PUT' && c.path.endsWith('/activity-members')).at(-1)?.body).toMatchObject({ members: [{ name: 'Alice', roles: ['code'], enabled: true }] })
})
test('narrow and dark layouts keep names legible and redact inaccessible evidence', async ({ page }, info) => {
  await fixture(page, { can: false })
  await page.addInitScript(() => localStorage.setItem('shepherd.theme', 'dark'))
  await page.setViewportSize({ width: 390, height: 1000 })
  await page.goto('/devops?view=authors&source=source-a')
  await expect(page.getByRole('cell', { name: 'Alice', exact: true })).toBeVisible()
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(391)
  await page.getByRole('button', { name: 'Alice 2026-11-06 2 次活动，最后 21:14', exact: true }).click()
  const drawer = page.getByRole('dialog')
  await expect(drawer.getByText('金额边界校验', { exact: false })).toHaveCount(0)
  await expect(drawer.getByText('边界缺陷', { exact: false })).toHaveCount(0)
  await page.screenshot({ path: info.outputPath('code-authors-mobile-dark.png'), fullPage: true, animations: 'disabled' })
})
test('requirement analysis loads the AntV distribution without altering evidence totals', async ({ page }, info) => {
  const errors: string[] = []; page.on('pageerror', e => errors.push(e.message))
  await fixture(page); await page.goto('/devops?view=authors&source=source-a')
  await page.getByRole('button', { name: '需求投入与质量', exact: true }).click()
  await expect(page.getByLabel('需求参与人数、代码变动、仓库与缺陷气泡图').locator('canvas')).toBeVisible()
  await expect(page.getByRole('cell', { name: 'REQ-1 金额边界校验', exact: true })).toBeVisible()
  await page.screenshot({ path: info.outputPath('code-authors-antv.png'), fullPage: true, animations: 'disabled' })
  expect(errors).toEqual([])
})
test('calendar boundaries and source protocols remain explicit', () => {
  expect(monthGrid('2024-02').flat().filter(Boolean)).toHaveLength(29)
  expect(dateKey('2026-11-02T16:10:00Z')).toBe('2026-11-03')
  expect(sourceHref('javascript:alert(1)')).toBeUndefined()
  expect(sourceHref('data:text/html,private')).toBeUndefined()
  expect(sourceHref('https://name:password@example.com')).toBeUndefined()
  expect(sourceHref('https://codeup.aliyun.com/org/repo')).toBe('https://codeup.aliyun.com/org/repo')
})
