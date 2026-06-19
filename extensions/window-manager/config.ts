import { watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { defineConfig } from '@/runtime/storage'

/// window-manager 扩展自管配置（持久化至 extensions/window-manager/config.json）。
export const config = defineConfig('window-manager', {
  customWidth: 1200,
  customHeight: 800,
  dragSnapEnabled: true,
})

/// 配置变更时同步拖拽 snap 状态到 Rust 端。
watch(
  config,
  (val) => {
    invoke(CMD.toggleDragSnap, {
      enabled: val.dragSnapEnabled,
      width: val.customWidth,
      height: val.customHeight,
    }).catch(() => {})
  },
  { deep: true },
)
