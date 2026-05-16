<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import MainView from '@/components/layout/MainView.vue'
import BaseDialog from '@/components/ui/BaseDialog.vue'
import ScreenshotOverlay from '@/modules/screenshot/Overlay.vue'
import { useSettingsStore } from '@/stores/settings'
import { useAppStore } from '@/stores/app'
import { useUpdateStore } from '@/stores/update'
import { pendingText } from '@/modules/translate'
import { isTauri } from '@/utils/tauri'

interface WindowRect { x: number; y: number; w: number; h: number; owner: string }
interface ScreenshotData { data_url: string; width: number; height: number; scale: number; mouse_x: number; mouse_y: number; windows: WindowRect[] }

let win: ReturnType<typeof getCurrentWindow> | null = null
if (isTauri) {
  win = getCurrentWindow()
}
const isScreenshot = win?.label === 'screenshot'

const showScreenshot = ref(false)
const screenshotData = ref<ScreenshotData | null>(null)

async function onScreenshotClose() {
  await invoke('exit_screenshot_mode').catch(() => {})
  showScreenshot.value = false
  screenshotData.value = null
}

const settings = useSettingsStore()
const appStore = useAppStore()
const updateStore = useUpdateStore()

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
let unlistenTranslateReady: (() => void) | null = null
let unlistenClickOutside: (() => void) | null = null
let unlistenScreenshotReady: (() => void) | null = null
// webkit_tuning 驯化事件监听（Req 1.6, 2.7）
let unlistenPreShow: (() => void) | null = null
let unlistenAwaitingPaint: (() => void) | null = null
let unlistenPainted: (() => void) | null = null
let translateReadyResolver: ((text: string) => void) | null = null

async function waitForSelectedText(): Promise<string> {
  if (translateReadyResolver) {
    translateReadyResolver('')
    translateReadyResolver = null
  }

  return new Promise<string>((resolve) => {
    translateReadyResolver = resolve
    setTimeout(async () => {
      if (translateReadyResolver !== resolve) return
      translateReadyResolver = null
      try {
        const cached = await invoke<string>('get_selected_text_cached')
        if (cached.trim()) {
          resolve(cached)
          return
        }
        const fallback = await invoke<string>('get_selected_text')
        resolve(fallback || '')
      } catch {
        resolve('')
      }
    }, 1500)
  })
}

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
  } catch (e) {
    console.error('Failed to register shortcut', e)
  }
}

