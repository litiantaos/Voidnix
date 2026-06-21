import { defineConfig } from '@/runtime/storage'

/// awake 扩展自管配置（持久化至 extensions/awake/config.json）。
export const config = defineConfig('extensions/awake/config', {
  mirrorMode: true,
})
