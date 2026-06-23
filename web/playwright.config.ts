import { defineConfig, devices } from '@playwright/test'

// E2E 冒烟:针对正在运行的 dev 栈(vite :5173 代理 → server :9180)。
// 前置:Shepherd server 跑在 :9180(见 scripts/agent/README 或 dev 启动命令)。
// vite 由下方 webServer 自动起(已起则复用)。首跑装浏览器:npx playwright install chromium。
const PORT = process.env.E2E_PORT || '5173'

export default defineConfig({
  testDir: './e2e',
  fullyParallel: false, // 共享后端状态,串行更稳
  workers: 1,
  retries: process.env.CI ? 1 : 0,
  timeout: 45_000,
  expect: { timeout: 10_000 },
  reporter: [['list']],
  use: {
    baseURL: process.env.E2E_BASE_URL || `http://localhost:${PORT}`,
    screenshot: 'only-on-failure',
    trace: 'on-first-retry',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  // vite(:5173)由 dev 流程自管(npm run dev),E2E 直接打现成栈。
  // 想让 Playwright 代起 vite,设 E2E_START_VITE=1。
  webServer: process.env.E2E_START_VITE
    ? { command: 'npm run dev', url: `http://localhost:${PORT}`, reuseExistingServer: true, timeout: 60_000 }
    : undefined,
})
