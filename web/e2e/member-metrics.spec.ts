import { test, expect, type Page } from '@playwright/test'

test.use({ viewport:{ width:1600,height:1100 } })
const endForMonth=(month:string) => { const [year,value]=month.split('-').map(Number);return `${value===12?year+1:year}-${String(value===12?1:value+1).padStart(2,'0')}-01T00:00:00+08:00` }
const source = { id:'source-a',name:'测试需求池',project_id:'project' }
const requirement = { externalId:'req-a',serial:'REQ-42',title:'登录需求',status:'已发布',due:'2026-10-15',delivered:true,currentDelivered:true,newInMonth:true,deliveryClass:'deliveredLate',bugIds:['b1'] }
const alice = { name:'研发甲',group:'研发组',newRequirements:6,deliveredRequirements:4,linkedBugs:2,bugAffectedRequirements:1,bugRate:25,processedBugs:2,qualityBugs:2,reopenUnknownBugs:0,reopenCount:2,reopenedBugs:1,reopenEvaluatedBugs:2,reopenRate:50,bugs:[{externalId:'b1',serial:'BUG-1',title:'登录失败',status:'已解决',createdAt:'2026-07-01T00:00:00+08:00',bugType:'程序逻辑',responsibles:['研发甲'],developers:['修复人'],testers:['测试人'],reporter:'发现人',processed:true,historyAvailable:true,history:[{id:'1',at:'2026-10-10T02:00:00Z',from:'已解决',to:'重新打开'},{id:'2',at:'2026-10-15T03:00:00Z',from:'已解决',to:'重新打开'}],reopenCount:2,reopenTimes:['2026-10-10T02:00:00Z','2026-10-15T03:00:00Z']},{externalId:'b2',serial:'BUG-2',title:'样式问题',status:'已验证',createdAt:'2026-10-01T00:00:00+08:00',bugType:'界面',responsibles:['研发甲'],developers:['修复人'],testers:[],reporter:'发现人',processed:true,historyAvailable:true,history:[],reopenCount:0,reopenTimes:[]}],dueRequirements:5,onTime:3,deliveredLate:1,overdueAtEnd:1,unknownDelivery:0,delayRate:20,radar:[40,75,80],requirements:[requirement] }
const bob = { ...alice,name:'研发乙',group:'协作组',deliveredRequirements:0,linkedBugs:0,bugAffectedRequirements:0,bugRate:null,processedBugs:0,qualityBugs:0,reopenUnknownBugs:0,reopenCount:0,reopenedBugs:0,reopenEvaluatedBugs:0,reopenRate:null,bugs:[],dueRequirements:0,onTime:0,deliveredLate:0,overdueAtEnd:0,unknownDelivery:0,delayRate:null,radar:[0,null,null],requirements:[] }
async function fixture(page:Page,enabled=true,comparison=false) {
  const calls:URL[]=[]
  await page.addInitScript(() => { localStorage.setItem('shepherd.token','radar-test');localStorage.setItem('shepherd.projectId','project');localStorage.setItem('shepherd.theme','light') })
  await page.route('**/*',async route => {
    if (!['fetch','xhr'].includes(route.request().resourceType())) return route.continue()
    const url=new URL(route.request().url()),path=url.pathname
    let body:unknown={ items:[],total:0 }
    if (path==='/organization') body={ items:[{ id:'org' }] }
    if (path==='/project') body={ items:[{ id:'project',name:'云效 · 需求池',organizationId:'org' }] }
    if (path==='/api/plugins') body={ items:[{ id:'yunxiao',name:'云效同步',enabled }] }
    if (path==='/api/devops/sources') body={ items:[source,{ ...source,id:'source-b',name:'另一需求池' }] }
    if (path.endsWith('/dashboard')) body={ source,authors:[],totals:{ requirements:1,bugs:2,sprints:0,delivered:1,delayed:0 } }
    if (path.endsWith('/member-metrics')) {
      calls.push(url)
      const target=Number(url.searchParams.get('requirementTarget'))
      body={ source,metrics:{ month:url.searchParams.get('month'),start:comparison ? '2026-11-01T00:00:00+08:00' : '2026-10-01T00:00:00+08:00',endExclusive:comparison ? '2026-11-04T12:00:00+08:00' : endForMonth(url.searchParams.get('month') || '2026-10'),timezone:'Asia/Shanghai',requirementTarget:target,members:[{ ...alice,radar:[Math.min(100,4/target*100),75,80] },bob],newValidBugs:3,linkedValidBugs:2,requirementsMissingDue:1,requirementsMissingTime:0,snapshotBased:true,...(comparison ? { comparisons:{ sameElapsedTime:true,previousMonth:{month:"2026-10",start:"2026-10-01T00:00:00+08:00",endExclusive:"2026-10-04T12:00:00+08:00",members:[{...alice,deliveredRequirements:2,bugRate:50}]},previousYear:{month:"2025-11",start:"2025-11-01T00:00:00+08:00",endExclusive:"2025-11-04T12:00:00+08:00",members:[{...alice,deliveredRequirements:0,bugRate:null}]} } } : {}) } }
    }
    await route.fulfill({ json:body })
  })
  return calls
}

