import { test, expect, type Page } from '@playwright/test'

test.use({ viewport: { width: 1600, height: 1000 } })
const rich = JSON.stringify({ htmlValue: '<article><p><strong>数据库新增和变更</strong></p><p>代码与字典配置</p><p>需要发布的系统</p><p>回归范围</p><script>window.__richTextXss=true</script><img src="x" onerror="window.__richTextXss=true"></article>', jsonMLValue: ['root', {}, ['internal']] })
const version = { version: 1, description: rich, acceptanceCriteria: ['可追溯'] }
const native = { id: 'native', projectId: 'project', title: 'REQ-42 登录需求', status: 'DRAFT', baselineVersion: 1, versions: [version] }
const source = { id: 'source', name: '需求池', project_id: 'project', code_since: '2026-10-01', code_until: '2026-11-01' }
const output = {
  source, requirement: { externalId: 'cloud', serial: 'REQ-42', title: '登录需求', creator: '产品甲', owner: '当前负责人', status: '已发布' },
  output: {
    bugTotal: 2, commitTotal: 3, testers: [{ name: '测试乙', bugs: 2 }], developers: [{ name: '研发丙', commits: 3 }],
    bugStates: [{ status: '修复中', count: 2 }],
    bugs: [{ externalId: 'bug', serial: 'BUG-1', title: '登录令牌异常', creator: '测试乙', owner: '研发丙', status: '修复中' }],
    commits: [{ externalId: 'commit-a', sha: 'abcdef123456', title: '修复登录', repo: '服务端仓库', author: '研发丙', additions: 12, deletions: 3 }, { externalId: 'commit-b', sha: 'bbcdef123456', title: '更新登录界面', repo: '前端仓库', author: '研发丙', additions: null, deletions: null }],
    repositories: [{ id: 'a', name: '服务端仓库', commits: 2, additions: 12, deletions: 3, unknownStats: 0, authors: ['研发丙'] }, { id: 'b', name: '前端仓库', commits: 1, additions: null, deletions: null, unknownStats: 1, authors: ['研发丙'] }],
  }, permissions: { bugs: true }, pageSize: 20, bugPage: 1, commitPage: 1,
}
async function fixture(page: Page, enabled = true) {
  const calls: URL[] = []
  await page.addInitScript(() => { localStorage.setItem('shepherd.token', 'role-output-test'); localStorage.setItem('shepherd.projectId', 'project'); localStorage.setItem('shepherd.theme', 'light') })
  await page.route('**/*', async route => {
    if (!['fetch','xhr'].includes(route.request().resourceType())) return route.continue()
    const url = new URL(route.request().url()), path = url.pathname
    let body: unknown = { items: [], total: 0 }
    if (path === '/organization') body = { items: [{ id: 'org' }] }
    if (path === '/project') body = { items: [{ id: 'project', name: 'Project', organizationId: 'org' }] }
    if (path === '/api/plugins') body = { items: [{ id: 'yunxiao', name: 'Yunxiao', enabled }] }
    if (path === '/requirement') body = { items: [native], total: 1, totalPages: 1 }
    if (path === '/requirement/native') body = native
    if (path === '/requirement/native/version/1') body = version
    if (path === '/api/devops/sources') body = { items: [source] }
    if (path.endsWith('/dashboard')) body = { source, authors: [], totals: { requirements: 1, bugs: 2, sprints: 0, delivered: 1, delayed: 0 }, bugStatistics: { total: 2, new: 1, linked: 2, unlinked: 0, statuses: [{ status: '修复中', count: 2 }], reporters: [{ name: '测试乙', count: 2 }] } }
    if (path.endsWith('/bug-statistics')) body = { source, from:'2026-11-01',through:'2026-11-07',timezone:'Asia/Shanghai',total:2,new:2,linked:2,unlinked:0,unassigned:0,missingTime:0,statuses:[{status:'修复中',count:2}],people:[{name:'测试乙',count:2}],reporters:[{name:'测试乙',count:2}],personOptions:['测试乙'],groups:['研发组'],trend:[{date:'2026-11-07',value:2}],priorities:[] }
    if (path.endsWith('/team-activity')) body = { source,role:'code',days:7,from:'2026-11-01',through:'2026-11-07',permissions:{requirements:true,bugs:true,testcases:true},events:[{id:'commit:a',externalId:'repo:a',actor:'研发丙',kind:'commit',date:'2026-11-07',at:'2026-11-07T15:00:00+08:00',repoId:'a',repo:'服务端仓库',additions:12,deletions:3,requirements:[{externalId:'cloud',serial:'REQ-42',title:'登录需求'}]}],bugs:[],members:[] }
    if (path.endsWith('/records')) body = { items: [{ externalId: 'cloud', serial: 'REQ-42', title: '登录需求' }], total: 1 }
    if (path === '/api/devops/requirement-output') { calls.push(url); body = output }
    if (['/api/module','/functional-case','/api/definition','/requirement/native/functional-coverage'].includes(path)) body = []
    if (path === '/requirement/functional-coverage') body = {}
    await route.fulfill({ json: body })
  })
  return calls
}

