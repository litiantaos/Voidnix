<template>
  <div
    class="p-2 border border-black/10 rounded-lg bg-surface/95 flex gap-2 shadow-xl items-center absolute z-50 backdrop-blur-sm"
    :style="style"
  >
    <!-- 工具按钮 -->
    <BaseButton
      v-for="t in tools" :key="t.label"
      size="icon"
      :variant="activeTool === t.id ? 'primary' : 'default'"
      :title="t.label"
      @click="$emit('tool', t.id)"
    >
      <span :class="t.icon" />
    </BaseButton>

    <div class="mx-0.5 bg-black/10 shrink-0 h-5 w-px" />

    <!-- 颜色选择器（模糊工具时不显示） -->
    <template v-if="activeTool !== 'blur'">
      <button
        class="border-2 rounded-full shrink-0 h-5 w-5 shadow-sm transition-transform active:scale-95"
        :style="{ background: color, borderColor: color === '#ffffff' ? '#d1d5db' : 'white' }"
        title="颜色"
        @click="showColors = !showColors"
      />

      <Transition name="palette-popup">
        <div
          v-if="showColors"
          class="p-2 border border-black/10 rounded-lg bg-surface/95 flex gap-2 shadow-xl items-center left-0 absolute z-10 backdrop-blur-sm -bottom-100%"
          @click.stop
        >
          <button
            v-for="c in colors" :key="c"
            class="border-2 rounded-full shrink-0 h-5 w-5 shadow-sm transition-transform active:scale-95"
            :class="{ 'ring-2 ring-accent ring-offset-1': c === color }"
            :style="{ background: c, borderColor: c === '#ffffff' ? '#d1d5db' : 'white' }"
            :title="c"
            @click="$emit('color', c); showColors = false"
          />
        </div>
      </Transition>
    </template>

    <!-- 线宽（模糊工具时不显示） -->
    <div v-if="activeTool !== 'blur'" class="flex gap-0.5 h-7 items-center">
      <input
        :value="lineWidth"
        type="number"
        min="1"
        max="20"
        class="text-xs text-tx-primary text-center outline-none border border-black/10 rounded bg-black/5 h-5 w-7 [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none"
        @change="onLineWidthChange"
        @keydown.up.prevent="emit('line-width', Math.min(20, lineWidth + 1))"
        @keydown.down.prevent="emit('line-width', Math.max(1, lineWidth - 1))"
      />
      <div class="flex flex-col gap-px h-5 justify-center">
        <button
          class="text-[9px] text-tx-muted leading-none rounded-sm flex h-2.5 w-4 items-center justify-center hover:text-tx-primary hover:bg-black/8"
          title="加粗"
          @click="emit('line-width', Math.min(20, lineWidth + 1))"
        >▲</button>
        <button
          class="text-[9px] text-tx-muted leading-none rounded-sm flex h-2.5 w-4 items-center justify-center hover:text-tx-primary hover:bg-black/8"
          title="减细"
          @click="emit('line-width', Math.max(1, lineWidth - 1))"
        >▼</button>
      </div>
    </div>

    <!-- 模糊度（仅 blur 工具时显示） -->
    <div v-else class="flex gap-0.5 h-7 items-center" title="模糊度">
      <span class="i-ri-contrast-drop-line text-sm text-tx-muted mr-0.5 shrink-0" />
      <input
        :value="blurAmount"
        type="number"
        min="1"
        max="50"
        class="text-xs text-tx-primary text-center outline-none border border-black/10 rounded bg-black/5 h-5 w-8 [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none"
        @change="onBlurAmountChange"
        @keydown.up.prevent="emit('blur-amount', Math.min(50, blurAmount + 1))"
        @keydown.down.prevent="emit('blur-amount', Math.max(1, blurAmount - 1))"
      />
      <div class="flex flex-col gap-px h-5 justify-center">
        <button
          class="text-[9px] text-tx-muted leading-none rounded-sm flex h-2.5 w-4 items-center justify-center hover:text-tx-primary hover:bg-black/8"
          title="加深"
          @click="emit('blur-amount', Math.min(50, blurAmount + 1))"
        >▲</button>
        <button
          class="text-[9px] text-tx-muted leading-none rounded-sm flex h-2.5 w-4 items-center justify-center hover:text-tx-primary hover:bg-black/8"
          title="减弱"
          @click="emit('blur-amount', Math.max(1, blurAmount - 1))"
        >▼</button>
      </div>
    </div>

    <div class="mx-0.5 bg-black/10 shrink-0 h-5 w-px" />

    <!-- 操作按钮 -->
    <BaseButton size="icon" title="OCR 识别" @click="$emit('ocr')">
      <span class="i-ri-scan-line" />
    </BaseButton>
    <BaseButton size="icon" title="钉图" @click="$emit('pin')">
      <span class="i-ri-pushpin-line" />
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
  blurAmount: number
  screenHeight: number
  screenWidth: number
}>()

const emit = defineEmits<{
  (e: 'tool', t: Tool): void
  (e: 'color', c: string): void
  (e: 'line-width', n: number): void
  (e: 'blur-amount', n: number): void
  (e: 'ocr'): void
  (e: 'pin'): void
  (e: 'copy'): void
  (e: 'save'): void
  (e: 'cancel'): void
}>()

const showColors = ref(false)

const tools: { id: Tool; label: string; icon: string }[] = [
  { id: 'rect',  label: '矩形',  icon: 'i-ri-rectangle-line' },
  { id: 'line',  label: '直线',  icon: 'i-ri-subtract-line' },
  { id: 'arrow', label: '箭头',  icon: 'i-ri-arrow-right-line' },
  { id: 'text',  label: '文字',  icon: 'i-ri-text' },
  { id: 'blur',  label: '模糊',  icon: 'i-ri-blur-off-line' },
]

const colors = [
  '#ff3b30', '#ff9500', '#ffcc00', '#34c759',
  '#007aff', '#5856d6', '#af52de', '#ffffff', '#000000',
]

function onLineWidthChange(e: Event) {
  const target = e.target as HTMLInputElement
  const v = parseInt(target.value)
  if (!isNaN(v)) {
    const next = Math.max(1, Math.min(20, v))
    target.value = String(next)
    emit('line-width', next)
  } else {
    target.value = String(props.lineWidth)
  }
}

function onBlurAmountChange(e: Event) {
  const target = e.target as HTMLInputElement
  const v = parseInt(target.value)
  if (!isNaN(v)) {
    const next = Math.max(1, Math.min(50, v))
    target.value = String(next)
    emit('blur-amount', next)
  } else {
    target.value = String(props.blurAmount)
  }
}

const PALETTE_H = 40
const PALETTE_GAP = 8

const style = computed(() => {
  const { x, y, w, h } = props.sel
  const paletteW = 360
  const leftAligned = Math.max(4, x)
  const rightAligned = x + w - paletteW
  const left = leftAligned + paletteW <= props.screenWidth ? leftAligned : Math.max(4, rightAligned)

  const below = y + h + PALETTE_GAP
  const above = y - PALETTE_H - PALETTE_GAP
  let top: number
  if (below + PALETTE_H <= props.screenHeight) {
    top = below
  } else if (above >= 0) {
    top = above
  } else {
    top = y + h - PALETTE_H - PALETTE_GAP
  }

  return { top: `${top}px`, left: `${left}px` }
})
</script>

<style scoped>
.palette-popup-enter-active,
.palette-popup-leave-active {
  transition: opacity 0.12s ease, transform 0.12s ease;
}
.palette-popup-enter-from,
.palette-popup-leave-to {
  opacity: 0;
  transform: translateY(4px);
}
</style>
