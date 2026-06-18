import { defineConfig } from '@/runtime/storage'

/// screenshot 扩展自管配置（持久化至 extensions/screenshot/config.json）。
export const config = defineConfig('screenshot', {
  savePath: '',
})
