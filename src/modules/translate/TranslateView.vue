<script setup lang="ts">
import { ref, watch, nextTick, onMounted } from 'vue'
import {
  translateResults,
  isTranslating,
  translateText,
  pendingText,
  inputText,
} from './index'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { getCurrentWindow } from '@tauri-apps/api/window'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import BaseTextarea from '@/components/ui/BaseTextarea.vue'

const textareaRef = ref<InstanceType<typeof BaseTextarea>>()

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

function handleSubmit() {
  const text = inputText.value.trim()
  if (text) {
    translateText(text)
  }
}

async function handleCopy(translation: string) {
  if (!translation) return
  try {
    await writeText(translation)
    getCurrentWindow().hide()
  } catch (e) {
    console.error('Failed to copy:', e)
  }
}

onMounted(() => {
  nextTick(() => textareaRef.value?.focus())
})
</script>

<template>
  <div class="p-5 flex flex-col gap-4 h-full overflow-y-auto">
    <BaseTextarea
      ref="textareaRef"
      v-model="inputText"
      placeholder="输入文本"
      :rows="1"
      :max-height="0"
      @submit="handleSubmit"
    />

    <BaseEmptyState
      v-if="isTranslating && translateResults.length === 0"
      icon="i-ri-loader-4-line"
      title="翻译中..."
      loading
    />

    <div v-if="translateResults.length > 0" class="flex flex-col gap-3">
      <div
        v-for="(result, index) in translateResults"
        :key="index"
        class="p-3 rounded-md bg-black/4"
        tabindex="0"
        @dblclick="!result.loading && handleCopy(result.translation)"
        @keydown.enter.prevent="
          !result.loading && handleCopy(result.translation)
        "
      >
        <div class="text-xs text-tx-faint mb-1.5">{{ result.engine }}</div>
        <div
          v-if="result.loading"
          class="i-ri-loader-4-line text-base text-tx-muted animate-spin"
        ></div>
        <p
          v-else
          class="text-sm text-tx-primary leading-relaxed wrap-break-word"
        >
          {{ result.translation }}
        </p>
      </div>
    </div>
  </div>
</template>
