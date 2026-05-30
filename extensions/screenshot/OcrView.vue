<template>
  <div class="p-5 flex flex-col gap-4 h-full overflow-y-auto">
    <!-- 截图预览 -->
    <div
      v-if="imageUrl"
      class="border border-black/8 rounded-lg bg-black/4 shrink-0 overflow-hidden"
    >
      <img :src="imageUrl" class="h-auto max-h-40 w-full block object-contain" alt="截图预览" />
    </div>

    <!-- 识别中 -->
    <BaseEmptyState v-if="isLoading" icon="i-ri-loader-4-line" title="识别中..." loading />

    <!-- 错误 -->
    <div v-else-if="error" class="text-sm text-red-500 p-3 rounded-md bg-red-50">
      {{ error }}
    </div>

    <!-- 结果：可编辑文本框，回车复制 -->
    <template v-else-if="ocrText">
      <BaseTextarea
        ref="textareaRef"
        v-model="ocrText"
        :rows="4"
        :max-height="0"
        placeholder="识别结果"
        @keydown.enter.exact="handleCopy"
      />
    </template>

    <!-- 空状态 -->
    <BaseEmptyState
      v-else-if="!isLoading && !imageUrl"
      icon="i-ri-scan-line"
      title="从截屏触发 OCR"
      description="截图后点击工具栏的 OCR 按钮"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, watch, onMounted, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { pendingOcrData } from './index'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import BaseTextarea from '@/components/ui/BaseTextarea.vue'

const imageUrl = ref('')
const ocrText = ref('')
const isLoading = ref(false)
const error = ref('')
const textareaRef = ref<InstanceType<typeof BaseTextarea>>()

async function runOcr(data: NonNullable<typeof pendingOcrData.value>) {
  isLoading.value = true
  error.value = ''
  ocrText.value = ''
  try {
    const text = await invoke<string>('ocr_image', {
      selX: data.selX,
      selY: data.selY,
      selW: data.selW,
      selH: data.selH,
      scale: data.scale,
      annotationPng: data.annotationPng,
    })
    ocrText.value = text || '未识别到文字'
  } catch (e) {
    error.value = String(e)
  } finally {
    isLoading.value = false
    nextTick(() => textareaRef.value?.focus())
  }
}

watch(
  pendingOcrData,
  (data) => {
    if (!data) return
    imageUrl.value = data.previewPng || ''
    runOcr(data)
    pendingOcrData.value = null
  },
  { immediate: true },
)

onMounted(() => {
  nextTick(() => textareaRef.value?.focus())
})

async function handleCopy() {
  if (!ocrText.value.trim()) return
  await writeText(ocrText.value)
  // 不自动关闭窗口，用户可以继续编辑或按 Esc 关闭
}
</script>
