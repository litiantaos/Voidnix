import { watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { defineConfig } from '@/runtime/storage'

/// clipboard 扩展自管配置（持久化至 extensions/clipboard/config.json）。
/// version: 1 —— schema 版本号，磁盘不匹配时清空用 defaults（自开发自用）。
export const config = defineConfig(
  'extensions/clipboard/config',
  {
    maxDays: 30,
  },
  { version: 1 },
)

/// maxDays 变更同步 Rust 端 AtomicI32（替代 Rust 直读 config.json）。
/// immediate: true —— 启动时用已持久化配置首次同步，否则 Rust 用默认 30 直到下次变更。
watch(
  () => config.maxDays,
  (v) => {
    invoke(CMD.setClipboardMaxDays, { maxDays: v }).catch((e: unknown) => {
      console.error('[clipboard] setClipboardMaxDays failed:', e)
    })
  },
  { immediate: true },
)
