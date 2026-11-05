import { type Page, type Locator, expect } from '@playwright/test'

/** Task center page object. */
export class TaskCenterPage {
  constructor(readonly page: Page) {}

  async goto() {
    await this.page.goto('/system/tasks')
  }

  table(): Locator {
    return this.page.getByRole('table').first()
  }

  /** The table renders, including when empty. */
  async expectLoaded() {
    await expect(this.table()).toBeVisible({ timeout: 15_000 })
    await expect(this.page.getByText(/敬请期待|Coming soon/)).toHaveCount(0)
  }
}
