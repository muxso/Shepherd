import { test, expect, type Page } from '@playwright/test'
import { monthGrid, dateKey } from '../src/plugins/yunxiao/codeActivity'
test.setTimeout(90_000)
const source={id:'source-a',name:'测试需求池',project_id:'project'}
const base={author:'研发甲',group:'研发组',repoId:'1',repo:'org/service-api',languages:['Java'],linked:true,requirements:[{externalId:'req-1',serial:'REQ-1',title:'支付申请负数校验'}],additions:20,deletions:5}
const commits=[
 {...base,id:'a',sha:'a',authoredAt:'2026-11-01T02:00:00Z'},
 {...base,id:'b',sha:'b',repoId:'2',repo:'org/console-web',languages:['TypeScript','Vue'],authoredAt:'2026-11-01T10:30:00Z'},
 {...base,id:'c',sha:'c',author:'研发乙',repoId:'2',repo:'org/console-web',languages:['TypeScript','Vue'],authoredAt:'2026-11-02T16:10:00Z',additions:null,deletions:null,requirements:[],linked:false},
]
async function fixture(page:Page,can=true) {
 const calls:URL[]=[]
 await page.addInitScript(()=>{localStorage.setItem('shepherd.token','test');localStorage.setItem('shepherd.projectId','project');localStorage.setItem('shepherd.theme','light')})
 await page.route('**/*',async route=>{
  if(!['fetch','xhr'].includes(route.request().resourceType())) return route.continue()
  const url=new URL(route.request().url()),path=url.pathname
  let body:unknown={items:[],total:0}
  if(path==='/organization')body={items:[{id:'org'}]}
  if(path==='/project')body={items:[{id:'project',name:'项目',organizationId:'org'}]}
  if(path==='/api/plugins')body={items:[{id:'yunxiao',enabled:true,name:'云效同步'}]}
  if(path==='/api/devops/sources')body={items:[source]}
  if(path.endsWith('/code-authors')) {calls.push(url);body={source,month:url.searchParams.get('month'),timezone:'Asia/Shanghai',commits:commits.map(c=>({...c,requirements:can?c.requirements:[]})),permissions:{requirements:can}}}
  if(path.endsWith('/dashboard'))body={source,authors:[],totals:{}}
  await route.fulfill({json:body})
 })
 return calls
}
test('daily actual last time, repository and requirement tooltip, line gaps and drilldown',async({page},info)=>{
 const calls=await fixture(page)
 await page.goto('/devops?view=authors&month=2026-11&source=source-a')
 await page.getByText('提交趋势',{exact:true}).click()
 const first=page.getByRole('button',{name:'2026-11-01 最后提交 18:30:00，2 次提交',exact:true})
 await expect(first).toBeVisible()
 expect(calls[0].searchParams.get('month')).toBe('2026-11')
 await expect(page.getByRole('button',{name:'2026-11-03 最后提交 00:10:00，1 次提交',exact:true})).toBeVisible()
 await first.focus(); await first.press('Enter')
 const tip=page.getByTestId('code-author-point-detail')
 await expect(tip.getByText('org/service-api · 1 次',{exact:false})).toBeVisible()
 await expect(tip.getByText('org/console-web · 1 次',{exact:false})).toBeVisible()
 await expect(tip.getByRole('button',{name:'REQ-1 支付申请负数校验'})).toBeVisible()
 await first.press('Tab')
 await page.getByText('变更行数',{exact:true}).click()
 await expect(page.getByText('空心点表示当天存在未采集的行数',{exact:false})).toBeVisible()
 await page.locator('.ant-table-row-expand-icon').first().click()
 await expect(page.getByText('2026-11-01 18:30:00',{exact:true}).first()).toBeVisible()
 await page.getByTestId('devops-scroll').evaluate(el=>el.scrollTop=0)
 await page.screenshot({path:info.outputPath('code-authors-desktop.png'),fullPage:true,animations:'disabled'})
 await first.focus(); await first.press('Enter')
 await tip.getByRole('button',{name:'REQ-1 支付申请负数校验'}).click()
 await expect(page).toHaveURL(/reference=req-1/)
})
test('language and author filters, month selection and browser refresh preserve scope',async({page})=>{
 const calls=await fixture(page)
 await page.goto('/devops?view=authors&month=2026-11&source=source-a')
 await expect(page.getByText('本期 3 次提交',{exact:false})).toBeVisible()
 await page.getByLabel('仓库语言筛选',{exact:true}).first().click()
 await page.locator('.ant-select-dropdown').getByTitle('Java',{exact:true}).click()
 await expect(page.getByText('本期 1 次提交',{exact:false})).toBeVisible()
 await page.getByRole('button',{name:'研发甲',exact:true}).click()
 await expect(page.getByText('研发甲 · 每日最后提交日历',{exact:true})).toBeVisible()
 await page.getByLabel('代码统计月份',{exact:true}).click()
 await page.locator('.ant-picker-dropdown').getByTitle('2026-10',{exact:true}).click()
 await expect.poll(()=>calls.at(-1)?.searchParams.get('month')).toBe('2026-10')
 await page.reload()
 await expect(page.getByLabel('代码统计月份',{exact:true})).toHaveValue('2026年10月')
})
test('no requirement permission, contained narrow layout',async({page},info)=>{
 await fixture(page,false)
 await page.setViewportSize({width:390,height:1000})
 await page.goto('/devops?view=authors&month=2026-11&source=source-a')
 await page.getByText('提交趋势',{exact:true}).click()
 const point=page.getByRole('button',{name:'2026-11-01 最后提交 18:30:00，2 次提交',exact:true})
 await expect(point).toBeVisible()
 expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBeLessThanOrEqual(390)
 await page.screenshot({path:info.outputPath('code-authors-mobile-layout.png'),fullPage:true,animations:'disabled'})
 await point.focus()
 await point.press('Enter')
 await expect(page.getByTestId('code-author-point-detail').getByText('无需求读取权限')).toBeVisible()
 await expect(page.getByText('支付申请负数校验',{exact:false})).toHaveCount(0)
 await page.screenshot({path:info.outputPath('code-authors-mobile.png'),fullPage:true,animations:'disabled'})
})


