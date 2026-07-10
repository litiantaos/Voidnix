<template>
  <!-- 形状控制点覆盖层（选中形状 / 绘制中的 blur 形状时显示） -->
  <template v-if="shape && phase === 'annotate'">
    <!-- 矩形/模糊：8个控制点 + (矩形)圆角/旋转控制点 -->
    <template v-if="shape.type === 'rect' || shape.type === 'blur'">
      <div
        v-for="hp in handles"
        :key="hp.id"
        pointer-events="auto"
        absolute
        z="100"
        :class="{
          'cursor-ns-resize': hp.id === 'cr',
          'cursor-grab hover:cursor-grab active:cursor-grabbing': hp.id === 'rot',
        }"
        :style="hp.style"
        @mousedown.stop="onDrag(hp.id, $event)"
      >
        <!-- 圆角控制点：四段弧 + 透明命中区 -->
        <template v-if="hp.id === 'cr'">
          <div h="4" w="4" absolute class="-translate-x-1/2 -translate-y-1/2" />
          <svg
            class="overlay-abs -translate-x-1/2 -translate-y-1/2"
            width="11"
            height="11"
            viewBox="0 0 11 11"
          >
            <g stroke="var(--color-accent)" stroke-width="1.5" fill="none" stroke-linecap="round">
              <path d="M 3.4 1.45 A 4.25 4.25 0 0 1 7.6 1.45" />
              <path d="M 9.55 3.4 A 4.25 4.25 0 0 1 9.55 7.6" />
              <path d="M 7.6 9.55 A 4.25 4.25 0 0 1 3.4 9.55" />
              <path d="M 1.45 7.6 A 4.25 4.25 0 0 1 1.45 3.4" />
            </g>
          </svg>
        </template>
        <!-- 旋转控制点：环形箭头 -->
        <template v-else-if="hp.id === 'rot'">
          <div h="4" w="4" absolute class="-translate-x-1/2 -translate-y-1/2" />
          <svg
            class="overlay-abs -translate-x-1/2 -translate-y-1/2"
            width="12"
            height="12"
            viewBox="0 0 12 12"
          >
            <g
              stroke="var(--color-accent)"
              stroke-width="1.4"
              fill="none"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path d="M 9.5 4 A 4 4 0 1 0 8.7 8.7" />
              <path d="M 9.5 1.5 L 9.5 4 L 7 4" />
            </g>
          </svg>
        </template>
        <!-- 8个尺寸控制点：圆形 + 方向光标 -->
        <div
          v-else
          border="~ accent"
          rounded="full"
          bg="white"
          h="2.5"
          w="2.5"
          absolute
          class="-translate-x-1/2 -translate-y-1/2"
          :style="{
            cursor: handleCursor(hp.id, shape.rotation ?? 0),
          }"
        />
      </div>
    </template>
    <!-- 直线/箭头：首尾2个控制点 -->
    <template v-else-if="shape.type === 'line' || shape.type === 'arrow'">
      <div
        v-for="hp in handles"
        :key="hp.id"
        pointer-events="auto"
        absolute
        z="100"
        :style="hp.style"
        @mousedown.stop="onDrag(hp.id, $event)"
      >
        <div
          border="~ accent"
          rounded="full"
          bg="white"
          h="2.5"
          w="2.5"
          cursor="move"
          absolute
          class="-translate-x-1/2 -translate-y-1/2"
        />
      </div>
    </template>
    <!-- 文本：只有右边中间一个控制点（调整宽度） -->
    <template v-else-if="shape.type === 'text'">
      <div
        v-for="hp in handles"
        :key="hp.id"
        pointer-events="auto"
        absolute
        z="100"
        :style="hp.style"
        @mousedown.stop="onDrag(hp.id, $event)"
      >
        <div
          border="~ accent"
          rounded="full"
          bg="white"
          h="2.5"
          w="2.5"
          cursor="ew-resize"
          absolute
          class="-translate-x-1/2 -translate-y-1/2"
        />
      </div>
    </template>
  </template>
</template>

<script setup lang="ts">
import type { Phase, Shape } from '../composables/useTypes'

export interface ShapeHandlePoint {
  id: string
  style: Record<string, string>
}

defineProps<{
  shape: Shape | null
  phase: Phase
  handles: ShapeHandlePoint[]
  handleCursor: (id: string, rotation: number) => string
}>()

const emit = defineEmits<{
  drag: [id: string, e: MouseEvent]
}>()

function onDrag(id: string, e: MouseEvent) {
  emit('drag', id, e)
}
</script>
