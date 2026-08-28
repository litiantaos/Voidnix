<template>
  <!-- 顶距交给 scrollContainer CHROME_HEIGHT（已含栏底 gap），勿再 p-t 叠双层 -->
  <div flex="~ col" gap="3" :class="{ 'pb-3': !ocrText && !error }">
    <!-- 截图预览：cover 缩放铺满容器，长边溢出可上下/左右滚动；识别中遮罩覆盖 -->
    <div
      v-if="imageUrl"
      ref="previewRef"
      m="x-3"
      relative
      class="hide-scrollbar border border-divider radius-panel border-solid fill-ctrl"
      h="44"
      shrink="0"
      overflow="auto"
    >
      <img
        :src="imageUrl"
        block
        max-w="none"
        w="full"
        h="full"
        object="cover left-top"
        :alt="t('screenshot.previewAlt')"
        @load="onPreviewLoad"
      />
      <Transition
        enter-active-class="transition duration-[var(--duration-fast)] ease-out"
        enter-from-class="opacity-0"
        enter-to-class="opacity-100"
        leave-active-class="transition duration-[var(--duration-fastest)] ease-in"
        leave-from-class="opacity-100"
        leave-to-class="opacity-0"
      >
        <div v-if="isLoading" class="fill-strong flex inset-0 absolute backdrop-blur-xs">
          <BaseEmptyState loading />
        </div>
      </Transition>
    </div>

    <!-- 错误 -->
    <div v-if="error" p="x-3 b-3" shrink="0">
      <div text="sm danger" p="3" class="radius-ctrl bg-danger-soft">
        {{ error }}
      </div>
    </div>

    <!-- 结果 -->
    <template v-else-if="ocrText">
      <div p="x-3">
        <BaseTextarea
          v-model="ocrText"
          rounded="panel"
          :rows="4"
          :max-height="0"
          :submit-on-enter="false"
          :placeholder="t('screenshot.ocrResult')"
        />
      </div>

      <!-- 操作列表（原按钮组改为列表项，回车触发）-->
      <BaseList
        :items="ocrActions"
        v-model:selected-index="actionIndex"
        group-field="group"
        :group-title="() => t('screenshot.actions')"
        @execute="onAction"
      >
        <template #item="{ item }">
          <BaseListItem :title="item.label" />
        </template>
      </BaseList>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { emit } from '@tauri-apps/api/event'
import { CMD } from '@/commands'
import { t } from '@/runtime/i18n'
import { copyAndHide, useAppStore } from '@/stores/app'
import { pendingOcrData } from './index'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import BaseTextarea from '@/components/ui/BaseTextarea.vue'
import BaseList from '@/components/ui/BaseList.vue'
import BaseListItem from '@/components/ui/BaseListItem.vue'

interface OcrResult {
  text: string
  qr: string[]
}

interface OcrAction {
  id: string
  label: string
  group: string
  run: () => void | Promise<void>
}

const appStore = useAppStore()
const imageUrl = ref('')
const ocrText = ref('')
const isLoading = ref(false)
const error = ref('')
const previewRef = ref<HTMLElement>()
const actionIndex = ref(0)

// 截图 cover 缩放：短边撑满容器、长边溢出（容器 overflow-auto 可上下/左右滚动）
// 双阶段：加载前用 CSS object-fit:cover（object-position:left top 对齐 scroll 0,0）
// 零滞后且无跳变；@load 后按 natural 尺寸手算精确 cover 尺寸写入内联宽高，
// 切换为可滚动模式——切换前后短边撑满方式与左上对齐完全一致，视觉连续。
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
      ocrText.value = result.text || t('screenshot.noContent')
    }
  } catch (e) {
    error.value = String(e)
  } finally {
    isLoading.value = false
    // 操作列表接管键盘：默认选中首项（复制），回车直接复制；点击 textarea 可编辑
    actionIndex.value = 0
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
  appStore.setActiveExtension('translate')
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

// 操作列表（原按钮组改为列表项）：识别完成后默认选中首项（复制），回车触发
const ocrActions = computed<OcrAction[]>(() => {
  if (!ocrText.value.trim()) return []
  return [
    { id: 'copy', label: t('screenshot.copy'), group: 'actions', run: handleCopy },
    { id: 'translate', label: t('screenshot.translate'), group: 'actions', run: handleTranslate },
    { id: 'trimSpaces', label: t('screenshot.trimSpaces'), group: 'actions', run: trimSpaces },
    {
      id: 'trimNewlines',
      label: t('screenshot.trimNewlines'),
      group: 'actions',
      run: trimNewlines,
    },
    {
      id: 'trimEmptyLines',
      label: t('screenshot.trimEmptyLines'),
      group: 'actions',
      run: trimEmptyLines,
    },
  ]
})

function onAction(action: OcrAction) {
  void action.run()
}
</script>
