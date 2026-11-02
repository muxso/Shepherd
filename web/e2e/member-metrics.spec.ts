import { test, expect, type Page } from '@playwright/test'

test.use({ viewport:{ width:1600,height:1100 } })
const source = { id:'source-a',name:'测试需求池',project_id:'project' }
const requirement = { externalId:'req-a',serial:'REQ-42',title:'登录需求',status:'已发布',due:'2026-10-15',delivered:true,newInMonth:true,deliveryClass:'deliveredLate',bugIds:['b1'] }
const alice = { name:'研发甲',group:'研发组',newRequirements:6,deliveredRequirements:4,linkedBugs:2,bugAffectedRequirements:1,bugRate:25,dueRequirements:5,onTime:3,deliveredLate:1,overdueAtEnd:1,unknownDelivery:0,delayRate:40,radar:[40,75,60],requirements:[requirement] }
const bob = { ...alice,name:'研发乙',group:'协作组',deliveredRequirements:0,linkedBugs:0,bugAffectedRequirements:0,bugRate:null,dueRequirements:0,onTime:0,deliveredLate:0,overdueAtEnd:0,unknownDelivery:0,delayRate:null,radar:[0,null,null],requirements:[] }
async function fixture(page:Page,enabled=true) {
  const calls:URL[]=[]
  await page.addInitScript(() => { localStorage.setItem('shepherd.token','radar-test');localStorage.setItem('shepherd.projectId','project');localStorage.setItem('shepherd.theme','light') })
  await page.route('**/*',async route => {
    if (!['fetch','xhr'].includes(route.request().resourceType())) return route.continue()
    const url=new URL(route.request().url()),path=url.pathname
    let body:unknown={ items:[],total:0 }
    if (path==='/organization') body={ items:[{ id:'org' }] }
    if (path==='/project') body={ items:[{ id:'project',name:'项目',organizationId:'org' }] }
    if (path==='/api/plugins') body={ items:[{ id:'yunxiao',name:'云效同步',enabled }] }
    if (path==='/api/devops/sources') body={ items:[source,{ ...source,id:'source-b',name:'另一需求池' }] }
    if (path.endsWith('/dashboard')) body={ source,authors:[],totals:{ requirements:1,bugs:2,sprints:0,delivered:1,delayed:0 } }
    if (path.endsWith('/member-metrics')) {
      calls.push(url)
      const target=Number(url.searchParams.get('requirementTarget'))
      body={ source,metrics:{ month:url.searchParams.get('month'),start:'2026-10-01T00:00:00+08:00',endExclusive:'2026-11-01T00:00:00+08:00',timezone:'Asia/Shanghai',requirementTarget:target,members:[{ ...alice,radar:[Math.min(100,4/target*100),75,60] },bob],newValidBugs:3,linkedValidBugs:2,requirementsMissingDue:1,requirementsMissingTime:0,snapshotBased:true } }
    }
    await route.fulfill({ json:body })
  })
  return calls
}

test('monthly table and filled radars preserve raw rates, sample sizes and missing values',async ({ page }) => {
  const calls=await fixture(page)
  await page.goto('/devops?view=radar&month=2026-10')
  const panel=page.getByRole('region',{ name:'个人月度雷达指标' })
  await expect(panel.getByRole('columnheader',{ name:'关联Bug率 ↓',exact:true })).toBeVisible()
  await expect(panel.getByText('25.0%',{exact:true}).first()).toBeVisible()
  await expect(panel.getByText('1 / 4 项需求',{exact:true})).toBeVisible()
  await expect(panel.getByRole('img',{ name:'研发甲个人雷达图' })).toBeVisible()
  await expect(panel.getByRole('img',{ name:'研发甲个人雷达图' }).locator('.yx-radar-value')).toHaveCount(1)
  await expect(panel.getByRole('img',{ name:'研发乙个人雷达图' }).locator('.yx-radar-value')).toHaveCount(0)
  await expect(panel.getByText('部分维度待评价')).toBeVisible()
  expect(calls.at(-1)?.searchParams.get('month')).toBe('2026-10')
  await panel.getByRole('button',{ name:'研发甲',exact:true }).first().click()
  const drawer=page.getByRole('dialog',{ name:'研发甲 · 月度需求明细' })
  await expect(drawer.getByText('延期交付',{ exact:true })).toBeVisible()
  await drawer.getByRole('button',{ name:'关闭',exact:true }).click()
  await page.screenshot({ path:'/tmp/shepherd-member-radar-light.png',fullPage:true,animations:'disabled' })
})
test('month, group, target and source controls do not retain old results',async ({ page }) => {
  const calls=await fixture(page)
  await page.goto('/devops?view=radar&month=2026-10')
  const panel=page.getByRole('region',{ name:'个人月度雷达指标' })
  await expect(panel.getByRole('img',{ name:'研发甲个人雷达图' })).toBeVisible()
  await panel.getByLabel('统计月份',{ exact:true }).fill('2026-09')
  await expect.poll(() => calls.at(-1)?.searchParams.get('month')).toBe('2026-09')
  await panel.getByLabel('个人交付参考目标',{ exact:true }).fill('20')
  await panel.getByRole('button',{ name:'应用参考目标' }).click()
  await expect.poll(() => calls.at(-1)?.searchParams.get('requirementTarget')).toBe('20')
  await panel.getByLabel('雷达组织分组').first().click()
  await page.locator('.ant-select-dropdown').getByTitle('协作组',{ exact:true }).click()
  await expect(panel.getByRole('img',{ name:'研发甲个人雷达图' })).toHaveCount(0)
  await expect(panel.getByRole('img',{ name:'研发乙个人雷达图' })).toBeVisible()
  await page.getByLabel('云效项目').first().click()
  await page.getByTitle('另一需求池',{ exact:true }).click()
  await expect.poll(() => calls.at(-1)?.pathname).toBe('/api/devops/sources/source-b/member-metrics')
  await expect(panel.getByRole('img',{ name:'研发甲个人雷达图' })).toBeVisible()
})
test('disabled plugin performs no member metrics calls',async ({ page }) => {
  const calls=await fixture(page,false)
  await page.goto('/devops?view=radar&month=2026-10')
  await expect(page.getByRole('region',{ name:'个人月度雷达指标' })).toHaveCount(0)
  expect(calls).toHaveLength(0)
})

