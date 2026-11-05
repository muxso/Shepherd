import { test } from './fixtures'

// The task center loads with a real table.
test('任务中心 /system/tasks 加载', async ({ taskCenter }) => {
  await taskCenter.goto()
  await taskCenter.expectLoaded()
})
