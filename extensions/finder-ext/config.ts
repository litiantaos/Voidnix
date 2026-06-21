import { defineConfig } from '@/runtime/storage'

/// finder-ext 扩展自管配置。
///
/// H11：enabled 变更的 Rust 同步由 View.vue 的 toggle 显式 invoke + 错误反馈处理
/// （AGENTS「显式 invoke，勿用 watch 静默吞错」规约）。
export const config = defineConfig('extensions/finder-ext/config', {
  enabled: false,
})
