<template>
  <!-- 遮罩层 -->
  <template v-if="hasSelection">
    <div class="mask-smoke" pointer-events="none" fixed :style="maskTop" />
    <div class="mask-smoke" pointer-events="none" fixed :style="maskBottom" />
    <div class="mask-smoke" pointer-events="none" fixed :style="maskLeft" />
    <div class="mask-smoke" pointer-events="none" fixed :style="maskRight" />
  </template>
  <template v-else-if="phase === 'select' && hoverWindow">
    <div class="mask-smoke" pointer-events="none" fixed :style="hoverMaskTop" />
    <div class="mask-smoke" pointer-events="none" fixed :style="hoverMaskBottom" />
    <div class="mask-smoke" pointer-events="none" fixed :style="hoverMaskLeft" />
    <div class="mask-smoke" pointer-events="none" fixed :style="hoverMaskRight" />
  </template>
  <div v-else class="mask-smoke" pointer-events="none" inset="0" fixed />

  <!-- 十字线：1px 线 transform 居中，消除线右/下偏 0.5px -->
  <div
    v-if="showCrossH"
    bg="accent/80"
    class="overlay-abs"
    style="left: 0; right: 0; height: 1px; top: var(--cross-y); transform: translateY(-0.5px)"
  />
  <div
    v-if="showCrossV"
    bg="accent/80"
    class="overlay-abs"
    style="top: 0; bottom: 0; width: 1px; left: var(--cross-x); transform: translateX(-0.5px)"
  />

  <!-- 窗口高亮 -->
  <template v-if="!hasSelection && phase === 'select' && hoverWindow">
    <div class="overlay-abs" :style="[hoverWindowStyle, edgeOutline]">
      <div
        text="xs primary"
        p="x-1.5 y-0.5"
        rounded
        bg="surface"
        class="overlay-abs"
        select="none"
        shadow
        :style="hoverSizeStyle"
      >
        {{ Math.round(hoverWindow.w) }}×{{ Math.round(hoverWindow.h) }}
      </div>
    </div>
  </template>

  <!-- 选区边框 + 8 控制点（滚动阶段仅边框） -->
  <template v-if="hasSelection && phase !== 'scroll'">
    <div class="overlay-abs" :style="[selectionStyle, edgeOutline]">
      <div
        text="xs primary"
        p="x-1.5 y-0.5"
        rounded
        bg="surface"
        class="overlay-abs"
        select="none"
        shadow
        :style="selSizeStyle"
      >
        {{ Math.round(sel.w) }}×{{ Math.round(sel.h) }}
      </div>
      <div
        v-for="h in handles"
        :key="h.id"
        border="~ accent"
        rounded="sm"
        bg="white"
        h="2"
        w="2"
        pointer-events="auto"
        absolute
        :style="h.style"
        @mouseenter="emit('handle-enter', h.id)"
        @mouseleave="emit('handle-leave')"
        @mousedown.stop="emit('handle-resize', h.id, $event)"
      />
    </div>
  </template>
  <template v-if="hasSelection && phase === 'scroll'">
    <div class="overlay-abs" :style="[selectionStyle, edgeOutline]" />
  </template>
</template>

<script setup lang="ts">
import type { CSSProperties } from 'vue'
import type { Phase, Sel, WindowRect } from '../composables/useTypes'

defineProps<{
  phase: Phase
  hasSelection: boolean
  hoverWindow: WindowRect | null
  sel: Sel
  showCrossH: boolean
  showCrossV: boolean
  maskTop: CSSProperties
  maskBottom: CSSProperties
  maskLeft: CSSProperties
  maskRight: CSSProperties
  hoverMaskTop: CSSProperties
  hoverMaskBottom: CSSProperties
  hoverMaskLeft: CSSProperties
  hoverMaskRight: CSSProperties
  selectionStyle: CSSProperties
  edgeOutline: CSSProperties
  hoverWindowStyle: CSSProperties
  selSizeStyle: CSSProperties
  hoverSizeStyle: CSSProperties
  handles: Array<{ id: string; style: CSSProperties }>
}>()

const emit = defineEmits<{
  'handle-enter': [id: string]
  'handle-leave': []
  'handle-resize': [id: string, e: MouseEvent]
}>()
</script>
