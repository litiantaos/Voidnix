import { defineConfig } from '@/runtime/storage'

/// clipboard 扩展自管配置（持久化至 extensions/clipboard/config.json）。
export const config = defineConfig('clipboard', {
  maxDays: 30,
})
