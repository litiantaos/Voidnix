<template>
  <img
    v-if="previewDataUrl"
    :src="previewDataUrl"
    class="rounded-md block absolute z-50"
    :style="imgStyle"
    draggable="false"
    @mousedown.stop
    @wheel.stop
  />
</template>

<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{
  previewDataUrl: string
  previewWidth: number
  previewHeight: number
  screenWidth: number
  screenHeight: number
  sel: { x: number; y: number; w: number; h: number }
  dpr: number
}>()

const SCREEN_MARGIN = 24

// 图片：垂直居中，max-width 不超过屏幕剩余宽度的合理比例（这里取固定 360px），
// max-height = 屏幕高度 - 2*SCREEN_MARGIN，等同于左右距屏的边距。
// 浏览器会按图片自身比例 contain 进 maxW × maxH，不裁剪、不变形。
// 选区在屏幕右半时贴左，否则贴右。
const imgStyle = computed(() => {
  const selCenter = props.sel.x + props.sel.w / 2
  const showOnLeft = selCenter > props.screenWidth / 2
  const maxH = props.screenHeight - SCREEN_MARGIN * 2
  const base: Record<string, string> = {
    maxWidth: '360px',
    maxHeight: `${maxH}px`,
    top: '50%',
    transform: 'translateY(-50%)',
  }
  if (showOnLeft) {
    base.left = `${SCREEN_MARGIN}px`
  } else {
    base.right = `${SCREEN_MARGIN}px`
  }
  return base
})
</script>
