<template>
  <div p="5" flex="~ col" gap="4" h="full" overflow="y-auto">
    <!-- 截图预览 -->
    <div v-if="imageUrl" border="~ black/8" rounded="lg" bg="black/4" shrink="0" overflow="hidden">
      <img :src="imageUrl" h="auto" max-h="40" w="full" block object="contain" alt="截图预览" />
    </div>

    <!-- 识别中 -->
    <BaseEmptyState v-if="isLoading" icon="i-ri-loader-4-line" title="识别中..." loading />

    <!-- 错误 -->
    <div v-else-if="error" text="sm red-500" p="3" rounded="md" bg="red-50">
      {{ error }}
    </div>

    <!-- 结果 -->
    <template v-else-if="ocrText">
      <BaseTextarea
        ref="textareaRef"
        v-model="ocrText"
        :rows="4"
        :max-height="0"
        :submit-on-enter="false"
        placeholder="识别结果"
      />

      <!-- 操作栏 -->
      <div class="action-footer" shrink="0">
        <BaseButton icon="i-ri-file-copy-line" @click="handleCopy">复制</BaseButton>
        <BaseButton icon="i-ri-translate-2" @click="handleTranslate">翻译</BaseButton>
      </div>
    </template>

    <!-- 空状态 -->
    <BaseEmptyState
      v-else-if="!isLoading && !imageUrl"
      icon="i-ri-qr-scan-2-line"
      title="从截屏触发识别"
      description="截图后点击工具栏的识别按钮，支持文字和二维码"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, watch, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { copyAndHide } from '@/utils/clipboard'
import { useAppStore } from '@/stores/app'
import { pendingText } from '@ext/translate'
import { pendingOcrData } from './index'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import BaseTextarea from '@/components/ui/BaseTextarea.vue'
import BaseButton from '@/components/ui/BaseButton.vue'

interface OcrResult {
  text: string
  qr: string[]
}

const appStore = useAppStore()
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
    const result = await invoke<OcrResult>(CMD.ocrImage, {
      selX: data.selX,
      selY: data.selY,
      selW: data.selW,
      selH: data.selH,
      scale: data.scale,
      annotationPng: data.annotationPng,
    })
    if (result.qr?.length) {
      ocrText.value = result.qr.join('\n')
    } else {
      ocrText.value = result.text || '未识别到内容'
    }
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

async function handleCopy() {
  if (!ocrText.value.trim()) return
  await copyAndHide(ocrText.value)
}

function handleTranslate() {
  if (!ocrText.value.trim()) return
  pendingText.value = ocrText.value
  appStore.setActiveModule('translate')
}
</script>