test('calendar defaults to each person last Shanghai time, keeps different authors separate, and drills into repositories',async({page},info)=>{
 await fixture(page)
 await page.route('**/code-authors?**',route=>route.fulfill({json:{source,month:'2026-11',timezone:'Asia/Shanghai',commits:[...commits,{...base,id:'later-other',sha:'later-other',author:'研发乙',authoredAt:'2026-11-01T15:45:00Z'}],permissions:{requirements:true}}}))
 await page.goto('/devops?view=authors&month=2026-11&source=source-a')
 const calendar=page.getByRole('region',{name:'个人每日最后提交日历'})
 await expect(calendar).toBeVisible()
 await expect(page.getByRole('group',{name:'每日代码提交折线趋势，横轴为每日最后提交时间'})).toHaveCount(0)
 const day=calendar.locator('[data-date="2026-11-01"]')
 await expect(day.getByRole('button',{name:'2026-11-01 研发甲 最后提交 18:30:00，2 次提交',exact:true})).toBeVisible()
 await expect(day.getByRole('button',{name:'2026-11-01 研发乙 最后提交 23:45:00，1 次提交',exact:true})).toBeVisible()
 await expect(calendar.locator('[data-date="2026-11-03"]').getByRole('button',{name:'2026-11-03 研发乙 最后提交 00:10:00，1 次提交',exact:true})).toBeVisible()
 await expect(calendar.locator('[data-date="2026-11-02"]')).toContainText('暂无记录')
 await day.getByRole('button',{name:'2026-11-01 研发甲 最后提交 18:30:00，2 次提交',exact:true}).focus()
 await page.keyboard.press('Enter')
 const drawer=page.getByRole('dialog',{name:'2026-11-01 · 每日最后提交明细'})
 await expect(drawer.locator('.yx-calendar-detail-time time')).toBeVisible()
 await expect(drawer.getByText('org/service-api',{exact:true}).first()).toBeVisible()
 await expect(drawer.getByText('org/console-web',{exact:true}).first()).toBeVisible()
 await expect(drawer.getByRole('button',{name:'REQ-1 支付申请负数校验'})).toBeVisible()
 await expect(page.locator('.yx-calendar-drawer .ant-drawer-content-wrapper')).toHaveCSS('transform','none')
 await drawer.getByRole('button',{name:'关闭',exact:true}).click()
 await page.getByTestId('devops-scroll').evaluate(el=>el.scrollTop=0)
 await page.screenshot({path:info.outputPath('code-authors-calendar.png'),fullPage:true,animations:'disabled'})
})

test('calendar respects language filtering, leap months and mobile permissions',async({page},info)=>{
 await fixture(page,false)
 await page.setViewportSize({width:390,height:1100})
 await page.goto('/devops?view=authors&month=2026-11&source=source-a')
 const calendar=page.getByRole('region',{name:'个人每日最后提交日历'})
 await expect(calendar).toBeVisible()
 await expect.poll(()=>page.evaluate(()=>document.documentElement.scrollWidth)).toBeLessThanOrEqual(391)
 const button=calendar.getByRole('button',{name:'2026-11-01 研发甲 最后提交 18:30:00，2 次提交',exact:true})
 await button.click()
 const drawer=page.getByRole('dialog',{name:'2026-11-01 · 每日最后提交明细'})
 await expect(drawer.getByText('无需求读取权限')).toBeVisible()
 await expect(page.getByText('支付申请负数校验',{exact:false})).toHaveCount(0)
 await expect(page.locator('.yx-calendar-drawer .ant-drawer-content-wrapper')).toHaveCSS('transform','none')
 await drawer.getByRole('button',{name:'关闭',exact:true}).click()
 await page.getByLabel('仓库语言筛选',{exact:true}).first().click()
 await page.locator('.ant-select-dropdown').getByTitle('Java',{exact:true}).click()
 await expect(calendar.getByRole('button',{name:'2026-11-01 研发甲 最后提交 10:00:00，1 次提交',exact:true})).toBeVisible()
 await expect(calendar.getByRole('button',{name:'2026-11-01 研发甲 最后提交 18:30:00，2 次提交',exact:true})).toHaveCount(0)
 await page.screenshot({path:info.outputPath('code-authors-calendar-mobile.png'),fullPage:true,animations:'disabled'})
})


test('calendar dates handle leap years, month starts and Shanghai midnight',()=>{
 const days=(m:string)=>monthGrid(m).flat().filter(Boolean)
 expect(days('2024-02')).toHaveLength(29)
 expect(days('2025-02')).toHaveLength(28)
 expect(days('2024-02').at(-1)).toBe('2024-02-29')
 expect(monthGrid('2026-11')[0][6]).toBe('2026-11-01')
 expect(dateKey('2026-11-02T16:10:00Z')).toBe('2026-11-03')
})
