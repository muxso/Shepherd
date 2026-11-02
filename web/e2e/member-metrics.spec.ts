import { test, expect, type Page } from '@playwright/test'

test.use({ viewport:{ width:1600,height:1100 } })
const endForMonth=(month:string) => { const [year,value]=month.split('-').map(Number);return `${value===12?year+1:year}-${String(value===12?1:value+1).padStart(2,'0')}-01T00:00:00+08:00` }
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
      body={ source,metrics:{ month:url.searchParams.get('month'),start:'2026-10-01T00:00:00+08:00',endExclusive:endForMonth(url.searchParams.get('month') || '2026-10'),timezone:'Asia/Shanghai',requirementTarget:target,members:[{ ...alice,radar:[Math.min(100,4/target*100),75,60] },bob],newValidBugs:3,linkedValidBugs:2,requirementsMissingDue:1,requirementsMissingTime:0,snapshotBased:true } }
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
  await panel.getByLabel('统计月份',{ exact:true }).click()
  await page.locator('.ant-picker-dropdown').getByTitle('2026-09',{exact:true}).click()
  await expect.poll(() => calls.at(-1)?.searchParams.get('month')).toBe('2026-09')
  await panel.getByLabel('个人交付参考目标',{ exact:true }).fill('20')
  await panel.getByRole('button',{ name:'应用参考目标' }).click()
  await expect.poll(() => calls.at(-1)?.searchParams.get('requirementTarget')).toBe('20')
  await panel.getByLabel('雷达组织分组').first().click()
  await page.locator('.ant-select-dropdown').getByTitle('协作组',{ exact:true }).click()
  await expect(panel.getByRole('img',{ name:'研发甲个人雷达图' })).toHaveCount(0)
  await expect(panel.getByRole('img',{ name:'协作组组内成员雷达对比图' })).toBeVisible()
  await panel.getByText('个人雷达',{exact:true}).click()
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

test('Chinese month picker blocks future dates and restores a shared group after reload',async ({ page }) => {
  await page.clock.setFixedTime(new Date('2026-11-02T02:00:00Z'))
  const calls=await fixture(page)
  await page.goto('/devops?view=radar&month=2026-10&group=研发组')
  const panel=page.getByRole('region',{name:'个人月度雷达指标'})
  await expect(panel.getByRole('img',{name:'研发组组内成员雷达对比图'})).toBeVisible()
  await expect(panel.getByLabel('统计月份',{exact:true})).toHaveValue('2026年10月')
  await panel.getByLabel('统计月份',{exact:true}).click()
  const popup=page.locator('.ant-picker-dropdown')
  await expect(popup).not.toContainText('Oct')
  await expect(popup.getByTitle('2026-12',{exact:true})).toHaveClass(/ant-picker-cell-disabled/)
  await popup.getByTitle('2026-09',{exact:true}).click()
  await expect.poll(()=>calls.at(-1)?.searchParams.get('month')).toBe('2026-09')
  await expect(panel.getByRole('img',{name:'研发组组内成员雷达对比图'})).toBeVisible()
  expect(new URL(page.url()).searchParams.get('group')).toBe('研发组')
  await page.reload()
  await expect(panel.getByLabel('统计月份',{exact:true})).toHaveValue('2026年09月')
  await expect(panel.getByRole('img',{name:'研发组组内成员雷达对比图'})).toBeVisible()
  await page.screenshot({path:'/tmp/shepherd-group-radar.png',fullPage:true,animations:'disabled'})
})

