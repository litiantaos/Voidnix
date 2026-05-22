<template>
  <component :is="activeWindowView" v-if="activeWindowView" />

  <!-- 主窗口：正常启动器 -->
  <template v-else>
    <MainView />
  </template>

  <BaseDialog
    v-if="appStore.isDialogOpen && appStore.dialogOptions"
    :title="appStore.dialogOptions.title"
    :message="appStore.dialogOptions.message"
    :size="appStore.dialogOptions.size"
    :kind="appStore.dialogOptions.kind"
    :ok-label="appStore.dialogOptions.okLabel"
    :cancel-label="appStore.dialogOptions.cancelLabel"
    :show-cancel="appStore.dialogOptions.showCancel"
    @confirm="appStore.resolveConfirm(true)"
    @cancel="appStore.resolveConfirm(false)"
  />
</template>

<script setup lang="ts">
import { onMounted, onUnmounted, watch, shallowRef, type Component } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import MainView from '@/components/layout/MainView.vue'
import BaseDialog from '@/components/ui/BaseDialog.vue'
import { useSettingsStore } from '@/stores/settings'
import { useAppStore } from '@/stores/app'
import { useUpdateStore } from '@/stores/update'
import { isTauri } from '@/utils/tauri'
import { getAllModules, getModule } from '@/core/module-registry'

let win: ReturnType<typeof getCurrentWindow> | null = null
if (isTauri) {
  win = getCurrentWindow()
}

const activeWindowView = shallowRef<Component | null>(null)

let allGlobalShortcuts: { id: string; default?: string; onExecute: (wasVisible: boolean) => void }[] = []
if (win?.label) {
  for (const mod of getAllModules()) {
    if (mod.windowViews) {
      for (const [prefix, viewComp] of Object.entries(mod.windowViews)) {
        if (win.label.startsWith(prefix)) {
          activeWindowView.value = viewComp
          break
        }
      }
    }
    if (activeWindowView.value) break
  }
}

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
    await win.hide()
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

let unlistenFocus: (() => void) | null = null
let unlistenShortcut: (() => void) | null = null
let unlistenOpenModule: (() => void) | null = null
let unlistenShowingWindow: (() => void) | null = null
let unlistenClickOutside: (() => void) | null = null
// webkit_tuning 驯化事件监听（Req 1.6, 2.7）
let unlistenPreShow: (() => void) | null = null
let unlistenAwaitingPaint: (() => void) | null = null
let unlistenPainted: (() => void) | null = null

async function setupGlobalShortcut(
  id: string,
  newShortcut: string,
  oldShortcut?: string,
) {
  if (!isTauri) return

  try {
    await invoke('register_global_shortcut', {
      id: id,
      newShortcut: newShortcut,
      oldShortcut: oldShortcut || null,
    })
    appStore.clearShortcutError(id)
  } catch (e) {
    const msg = String(e)
    appStore.setShortcutError(id, msg)
  }
}

