<template>
  <BaseDialog
    :title="t('settings.checkUpdate')"
    :ok-label="okLabel"
    :cancel-label="t('updateDialog.later')"
    :close-on-confirm="false"
    size="sm"
    @confirm="onConfirm"
    @cancel="emit('close')"
  >
    <div flex="~ col" gap="3">
      <div text="xs secondary" flex gap="2" items="center">
        <span>{{ t('updateDialog.newVersionFound') }}</span>
        <span font="mono">v{{ updateStore.info?.currentVersion }}</span>
        <span class="i-ri-arrow-right-line" text="muted"></span>
        <span text="accent" font="medium mono">v{{ updateStore.info?.newVersion }}</span>
      </div>

      <p v-if="updateStore.info?.body" text="xs secondary" leading="relaxed" line-clamp="4">
        {{ updateStore.info.body }}
      </p>

      <!-- 下载进度：下载中显示百分比，下载完成后保留满进度条 -->
      <div v-if="updateStore.downloading || updateStore.downloaded" flex="~ col" gap="1.5">
        <div class="progress-track">
          <div class="progress-fill" :style="{ width: `${pct}%` }"></div>
        </div>
        <div v-if="pct > 0" text="xs muted text-right" font="mono">{{ pct }}%</div>
      </div>

      <p v-if="updateStore.error" text="xs danger">{{ updateStore.error }}</p>
    </div>
  </BaseDialog>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { relaunch } from '@tauri-apps/plugin-process'
import BaseDialog from './BaseDialog.vue'
import { t } from '@/runtime/i18n'
import { useUpdateStore } from '@/stores/update'

const emit = defineEmits<{ close: [] }>()

const updateStore = useUpdateStore()
const installing = ref(false)

const pct = computed(() => Math.round((updateStore.progress ?? 0) * 100))

const okLabel = computed(() => {
  if (updateStore.downloading) return t('settings.downloading')
  if (updateStore.downloaded)
    return installing.value ? t('updateDialog.installing') : t('updateDialog.installNow')
  return t('settings.downloadAndInstall')
})

async function onConfirm() {
  if (updateStore.downloaded) {
    installing.value = true
    try {
      await updateStore.install()
      await relaunch()
    } catch {
      installing.value = false
    }
  } else if (!updateStore.downloading) {
    await updateStore.download()
  }
}
</script>

<style scoped>
.progress-track {
  height: 6px;
  background: var(--color-fill-18);
  border-radius: 9999px;
  overflow: hidden;
}
.progress-fill {
  height: 100%;
  background: var(--color-accent);
  border-radius: 9999px;
  transition: width var(--duration-fast) var(--ease-out);
}
</style>
