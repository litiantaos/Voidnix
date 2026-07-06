<template>
  <BaseEmptyState v-if="!isConfigured" icon="i-ri-settings-3-line" title="请先配置翻译服务" />

  <div v-else flex="~ col">
    <div p="x-5 b-2 t-5">
      <BaseTextarea
        ref="textareaRef"
        v-model="inputText"
        placeholder="输入文本"
        :rows="1"
        :max-height="0"
        @submit="handleSubmit"
      />
    </div>

    <div v-if="translateResults.length > 0" p="x-3">
      <BaseList
        :items="translateResults"
        v-model:selected-index="selectedIndex"
        @execute="onExecuteResult"
      >
        <template #item="{ item, selected }">
          <BaseListItem :selected="selected" multiline-title :subtitle="item.engine">
            <template #title>
              <div
                v-if="item.loading && !item.translation"
                class="i-ri-loader-4-line animate-spin"
                text="base tx-muted"
              />
              <span v-else leading="relaxed" font="normal" wrap="break-word">
                {{ item.translation }}
              </span>
            </template>
          </BaseListItem>
        </template>
      </BaseList>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick, onMounted, onActivated } from 'vue'

import { translateResults, isTranslating, translateText, pendingText, inputText } from './index'
import { config as translateConfig } from './config'
import { copyAndHide } from '@/stores/app'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import BaseTextarea from '@/components/ui/BaseTextarea.vue'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'
import type { TranslateResult } from './index'

const textareaRef = ref<InstanceType<typeof BaseTextarea>>()
const selectedIndex = ref(0)

const isConfigured = computed(() =>
  translateConfig.configs.some(
    (c) =>
      (c.type === 'youdao' && c.appKey && c.appSecret) ||
      (c.type === 'ai' && c.endpoint && c.apiKey),
  ),
)

watch(
  pendingText,
  (text) => {
    if (text) {
      inputText.value = text
      pendingText.value = ''
      translateText(text)
    }
  },
  { immediate: true },
)

watch(
  () => inputText.value === '',
  (empty) => {
    if (empty) {
      translateResults.value = []
      isTranslating.value = false
    }
  },
)

watch(translateResults, () => {
  selectedIndex.value = 0
})

function handleSubmit() {
  const text = inputText.value.trim()
  if (!text) return
  translateText(text)
  ;(document.activeElement as HTMLElement)?.blur()
}

async function onExecuteResult(result: TranslateResult) {
  if (!result.translation || result.loading) return
  try {
    await copyAndHide(result.translation)
  } catch (e) {
    console.error('Failed to copy:', e)
  }
}

onMounted(() => {
  if (!isTranslating.value) {
    nextTick(() => textareaRef.value?.focus())
  }
})

onActivated(() => {
  if (!isTranslating.value) {
    nextTick(() => textareaRef.value?.focus())
  }
})
</script>
