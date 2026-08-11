<template>
  <div class="flex-col-full">
    <BaseSettingsList :items="items" />
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { CMD } from '@/commands'
import { useAppStore } from '@/stores/app'
import { t } from '@/runtime/i18n'
import BaseSettingsList from '@/components/ui/BaseSettingsList.vue'
import type { SettingItem } from '@/types/settings'

const isEnabled = ref(false)
const appStore = useAppStore()
let unlistenFn: UnlistenFn | undefined

const enable = async () => {
  if (isEnabled.value) return
  try {
    await invoke(CMD.setCleanModeEnabled, { enabled: true })
    isEnabled.value = true
  } catch (e) {
    appStore.showStatus(`${e ?? t('common.unknownError')}`, { duration: 6000, kind: 'error' })
  }
}

onMounted(async () => {
  try {
    isEnabled.value = await invoke<boolean>(CMD.isCleanModeEnabled)
  } catch (e) {
    console.error('[clean-mode] isCleanModeEnabled failed:', e)
  }
  unlistenFn = await listen('clean-mode-exit', () => {
    isEnabled.value = false
  })
})

onBeforeUnmount(() => {
  unlistenFn?.()
})

const items = computed<SettingItem[]>(() => [
  {
    id: 'enable',
    title: t('cleanMode.title'),
    subtitle: t('cleanMode.subtitle'),
    group: t('common.group.general'),
    type: 'button',
    label: isEnabled.value ? t('common.enabled') : t('common.disabled'),
    variant: isEnabled.value ? 'primary' : 'default',
    action: enable,
  },
])
</script>
