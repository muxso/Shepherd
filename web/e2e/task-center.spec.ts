import { test, expect } from './fixtures'

// 任务中心(派发的 Claude 实现的页面)能正常加载。
test('任务中心 /system/tasks 加载', async ({ page }) => {
  await page.goto('/system/tasks')
  // 页面渲染:任务表格出现(空数据也会渲染表头)。
  await expect(page.getByRole('table').first()).toBeVisible({ timeout: 15_000 })
  // 命中的是 TaskCenter 而非 ComingSoon 占位。
  await expect(page.getByText(/敬请期待|Coming soon/)).toHaveCount(0)
})
