<template>
  <div p="3" flex="~ col" gap="3">
    <!-- 截图预览：cover 缩放铺满容器，长边溢出可上下/左右滚动；识别中遮罩覆盖 -->
    <div
      v-if="imageUrl"
      ref="previewRef"
      relative
      class="hide-scrollbar border border-divider radius-panel border-solid fill-ctrl"
      h="44"
      shrink="0"
      overflow="auto"
    >
      <img :src="imageUrl" block max-w="none" alt="截图预览" @load="onPreviewLoad" />
      <div v-if="isLoading" class="bg-white/20 flex inset-0 absolute backdrop-blur-xs">
        <BaseEmptyState loading />
      </div>
    </div>

    <!-- 错误 -->
    <div v-if="error" text="sm danger" p="3" class="radius-ctrl bg-danger-soft">
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
      <div class="flex flex-wrap gap-2" shrink="0">
        <BaseButton icon="i-ri-file-copy-line" @click="handleCopy">复制</BaseButton>
        <BaseButton icon="i-ri-translate-2" @click="handleTranslate">翻译</BaseButton>
        <BaseButton icon="i-ri-space" @click="trimSpaces">去空格</BaseButton>
        <BaseButton icon="i-ri-corner-down-left-line" @click="trimNewlines">去换行</BaseButton>
        <BaseButton icon="i-ri-text-spacing" @click="trimEmptyLines">去空行</BaseButton>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { emit } from '@tauri-apps/api/event'
import { CMD } from '@/commands'
import { copyAndHide, useAppStore } from '@/stores/app'
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
const previewRef = ref<HTMLElement>()

// 截图 cover 缩放：短边撑满容器、长边溢出（容器 overflow-auto 可上下/左右滚动）
// 纯 object-cover 会把内容裁进固定 box 不产生溢出，故按 natural 尺寸手算 cover 缩放后赋给 img
function onPreviewLoad(e: Event) {
  const img = e.target as HTMLImageElement
  const box = previewRef.value
  if (!img || !box) return
  const cw = box.clientWidth
  const ch = box.clientHeight
  const nw = img.naturalWidth
  const nh = img.naturalHeight
  if (!cw || !ch || !nw || !nh) return
  const scale = Math.max(cw / nw, ch / nh)
  img.style.width = `${Math.round(nw * scale)}px`
  img.style.height = `${Math.round(nh * scale)}px`
}

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
    // preventScroll：窗口高度动画起步阶段可视区小于内容，focus 默认会 scrollIntoView 把
    // textarea 滚进视窗导致内容跳到底部；阻止滚动，让内容始终从顶部展开
    nextTick(() => textareaRef.value?.focus({ preventScroll: true }))
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

async function handleTranslate() {
  if (!ocrText.value.trim()) return
  // 跨扩展通信走事件总线（C9）：screenshot 不再直依赖 translate 内部状态。
  // translate 扩展 setup 监听 'translate-pending-text'，写入自身 pendingText。
  await emit('translate-pending-text', ocrText.value)
  appStore.setActiveModule('translate')
}

function trimSpaces() {
  ocrText.value = ocrText.value.replace(/[ \t\u3000]+/g, '')
}

function trimNewlines() {
  ocrText.value = ocrText.value.replace(/[\r\n]+/g, '')
}

function trimEmptyLines() {
  ocrText.value = ocrText.value
    .split('\n')
    .filter((line) => line.trim() !== '')
    .join('\n')
}
</script>
