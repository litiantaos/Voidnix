import { watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { CMD } from '@/commands'
import { isTauri } from '@/utils/tauri'
import { useSettingsStore, type Appearance } from '@/stores/settings'

/// 主题运行时：读 settings.appearance + 监听系统外观 → 计算 resolved → 应用。
///
/// - DOM：写 <html data-theme="dark|light">，供 theme.css `[data-theme]` 覆盖
/// - 原生：调 set_window_appearance 设 NSWindow appearance
///
/// 关键：NSWindow appearance 同步驱动 WKWebView 的 prefers-color-scheme，故
/// auto 模式传 mode='auto' 让 Rust 设 appearance=None（跟随系统），此时
/// matchMedia 仍反映系统真实状态，前端据此设 DOM；light/dark 强制覆盖。
///
/// 跨窗口：set_window_appearance 对全部窗口统一生效（全局副作用），仅由 main
/// 驱动。子窗口（screenshot/pin/snap-panel）settings.appearance 因 storage 跳过
/// 跨窗口同步恒为默认 'auto'，若亦调此命令会用陈旧 'auto' 冲掉 main 的强制模式；
/// 其原生 appearance 由 Rust 在窗口创建时按缓存值 apply_cached_appearance 应用，
/// 故子窗口 matchMedia 仍随强制模式，DOM 据此设 data-theme 与 main 一致。

let initialized = false

const mql = typeof window !== 'undefined' ? window.matchMedia('(prefers-color-scheme: dark)') : null

function resolveDark(mode: Appearance): boolean {
  if (mode === 'auto') return mql?.matches ?? false
  return mode === 'dark'
}

function applyTheme(mode: Appearance) {
  const dark = resolveDark(mode)
  document.documentElement.dataset.theme = dark ? 'dark' : 'light'
  // 全局副作用命令仅 main 驱动：子窗口调会用陈旧 'auto' 冲掉 main 的强制模式
  if (isTauri && getCurrentWindow().label === 'main') {
    invoke<void>(CMD.setWindowAppearance, { mode }).catch(() => {})
  }
}

/// 初始化主题：main.ts pinia 就绪后调用一次。
/// 首次读 default 'auto'，磁盘值 backfill 后 watch 自动纠正（窗口默认隐藏，无可见闪烁）。
export function initTheme() {
  if (initialized) return
  initialized = true

  const settings = useSettingsStore()
  applyTheme(settings.appearance)
  watch(() => settings.appearance, applyTheme)

  // auto 模式下响应系统外观切换；手动模式忽略（已被强制覆盖）。
  // 应用生命周期常驻监听，无需卸载（与 main.ts contextmenu 同范式）。
  mql?.addEventListener('change', () => {
    if (settings.appearance === 'auto') applyTheme('auto')
  })
}
