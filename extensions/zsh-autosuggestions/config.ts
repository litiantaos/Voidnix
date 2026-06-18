import { watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { defineConfig } from '@/runtime/storage'

/// zsh-autosuggestions 扩展自管配置。
export const config = defineConfig('zsh-autosuggestions', {
  enabled: false,
})

/// enabled 变更时同步到 Rust 端（安装/卸载 zsh 集成）。
watch(
  () => config.enabled,
  (enabled) => {
    invoke('set_zsh_autosuggestions_enabled', { enabled }).catch(() => {})
  },
)
