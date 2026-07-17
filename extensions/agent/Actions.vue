<template>
  <BaseSelect
    v-if="modelOptions.length > 0"
    :model-value="agentConfig.activeProviderModelKey"
    :options="modelOptions"
    class="max-w-50"
    @update:model-value="handleModelChange"
  />
  <BaseButton
    icon="i-ri-add-line"
    title="新会话"
    aria-label="新会话"
    @click="handleNewConversation"
  />
  <BaseButton
    :icon="appStore.activeSubview === 'config' ? 'i-ri-settings-3-fill' : 'i-ri-settings-3-line'"
    :title="appStore.activeSubview === 'config' ? '关闭设置' : '设置'"
    :aria-label="appStore.activeSubview === 'config' ? '关闭设置' : '设置'"
    @click="toggleConfig"
  />
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { providerLabelFromUrl } from '@/utils/format'
import { useAppStore } from '@/stores/app'
import { useAgentChat } from './agent'
import { config as agentConfig, setActiveProviderModelKey } from './config'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseSelect from '@/components/ui/BaseSelect.vue'

const appStore = useAppStore()
const agent = useAgentChat()

const modelOptions = computed(() => {
  if (agentConfig.aiProviders.length <= 1) {
    const config = agentConfig.aiProviders[0]
    if (!config) return []
    return config.models
      .filter((m) => m.trim())
      .map((m) => ({ label: m, value: `${config.id}::${m}` }))
  }
  return agentConfig.aiProviders.map((config) => ({
    label: providerLabelFromUrl(config.endpoint, 'API'),
    options: config.models
      .filter((m) => m.trim())
      .map((m) => ({ label: m, value: `${config.id}::${m}` })),
  }))
})

async function handleModelChange(val: string | number) {
  await setActiveProviderModelKey(String(val))
}

function handleNewConversation() {
  agent.newConversation()
}

function toggleConfig() {
  if (appStore.activeSubview === 'config') appStore.closeSubview()
  else appStore.openSubview('config')
}
</script>