onMounted(async () => {
  if (activeWindowView.value) {
    // 如果渲染的是独立窗口视图，不再执行主窗口的初始化逻辑（如拉取设置、检查更新、注册快捷键等）
    return
  }

  try {
    await settings.loadSettings()
  } catch (e) {
    console.error('Settings load error:', e)
  }

  if (isTauri) {
    setTimeout(async () => {
      const hasUpdate = await updateStore.check()
      if (hasUpdate) {
        await updateStore.download()
      }
    }, 3000)
  }

  if (isTauri) {
    await setupGlobalShortcut('main', settings.globalShortcut)

    allGlobalShortcuts = getAllModules()
      .flatMap((m) => (m.globalShortcuts || []))
      .filter((s) => s.id !== 'main')

    for (const sc of allGlobalShortcuts) {
      await setupGlobalShortcut(sc.id, effectiveShortcut(sc.id, sc.default))
    }

    watch(
      () => settings.globalShortcut,
      async (newVal, oldVal) => {
        await setupGlobalShortcut('main', newVal, oldVal)
      },
    )

    watch(
      () => settings.shortcutOverrides,
      async (_newVal, oldVal) => {
        for (const sc of allGlobalShortcuts) {
          const newS = effectiveShortcut(sc.id, sc.default)
          const oldS = oldVal?.[sc.id] || sc.default || ''
          if (newS !== oldS) {
            await setupGlobalShortcut(sc.id, newS, oldS)
          }
        }
      },
      { deep: true },
    )

    unlistenShortcut = await listen<{ id: string; wasVisible: boolean }>(
      'shortcut-pressed',
      async (event) => {
        if (appStore.shortcutRecording) return
        markSkip()
        const shortcutId = event.payload.id
        const wasVisible = event.payload.wasVisible

        if (shortcutId === 'main') {
          if (wasVisible) {
            invoke('hide_window').catch(() => {})
            return
          }
        } else {
          // Dynamic shortcut resolution via modules
          for (const mod of getAllModules()) {
            if (mod.globalShortcuts) {
              const sc = mod.globalShortcuts.find((s) => s.id === shortcutId)
              if (sc) {
                sc.onExecute(wasVisible)
                return
              }
            }
          }
        }
      },
    )

    unlistenOpenModule = await listen<string>('open-module', (event) => {
      markSkip()
      const moduleId = event.payload
      if (moduleId) {
        appStore.setActiveModule(moduleId)
        appStore.setSearchQuery('')
      }
    })

    unlistenShowingWindow = await listen('showing-window', () => {
      markSkip()
    })

    unlistenClickOutside = await listen('click-outside', () => {
      invoke('hide_window').catch(() => {})
    })

    // 通用模块面板事件：任何模块都可以通过 Rust `open_module_panel` 触发
    await listen<{ moduleId: string; payload: unknown }>(
      'open-module-panel',
      (e) => {
        markSkip()
        const { moduleId, payload } = e.payload
        appStore.setActiveModule(moduleId)
        appStore.setSearchQuery('')
        appStore.showPanel = true
        const mod = getModule(moduleId)
        if (mod?.onOpenPanel) {
          mod.onOpenPanel(payload)
        }
      },
    )

    // webkit_tuning 驯化事件（Req 1.6, 2.7）
    // pre-show：触发 rAF 让 WebKit 渲染管线就绪，严格先于 alpha=1
    unlistenPreShow = await listen('webkit-tuning:pre-show', () => {
      requestAnimationFrame(() => {
        /* 触发同步 layout，避免首帧白底 */
      })
    })
    // awaiting-paint：80ms 超时 fallback，显示骨架占位
    unlistenAwaitingPaint = await listen('webkit-tuning:awaiting-paint', () => {
      appStore.showPaintSkeleton = true
    })
    // painted：首帧呈现完成，撤掉骨架
    unlistenPainted = await listen('webkit-tuning:painted', () => {
      appStore.showPaintSkeleton = false
    })

    unlistenFocus = await win!.onFocusChanged(
      ({ payload: focused }: { payload: boolean }) => {
        if (focused) {
          window.dispatchEvent(new CustomEvent('window-focused'))
        } else if (
          Date.now() - lastShortcutTime > 200 &&
          Date.now() - appStore.lastDialogCloseTime > 300 &&
          !appStore.isDialogOpen &&
          !appStore.suppressBlur
        ) {
          invoke<boolean>('is_app_active')
            .then((active) => {
              if (active) return
              invoke('hide_window').catch(() => {})
            })
            .catch(() => {
              invoke('hide_window').catch(() => {})
            })
        }
      },
    )
  }

  document.addEventListener('keydown', onLocalShortcut)
})

onUnmounted(async () => {
  if (activeWindowView.value) {
    return
  }

  document.removeEventListener('keydown', onLocalShortcut)
  if (isTauri) {
    if (unlistenFocus) unlistenFocus()
    if (unlistenShortcut) unlistenShortcut()
    if (unlistenOpenModule) unlistenOpenModule()
    if (unlistenShowingWindow) unlistenShowingWindow()
    if (unlistenClickOutside) unlistenClickOutside()
    if (unlistenPreShow) unlistenPreShow()
    if (unlistenAwaitingPaint) unlistenAwaitingPaint()
    if (unlistenPainted) unlistenPainted()

    await invoke('register_global_shortcut', {
      id: 'main',
      newShortcut: '',
      oldShortcut: settings.globalShortcut,
    }).catch(() => {})

    for (const sc of allGlobalShortcuts) {
      const effective = effectiveShortcut(sc.id, sc.default)
      if (effective) {
        await invoke('register_global_shortcut', {
          id: sc.id,
          newShortcut: '',
          oldShortcut: effective,
        }).catch(() => {})
      }
    }
  }
})
</script>
