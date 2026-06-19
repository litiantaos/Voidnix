import { defineConfig } from '@/runtime/storage'

/// zsh-autosuggestions 扩展自管配置。
/// enabled 切换由 View.vue toggle 显式 invoke（含错误反馈），不在 watch 中静默同步。
export const config = defineConfig('zsh-autosuggestions', {
  enabled: false,
})