test('late month responses cannot overwrite the current month or reset the selected team',async ({ page }) => {
  await fixture(page)
  let release=()=>{}
  const gate=new Promise<void>(resolve=>{ release=resolve })
  await page.route('**/member-metrics**',async route=>{
    const url=new URL(route.request().url());if(!url.pathname.endsWith('/member-metrics'))return route.fallback()
    const month=url.searchParams.get('month')
    if(month==='2026-09')await gate
    return route.fulfill({json:{source,metrics:{month,endExclusive:'2026-11-01T00:00:00+08:00',requirementTarget:10,members:[{...alice,deliveredRequirements:month==='2026-09'?9:4}],newValidBugs:2,linkedValidBugs:2,requirementsMissingDue:0,requirementsMissingTime:0}}})
  })
  await page.goto('/devops?view=radar&month=2026-10&group=研发组')
  const panel=page.getByRole('region',{name:'个人月度雷达指标'})
  const chart=panel.getByRole('img',{name:'研发组组内成员雷达对比图'})
  await expect(chart).toBeVisible()
  await panel.getByLabel('统计月份',{exact:true}).click()
  await page.locator('.ant-picker-dropdown').getByTitle('2026-09',{exact:true}).click()
  await expect(chart).toHaveCount(0)
  await expect(panel.getByLabel('雷达组织分组').first()).toContainText('研发组')
  await panel.getByLabel('统计月份',{exact:true}).click()
  await page.locator('.ant-picker-dropdown').getByTitle('2026-10',{exact:true}).click()
  await expect(chart).toBeVisible()
  const late=page.waitForResponse(r=>r.url().includes('/member-metrics')&&new URL(r.url()).searchParams.get('month')==='2026-09')
  release();await late
  await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))))
  await expect(panel.getByLabel('统计月份',{exact:true})).toHaveValue('2026年10月')
  await expect(panel.locator('.yx-members-table .ant-table-row .yx-member-number strong').first()).toHaveText('4')
  await expect(chart).toBeVisible()
})

test('a team with no records in the selected month never falls back to all teams',async ({ page })=>{
  await fixture(page)
  await page.goto('/devops?view=radar&month=2026-10&group=不存在的小组')
  const panel=page.getByRole('region',{name:'个人月度雷达指标'})
  await expect(panel.getByLabel('雷达组织分组').first()).toContainText('不存在的小组')
  await expect(panel.locator('.yx-members-table .ant-table-row')).toHaveCount(0)
  await expect(panel.getByText('请选择本组成员进行对比',{exact:true})).toBeVisible()
  await expect(panel.locator('svg.yx-group-radar,svg.yx-member-radar')).toHaveCount(0)
})

test('group comparisons show multiple people, preserve missing values, and can switch to individual cards',async ({ page })=>{
  await fixture(page)
  await page.route('**/member-metrics**',route=>{
    if(!new URL(route.request().url()).pathname.endsWith('/member-metrics'))return route.fallback()
    return route.fulfill({json:{source,metrics:{month:'2026-10',endExclusive:'2026-11-01T00:00:00+08:00',requirementTarget:10,members:[alice,{...bob,group:'研发组'}],newValidBugs:2,linkedValidBugs:2,requirementsMissingDue:0,requirementsMissingTime:0}}})
  })
  await page.goto('/devops?view=radar&month=2026-10&group=研发组')
  const panel=page.getByRole('region',{name:'个人月度雷达指标'})
  const chart=panel.getByRole('img',{name:'研发组组内成员雷达对比图'})
  await expect(chart.locator('g[data-member]')).toHaveCount(2)
  await expect(chart.locator('g[data-member="研发甲"] polygon')).toHaveCount(1)
  await expect(chart.locator('g[data-member="研发乙"] polygon')).toHaveCount(0)
  await expect(panel.getByText('已选 2 / 2 人',{exact:true})).toBeVisible()
  await panel.getByText('个人雷达',{exact:true}).click()
  await expect(panel.getByRole('img',{name:'研发甲个人雷达图'})).toBeVisible()
  await expect(panel.getByRole('img',{name:'研发乙个人雷达图'})).toBeVisible()
  await page.goBack()
  await expect(chart).toBeVisible()
})

test('Shanghai month defaults are independent of the browser timezone at year boundaries',async ({ browser })=>{
  const context=await browser.newContext({timezoneId:'America/Los_Angeles'})
  const page=await context.newPage()
  try {
    await page.clock.setFixedTime(new Date('2026-12-31T16:30:00Z'))
    const calls=await fixture(page)
    await page.goto('/devops?view=radar')
    await expect(page.getByLabel('统计月份',{exact:true})).toHaveValue('2026年12月')
    await expect.poll(()=>calls.at(-1)?.searchParams.get('month')).toBe('2026-12')
  }finally{await context.close()}
})

