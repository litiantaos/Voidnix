<template>
  <div
    class="px-2 py-1.5 border border-black/10 rounded-lg bg-surface flex gap-1 shadow-xl items-center absolute z-50"
    :style="style"
  >
    <BaseButton
      v-for="t in tools" :key="t.label"
      size="icon"
      :variant="activeTool === t.id ? 'primary' : 'default'"
      :title="t.label"
      @click="$emit('tool', t.id)"
    >
      <span v-if="t.id === 'rect'"  class="i-ri-rectangle-line" />
      <span v-if="t.id === 'line'"  class="i-ri-subtract-line" />
      <span v-if="t.id === 'arrow'" class="i-ri-arrow-right-line" />
      <span v-if="t.id === 'text'"  class="i-ri-text" />
      <span v-if="t.id === 'blur'"  class="i-ri-blur-off-line" />
    </BaseButton>

    <div class="bg-border mx-0.5 h-5 w-px" />

    <!-- 颜色 -->
    <div class="relative">
      <button class="border-2 border-white/50 rounded-full h-5 w-5 shadow"
        :style="{ background: color }" title="颜色" @click="showColors = !showColors" />
      <div v-if="showColors"
        class="p-1.5 border border-black/10 rounded-lg bg-surface flex gap-1 shadow-xl bottom-8 left-0 absolute"
      >
        <button v-for="c in colors" :key="c"
          class="border-2 rounded-full h-5 w-5 transition-transform hover:scale-110"
          :style="{ background: c, borderColor: color === c ? 'white' : 'transparent' }"
          @click="$emit('color', c); showColors = false"
        />
      </div>
    </div>

    <!-- 线宽加减数字框 -->
    <div class="flex gap-0.5 items-center">
      <BaseButton size="icon" title="减细" @click="changeLineWidth(-1)">−</BaseButton>
      <span class="text-xs text-tx-primary text-center w-4 select-none">{{ lineWidth }}</span>
      <BaseButton size="icon" title="加粗" @click="changeLineWidth(1)">+</BaseButton>
    </div>

    <div class="bg-border mx-0.5 h-5 w-px" />

    <BaseButton size="icon" title="OCR" @click="$emit('ocr')">
      <span class="i-ri-scan-line" />
    </BaseButton>
    <BaseButton size="icon" title="复制 (Enter)" @click="$emit('copy')">
      <span class="i-ri-file-copy-line" />
    </BaseButton>
    <BaseButton size="icon" title="保存" @click="$emit('save')">
      <span class="i-ri-save-line" />
    </BaseButton>
    <BaseButton size="icon" title="取消 (Esc)" @click="$emit('cancel')">
      <span class="i-ri-close-line" />
    </BaseButton>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import BaseButton from '@/components/ui/BaseButton.vue'

type Tool = 'rect' | 'line' | 'arrow' | 'text' | 'blur' | null

const props = defineProps<{
  sel: { x: number; y: number; w: number; h: number }
  activeTool: Tool
  color: string
  lineWidth: number
  screenHeight: number
  screenWidth: number
}>()

const emit = defineEmits<{
  (e: 'tool', t: Tool): void
  (e: 'color', c: string): void
  (e: 'line-width', n: number): void
  (e: 'ocr'): void
  (e: 'copy'): void
  (e: 'save'): void
  (e: 'cancel'): void
}>()

const showColors = ref(false)

const tools: { id: Tool; label: string }[] = [
  { id: 'rect',  label: '矩形' },
  { id: 'line',  label: '直线' },
  { id: 'arrow', label: '箭头' },
  { id: 'text',  label: '文字' },
  { id: 'blur',  label: '模糊' },
]

const colors = ['#ff3b30', '#ff9500', '#ffcc00', '#34c759', '#007aff', '#5856d6', '#ffffff', '#000000']

function changeLineWidth(delta: number) {
  const next = Math.max(1, Math.min(20, props.lineWidth + delta))
  emit('line-width', next)
}

const style = computed(() => {
  const { x, y, w, h } = props.sel
  const toolbarH = 44
  const toolbarW = 320
  const gap = 8

  // 水平位置：优先对齐选区左边，超出屏幕右边时右对齐选区右边
  const leftAligned = Math.max(4, x)
  const rightAligned = x + w - toolbarW
  const left = leftAligned + toolbarW <= props.screenWidth ? leftAligned : Math.max(4, rightAligned)

  // 垂直位置：优先显示在选区下方外侧，不够时显示在上方外侧，都不够时显示在内侧底部
  const below = y + h + gap
  const above = y - toolbarH - gap
  let top: number
  if (below + toolbarH <= props.screenHeight) {
    top = below
  } else if (above >= 0) {
    top = above
  } else {
    // 选区占满屏幕，显示在内侧底部
    top = y + h - toolbarH - gap
  }

  return { top: `${top}px`, left: `${left}px` }
})
</script>
