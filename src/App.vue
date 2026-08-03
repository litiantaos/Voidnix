<template>
  <!-- 主窗口 -->
  <MainView />

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
import { onErrorCaptured } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import MainView from '@/components/layout/MainView.vue'
import BaseDialog from '@/components/ui/BaseDialog.vue'
import ToastOverlay from '@/components/ui/ToastOverlay.vue'
import { useAppLifecycle } from '@/composables/useAppLifecycle'
import { useAppStore } from '@/stores/app'
import { isTauri } from '@/utils/tauri'

const appStore = useAppStore()

let win: ReturnType<typeof getCurrentWindow> | null = null
if (isTauri) {
  win = getCurrentWindow()
}

onErrorCaptured((err) => {
  console.error('[Voidnix] Uncaught component error:', err)
  return false
})

useAppLifecycle(win)
</script>
