import { watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { isTauri } from '@/utils/tauri'
import { defineConfig } from '@/runtime/storage'

/// system-status 扩展自管配置（持久化至 extensions/system-status/config.json）。
/// menubarVisible：菜单栏下拉状态行常驻显示。
export const config = defineConfig('extensions/system-status/config', {
  menubarVisible: true,
})

/// menubarVisible 同步（Config 字段型：View 仅改 config，Rust 侧段显隐）。
/// index.ts 顶部 import './config' 激活（与 locales 同 side-effect 范式），
/// 启动期磁盘值回填即经 watch immediate 同步，不依赖 View 挂载。
watch(
  () => config.menubarVisible,
  (visible) => {
    if (!isTauri) return
    invoke(CMD.setSystemStatusMenubarVisible, { visible }).catch((e: unknown) => {
      console.error('[system-status] setSystemStatusMenubarVisible failed:', e)
    })
  },
  { immediate: true },
)
