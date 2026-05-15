<template>
  <div
    class="px-2 py-1.5 border border-black/10 rounded-lg bg-surface flex gap-1 shadow-xl items-center absolute z-50"
    :style="style"
  >
    <button v-for="t in tools" :key="t.label"
      class="rounded flex h-7 w-7 transition-colors items-center justify-center"
      :class="activeTool === t.id ? 'bg-accent text-white' : 'ui-hover text-tx-secondary'"
      :title="t.label" @click="$emit('tool', t.id)"
    >
      <!-- 静态图标，UnoCSS 能扫描到 -->
      <span v-if="t.id === 'rect'"  class="i-ri-rectangle-line" />
      <span v-if="t.id === 'line'"  class="i-ri-subtract-line" />
      <span v-if="t.id === 'arrow'" class="i-ri-arrow-right-line" />
      <span v-if="t.id === 'text'"  class="i-ri-text" />
      <span v-if="t.id === 'blur'"  class="i-ri-blur-off-line" />
    </button>

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

    <!-- 线宽 -->
    <select class="text-xs text-tx-primary px-1 border border-black/10 rounded bg-surface h-6"
      :value="lineWidth"
      @change="$emit('line-width', Number(($event.target as HTMLSelectElement).value))"
    >
      <option value="1">细</option>
      <option value="2">中</option>
      <option value="4">粗</option>
    </select>

    <div class="bg-border mx-0.5 h-5 w-px" />

    <button class="text-tx-secondary rounded ui-hover flex h-7 w-7 items-center justify-center" title="OCR" @click="$emit('ocr')">
      <span class="i-ri-scan-line" />
    </button>
    <button class="text-tx-secondary rounded ui-hover flex h-7 w-7 items-center justify-center" title="复制 (Enter)" @click="$emit('copy')">
      <span class="i-ri-file-copy-line" />
    </button>
    <button class="text-tx-secondary rounded ui-hover flex h-7 w-7 items-center justify-center" title="保存" @click="$emit('save')">
      <span class="i-ri-save-line" />
    </button>
    <button class="text-tx-muted rounded ui-hover flex h-7 w-7 items-center justify-center" title="取消 (Esc)" @click="$emit('cancel')">
      <span class="i-ri-close-line" />
    </button>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'

type Tool = 'rect' | 'line' | 'arrow' | 'text' | 'blur' | null

const props = defineProps<{
  sel: { x: number; y: number; w: number; h: number }
  activeTool: Tool
  color: string
  lineWidth: number
  screenHeight: number
}>()

defineEmits<{
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

const style = computed(() => {
  const { x, y, h } = props.sel
  const toolbarH = 44
  const gap = 8
  const below = y + h + gap
  const above = y - toolbarH - gap
  const top = below + toolbarH < props.screenHeight ? below : above
  const left = Math.max(4, Math.min(x, window.innerWidth - 320))
  return { top: `${top}px`, left: `${left}px` }
})
</script>
