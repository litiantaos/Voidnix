import { watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { defineConfig } from '@/runtime/storage'

/// window-manager 扩展自管配置（持久化至 extensions/window-manager/config.json）。
export const config = defineConfig('extensions/window-manager/config', {
  customWidth: 1200,
  customHeight: 800,
  dragSnapEnabled: true,
})

/// 配置变更时同步拖拽 snap 状态到 Rust 端。
/// immediate: true —— 启动时用已持久化配置首次同步，否则 drag monitor 不启动（用户须手动关开一次才生效）。
watch(
  config,
  (val) => {
    invoke(CMD.toggleDragSnap, {
      enabled: val.dragSnapEnabled,
      customWidth: val.customWidth,
      customHeight: val.customHeight,
    }).catch((e: unknown) => {
      // 不静默吞错：失败时记录便于排查（UI 与 Rust 状态可能短暂不一致）
      console.error('[window-manager] toggleDragSnap failed:', e)
    })
  },
  { deep: true, immediate: true },
)
