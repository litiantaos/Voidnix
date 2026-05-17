<template>
  <BaseSelect
v-if="modelOptions.length > 0"
     :model-value="settings.activeModelKey"
     :options="modelOptions"
     class="min-w-0 w-50"
    @update:model-value="handleModelChange"
  />
  <BaseButton size="icon" @click="newConversation">
    <div class="i-ri-add-line text-sm"></div>
  </BaseButton>
  <BaseButton size="icon" @click="appStore.toggleSettings()">
    <div class="i-ri-settings-3-line text-sm"></div>
  </BaseButton>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useSettingsStore } from '@/stores/settings'
import { useAppStore } from '@/stores/app'
import { newConversation } from './index'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseSelect from '@/components/ui/BaseSelect.vue'

const settings = useSettingsStore()
const appStore = useAppStore()

function providerLabel(url: string, fallback: string): string {
  if (!url) return fallback
  try {
    const parts = new URL(url).hostname.split('.')
    if (parts.length >= 2) return parts[parts.length - 2].toUpperCase()
    return parts[0].toUpperCase()
  } catch {
    return fallback
  }
}

const modelOptions = computed(() => {
  if (settings.chatConfigs.length <= 1) {
    const config = settings.chatConfigs[0]
    if (!config) return []
    return config.models
      .filter((m) => m.trim())
      .map((m) => ({ label: m, value: `${config.id}::${m}` }))
  }
  return settings.chatConfigs.map((config) => ({
    label: providerLabel(config.endpoint, 'API'),
    options: config.models
      .filter((m) => m.trim())
      .map((m) => ({ label: m, value: `${config.id}::${m}` })),
  }))
})

async function handleModelChange(val: string | number) {
  await settings.setActiveModelKey(String(val))
}
</script>
