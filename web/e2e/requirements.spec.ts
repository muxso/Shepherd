import { test, expect, createRequirement } from './fixtures'

// 需求 → UI 点「自动拆分」→ 任务图 + 功能用例覆盖自动生成(端到端验证近期核心能力)。
test('自动拆分 → 任务图 + 功能用例覆盖自动生成', async ({ page, request, auth }) => {
  const title = `E2E-拆分覆盖-${Date.now()}`
  const criteria = ['标准一:登录成功', '标准二:错误密码拒绝']
  await createRequirement(request, auth.token, auth.projectId, title, criteria)

  await page.goto('/requirement')
  await page.getByText(title).first().click()

  // 需求信息(默认 tab)→ 点「自动拆分」(前端拆分 + 记 decompId)。
  await page.getByRole('button', { name: '自动拆分' }).click()

  // 拆分 / 交付 / 验证:应渲染任务图。
  await page.getByRole('tab', { name: /拆分 \/ 交付 \/ 验证/ }).click()
  await expect(page.getByText(/工作量合计/)).toBeVisible()

  // 功能用例覆盖:两条标准应各自动关联一个功能用例 → 覆盖率 2/2(覆盖率即来自已关联用例数)。
  await page.getByRole('tab', { name: '功能用例覆盖' }).click()
  await expect(page.getByText(/覆盖率.*2\s*\/\s*2/)).toBeVisible()
})

// 需求列表显示覆盖率徽标(① 列表徽标)。
test('需求列表带覆盖率徽标', async ({ page, request, auth }) => {
  const title = `E2E-列表徽标-${Date.now()}`
  await createRequirement(request, auth.token, auth.projectId, title, ['唯一标准'])
  await page.goto('/requirement')
  const row = page.getByRole('row').filter({ hasText: title })
  await expect(row).toBeVisible()
  await expect(row.getByText(/%/).first()).toBeVisible() // 行内覆盖率百分比徽标
})
