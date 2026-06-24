import { test, expect } from './fixtures'
import { decompose } from './flows'

// 需求 → 自动拆分 → 任务图 + 功能用例覆盖自动生成(组合 flow + page-object)。
test('自动拆分 → 任务图 + 功能用例覆盖自动生成', async ({ requirements, createReq }) => {
  const { title } = await createReq(['标准一:登录成功', '标准二:错误密码拒绝'])

  await decompose(requirements, title)

  await requirements.openDecompositionTab()
  await expect(requirements.workloadTotal()).toBeVisible()

  await requirements.openCoverageTab()
  await requirements.expectCoverage(2, 2) // 两条标准各自动关联一个功能用例
})

// 需求列表显示覆盖率徽标(① 列表徽标)。
test('需求列表带覆盖率徽标', async ({ requirements, createReq }) => {
  const { title } = await createReq(['唯一标准'])
  await requirements.goto()
  await expect(requirements.row(title)).toBeVisible()
  await expect(requirements.badge(title)).toBeVisible()
})
