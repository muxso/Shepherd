import { test } from './fixtures'

// 任务中心(派发的 Claude 实现的页面)能正常加载。
test('任务中心 /system/tasks 加载', async ({ taskCenter }) => {
  await taskCenter.goto()
  await taskCenter.expectLoaded()
})
