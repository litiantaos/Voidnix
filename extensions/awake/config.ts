import { watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { defineConfig } from '@/runtime/storage'

export type AwakeDisplayMode = 'mirror' | 'extend'

/// awake 扩展自管配置（持久化至 extensions/awake/config.json）。
export const config = defineConfig('extensions/awake/config', {
  displayMode: 'mirror' as AwakeDisplayMode,
})

/// Rust 状态同步：displayMode 走 watch(immediate: true) 推送到 Rust。
/// immediate 关键：启动期磁盘值回填触发 watch，无需进入 View 即同步。
watch(
  () => config.displayMode,
  (mode) => {
    invoke(CMD.setAwakeDisplayMode, { mode }).catch((e: unknown) => {
      console.error('[awake] setAwakeDisplayMode failed:', e)
    })
  },
  { immediate: true },
)
