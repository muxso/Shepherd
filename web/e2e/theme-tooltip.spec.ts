import { test,expect } from './fixtures'

test('tooltips keep readable text across user palettes',async({page,auth})=>{
 let preference={mode:'light',primaryColor:'#4D9F92'}
 await page.route('**/auth/theme',async route=>{if(route.request().method()==='GET')await route.fulfill({json:{userId:'u-admin',theme:preference}});else await route.continue()})
 for(const [mode,primaryColor]of[['light','#4D9F92'],['dark','#4D9F92'],['light','#FFFF00'],['dark','#FFFFFF'],['light','#000000'],['light','#328C96'],['dark','#B67842']]){
  preference={mode,primaryColor}
  await page.goto(`/p/${auth.projectId}/functional-case`)
  await expect(page.locator('html')).toHaveAttribute('data-theme',mode)
  await expect(page.locator('html')).toHaveAttribute('data-primary-color',primaryColor)
  await page.locator('button:has(.anticon-share-alt)').hover()
  const body=page.locator('.ant-tooltip:visible .ant-tooltip-container')
  await expect(body).toBeVisible()
  const ratio=await body.evaluate(element=>{
   const s=getComputedStyle(element),luminance=(v:string)=>v.match(/[\d.]+/g)!.slice(0,3).map(Number).map(n=>n/255).map(n=>n<=.04045?n/12.92:((n+.055)/1.055)**2.4).reduce((sum,n,i)=>sum+n*[.2126,.7152,.0722][i],0)
   const a=luminance(s.color),b=luminance(s.backgroundColor)
   return(Math.max(a,b)+.05)/(Math.min(a,b)+.05)
  })
  expect(ratio,`${mode} ${primaryColor}`).toBeGreaterThanOrEqual(7)
  const action=page.locator('.ant-btn-color-primary.ant-btn-variant-solid:visible:not(:disabled)').first()
  await action.hover();await page.waitForTimeout(220)
  const style=await action.evaluate(element=>{const s=getComputedStyle(element);return{color:s.color,background:s.backgroundColor}})
  expect(style.color).toBe('rgb(255, 255, 255)')
  const contrast=await action.evaluate(element=>{const s=getComputedStyle(element);const luminance=(v:string)=>v.match(/[\d.]+/g)!.slice(0,3).map(Number).map(n=>n/255).map(n=>n<=.04045?n/12.92:((n+.055)/1.055)**2.4).reduce((sum,n,i)=>sum+n*[.2126,.7152,.0722][i],0);const a=luminance(s.color),b=luminance(s.backgroundColor);return(Math.max(a,b)+.05)/(Math.min(a,b)+.05)})
  expect(contrast,`primary action ${mode} ${primaryColor}`).toBeGreaterThanOrEqual(4.5)
 }
})


test('appearance preview and saved primary action use white text',async({page,request,auth})=>{
 const headers={authorization:`Bearer ${auth.token}`};const initial=await(await request.get('/auth/theme',{headers})).json()
 try{
  await page.goto(`/p/${auth.projectId}/functional-case`)
  await page.locator('.ant-avatar').click();await page.getByRole('menuitem',{name:'个人中心'}).click()
  await page.getByText('主题与外观',{exact:true}).first().click()
  await page.getByRole('button',{name:'湖水青',exact:true}).click()
  const action=page.getByLabel('主题效果预览').getByRole('button',{name:'主要操作',exact:true})
  await expect(action).toHaveCSS('color','rgb(255, 255, 255)')
  await expect(page.getByRole('button',{name:'保存并应用主题',exact:true})).toHaveCSS('color','rgb(255, 255, 255)')
  await page.getByRole('button',{name:'保存并应用主题',exact:true}).click()
  await expect(page.locator('html')).toHaveAttribute('data-primary-color','#328C96')
  await expect(page.getByText('主题已保存并应用',{exact:true})).toBeVisible()
 }finally{await request.put('/auth/theme',{headers,data:initial.theme||{mode:'dark',primaryColor:'#4D9F92'}})}
})
