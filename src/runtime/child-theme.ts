/// 子窗口主题初始化（无 Pinia 依赖）。
///
/// main 窗口走 runtime/theme.ts（settings store 驱动）；
/// screenshot / snap-panel / pin 等独立入口窗口用此轻量版本：
/// 读 main 缓存的 appearance → 写 DOM data-theme → 监听 appearance-changed 事件。
///
/// Rust 侧已通过 apply_cached_appearance 设 NSWindow appearance（驱动 prefers-color-scheme），
/// 但 theme.css 用 [data-theme] 选择器，故仍需 DOM 层同步。

import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { CMD } from '@/commands'

type Appearance = 'auto' | 'light' | 'dark'

const mql = typeof window !== 'undefined' ? window.matchMedia('(prefers-color-scheme: dark)') : null

function resolveDark(mode: Appearance): boolean {
  if (mode === 'auto') return mql?.matches ?? false
  return mode === 'dark'
}

function applyDomTheme(mode: Appearance) {
  document.documentElement.dataset.theme = resolveDark(mode) ? 'dark' : 'light'
}

export function initChildTheme() {
  // 先按系统外观设默认值，再读 main 缓存的强制值纠正
  applyDomTheme('auto')

  invoke<Appearance | null>(CMD.getCachedAppearance)
    .then((mode) => {
      if (mode) applyDomTheme(mode)
    })
    .catch(() => {})

  listen<Appearance>('appearance-changed', (event) => {
    applyDomTheme(event.payload)
  }).catch(() => {})

  // auto 模式下响应系统外观切换
  mql?.addEventListener('change', () => {
    invoke<Appearance | null>(CMD.getCachedAppearance)
      .then((mode) => {
        if (!mode || mode === 'auto') applyDomTheme('auto')
      })
      .catch(() => {})
  })
}
