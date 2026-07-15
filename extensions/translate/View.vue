<template>
  <BaseEmptyState v-if="!isConfigured" icon="i-ri-settings-3-line" title="请先配置翻译服务" />

  <div v-else flex="~ col">
    <!-- 顶距交给 scrollContainer CHROME_HEIGHT（已含栏底 gap），勿再 p-t 叠双层 -->
    <div p="x-3 b-3">
      <BaseTextarea
        ref="textareaRef"
        v-model="inputText"
        class="translate-input"
        rounded="panel"
        placeholder="输入文本"
        :rows="1"
        :max-height="0"
        @submit="handleSubmit"
      />
    </div>

    <div v-if="translateResults.length > 0">
      <BaseList
        :items="translateResults"
        v-model:selected-index="selectedIndex"
        navigate-on-input
        @execute="onExecuteResult"
      >
        <template #item="{ item }">
          <BaseListItem multiline-title :subtitle="item.engine">
            <template #title>
              <div
                v-if="item.loading && !item.translation"
                class="i-ri-loader-4-line animate-spin"
                text="base muted"
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
      // 选词翻译进入：翻译启动即让出输入框焦点，确保结果出来后回车直接复制（而非触发提交）
      textareaRef.value?.blur()
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

// 正在翻译中（流式未完成）重新激活时不抢焦点；其余情况聚焦输入框。
// 注：选词翻译的焦点让出由 pendingText watch 的 blur 兜底（pendingText 由快捷键
// 异步取词后设置，远晚于 onActivated，无法在激活时判定）
function maybeFocusInput() {
  if (!isTranslating.value) {
    nextTick(() => textareaRef.value?.focus())
  }
}
onMounted(maybeFocusInput)
onActivated(maybeFocusInput)
</script>

<style scoped>
/* 与 Agent 输入框一致：默认描边 + 聚焦改色并关 ui-ctrl inset ring，避免双线 */
:deep(.translate-input) {
  border: 1px solid var(--color-border);
  transition: border-color 150ms cubic-bezier(0, 0, 0.2, 1);
}

:deep(.translate-input:focus-within) {
  border-color: color-mix(in srgb, var(--color-accent) 50%, transparent);
  --un-ring-shadow: 0 0 #0000;
  --un-inset-ring-shadow: 0 0 #0000;
  --un-ring-offset-shadow: 0 0 #0000;
  box-shadow: none !important;
}
</style>
