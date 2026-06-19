import { watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { defineConfig } from '@/runtime/storage'

/// finder-ext 扩展自管配置。
export const config = defineConfig('finder-ext', {
  enabled: false,
})

/// enabled 变更时同步到 Rust 端（启用/禁用 Finder 扩展）。
watch(
  () => config.enabled,
  (enabled) => {
    invoke(CMD.setFinderExtEnabled, { enabled }).catch(() => {})
  },
)
