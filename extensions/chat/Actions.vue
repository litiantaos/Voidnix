<template>
  <BaseSelect
    v-if="modelOptions.length > 0"
    :model-value="settings.activeModelKey"
    :options="modelOptions"
    class="min-w-0 w-50"
    @update:model-value="handleModelChange"
  />
  <BaseButton icon="i-ri-add-line" @click="newConversation" />
  <BaseButton
    :icon="appStore.activePanel === 'settings' ? 'i-ri-settings-3-fill' : 'i-ri-settings-3-line'"
    @click="appStore.togglePanel('settings')"
  />
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { providerLabelFromUrl } from '@/utils/provider'
import { useSettingsStore } from '@/stores/settings'
import { useAppStore } from '@/stores/app'
import { newConversation } from './index'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseSelect from '@/components/ui/BaseSelect.vue'

const settings = useSettingsStore()
const appStore = useAppStore()

const modelOptions = computed(() => {
  if (settings.chatConfigs.length <= 1) {
    const config = settings.chatConfigs[0]
    if (!config) return []
    return config.models
      .filter((m) => m.trim())
      .map((m) => ({ label: m, value: `${config.id}::${m}` }))
  }
  return settings.chatConfigs.map((config) => ({
    label: providerLabelFromUrl(config.endpoint, 'API'),
    options: config.models
      .filter((m) => m.trim())
      .map((m) => ({ label: m, value: `${config.id}::${m}` })),
  }))
})

async function handleModelChange(val: string | number) {
  await settings.setActiveModelKey(String(val))
}
</script>
