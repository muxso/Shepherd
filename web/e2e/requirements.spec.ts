import { test, expect } from './fixtures'
import { decompose } from './flows'

// Decomposition creates a task graph and functional case coverage.
test('自动拆分 → 任务图 + 功能用例覆盖自动生成', async ({ requirements, createReq }) => {
  const { title } = await createReq(['标准一:登录成功', '标准二:错误密码拒绝'])

  await decompose(requirements, title)

  await requirements.openDecompositionTab()
  await expect(requirements.workloadTotal()).toBeVisible()

  await requirements.openCoverageTab()
  await requirements.expectCoverage(2, 2) // Each criterion links one functional case.
})

// Requirement rows display coverage badges.
test('需求列表带覆盖率徽标', async ({ requirements, createReq }) => {
  const { title } = await createReq(['唯一标准'])
  await requirements.goto()
  await expect(requirements.row(title)).toBeVisible()
  await expect(requirements.badge(title)).toBeVisible()
})
