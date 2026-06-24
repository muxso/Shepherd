import { type Page } from '@playwright/test'

/** 登录页对象:封装登录 UI 交互(选择器集中在此,spec 不碰 DOM 细节)。 */
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