test('requirement versions render sanitized rich text in the list and detail modal', async ({ page }) => {
  await fixture(page)
  await page.goto('/p/project/requirement')
  await page.getByRole('row').filter({ hasText: 'REQ-42 登录需求' }).click()
  const table = page.locator('.ant-table').filter({ has: page.getByRole('columnheader', { name: '版本说明', exact: true }) })
  await expect(table.getByText('数据库新增和变更')).toBeVisible()
  await expect(table).not.toContainText('htmlValue')
  await expect(table).not.toContainText('jsonMLValue')
  expect(await table.locator('.requirement-version-preview').evaluate(el => el.getBoundingClientRect().height)).toBeLessThanOrEqual(96)
  await table.getByRole('button', { name: '查看', exact: true }).click()
  const dialog = page.getByRole('dialog', { name: '版本明细 · v1' })
  await expect(dialog.getByText('代码与字典配置')).toBeVisible()
  await expect(dialog).not.toContainText('htmlValue')
  await expect(dialog.locator('script,[onerror]')).toHaveCount(0)
  expect(await page.evaluate(() => (window as typeof window & { __richTextXss?: boolean }).__richTextXss)).toBeUndefined()
  await page.screenshot({ path: '/tmp/shepherd-version-rendering.png', fullPage: true, animations: 'disabled' })
})

test('native requirement loads optional role output only when its plugin tab is selected', async ({ page }) => {
  const calls = await fixture(page)
  await page.goto('/p/project/requirement')
  await page.getByRole('row').filter({ hasText: 'REQ-42 登录需求' }).click()
  await expect(page.getByRole('tab', { name: '岗位产出', exact: true })).toBeVisible()
  expect(calls).toHaveLength(0)
  await page.getByRole('tab', { name: '岗位产出', exact: true }).click()
  const panel = page.getByRole('region', { name: '需求岗位产出' })
  await expect(panel.getByText('产品甲', { exact: true })).toBeVisible()
  await expect(panel.getByText('测试乙', { exact: true }).first()).toBeVisible()
  await expect(panel.getByText('研发丙', { exact: true }).first()).toBeVisible()
  expect(calls.at(-1)?.searchParams.get('projectId')).toBe('project')
  expect(calls.at(-1)?.searchParams.get('reference')).toBe('REQ-42')
  await expect(panel.getByText('服务端仓库', { exact: true }).first()).toBeVisible()
  await expect(panel.getByText('前端仓库', { exact: true }).first()).toBeVisible()
  await panel.getByRole('tab', { name: '关联仓库 (2)', exact: true }).click()
  await expect(panel.getByRole('columnheader', { name: '行数未知提交' })).toBeVisible()
  await panel.getByRole('tab', { name: '关联代码 (3)', exact: true }).click()
  await expect(panel.getByRole('cell', { name: '修复登录', exact: true })).toBeVisible()
  await expect(panel.getByRole('cell', { name: '更新登录界面', exact: true })).toBeVisible()
})

test('disabled plugin adds no role tab and makes no output calls', async ({ page }) => {
  const calls = await fixture(page, false)
  await page.goto('/p/project/requirement')
  await page.getByRole('row').filter({ hasText: 'REQ-42 登录需求' }).click()
  await expect(page.getByRole('tab', { name: '需求信息', exact: true })).toBeVisible()
  await expect(page.getByRole('tab', { name: '岗位产出', exact: true })).toHaveCount(0)
  expect(calls).toHaveLength(0)
})