test('monthly table and filled radars preserve raw rates, sample sizes and missing values',async ({ page }) => {
  const calls=await fixture(page)
  await page.goto('/devops?view=radar&month=2026-10')
  const panel=page.getByRole('region',{ name:'个人月度雷达指标' })
  await expect(panel.getByRole('columnheader',{ name:'含Bug交付需求占比 ↓' })).toBeVisible()
  await expect(panel.getByText('25.0%',{exact:true}).first()).toBeVisible()
  await expect(panel.getByText('1 / 4 项需求',{exact:true})).toBeVisible()
  await expect(panel.getByText('1 / 5 项需求',{exact:true})).toBeVisible()
  await expect(panel.getByText('20.0%',{exact:true}).first()).toBeVisible()
  await expect(panel.getByRole('img',{ name:'研发甲个人雷达图' })).toBeVisible()
  await expect(panel.getByRole('img',{ name:'研发甲个人雷达图' }).locator('.yx-radar-value')).toHaveCount(1)
  await expect(panel.getByRole('img',{ name:'研发乙个人雷达图' }).locator('.yx-radar-value')).toHaveCount(0)
  await expect(panel.getByText('部分维度待评价')).toBeVisible()
  expect(calls.at(-1)?.searchParams.get('month')).toBe('2026-10')
  await panel.getByRole('button',{ name:'研发甲',exact:true }).first().click()
  const drawer=page.getByRole('dialog',{ name:'研发甲 · 月度需求明细' })
  await expect(drawer.getByText('已交付，不计延期',{ exact:true })).toBeVisible()
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
    const table=panel.locator('.yx-member-delivery-table')
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
      await panel.getByText('需求交付 · 按需求参与人',{exact:true}).hover()
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
  await expect(panel.locator('.yx-member-delivery-table .ant-table-row .yx-member-number strong').first()).toHaveText('4')
  await expect(chart).toBeVisible()
})

test('a team with no records in the selected month never falls back to all teams',async ({ page })=>{
  await fixture(page)
  await page.goto('/devops?view=radar&month=2026-10&group=不存在的小组')
  const panel=page.getByRole('region',{name:'个人月度雷达指标'})
  await expect(panel.getByLabel('雷达组织分组').first()).toContainText('不存在的小组')
  await expect(panel.locator('.yx-member-delivery-table .ant-table-row')).toHaveCount(0)
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
    await expect(page.getByLabel('统计月份',{exact:true})).toHaveValue('2027年01月')
    await expect.poll(()=>calls.at(-1)?.searchParams.get('month')).toBe('2027-01')
  }finally{await context.close()}
})

