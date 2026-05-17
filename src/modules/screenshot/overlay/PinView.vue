<template>
  <div
    class="bg-transparent h-screen w-screen select-none relative overflow-hidden"
    :style="{ borderRadius: '10px' }"
    @mouseenter="hoverActive = true"
    @mouseleave="hoverActive = false"
    tabindex="0"
    @keydown.esc.exact="handleClose"
    @focus="onFocus"
  >
    <!-- 图片：仅图片区域是 drag region，工具栏不会触发窗口拖动 -->
    <img
      v-if="imageUrl"
      :src="imageUrl"
      class="h-full w-full block pointer-events-none object-cover"
      :style="{ borderRadius: '10px' }"
      alt=""
      data-tauri-drag-region
    />
    <!-- 全屏 drag region：图片底层之上、工具栏之下 -->
    <div
      class="inset-0 absolute"
      data-tauri-drag-region
    />

    <!-- 顶部工具栏：阻止 drag region 冒泡 -->
    <Transition name="bar">
      <div
        v-if="hoverActive"
        class="px-2 py-1 border border-black/15 rounded-lg bg-surface/90 flex gap-1 shadow-lg items-center right-2 top-2 absolute z-10 backdrop-blur-md"
        @mousedown.stop
        @mouseenter.stop
        @mousemove.stop
      >
        <!-- 透明度滑块 -->
        <span class="i-ri-contrast-line text-xs text-tx-muted shrink-0" />
        <input
          v-model.number="opacity"
          type="range"
          min="20"
          max="100"
          step="1"
          class="accent-accent h-1 w-20 cursor-ew-resize"
          @input="onOpacityChange"
          @mousedown.stop
          @mouseenter.stop
          @mousemove.stop
        />
        <!-- 关闭按钮 -->
        <button
          class="text-xs text-tx-secondary rounded flex h-5 w-5 items-center justify-center hover:text-tx-primary hover:bg-black/8"
          title="关闭 (Esc)"
          @click="handleClose"
          @mousedown.stop
        >
          <span class="i-ri-close-line text-sm" />
        </button>
      </div>
    </Transition>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { convertFileSrc, invoke } from '@tauri-apps/api/core'

const imageUrl = ref('')
const opacity = ref(100)
const hoverActive = ref(false)

let win: ReturnType<typeof getCurrentWindow> | null = null

onMounted(() => {
  win = getCurrentWindow()
  // 自动聚焦窗口让 esc 生效
  setTimeout(() => {
    const root = document.querySelector('[tabindex="0"]') as HTMLElement | null
    root?.focus()
  }, 100)

  const params = new URLSearchParams(window.location.search)
  const imgPath = params.get('img')
  if (imgPath) {
    imageUrl.value = convertFileSrc(imgPath) + `?t=${Date.now()}`
  }
})

function onFocus() {
  // 窗口获得焦点时确保 root 元素也获得焦点，让 esc 键生效
  setTimeout(() => {
    const root = document.querySelector('[tabindex="0"]') as HTMLElement | null
    root?.focus()
  }, 10)
}

async function onOpacityChange() {
  await invoke('set_pin_window_opacity', { opacity: opacity.value / 100 }).catch(() => {})
}

async function handleClose() {
  if (win) await win.close()
}
</script>

<style scoped>
.bar-enter-active,
.bar-leave-active {
  transition: opacity 0.15s ease, transform 0.15s ease;
}
.bar-enter-from,
.bar-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}

input[type='range'] {
  appearance: none;
  background: rgba(0, 0, 0, 0.1);
  border-radius: 9999px;
}
input[type='range']::-webkit-slider-thumb {
  appearance: none;
  width: 12px;
  height: 12px;
  border-radius: 50%;
  background: #3b82f6;
  cursor: ew-resize;
  border: 1px solid white;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.2);
}
</style>
