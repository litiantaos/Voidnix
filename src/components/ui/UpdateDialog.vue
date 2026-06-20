<template>
  <BaseDialog
    title="发现新版本"
    :ok-label="installing ? '安装中…' : '立即安装并重启'"
    cancel-label="稍后"
    size="sm"
    @confirm="onConfirm"
    @cancel="emit('close')"
  >
    <div flex="~ col" gap="3">
      <div text="xs tx-subtle" flex gap="2" items="center">
        <span text="tx-muted">当前版本</span>
        <span font="mono">v{{ updateStore.info?.currentVersion }}</span>
        <span class="i-ri-arrow-right-line" text="tx-hint"></span>
        <span text="accent" font="medium mono">v{{ updateStore.info?.newVersion }}</span>
      </div>

      <p v-if="updateStore.info?.body" text="xs tx-subtle" leading="relaxed" line-clamp="4">
        {{ updateStore.info.body }}
      </p>

      <BaseButton
        variant="ghost"
        icon="i-ri-external-link-line"
        class="!text-xs !text-accent/80 !justify-start hover:!text-accent"
        @click="openReleases"
      >
        查看完整更新说明
      </BaseButton>
    </div>
  </BaseDialog>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { open } from '@tauri-apps/plugin-shell'
import { relaunch } from '@tauri-apps/plugin-process'
import BaseDialog from './BaseDialog.vue'
import BaseButton from './BaseButton.vue'
import { useUpdateStore } from '@/stores/update'
import { isTauri } from '@/utils/tauri'

const emit = defineEmits<{ (e: 'close'): void }>()

const updateStore = useUpdateStore()
const installing = ref(false)

async function openReleases() {
  if (isTauri) {
    await open('https://github.com/litiantao/Voidnix/releases')
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
