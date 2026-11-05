import { type Page, type Locator, expect } from '@playwright/test'

/** Requirement list, decomposition, and coverage interactions. */
export class RequirementsPage {
  constructor(readonly page: Page) {}

  async goto() {
    await this.page.goto('/requirement')
  }

  /** Find a requirement row by title. */
  row(title: string): Locator {
    return this.page.getByRole('row').filter({ hasText: title })
  }

  /** Coverage badge in the requirement row. */
  badge(title: string): Locator {
    return this.row(title).getByText(/%/).first()
  }

  /** Open requirement details. */
  async open(title: string) {
    await this.page.getByText(title).first().click()
  }

  tab(name: string | RegExp): Locator {
    return this.page.getByRole('tab', { name })
  }

  /** Decompose from the information tab and retain the decomposition ID. */
  async breakdown() {
    await this.page.getByRole('button', { name: '自动拆分' }).click()
  }

  async openDecompositionTab() {
    await this.tab(/拆分 \/ 交付 \/ 验证/).click()
  }

  async openCoverageTab() {
    await this.tab('功能用例覆盖').click()
  }

  /** Wait for the task graph. */
  workloadTotal(): Locator {
    return this.page.getByText(/工作量合计/)
  }

  /** Assert the covered and total case counts. */
  async expectCoverage(covered: number, total: number) {
    await expect(
      this.page.getByText(new RegExp(`覆盖率.*${covered}\\s*/\\s*${total}`)),
    ).toBeVisible()
  }
}