test('invalid shared month is repaired before any invalid API request is sent',async ({ page })=>{
  await page.clock.setFixedTime(new Date('2026-11-02T02:00:00Z'))
  const calls=await fixture(page)
  await page.goto('/devops?view=radar&month=2026-13&group=研发组')
  await expect(page.getByLabel('统计月份',{exact:true})).toHaveValue('2026年10月')
  await expect.poll(()=>new URL(page.url()).searchParams.get('month')).toBe('2026-10')
  await expect.poll(()=>calls.length).toBeGreaterThan(0)
  expect(calls.every(url=>url.searchParams.get('month')==='2026-10')).toBe(true)
  await expect(page.getByRole('img',{name:'研发组组内成员雷达对比图'})).toBeVisible()
})

test('a selected team comparison supports six people and retains explicit selection after reload',async ({page})=>{
  await fixture(page)
  const members=Array.from({length:8},(_,i)=>({...alice,name:`研发${i}`}))
  await page.route('**/member-metrics**',route=>{
    if(!new URL(route.request().url()).pathname.endsWith('/member-metrics'))return route.fallback()
    return route.fulfill({json:{source,metrics:{month:'2026-10',endExclusive:endForMonth('2026-10'),requirementTarget:10,members,newValidBugs:2,linkedValidBugs:2,requirementsMissingDue:0,requirementsMissingTime:0}}})
  })
  await page.goto('/devops?view=radar&month=2026-10&group=研发组&compare='+encodeURIComponent(JSON.stringify(['研发0','研发7'])))
  const panel=page.getByRole('region',{name:'个人月度雷达指标'})
  const chart=panel.getByRole('img',{name:'研发组组内成员雷达对比图'})
  await expect(chart.locator('g[data-member]')).toHaveCount(2)
  await expect(chart.locator('g[data-member="研发7"]')).toHaveCount(1)
  await page.reload()
  await expect(chart.locator('g[data-member="研发7"]')).toHaveCount(1)
  await page.goto('/devops?view=radar&month=2026-10&group=研发组')
  await expect(chart.locator('g[data-member]')).toHaveCount(6)
  await expect(panel.getByText('已选 6 / 8 人',{exact:true})).toBeVisible()
})

for(const width of [700,390]) {
  test(`group comparison and Chinese calendar fit ${width}px`,async({page})=>{
    await fixture(page)
    await page.setViewportSize({width,height:1100})
    await page.goto('/devops?view=radar&month=2026-10&group=研发组')
    const panel=page.getByRole('region',{name:'个人月度雷达指标'})
    await expect(panel.getByRole('img',{name:'研发组组内成员雷达对比图'})).toBeVisible()
    const bounds=await panel.evaluate(el=>({left:el.getBoundingClientRect().left,right:el.getBoundingClientRect().right,viewport:window.innerWidth}))
    expect(bounds.left).toBeGreaterThanOrEqual(0);expect(bounds.right).toBeLessThanOrEqual(width+1)
    await panel.getByLabel('统计月份',{exact:true}).click()
    const popup=page.locator('.ant-picker-dropdown')
    await expect(popup).toBeVisible()
    const box=await popup.boundingBox()
    expect(box?.x).toBeGreaterThanOrEqual(0);expect((box?.x||0)+(box?.width||0)).toBeLessThanOrEqual(width+1)
    await page.keyboard.press('Escape')
  })
}

