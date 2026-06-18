<template>
  <BaseSelect
    v-if="modelOptions.length > 0"
    :model-value="settings.activeProviderModelKey"
    :options="modelOptions"
    class="min-w-0 w-50"
    @update:model-value="handleModelChange"
  />
  <BaseButton icon="i-ri-add-line" @click="handleNewConversation" />
  <BaseButton
    :icon="appStore.activeSubview === 'settings' ? 'i-ri-settings-3-fill' : 'i-ri-settings-3-line'"
    @click="appStore.toggleSubview('settings')"
  />
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { providerLabelFromUrl } from '@/utils/format'
import { useSettingsStore } from '@/stores/settings'
import { useAppStore } from '@/stores/app'
import { useAgentChat } from './agent'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseSelect from '@/components/ui/BaseSelect.vue'

const settings = useSettingsStore()
const appStore = useAppStore()
const agent = useAgentChat()

const modelOptions = computed(() => {
  if (settings.aiProviders.length <= 1) {
    const config = settings.aiProviders[0]
    if (!config) return []
    return config.models
      .filter((m) => m.trim())
      .map((m) => ({ label: m, value: `${config.id}::${m}` }))
  }
  return settings.aiProviders.map((config) => ({
    label: providerLabelFromUrl(config.endpoint, 'API'),
    options: config.models
      .filter((m) => m.trim())
      .map((m) => ({ label: m, value: `${config.id}::${m}` })),
  }))
})

async function handleModelChange(val: string | number) {
  await settings.setActiveProviderModelKey(String(val))
}

function handleNewConversation() {
  agent.newConversation()
}
</script>
