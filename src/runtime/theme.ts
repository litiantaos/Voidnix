import { watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { listen } from '@tauri-apps/api/event'
import { CMD } from '@/commands'
import { isTauri } from '@/utils/tauri'
import { useSettingsStore, type Appearance } from '@/stores/settings'

/// 主题运行时：读 settings.appearance + 监听系统外观 → 计算 resolved → 应用。
///
/// - DOM：写 <html data-theme="dark|light">，供 theme.css `[data-theme]` 覆盖
/// - 原生：调 set_window_appearance 设 NSWindow appearance（仅 main 驱动，全局副作用）
///
/// 关键：NSWindow appearance 同步驱动 WKWebView 的 prefers-color-scheme，故
/// auto 模式传 mode='auto' 让 Rust 设 appearance=None（跟随系统），此时
/// matchMedia 仍反映系统真实状态，前端据此设 DOM；light/dark 强制覆盖。
///
/// 跨窗口三类路径：
/// - main：settings.appearance（reactive + watch + 持久化），驱动全局 set_window_appearance
/// - screenshot/snap-panel：Rust 设原生 appearance（invisible 创建，apply_cached_appearance 安全），
///   但 WKWebView 在 invisible 状态下不派发 matchMedia change，无法实时响应主题切换；
///   改监听 appearance-changed 事件 + 读 get_cached_appearance 更新 DOM
/// - pin：visible 创建不可设 setAppearance（刚 build 的 WKWebView 上触发 prefers-color-scheme
///   重算死锁主线程），同样靠事件 + get_cached_appearance 驱动 DOM

let initialized = false

const mql = typeof window !== 'undefined' ? window.matchMedia('(prefers-color-scheme: dark)') : null

function resolveDark(mode: Appearance): boolean {
  if (mode === 'auto') return mql?.matches ?? false
  return mode === 'dark'
}

function applyDomTheme(mode: Appearance) {
  const dark = resolveDark(mode)
  document.documentElement.dataset.theme = dark ? 'dark' : 'light'
}

function applyTheme(mode: Appearance) {
  applyDomTheme(mode)
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

  if (!isTauri || getCurrentWindow().label === 'main') {
    // main：settings.appearance 驱动，watch 响应设置页实时切换
    applyTheme(settings.appearance)
    watch(() => settings.appearance, applyTheme)
    // auto 模式下响应系统外观切换；手动模式忽略（已被强制覆盖）。
    mql?.addEventListener('change', () => {
      if (settings.appearance === 'auto') applyTheme('auto')
    })
    return
  }

  // 子窗口（screenshot/snap-panel/pin）：settings.appearance 恒 'auto' 不可用。
  // 读 get_cached_appearance 获取 main 的强制值；监听 appearance-changed 事件实时响应切换。
  applyDomTheme('auto')
  invoke<Appearance | null>(CMD.getCachedAppearance)
    .then((mode) => {
      if (mode) applyDomTheme(mode)
    })
    .catch(() => {})

  listen<Appearance>('appearance-changed', (event) => {
    applyDomTheme(event.payload)
  }).catch(() => {})

  // auto 模式下响应系统外观切换（Rust 设原生 appearance=None 时 matchMedia 随系统）。
  // screenshot/snap-panel invisible 时 WKWebView 不派发 matchMedia change，
  // 但窗口可见时会补一次；pin 无原生 appearance 故 auto 时 matchMedia 始终反映系统。
  mql?.addEventListener('change', () => {
    invoke<Appearance | null>(CMD.getCachedAppearance)
      .then((mode) => {
        if (!mode || mode === 'auto') applyDomTheme('auto')
      })
      .catch(() => {})
  })
}