test('invalid shared month is repaired before any invalid API request is sent',async ({ page })=>{
  await page.clock.setFixedTime(new Date('2026-11-02T02:00:00Z'))
  const calls=await fixture(page)
  await page.goto('/devops?view=radar&month=2026-13&group=研发组')
  await expect(page.getByLabel('统计月份',{exact:true})).toHaveValue('2026年11月')
  await expect.poll(()=>new URL(page.url()).searchParams.get('month')).toBe('2026-11')
  await expect.poll(()=>calls.length).toBeGreaterThan(0)
  expect(calls.every(url=>url.searchParams.get('month')==='2026-11')).toBe(true)
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
    {...alice,name:'算法甲',group:'AI算法组',deliveredRequirements:6,linkedBugs:0,bugAffectedRequirements:0,bugRate:0,radar:[60,100,80]},
    {...alice,name:'算法乙',group:'AI算法组',deliveredRequirements:4,bugRate:25,processedBugs:2,qualityBugs:2,reopenUnknownBugs:0,reopenCount:2,reopenedBugs:1,reopenEvaluatedBugs:2,reopenRate:50,bugs:[{externalId:'b1',serial:'BUG-1',title:'登录失败',status:'已解决',createdAt:'2026-07-01T00:00:00+08:00',bugType:'程序逻辑',responsibles:['研发甲'],developers:['修复人'],testers:['测试人'],reporter:'发现人',processed:true,historyAvailable:true,history:[{id:'1',at:'2026-10-10T02:00:00Z',from:'已解决',to:'重新打开'},{id:'2',at:'2026-10-15T03:00:00Z',from:'已解决',to:'重新打开'}],reopenCount:2,reopenTimes:['2026-10-10T02:00:00Z','2026-10-15T03:00:00Z']},{externalId:'b2',serial:'BUG-2',title:'样式问题',status:'已验证',createdAt:'2026-10-01T00:00:00+08:00',bugType:'界面',responsibles:['研发甲'],developers:['修复人'],testers:[],reporter:'发现人',processed:true,historyAvailable:true,history:[],reopenCount:0,reopenTimes:[]}],dueRequirements:3,onTime:0,deliveredLate:1,overdueAtEnd:2,delayRate:66.7,radar:[40,75,33.3]},
    {...alice,name:'算法丙',group:'AI算法组',deliveredRequirements:8,linkedBugs:1,bugRate:12.5,dueRequirements:4,onTime:3,deliveredLate:1,overdueAtEnd:0,delayRate:0,radar:[80,87.5,100]},
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
  const panel=page.getByRole('region',{name:'个人月度雷达指标'}), table=panel.locator('.yx-member-delivery-table')
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
  const sortInput=panel.getByRole('combobox',{name:'成员排序'})
  await sortInput.press('ArrowUp')
  await sortInput.press('ArrowUp')
  await sortInput.press('Enter')
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


test('reopen counts, distinct Bug ratio, coverage and drilldown stay accurate and sortable',async ({ page }) => {
  await fixture(page)
  const complete={...alice,name:'历史完整'}
  const unknown={...alice,name:'历史未采集',reopenUnknownBugs:2,reopenCount:0,reopenedBugs:0,reopenEvaluatedBugs:0,reopenRate:null,bugs:alice.bugs.map(b=>({...b,reopenCount:null,reopenTimes:[]}))}
  const zero={...alice,name:'无重开',reopenCount:0,reopenedBugs:0,reopenEvaluatedBugs:2,reopenRate:0,bugs:alice.bugs.map(b=>({...b,reopenCount:0,reopenTimes:[]}))}
  await page.route('**/member-metrics**',route => { if (!new URL(route.request().url()).pathname.endsWith('/member-metrics')) return route.fallback(); return route.fulfill({json:{source,metrics:{month:'2026-10',endExclusive:endForMonth('2026-10'),requirementTarget:10,members:[unknown,zero,complete]}}}) })
  await page.goto('/devops?view=radar&month=2026-10&sort=reopenCount&order=desc')
  const panel=page.getByRole('region',{name:'个人月度雷达指标'}), table=panel.locator('.yx-member-quality-table')
  const rows=table.locator('.ant-table-row')
  await expect(rows.first().getByRole('button',{name:'历史完整',exact:true})).toBeVisible()
  await expect(rows.first().getByText('50.0%',{exact:true})).toBeVisible()
  await expect(rows.last().getByText('待同步',{exact:true})).toBeVisible()
  await expect(rows.last().getByText('已核验 0 / 2 个Bug')).toHaveCount(2)
  await panel.getByRole('columnheader',{name:'状态重开次数'}).click()
  await expect(rows.first().getByRole('button',{name:'无重开',exact:true})).toBeVisible()
  await expect(rows.last().getByRole('button',{name:'历史未采集',exact:true})).toBeVisible()
  await panel.getByLabel('成员排序').first().click()
  await page.locator('.ant-select-dropdown').getByTitle('重开Bug占比降序',{exact:true}).click()
  await expect(rows.first().getByRole('button',{name:'历史完整',exact:true})).toBeVisible()
  await expect(table.locator('.ant-table-row').first().getByText('2 次',{exact:true})).toBeVisible()
  await expect(panel.locator('.yx-member-radar-card').first()).not.toContainText('重开')
  await page.reload()
  await expect(rows.first().getByRole('button',{name:'历史完整',exact:true})).toBeVisible()
  await page.mouse.move(10,10)
  await page.screenshot({path:'/tmp/shepherd-member-reopen.png',fullPage:true,animations:'disabled'})
  await rows.first().getByRole('button',{name:'历史完整',exact:true}).click()
  const drawer=page.getByRole('dialog')
  await expect(drawer.getByText('本人责任处理缺陷',{exact:true})).toBeVisible()
  await drawer.getByRole('button',{name:'BUG-1',exact:true}).click()
  const history=page.getByRole('dialog',{name:'BUG-1 · 云效状态流水'})
  await expect(history.getByText('2026/10/15 11:00',{exact:true})).toBeVisible()
  await expect(history.getByText('重开事件',{exact:true})).toHaveCount(2)
})


test('Bug audit includes July legacy and unlinked Bugs, separates responsibility and exposes actual status evidence',async ({page}) => {
  await fixture(page)
  const legacy={...alice.bugs[0],externalId:'old',serial:'BUG-LEGACY',createdAt:'2026-07-17T00:00:00+08:00',responsibles:['原缺陷责任人'],developers:['本次修复人'],testers:['源测试人员'],reporter:'发现人',reopenCount:1,reopenTimes:['2026-10-19T09:00:00Z'],history:[{id:'original',at:'2026-09-16T08:00:00Z',from:'待处理',to:'已解决'},{id:'reopen',at:'2026-10-19T09:00:00Z',from:'已解决',to:'重新打开'}]}
  const communication={...alice.bugs[1],externalId:'communication',serial:'BUG-COMM',reopenCount:0,responsibles:['原缺陷责任人'],history:[{id:'fixed',at:'2026-10-21T09:00:00Z',from:'待处理',to:'已解决'},{id:'verified',at:'2026-10-22T01:00:00Z',from:'已解决',to:'已验证'}]}
  const developer={...alice,name:'原缺陷责任人',newRequirements:0,deliveredRequirements:0,dueRequirements:0,processedBugs:2,qualityBugs:2,reopenCount:1,reopenedBugs:1,reopenRate:50,bugs:[legacy,communication]}
  await page.route('**/member-metrics**',route=>{if(!new URL(route.request().url()).pathname.endsWith('/member-metrics'))return route.fallback();return route.fulfill({json:{source,metrics:{month:'2026-10',endExclusive:endForMonth('2026-10'),requirementTarget:10,members:[developer],bugQuality:{totalSourceBugs:2,verifiedSourceBugs:2,unknownSourceBugs:0,processedBugs:2,reopenedBugs:1,reopenCount:1,reopenRate:50,rows:[legacy,communication]}}}})})
  await page.goto('/devops?view=radar&month=2026-10')
  const panel=page.getByRole('region',{name:'个人月度雷达指标'})
  await expect(panel.locator('.yx-member-delivery-table .ant-table-row').first().getByRole('button',{name:'原缺陷责任人'})).toBeVisible()
  const audit=page.locator('#yx-bug-quality')
  await expect(audit.getByText('状态重开率 50.0%',{exact:true})).toBeVisible()
  await audit.getByLabel('搜索缺陷重开流水').fill('BUG-LEGACY')
  await expect(audit.getByRole('button',{name:'BUG-LEGACY',exact:true})).toBeVisible()
  await expect(audit.getByText('原缺陷责任人',{exact:true})).toBeVisible()
  await expect(audit.getByText('本次修复人',{exact:true})).toBeVisible()
  await audit.getByRole('button',{name:'BUG-LEGACY',exact:true}).click()
  const drawer=page.getByRole('dialog',{name:'BUG-LEGACY · 云效状态流水'})
  await expect(drawer.getByText('创建：2026/07/17 00:00',{exact:true})).toBeVisible()
  await expect(drawer.getByText('2026/09/16 16:00',{exact:true})).toBeVisible()
  await expect(drawer.getByText('2026/10/19 17:00',{exact:true})).toBeVisible()
  await drawer.getByRole('button',{name:'关闭',exact:true}).click()
  await audit.getByLabel('搜索缺陷重开流水').fill('BUG-COMM')
  await expect(audit.locator('.ant-table-row td').nth(2)).toHaveText('0')
  await audit.getByLabel('搜索缺陷重开流水').fill('')
  await audit.screenshot({path:'/tmp/shepherd-bug-quality-correction.png',animations:'disabled'})
  await page.setViewportSize({width:390,height:1100})
  const bounds=await audit.evaluate(el=>({right:el.getBoundingClientRect().right,width:el.clientWidth,content:el.scrollWidth}))
  expect(bounds.right).toBeLessThanOrEqual(390)
  expect(bounds.content).toBeLessThanOrEqual(bounds.width+1)
  await expect(page.getByRole('combobox',{name:'项目切换'})).toBeVisible()
  const toolbar=await page.locator('.ms-topbar').evaluate(el=>({width:el.clientWidth,content:el.scrollWidth}))
  expect(toolbar.content).toBeLessThanOrEqual(toolbar.width+1)
  await page.screenshot({path:'/tmp/shepherd-mobile-statistics.png',fullPage:true,animations:'disabled'})
})


test('delivery participation and Bug responsibility keep their own denominators and event units',async ({page})=>{
  await fixture(page)
  await page.setViewportSize({width:1765,height:1800})
  const person={...alice,name:'研发成员',radar:[30,33.3,80],deliveredRequirements:3,linkedBugs:25,bugAffectedRequirements:2,bugRate:66.7,processedBugs:58,qualityBugs:60,reopenEvaluatedBugs:60,reopenedBugs:11,reopenCount:14,reopenRate:19,bugs:[]}
  await page.route('**/member-metrics**',route=>{if(!new URL(route.request().url()).pathname.endsWith('/member-metrics'))return route.fallback();return route.fulfill({json:{source,metrics:{month:'2026-10',endExclusive:endForMonth('2026-10'),requirementTarget:10,members:[person]}}})})
  await page.goto('/devops?view=radar&month=2026-10')
  const panel=page.getByRole('region',{name:'个人月度雷达指标'}),delivery=panel.locator('.yx-member-delivery-table'),quality=panel.locator('.yx-member-quality-table')
  await expect(delivery.getByText('25',{exact:true})).toBeVisible()
  await expect(delivery.getByText('2 / 3 项需求',{exact:true})).toBeVisible()
  await expect(delivery.getByRole('columnheader',{name:'状态重开次数'})).toHaveCount(0)
  await expect(quality.getByText('58 个',{exact:true})).toBeVisible()
  await expect(quality.getByText('11 个',{exact:true})).toBeVisible()
  await expect(quality.getByText('14 次',{exact:true})).toBeVisible()
  await expect(quality.getByText('19.0%',{exact:true})).toBeVisible()
  await expect(quality.getByText('11 / 58 个Bug',{exact:true})).toHaveCount(1)
  await expect(quality.getByRole('columnheader',{name:'关联Bug数'})).toHaveCount(0)
  const cells=quality.locator('.ant-table-row td')
  await expect(cells.nth(4)).not.toContainText('11 / 58')
  await expect(panel.locator('.yx-member-radars')).not.toContainText('重开')
  await quality.getByRole('columnheader',{name:'责任处理Bug数'}).click()
  await expect.poll(()=>new URL(page.url()).searchParams.get('sort')).toBe('processedBugs')
  await page.reload()
  await expect(quality.getByText('58 个',{exact:true})).toBeVisible()
  await page.screenshot({path:'/tmp/shepherd-metric-cohorts.png',fullPage:true,animations:'disabled'})
  await page.setViewportSize({width:390,height:1100})
  for(const card of [delivery,quality]){
    const b=await card.evaluate(el=>({right:el.getBoundingClientRect().right,width:el.clientWidth,content:el.scrollWidth}))
    expect(b.right).toBeLessThanOrEqual(390)
    expect(b.content).toBeLessThanOrEqual(b.width+1)
  }
})


test('default month advances in an open page and explicit historical selection stays fixed',async ({ page }) => {
  await page.clock.install({ time:new Date('2026-11-30T15:59:30Z') })
  const calls=await fixture(page)
  await page.goto('/devops?view=radar')
  await expect.poll(()=>calls.at(-1)?.searchParams.get('month')).toBe('2026-11')
  await page.clock.fastForward(61_000)
  await expect.poll(()=>calls.at(-1)?.searchParams.get('month')).toBe('2026-12')
  await page.goto('/devops?view=radar&month=2026-10')
  await page.clock.fastForward(61_000)
  await expect(page.getByLabel('统计月份',{exact:true})).toHaveValue('2026年10月')
  expect(calls.at(-1)?.searchParams.get('month')).toBe('2026-10')
})

test('monthly comparisons share member filtering and show percentage points and zero bases honestly',async ({ page }) => {
  await page.clock.setFixedTime(new Date('2026-11-04T04:00:00Z'))
  await fixture(page,true,true)
  await page.goto('/devops?view=radar')
  const card=page.locator('.ant-card').filter({has:page.getByText('月度同比 / 环比',{exact:true})})
  await expect(card.getByText('+100.0%',{exact:true})).toBeVisible()
  await expect(card.getByText('基期为0',{exact:true})).toBeVisible()
  await card.getByLabel('同比环比指标').first().click()
  await page.locator('.ant-select-dropdown:visible').getByTitle('含Bug交付需求占比',{exact:true}).click()
  await expect(card.getByText('-25.0 个百分点',{exact:true})).toBeVisible()
  await expect(card.getByRole('columnheader',{name:'环比（百分点）'})).toBeVisible()
  await page.screenshot({path:'/tmp/shepherd-monthly-comparisons.png',fullPage:true,animations:'disabled'})
  await page.getByPlaceholder('搜索成员').fill('研发乙')
  await expect(card.getByRole('cell',{name:'研发甲',exact:true})).toHaveCount(0)
  await expect(card.getByRole('cell',{name:'研发乙',exact:true})).toBeVisible()
  await page.setViewportSize({width:390,height:1000})
  // Ant Table updates its measured scroll container via ResizeObserver after viewport changes.
  // Await the final layout and allow the same one-pixel rounding as other responsive checks.
  await expect.poll(() => page.evaluate(()=>document.documentElement.scrollWidth)).toBeLessThanOrEqual(391)
  const bounds=await card.evaluate(el=>({right:el.getBoundingClientRect().right,width:el.clientWidth,content:el.scrollWidth}))
  expect(bounds.right).toBeLessThanOrEqual(391)
  expect(bounds.content).toBeLessThanOrEqual(bounds.width+1)
  await expect(page.getByRole('combobox',{name:'项目切换'})).toBeVisible()
  const toolbar=await page.locator('.ms-topbar').evaluate(el=>({width:el.clientWidth,content:el.scrollWidth}))
  expect(toolbar.content).toBeLessThanOrEqual(toolbar.width+1)
  await page.screenshot({path:'/tmp/shepherd-mobile-statistics.png',fullPage:true,animations:'disabled'})
})
