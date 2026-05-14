<script setup lang="ts">
import { onMounted, onUnmounted, watch } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import MainView from '@/components/layout/MainView.vue'
import BaseDialog from '@/components/ui/BaseDialog.vue'
import { useSettingsStore } from '@/stores/settings'
import { useAppStore } from '@/stores/app'
import { useUpdateStore } from '@/stores/update'
import { pendingText } from '@/modules/translate'
import { isTauri } from '@/utils/tauri'

let win: ReturnType<typeof getCurrentWindow> | null = null
if (isTauri) {
  win = getCurrentWindow()
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
  try {
    await settings.loadSettings()
  } catch (e) {
    console.error('Settings load error:', e)
  }

  // 启动后延迟检查更新，有新版本则后台静默下载
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

    watch(
      () => settings.globalShortcut,
      async (newVal, oldVal) => {
        await setupGlobalShortcut('main', newVal, oldVal)
      },
    )

    watch(
      () => settings.clipboardShortcut,
      async (newVal, oldVal) => {
        await setupGlobalShortcut('clipboard', newVal, oldVal)
      },
    )

    watch(
      () => settings.translateShortcut,
      async (newVal, oldVal) => {
        await setupGlobalShortcut('translate', newVal, oldVal)
      },
    )

    watch(
      () => settings.chatShortcut,
      async (newVal, oldVal) => {
        await setupGlobalShortcut('chat', newVal, oldVal)
      },
    )

    let lastTranslateShortcutTime = 0

    unlistenShortcut = await listen<{ id: string; wasVisible: boolean }>(
      'shortcut-pressed',
      async (event) => {
        markSkip()
        const shortcutId = event.payload.id
        // 以 Rust 端按下瞬间的窗口状态为准，避免与后端 WINDOW_VISIBLE 不同步
        // （比如 useSearchCommand 直接 invoke hide_window 时前端没有同步状态）。
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
          console.log('[translate] Shortcut pressed, wasVisible:', wasVisible)
          lastTranslateShortcutTime = now
          if (wasVisible && appStore.activeModuleId === 'translate') {
            console.log('[translate] Window already visible with translate module, hiding')
            invoke('hide_window').catch(() => {})
            return
          }
          console.log('[translate] Setting active module to translate')
          appStore.setActiveModule('translate')
          appStore.setSearchQuery('')
          if (wasVisible) {
            // Window already visible — Voidnix is frontmost, no selected text
            // in another app to extract. Just switch module, don't wait for text.
            console.log('[translate] Window visible, skipping text extraction')
          } else {
            // Window was hidden — another app was frontmost with possibly
            // selected text. Wait for the backend to extract it.
            try {
              const text = await waitForSelectedText()
              console.log('[translate] Got text:', text)
              const trimmedText = text.trim()
              pendingText.value = trimmedText
            } catch (e) {
              console.error('[translate] Error:', e)
              pendingText.value = ''
            }
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

    // Mark skip before the window shows, so the blur handler won't immediately hide it.
    // This is emitted by the backend right before window.show().
    unlistenShowingWindow = await listen('showing-window', () => {
      markSkip()
    })

    // Native click-outside monitor (NSEvent global monitor) detects clicks
    // outside the window regardless of focus state. Works on first run with
    // macOS Accessory policy where set_focus() may not work.
    unlistenClickOutside = await listen('click-outside', () => {
      console.log('[hide] click-outside triggered')
      invoke('hide_window').catch(() => {})
    })

    // Persistent listener: backend emits this after extracting selected text via
    // Accessibility API / AppleScript. Registered once at mount to avoid race conditions.
    unlistenTranslateReady = await listen<string>('translate-text-ready', (e) => {
      console.log('[translate] Received translate-text-ready event:', e.payload)
      if (translateReadyResolver) {
        console.log('[translate] Resolving pending promise with text:', e.payload)
        translateReadyResolver(e.payload || '')
        translateReadyResolver = null
      } else {
        console.log('[translate] No pending resolver, event ignored')
      }
    })

    unlistenFocus = await win!.onFocusChanged(({ payload: focused }: { payload: boolean }) => {
      if (focused) {
        window.dispatchEvent(new CustomEvent('window-focused'))
      } else if (
        Date.now() - lastShortcutTime > 200 &&
        Date.now() - appStore.lastDialogCloseTime > 300 &&
        !appStore.isDialogOpen
      ) {
        invoke<boolean>('is_app_active').then((active) => {
          console.log('[hide] onFocusChanged blur, is_app_active=', active)
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
  document.removeEventListener('keydown', onLocalShortcut)
  if (isTauri) {
    if (unlistenFocus) unlistenFocus()
    if (unlistenShortcut) unlistenShortcut()
    if (unlistenOpenModule) unlistenOpenModule()
    if (unlistenShowingWindow) unlistenShowingWindow()
    if (unlistenTranslateReady) unlistenTranslateReady()
    if (unlistenClickOutside) unlistenClickOutside()

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
  }
})
</script>

<template>
  <MainView />

  <!-- 全局确认弹窗（Store 驱动，Promise 式调用） -->
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
