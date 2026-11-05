import { defineConfig, devices } from '@playwright/test'

// E2E smoke tests against Vite on :5173 and the backend on :9180.
// Start the backend before running tests.
// Install Chromium once with npx playwright install chromium.
const PORT = process.env.E2E_PORT || '5173'

export default defineConfig({
  testDir: './e2e',
  fullyParallel: false, // Shared backend state requires serial tests.
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
  // Reuse the development server by default.
  // Set E2E_START_VITE=1 to let Playwright start Vite.
  webServer: process.env.E2E_START_VITE
    ? { command: 'npm run dev', url: `http://localhost:${PORT}`, reuseExistingServer: true, timeout: 60_000 }
    : undefined,
})
