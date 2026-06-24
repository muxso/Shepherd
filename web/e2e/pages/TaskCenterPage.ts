import { type Page, type Locator, expect } from '@playwright/test'

/** 任务中心页对象(/system/tasks)。 */
export class TaskCenterPage {
  constructor(readonly page: Page) {}

  async goto() {
    await this.page.goto('/system/tasks')
  }

  table(): Locator {
    return this.page.getByRole('table').first()
  }

  /** 加载成功:表格渲染(空数据也有表头)且非 ComingSoon 占位。 */
  async expectLoaded() {
    await expect(this.table()).toBeVisible({ timeout: 15_000 })
    await expect(this.page.getByText(/敬请期待|Coming soon/)).toHaveCount(0)
  }
}
