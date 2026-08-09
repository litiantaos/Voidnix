import { watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getAllWebviewWindows } from '@tauri-apps/api/webviewWindow'
import { CMD } from '@/commands'
import { defineConfig } from '@/runtime/storage'

/// window-manager 扩展自管配置（持久化至 extensions/window-manager/config.json）。
export const config = defineConfig('extensions/window-manager/config', {
  enabled: false,
  customWidth: 1200,
  customHeight: 800,
})

/// 自定义尺寸 floor/cap（权威在 native/mod.rs WIDTH_BOUNDS/HEIGHT_BOUNDS，
/// 须手动同步，check:wm-bounds CI 强制约束）。
export const BOUNDS = {
  customWidth: { floor: 200, cap: 4096 },
  customHeight: { floor: 200, cap: 4096 },
} as const

export function clampWidth(n: number): number {
  return Math.max(BOUNDS.customWidth.floor, Math.min(BOUNDS.customWidth.cap, n))
}

export function clampHeight(n: number): number {
  return Math.max(BOUNDS.customHeight.floor, Math.min(BOUNDS.customHeight.cap, n))
}

/// Rust 状态同步：所有 config 字段一律 watch(immediate: true) 推送到 Rust。
/// immediate 关键：启动期磁盘值回填触发 watch，避免「上次开启 → 重启丢失」回归。
/// View.vue 仅改 config 不显式 invoke（与 customWidth/Height 一致）。
watch(
  () => [config.customWidth, config.customHeight] as const,
  ([w, h]) => {
    invoke(CMD.setSnapSize, { width: clampWidth(w), height: clampHeight(h) }).catch(
      (e: unknown) => {
        console.error('[window-manager] setSnapSize failed:', e)
      },
    )
  },
  { immediate: true },
)

watch(
  () => config.enabled,
  (enabled, prevEnabled) => {
    invoke(CMD.setWindowManagerEnabled, { enabled }).catch((e: unknown) => {
      console.error('[window-manager] setWindowManagerEnabled failed:', e)
    })
    // 禁用时销毁 snap-panel 窗口释放 WebContent 进程（Rust 端仅停止 drag monitor，
    // 窗口销毁由前端发起——避免 Rust 端窗口操作在 LTO 布局变化下触发 WKWebView KVO 竞态）
    // 仅响应运行时显式 true→false 切换——defineConfig 异步回填磁盘值，启动期 immediate
    // 的默认 false 与回填的 true 之间，getAllWebviewWindows 异步回调可能晚于 create_snap_panel，
    // 误销毁刚创建的窗口（drag monitor 已 start 但 snap-panel 被销毁 → 拖窗吸附失效）
    if (!enabled && prevEnabled === true) {
      getAllWebviewWindows()
        .then((windows: ReadonlyArray<{ label: string; close(): Promise<void> }>) => {
          for (const w of windows) {
            if (w.label === 'snap-panel') {
              w.close().catch(() => {})
            }
          }
        })
        .catch(() => {})
    }
  },
  { immediate: true },
)