test('devops visualizes one requirement across product, testing and multiple repositories', async ({ page }) => {
  const calls = await fixture(page)
  await page.goto('/devops')
  await page.getByRole('tab', { name: '需求产出', exact: true }).click()
  await page.getByText('单项需求', { exact:true }).click()
  const panel = page.getByRole('region', { name: '需求岗位产出' })
  await expect(panel.getByText('产品 · 需求', { exact: true })).toBeVisible()
  await expect(panel.getByText('测试 · 缺陷', { exact: true })).toBeVisible()
  await expect(panel.getByText('研发 · 代码与仓库', { exact: true })).toBeVisible()
  await expect(panel.getByText('实际提交 · 2 个仓库')).toBeVisible()
  expect(calls.at(-1)?.searchParams.get('sourceId')).toBe('source')
  await page.screenshot({ path: '/tmp/shepherd-requirement-role-flow.png', fullPage: true, animations: 'disabled' })
  await page.getByRole('tab', { name: '缺陷统计', exact: true }).click()
  const stats = page.getByRole('region', { name: '云效缺陷统计' })
  await expect(stats.getByText('已关联需求')).toBeVisible()
  await expect(stats.getByRole('button', { name: '测试乙 2 个', exact: true })).toBeVisible()
})

test('requirement overview starts with a chart and drills into the selected requirement', async ({page}) => {
  const calls=await fixture(page)
  await page.goto('/devops?view=delivery&source=source')
  const overview=page.getByLabel('需求产出可视化总览')
  await expect(overview.getByLabel('需求参与人数、代码变动、仓库与缺陷气泡图')).toHaveAttribute('aria-busy','false')
  await expect(overview.locator('.ant-table')).toHaveCount(0)
  expect(calls).toHaveLength(0)
  await overview.getByRole('button',{name:/参与人数最多/}).click()
  const detail=page.getByRole('dialog',{name:'REQ-42 · 需求证据'})
  await detail.getByRole('button',{name:'查看需求产出',exact:true}).click()
  await expect(page.getByText('产品 · 需求',{exact:true})).toBeVisible()
  await expect.poll(()=>calls.at(-1)?.searchParams.get('reference')).toBe('cloud')
})

test('defect time, person and group filters are sent together with explicit person semantics', async ({page}) => {
  await fixture(page);const queries:URL[]=[]
  await page.route('**/bug-statistics?**',route=>{queries.push(new URL(route.request().url()));return route.fulfill({json:{source,from:'2026-11-01',through:'2026-11-07',timezone:'Asia/Shanghai',total:2,new:2,linked:1,unlinked:1,unassigned:0,missingTime:0,statuses:[{status:'修复中',count:2}],people:[{name:'测试乙',count:2}],reporters:[{name:'测试乙',count:2}],personOptions:['测试乙'],groups:['研发组'],trend:[{date:'2026-11-07',value:2}],priorities:[{name:'高',count:2}]}})})
  await page.goto('/devops?view=bugs&source=source')
  const panel=page.getByRole('region',{name:'云效缺陷统计'})
  await expect(panel.getByRole('img',{name:'所选范围新增缺陷趋势'})).toBeVisible()
  await panel.getByRole('combobox',{name:'缺陷时间范围',exact:true}).click();await page.locator('.ant-select-dropdown:visible').getByTitle('最近 30 天',{exact:true}).click()
  await panel.getByRole('combobox',{name:'缺陷人员口径',exact:true}).click();await page.locator('.ant-select-dropdown:visible').getByTitle('实际发现人',{exact:true}).click()
  await panel.getByRole('combobox',{name:'缺陷人员',exact:true}).click();await page.locator('.ant-select-dropdown:visible').getByTitle('测试乙',{exact:true}).click()
  await panel.getByRole('combobox',{name:'缺陷分组',exact:true}).click();await page.locator('.ant-select-dropdown:visible').getByTitle('研发组',{exact:true}).click()
  await expect.poll(()=>queries.at(-1)?.searchParams.get('days')).toBe('30')
  await expect.poll(()=>queries.at(-1)?.searchParams.get('personRole')).toBe('reporter')
  await expect.poll(()=>queries.at(-1)?.searchParams.get('person')).toBe('测试乙')
  await expect.poll(()=>queries.at(-1)?.searchParams.get('group')).toBe('研发组')
  const dates=panel.locator('.ant-picker-range input');await dates.first().fill('2026-11-01');await dates.first().press('Enter');await dates.nth(1).fill('2026-11-07');await dates.nth(1).press('Enter')
  await expect.poll(()=>queries.at(-1)?.searchParams.get('from')).toBe('2026-11-01')
  await expect.poll(()=>queries.at(-1)?.searchParams.get('through')).toBe('2026-11-07')
  expect(queries.at(-1)?.searchParams.has('days')).toBe(false)
})
