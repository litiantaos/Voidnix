<template>
  <component :is="activeWindowView" v-if="activeWindowView" />

  <!-- 主窗口 -->
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
import { onMounted, onUnmounted, watch, shallowRef, onErrorCaptured, type Component } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { listen } from '@tauri-apps/api/event'
import MainView from '@/components/layout/MainView.vue'
import BaseDialog from '@/components/ui/BaseDialog.vue'
import { useSettingsStore } from '@/stores/settings'
import { useAppStore } from '@/stores/app'
import { useUpdateStore } from '@/stores/update'
import { isTauri, hideWindow } from '@/utils/tauri'
import { getAllExtensions, getExtension } from '@/runtime/extension-registry'

let win: ReturnType<typeof getCurrentWindow> | null = null
if (isTauri) {
  win = getCurrentWindow()
}

const activeWindowView = shallowRef<Component | null>(null)

let allGlobalShortcuts: {
  id: string
  default?: string
  onExecute: (wasVisible: boolean) => void
}[] = []
if (win?.label) {
  for (const ext of getAllExtensions()) {
    if (ext.windowViews) {
      for (const [prefix, viewFn] of Object.entries(ext.windowViews)) {
        if (win.label.startsWith(prefix)) {
          activeWindowView.value = viewFn()
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

onErrorCaptured((err) => {
  console.error('[Voidnix] Uncaught component error:', err)
  return false
})

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

let unlistenFocus: (() => void) | null = null
let unlistenShortcut: (() => void) | null = null
let unlistenOpenModule: (() => void) | null = null
let unlistenClickOutside: (() => void) | null = null

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

    allGlobalShortcuts = getAllExtensions()
      .flatMap((e) => e.globalShortcuts || [])
      .filter((s) => s.id !== 'main')

    for (const sc of allGlobalShortcuts) {
      await setupGlobalShortcut(sc.id, effectiveShortcut(sc.id, sc.default))
    }

    watch(
      () => settings.globalShortcut,
      async (newVal) => {
        await setupGlobalShortcut('main', newVal)
      },
    )

    watch(
      () => settings.shortcutOverrides,
      async () => {
        for (const sc of allGlobalShortcuts) {
          await setupGlobalShortcut(sc.id, effectiveShortcut(sc.id, sc.default))
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
            hideWindow()
            return
          }
        } else {
          // Dynamic shortcut resolution via extensions
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

    unlistenOpenModule = await listen<string>('open-module', (event) => {
      markSkip()
      const moduleId = event.payload
      if (moduleId) {
        appStore.setActiveModule(moduleId)
        appStore.setSearchQuery('')
      }
    })

    unlistenClickOutside = await listen('click-outside', () => {
      hideWindow(true)
    })

    // 通用模块子视图事件：任何模块都可以通过 Rust `open_module_subview` 触发
    await listen<{ moduleId: string; subviewId: string; payload: unknown }>(
      'open-module-subview',
      (e) => {
        markSkip()
        const { moduleId, subviewId, payload } = e.payload
        appStore.setActiveModule(moduleId)
        appStore.setSearchQuery('')
        appStore.openSubview(subviewId)
        const ext = getExtension(moduleId)
        if (ext?.onOpenSubview) {
          ext.onOpenSubview(subviewId, payload)
        }
      },
    )

    unlistenFocus = await win!.onFocusChanged(({ payload: focused }: { payload: boolean }) => {
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
    })
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
    if (unlistenClickOutside) unlistenClickOutside()

    await invoke(CMD.registerGlobalShortcut, {
      id: 'main',
      shortcut: '',
    }).catch(() => {})

    for (const sc of allGlobalShortcuts) {
      await invoke(CMD.registerGlobalShortcut, {
        id: sc.id,
        shortcut: '',
      }).catch(() => {})
    }
  }
})
</script>
