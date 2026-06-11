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
    timeout: 15000,
  },
})
