<template>
  <BaseSelect
    v-if="modelOptions.length > 0"
    :model-value="effectiveProviderModelKey"
    :options="modelOptions"
    class="max-w-64"
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
import { useAppStore } from '@/stores/app'
import { useAgentChat } from './agent'
import {
  setProviderModelKey,
  modelSelectOptions,
  effectiveProviderModelKey,
} from './config'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseSelect from '@/components/ui/BaseSelect.vue'

const appStore = useAppStore()
const agent = useAgentChat()

const modelOptions = computed(() => modelSelectOptions())

async function handleModelChange(val: string | number) {
  setProviderModelKey(String(val))
}

function handleNewConversation() {
  agent.newConversation()
}

function toggleConfig() {
  if (appStore.activeSubview === 'config') appStore.closeSubview()
  else appStore.openSubview('config')
}
</script>