onMounted(async () => {
  if (isScreenshot) {
    window.addEventListener('__screenshot_ready', () => {
      const data = (window as unknown as { __screenshotData?: ScreenshotData }).__screenshotData
      if (!data) return
      // 连续触发时先 unmount 旧实例，确保 sel/phase/shapes 等状态重置
      showScreenshot.value = false
      screenshotData.value = null
      requestAnimationFrame(() => {
        screenshotData.value = data
        showScreenshot.value = true
      })
    })
    return
  }

  // ── 以下是主窗口逻辑 ──

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
    await setupGlobalShortcut('clipboard', settings.clipboardShortcut)
    await setupGlobalShortcut('translate', settings.translateShortcut)
    await setupGlobalShortcut('chat', settings.chatShortcut)
    await setupGlobalShortcut('screenshot', settings.screenshotShortcut)

    watch(() => settings.globalShortcut, async (newVal, oldVal) => {
      await setupGlobalShortcut('main', newVal, oldVal)
    })

    watch(() => settings.clipboardShortcut, async (newVal, oldVal) => {
      await setupGlobalShortcut('clipboard', newVal, oldVal)
    })

    watch(() => settings.translateShortcut, async (newVal, oldVal) => {
      await setupGlobalShortcut('translate', newVal, oldVal)
    })

    watch(() => settings.chatShortcut, async (newVal, oldVal) => {
      await setupGlobalShortcut('chat', newVal, oldVal)
    })

    watch(() => settings.screenshotShortcut, async (newVal, oldVal) => {
      await setupGlobalShortcut('screenshot', newVal, oldVal)
    })

    let lastTranslateShortcutTime = 0

    unlistenShortcut = await listen<{ id: string; wasVisible: boolean }>(
      'shortcut-pressed',
      async (event) => {
        markSkip()
        const shortcutId = event.payload.id
        const wasVisible = event.payload.wasVisible
        const now = Date.now()

        if (shortcutId === 'main') {
          if (wasVisible) {
            invoke('hide_window').catch(() => {})
            return
          }
        } else if (shortcutId === 'clipboard') {
          if (now - lastTranslateShortcutTime < 800) {
            return
          }
          if (wasVisible && appStore.activeModuleId === 'clipboard') {
            invoke('hide_window').catch(() => {})
            return
          }
          if (wasVisible) {
            appStore.setActiveModule('clipboard')
            appStore.setSearchQuery('')
            return
          }
          appStore.setActiveModule('clipboard')
          appStore.setSearchQuery('')
        } else if (shortcutId === 'translate') {
          lastTranslateShortcutTime = now
          if (wasVisible && appStore.activeModuleId === 'translate') {
            invoke('hide_window').catch(() => {})
            return
          }
          appStore.setActiveModule('translate')
          appStore.setSearchQuery('')
          if (wasVisible) {
            return
          }
          try {
            const text = await waitForSelectedText()
            pendingText.value = text.trim()
          } catch (e) {
            pendingText.value = ''
          }
        } else if (shortcutId === 'chat') {
          if (wasVisible && appStore.activeModuleId === 'chat') {
            invoke('hide_window').catch(() => {})
            return
          }
          if (wasVisible) {
            appStore.setActiveModule('chat')
            appStore.setSearchQuery('')
            return
          }
          appStore.setActiveModule('chat')
          appStore.setSearchQuery('')
        } else if (shortcutId === 'screenshot') {
          // screenshot 由 screenshot-ready 事件处理，此处忽略
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

    unlistenTranslateReady = await listen<string>('translate-text-ready', (e) => {
      if (translateReadyResolver) {
        translateReadyResolver(e.payload || '')
        translateReadyResolver = null
      }
    })

    unlistenScreenshotReady = await listen<ScreenshotData>('screenshot-ready', async (e) => {
      markSkip()
      await invoke('enter_screenshot_mode', { data: e.payload })
    })

    // webkit_tuning 驯化事件（Req 1.6, 2.7）
    // pre-show：触发 rAF 让 WebKit 渲染管线就绪，严格先于 alpha=1
    unlistenPreShow = await listen('webkit-tuning:pre-show', () => {
      requestAnimationFrame(() => { /* 触发同步 layout，避免首帧白底 */ })
    })
    // awaiting-paint：80ms 超时 fallback，显示骨架占位
    unlistenAwaitingPaint = await listen('webkit-tuning:awaiting-paint', () => {
      appStore.showPaintSkeleton = true
    })
    // painted：首帧呈现完成，撤掉骨架
    unlistenPainted = await listen('webkit-tuning:painted', () => {
      appStore.showPaintSkeleton = false
    })

    unlistenFocus = await win!.onFocusChanged(({ payload: focused }: { payload: boolean }) => {
      if (focused) {
        window.dispatchEvent(new CustomEvent('window-focused'))
      } else if (
        Date.now() - lastShortcutTime > 200 &&
        Date.now() - appStore.lastDialogCloseTime > 300 &&
        !appStore.isDialogOpen &&
        !appStore.suppressBlur
      ) {
        invoke<boolean>('is_app_active').then((active) => {
          if (active) return
          invoke('hide_window').catch(() => {})
        }).catch(() => {
          invoke('hide_window').catch(() => {})
        })
      }
    })
  }

  document.addEventListener('keydown', onLocalShortcut)
})

onUnmounted(async () => {
  if (isScreenshot) {
    return
  }

  document.removeEventListener('keydown', onLocalShortcut)
  if (isTauri) {
    if (unlistenFocus) unlistenFocus()
    if (unlistenShortcut) unlistenShortcut()
    if (unlistenOpenModule) unlistenOpenModule()
    if (unlistenShowingWindow) unlistenShowingWindow()
    if (unlistenTranslateReady) unlistenTranslateReady()
    if (unlistenClickOutside) unlistenClickOutside()
    if (unlistenScreenshotReady) unlistenScreenshotReady()
    if (unlistenPreShow) unlistenPreShow()
    if (unlistenAwaitingPaint) unlistenAwaitingPaint()
    if (unlistenPainted) unlistenPainted()

    await invoke('register_global_shortcut', {
      id: 'main',
      newShortcut: '',
      oldShortcut: settings.globalShortcut,
    }).catch(() => {})

    await invoke('register_global_shortcut', {
      id: 'clipboard',
      newShortcut: '',
      oldShortcut: settings.clipboardShortcut,
    }).catch(() => {})

    await invoke('register_global_shortcut', {
      id: 'translate',
      newShortcut: '',
      oldShortcut: settings.translateShortcut,
    }).catch(() => {})

    await invoke('register_global_shortcut', {
      id: 'chat',
      newShortcut: '',
      oldShortcut: settings.chatShortcut,
    }).catch(() => {})

    await invoke('register_global_shortcut', {
      id: 'screenshot',
      newShortcut: '',
      oldShortcut: settings.screenshotShortcut,
    }).catch(() => {})
  }
})
</script>

<template>
  <!-- 截图窗口：只渲染截图覆盖层 -->
  <template v-if="isScreenshot">
    <ScreenshotOverlay
      v-if="showScreenshot && screenshotData"
      :initial-screenshot="screenshotData"
      @close="onScreenshotClose"
    />
  </template>

  <!-- 主窗口：正常启动器 -->
  <template v-else>
    <MainView />
    <ScreenshotOverlay
      v-if="showScreenshot && screenshotData"
      :initial-screenshot="screenshotData"
      @close="onScreenshotClose"
    />
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
