import { type Page } from '@playwright/test'

/** Login interactions and selectors. */
export class LoginPage {
  constructor(private readonly page: Page) {}

  async goto() {
    await this.page.goto('/login')
  }

  async login(username: string, password: string) {
    await this.page.getByPlaceholder('admin').fill(username)
    await this.page.locator('input[type="password"]').fill(password)
    await this.page.locator('button[type="submit"]').click()
  }
}
