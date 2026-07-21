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
    :ok-label="appStore.dialogOptions.okLabel"
    :cancel-label="appStore.dialogOptions.cancelLabel"
    :show-cancel="appStore.dialogOptions.showCancel"
    @confirm="appStore.resolveConfirm(true)"
    @cancel="appStore.resolveConfirm(false)"
  />
  <ToastOverlay />
</template>

<script setup lang="ts">
import { shallowRef, onErrorCaptured, type Component } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import MainView from '@/components/layout/MainView.vue'
import BaseDialog from '@/components/ui/BaseDialog.vue'
import ToastOverlay from '@/components/ui/ToastOverlay.vue'
import { useAppLifecycle } from '@/composables/useAppLifecycle'
import { useAppStore } from '@/stores/app'
import { isTauri } from '@/utils/tauri'
import { getAllExtensions } from '@/runtime/extension-registry'

const appStore = useAppStore()

let win: ReturnType<typeof getCurrentWindow> | null = null
if (isTauri) {
  win = getCurrentWindow()
}

const activeWindowView = shallowRef<Component | null>(null)

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

onErrorCaptured((err) => {
  console.error('[Voidnix] Uncaught component error:', err)
  return false
})

useAppLifecycle(activeWindowView, win)
</script>
