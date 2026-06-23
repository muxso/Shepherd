import { type Page, type Locator, expect } from '@playwright/test'

/** 需求页对象:列表 + 详情(拆分 / 功能用例覆盖)。选择器集中此处。 */
export class RequirementsPage {
  constructor(readonly page: Page) {}

  async goto() {
    await this.page.goto('/requirement')
  }

  /** 列表里某需求的行(按标题过滤)。 */
  row(title: string): Locator {
    return this.page.getByRole('row').filter({ hasText: title })
  }

  /** 行内覆盖率百分比徽标(① 列表徽标)。 */
  badge(title: string): Locator {
    return this.row(title).getByText(/%/).first()
  }

  /** 打开某需求详情。 */
  async open(title: string) {
    await this.page.getByText(title).first().click()
  }

  tab(name: string | RegExp): Locator {
    return this.page.getByRole('tab', { name })
  }

  /** 需求信息 tab 里点「自动拆分」(前端拆分 + 记 decompId)。 */
  async breakdown() {
    await this.page.getByRole('button', { name: '自动拆分' }).click()
  }

  async openDecompositionTab() {
    await this.tab(/拆分 \/ 交付 \/ 验证/).click()
  }

  async openCoverageTab() {
    await this.tab('功能用例覆盖').click()
  }

  /** 拆分视图标志(任务图渲染后出现)。 */
  workloadTotal(): Locator {
    return this.page.getByText(/工作量合计/)
  }

  /** 断言功能用例覆盖率 = covered/total。 */
  async expectCoverage(covered: number, total: number) {
    await expect(
      this.page.getByText(new RegExp(`覆盖率.*${covered}\\s*/\\s*${total}`)),
    ).toBeVisible()
  }
}