test('three group members share the same radar scale with matching legends',async({page})=>{
  await fixture(page)
  const members=[
    {...alice,name:'算法甲',group:'AI算法组',deliveredRequirements:6,linkedBugs:0,bugAffectedRequirements:0,bugRate:0,radar:[60,100,60]},
    {...alice,name:'算法乙',group:'AI算法组',deliveredRequirements:4,bugRate:25,dueRequirements:3,onTime:0,deliveredLate:1,overdueAtEnd:2,delayRate:100,radar:[40,75,0]},
    {...alice,name:'算法丙',group:'AI算法组',deliveredRequirements:8,linkedBugs:1,bugRate:12.5,dueRequirements:4,onTime:3,deliveredLate:1,overdueAtEnd:0,delayRate:25,radar:[80,87.5,75]},
  ]
  await page.route('**/member-metrics**',route=>{
    if(!new URL(route.request().url()).pathname.endsWith('/member-metrics'))return route.fallback()
    return route.fulfill({json:{source,metrics:{month:'2026-10',endExclusive:endForMonth('2026-10'),requirementTarget:10,members,newValidBugs:2,linkedValidBugs:2,requirementsMissingDue:0,requirementsMissingTime:0}}})
  })
  await page.goto('/devops?view=radar&month=2026-10&group=AI算法组')
  const panel=page.getByRole('region',{name:'个人月度雷达指标'})
  const chart=panel.getByRole('img',{name:'AI算法组组内成员雷达对比图'})
  await expect(chart.locator('g[data-member]')).toHaveCount(3)
  await expect(panel.locator('.yx-group-radar-person')).toHaveCount(3)
  for(const p of members)await expect(chart.locator(`g[data-member="${p.name}"]`)).toHaveCount(1)
  await page.screenshot({path:'/tmp/shepherd-group-radar-preview.png',fullPage:true,animations:'disabled'})
})

test('global count sorting covers every page and keeps table, radars, team comparison and URL in sync',async ({ page }) => {
  await fixture(page)
  const members=Array.from({length:14},(_,i)=>({...alice,name:`成员${String(i).padStart(2,'0')}`,deliveredRequirements:i+1,linkedBugs:14-i}))
  await page.route('**/member-metrics**',route => { if (!new URL(route.request().url()).pathname.endsWith('/member-metrics')) return route.fallback(); return route.fulfill({json:{source,metrics:{month:'2026-10',endExclusive:endForMonth('2026-10'),requirementTarget:10,members}}}) })
  await page.goto('/devops?view=radar&month=2026-10')
  const panel=page.getByRole('region',{name:'个人月度雷达指标'}), table=panel.locator('.yx-members-table')
  const firstName=()=>table.locator('.ant-table-row .yx-member-name').first()
  await expect(firstName()).toHaveText('成员13')
  await panel.getByRole('navigation',{name:'成员统一分页'}).getByTitle('2',{exact:true}).click()
  await panel.getByRole('columnheader',{name:'关联Bug数'}).click()
  await expect(firstName()).toHaveText('成员00')
  await expect(panel.locator('.yx-member-radar-card').first()).toHaveAttribute('aria-label','成员00指标卡')
  await expect(panel.getByLabel('成员排序').first()).toContainText('关联Bug数降序')
  expect(new URL(page.url()).searchParams.get('sort')).toBe('bugs')
  await panel.getByRole('columnheader',{name:'关联Bug数'}).click()
  await expect(firstName()).toHaveText('成员13')
  await page.reload()
  await expect(firstName()).toHaveText('成员13')
  await panel.getByLabel('成员排序').first().click()
  await page.getByTitle('交付需求量升序',{exact:true}).click()
  await expect(firstName()).toHaveText('成员00')
  await panel.getByRole('columnheader',{name:'交付需求量'}).click()
  await expect(firstName()).toHaveText('成员13')
  await panel.getByLabel('雷达组织分组').first().click()
  await page.locator('.ant-select-dropdown').getByTitle('研发组',{exact:true}).click()
  await expect(panel.locator('.yx-group-radar-legend .yx-member-name').first()).toHaveText('成员13')
  await panel.getByRole('columnheader',{name:'关联Bug数'}).click()
  await expect(panel.locator('.yx-group-radar-legend .yx-member-name').first()).toHaveText('成员00')
  await page.screenshot({path:'/tmp/shepherd-member-sorting.png',fullPage:true,animations:'disabled'})
})