test('radars follow dark theme and fit a narrow viewport',async ({ page }) => {
  await fixture(page)
  await page.addInitScript(() => localStorage.setItem('shepherd.theme','dark'))
  await page.setViewportSize({ width:700,height:1100 })
  await page.goto('/devops?view=radar&month=2026-10')
  const chart=page.getByRole('img',{ name:'研发甲个人雷达图' })
  await expect(chart).toBeVisible()
  await expect(page.locator('html')).toHaveAttribute('data-theme','dark')
  const box=await chart.boundingBox()
  expect(box?.width).toBeLessThanOrEqual(360)
  expect(await chart.locator('.yx-radar-value').evaluate(el => getComputedStyle(el).fill)).not.toBe('none')
  await chart.screenshot({ path:'/tmp/shepherd-member-radar-dark.png',animations:'disabled' })
})

for (const width of [1765,700,390]) {
  test(`long member and group text stay contained at ${width}px; table and cards page together`,async ({ page }) => {
    await fixture(page)
    await page.setViewportSize({ width,height:1100 })
    const longName='很长的合法成员显示名称'.repeat(15)
    const longGroup='很长的分组名称'.repeat(12)
    const members=Array.from({ length:14 },(_,i) => ({ ...alice,name:`${String(i).padStart(2,'0')}${longName}`,group:longGroup,requirements:[{ ...requirement,title:'需求标题'.repeat(80) }] }))
    members.push({ ...bob,name:'无活动成员',group:longGroup,newRequirements:0 })
    await page.route('**/member-metrics**',route => { if (!new URL(route.request().url()).pathname.endsWith('/member-metrics')) return route.fallback(); return route.fulfill({ json:{ source,metrics:{ month:'2026-10',endExclusive:'2026-11-01T00:00:00+08:00',requirementTarget:10,members,newValidBugs:2,linkedValidBugs:2,requirementsMissingDue:0,requirementsMissingTime:0 } } }) })
    await page.goto('/devops?view=radar&month=2026-10')
    const panel=page.getByRole('region',{ name:'个人月度雷达指标' })
    const table=panel.locator('.yx-members-table')
    await expect(table.locator('.ant-table-row')).toHaveCount(6)
    await expect(panel.locator('.yx-member-radar-card')).toHaveCount(6)
    await expect(panel.getByText('筛选 14 / 已同步 15 人')).toBeVisible()
    // Data may legitimately have long names; visual truncation must not discard it.
    const name=table.locator('.yx-member-name > span').first()
    expect(await name.evaluate(el => el.scrollWidth>el.clientWidth)).toBe(true)
    const bounds=await panel.evaluate(el => ({ content:el.getBoundingClientRect().right,viewport:window.innerWidth,document:document.documentElement.scrollWidth }))
    expect(bounds.content).toBeLessThanOrEqual(width+1)
    expect(bounds.document).toBeLessThanOrEqual(width+1)
    for (const heading of await panel.locator('.yx-member-card-heading').all()) {
      expect(await heading.evaluate(el => el.scrollWidth<=el.clientWidth+1)).toBe(true)
    }
    const tableNames=() => table.locator('.yx-member-name').allTextContents()
    const cardNames=() => panel.locator('.yx-member-card-heading .yx-member-name').allTextContents()
    expect(await tableNames()).toEqual(await cardNames())
    await panel.getByRole('navigation',{ name:'成员统一分页' }).getByTitle('2',{exact:true}).click()
    await expect(table.locator('.ant-table-row')).toHaveCount(6)
    expect(await tableNames()).toEqual(await cardNames())
    await panel.getByRole('switch',{ name:'包含无活动成员' }).click()
    await expect(panel.getByText('筛选 15 / 已同步 15 人')).toBeVisible()
    if (width===1765) {
      const firstName=table.locator('.yx-member-name').first()
      await firstName.hover()
      await expect(page.getByRole('tooltip')).toContainText(longName)
      await firstName.click()
      const drawer=page.getByRole('dialog')
      await expect(drawer).toBeVisible()
      expect(await drawer.locator('.yx-member-name > span').first().evaluate(el => el.scrollWidth>el.clientWidth)).toBe(true)
      await drawer.getByRole('button',{ name:'关闭',exact:true }).click()
      await page.setViewportSize({ width,height:1900 })
      await panel.getByText('个人指标表',{exact:true}).hover()
      await page.screenshot({ path:'/tmp/shepherd-radar-ux-long.png',fullPage:true,animations:'disabled' })
    }
  })
}
