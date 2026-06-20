import { defineConfig } from '@playwright/test'

export default defineConfig({
  testDir: './e2e/specs',
  timeout: 30000,
  retries: 0,
  use: {
    baseURL: 'http://localhost:1420',
    trace: 'on-first-retry',
  },
  webServer: {
    command: 'bun run dev',
    port: 1420,
    reuseExistingServer: true,
    // CI 冷启动 + Vite 首次构建可能较慢，给充足窗口（本地 reuseExistingServer 不受影响）
    timeout: 60000,
  },
})
