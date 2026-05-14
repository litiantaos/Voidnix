<script setup lang="ts">
import { ref } from 'vue'
import { open } from '@tauri-apps/plugin-shell'
import { relaunch } from '@tauri-apps/plugin-process'
import BaseDialog from './BaseDialog.vue'
import { useUpdateStore } from '@/stores/update'
import { isTauri } from '@/utils/tauri'

const emit = defineEmits<{ (e: 'close'): void }>()

const updateStore = useUpdateStore()
const installing = ref(false)

async function openReleases() {
  if (isTauri) {
    await open('https://github.com/litiantaos/Voidnix/releases')
  }
}

async function onConfirm() {
  installing.value = true
  try {
    await updateStore.install()
    await relaunch()
  } catch {
    installing.value = false
  }
}
</script>

<template>
  <BaseDialog
    title="发现新版本"
    :ok-label="installing ? '安装中…' : '立即安装并重启'"
    cancel-label="稍后"
    size="sm"
    @confirm="onConfirm"
    @cancel="emit('close')"
  >
    <div class="flex flex-col gap-3">
      <div class="flex items-center gap-2 text-xs text-tx-subtle">
        <span class="text-tx-muted">当前版本</span>
        <span class="font-mono">v{{ updateStore.info?.currentVersion }}</span>
        <span class="i-ri-arrow-right-line text-tx-hint"></span>
        <span class="font-mono text-accent font-medium">v{{ updateStore.info?.newVersion }}</span>
      </div>

      <p v-if="updateStore.info?.body" class="text-xs text-tx-subtle leading-relaxed line-clamp-4">
        {{ updateStore.info.body }}
      </p>

      <button
        class="text-xs text-accent/80 hover:text-accent text-left transition-colors"
        @click="openReleases"
      >
        查看完整更新说明 →
      </button>
    </div>
  </BaseDialog>
</template>
