import {
  onMounted,
  onUnmounted,
  watch,
  type Component,
  type ShallowRef,
  type WatchStopHandle,
} from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { listen } from '@tauri-apps/api/event'
import { useSettingsStore } from '@/stores/settings'
import { useAppStore } from '@/stores/app'
import { useUpdateStore } from '@/stores/update'
import { isTauri, hideWindow } from '@/utils/tauri'
import { getAllExtensions, getExtension } from '@/runtime/extension-registry'

type Win = ReturnType<typeof import('@tauri-apps/api/window').getCurrentWindow> | null

/// 主窗口生命周期：全局快捷键注册/注销、窗口显隐、失焦防抖隐藏、模块/子视图事件监听。
/// 独立窗口视图（screenshot/snap-panel）由 activeWindowView 标识，命中时跳过主窗口生命周期。
export function useAppLifecycle(activeWindowView: ShallowRef<Component | null>, win: Win) {
  const settings = useSettingsStore()
  const appStore = useAppStore()
  const updateStore = useUpdateStore()

  function effectiveShortcut(id: string, fallback?: string): string {
    return settings.getShortcutOverride(id) || fallback || ''
  }

  let lastShortcutTime = 0
  function markSkip() {
    lastShortcutTime = Date.now()
  }

  async function toggleWindow() {
    if (!win) return
    const visible = await win.isVisible()
    if (visible) {
      hideWindow()
    } else {
      await win.show()
      await win.setFocus()
    }
  }

  function onLocalShortcut(e: KeyboardEvent) {
    if (appStore.shortcutRecording) return
    if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.code === 'Space') {
      e.preventDefault()
      markSkip()
      toggleWindow()
    }
  }

  // 统一管理事件订阅的清理函数，避免新增监听时漏接 unlisten（曾导致 open-module-subview 泄漏）
  const unlistenList: Array<() => void> = []
  const track = (fn: () => void): void => {
    unlistenList.push(fn)
  }
  // M-fe1：watch stop handle + update 检查 timer 一并纳入清理，避免 HMR/测试场景累积
  const watchStops: WatchStopHandle[] = []
  let updateTimer: ReturnType<typeof setTimeout> | null = null
  let allGlobalShortcuts: {
    id: string
    default?: string
    onExecute: (wasVisible: boolean) => void
  }[] = []

  async function setupGlobalShortcut(id: string, shortcut: string) {
    if (!isTauri) return
    try {
      await invoke(CMD.registerGlobalShortcut, { id, shortcut })
      appStore.clearShortcutError(id)
    } catch (e) {
      const msg = String(e)
      appStore.setShortcutError(id, msg)
    }
  }

  onMounted(async () => {
    // 独立窗口视图：跳过主窗口初始化（设置/更新/快捷键）
    if (activeWindowView.value) return

    try {
      await settings.loadSettings()
    } catch (e) {
      console.error('Settings load error:', e)
    }

    if (isTauri) {
      updateTimer = setTimeout(async () => {
        try {
          const hasUpdate = await updateStore.check()
          if (hasUpdate) await updateStore.download()
        } catch (e) {
          console.error('Update check failed:', e)
        }
      }, 3000)
    }

    if (isTauri) {
      await setupGlobalShortcut('main', settings.globalShortcut)

      allGlobalShortcuts = getAllExtensions()
        .flatMap((e) => e.globalShortcuts || [])
        .filter((s) => s.id !== 'main')

      for (const sc of allGlobalShortcuts) {
        await setupGlobalShortcut(sc.id, effectiveShortcut(sc.id, sc.default))
      }

      watchStops.push(
        watch(
          () => settings.globalShortcut,
          async (newVal) => {
            await setupGlobalShortcut('main', newVal)
          },
        ),
      )

      watchStops.push(
        watch(
          () => settings.shortcutOverrides,
          async () => {
            for (const sc of allGlobalShortcuts) {
              await setupGlobalShortcut(sc.id, effectiveShortcut(sc.id, sc.default))
            }
          },
          { deep: true },
        ),
      )

      const unlistenShortcut = await listen<{ id: string; wasVisible: boolean }>(
        'shortcut-pressed',
        async (event) => {
          if (appStore.shortcutRecording) return
          markSkip()
          const shortcutId = event.payload.id
          const wasVisible = event.payload.wasVisible

          if (shortcutId === 'main') {
            if (wasVisible) {
              hideWindow()
              return
            }
          } else {
            // 扩展快捷键动态分发
            for (const ext of getAllExtensions()) {
              if (ext.globalShortcuts) {
                const sc = ext.globalShortcuts.find((s) => s.id === shortcutId)
                if (sc) {
                  sc.onExecute(wasVisible)
                  return
                }
              }
            }
          }
        },
      )
      track(unlistenShortcut)

      const unlistenOpenModule = await listen<string>('open-module', (event) => {
        markSkip()
        const moduleId = event.payload
        if (moduleId) {
          appStore.setActiveModule(moduleId)
          appStore.setSearchQuery('')
        }
      })
      track(unlistenOpenModule)

      const unlistenClickOutside = await listen('click-outside', () => {
        hideWindow(true)
      })
      track(unlistenClickOutside)

      // 通用模块子视图事件：任何模块都可以通过 Rust `open_module_subview` 触发
      const unlistenSubview = await listen<{
        moduleId: string
        subviewId: string
        payload: unknown
      }>('open-module-subview', (e) => {
        markSkip()
        const { moduleId, subviewId, payload } = e.payload
        appStore.setActiveModule(moduleId)
        appStore.setSearchQuery('')
        appStore.openSubview(subviewId)
        const ext = getExtension(moduleId)
        if (ext?.onOpenSubview) {
          ext.onOpenSubview(subviewId, payload)
        }
      })
      track(unlistenSubview)

      const unlistenFocus = await win!.onFocusChanged(
        ({ payload: focused }: { payload: boolean }) => {
          if (focused) {
            window.dispatchEvent(new CustomEvent('window-focused'))
          } else if (
            Date.now() - lastShortcutTime > 200 &&
            Date.now() - appStore.lastDialogCloseTime > 300 &&
            !appStore.isDialogOpen &&
            !appStore.suppressBlur
          ) {
            invoke<boolean>(CMD.isAppActive)
              .then((active) => {
                if (active) return
                hideWindow(true)
              })
              .catch(() => {
                hideWindow(true)
              })
          }
        },
      )
      track(unlistenFocus)
    }

    document.addEventListener('keydown', onLocalShortcut)
  })

  onUnmounted(() => {
    if (activeWindowView.value) return

    document.removeEventListener('keydown', onLocalShortcut)
    // M-fe1：清理 watch + timer，与 unlisten 一并回收
    watchStops.forEach((stop) => stop())
    watchStops.length = 0
    if (updateTimer) {
      clearTimeout(updateTimer)
      updateTimer = null
    }
    if (isTauri) {
      unlistenList.forEach((fn) => {
        try {
          fn()
        } catch (e) {
          console.error('unlisten failed:', e)
        }
      })
      unlistenList.length = 0

      // 同步触发注销（fire-and-forget）：Tauri app 退出前会等待当前 task tick
      void invoke(CMD.registerGlobalShortcut, {
        id: 'main',
        shortcut: '',
      }).catch(() => {})

      for (const sc of allGlobalShortcuts) {
        void invoke(CMD.registerGlobalShortcut, {
          id: sc.id,
          shortcut: '',
        }).catch(() => {})
      }
    }
  })
}
