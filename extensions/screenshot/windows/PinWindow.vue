<template>
  <div
    class="bg-transparent h-screen w-screen select-none relative overflow-hidden"
    :style="{ borderRadius: '10px' }"
    @mouseenter="hoverActive = true"
    @mouseleave="hoverActive = false"
    @mousedown="onWindowMouseDown"
    tabindex="0"
    @keydown.esc.exact="handleClose"
    @focus="onFocus"
  >
    <!-- 图像由 native CALayer 直接贴在 contentView 下渲染，不走 <img>。 -->
    <Transition name="bar">
      <div
        v-if="hoverActive"
        class="flex gap-2 items-center right-2 top-2 absolute z-10"
        @mousedown.stop
      >
        <!-- 透明度拖动条 -->
        <div
          class="px-2 border border-black/10 rounded-md bg-surface/90 flex h-7 shadow-lg items-center"
          title="透明度"
        >
          <BaseSlider
            v-model="opacity"
            :min="20"
            :max="100"
            width="80px"
            value-width="32px"
            suffix="%"
            @update:model-value="onOpacityChange"
          />
        </div>

        <!-- 关闭按钮 -->
        <BaseButton
          class="shadow-lg !border !border-black/10 !border-solid !bg-surface/90 hover:!bg-surface"
          title="关闭 (Esc)"
          icon="i-ri-close-line"
          @click="handleClose"
        />
      </div>
    </Transition>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { getCurrentWindow, PhysicalPosition } from '@tauri-apps/api/window'
import { invoke } from '@tauri-apps/api/core'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseSlider from '@/components/ui/BaseSlider.vue'

const opacity = ref(100)
const hoverActive = ref(false)

let win: ReturnType<typeof getCurrentWindow> | null = null

// 整窗拖动：无装饰 NSWindow 没有原生可拖区域，WKWebView 又拦截 mouseDown 让
// movableByWindowBackground 失效；Tauri 内置 startDragging 走 IPC 异步，
// LSUIElement 应用下 NSApp.currentEvent 可能已过期导致调用失败。
// 这里用 JS 监听 mousedown + 全局 mousemove，按屏幕坐标差直接 setPosition，
// rAF 节流避免 IPC 过载。工具栏 @mousedown.stop 让按钮 click 不被拖动逻辑接管。
let dragScale = 1
let dragOrigScreen = { x: 0, y: 0 }
let dragOrigWindow = { x: 0, y: 0 }
let isDragging = false
let rafId = 0
let pendingPos: { x: number; y: number } | null = null

onMounted(() => {
  win = getCurrentWindow()
  dragScale = window.devicePixelRatio || 1
  const root = document.querySelector('[tabindex="0"]') as HTMLElement | null
  root?.focus()
})

onUnmounted(() => {
  document.removeEventListener('mousemove', onDocMouseMove)
  document.removeEventListener('mouseup', onDocMouseUp)
  if (rafId) cancelAnimationFrame(rafId)
})

function onFocus() {
  const root = document.querySelector('[tabindex="0"]') as HTMLElement | null
  root?.focus()
}

async function onWindowMouseDown(e: MouseEvent) {
  if (e.button !== 0 || !win) return
  e.preventDefault()
  try {
    const pos = await win.outerPosition()
    dragOrigWindow = { x: pos.x, y: pos.y }
    dragOrigScreen = { x: e.screenX, y: e.screenY }
    isDragging = true
    document.addEventListener('mousemove', onDocMouseMove)
    document.addEventListener('mouseup', onDocMouseUp, { once: true })
  } catch {
    isDragging = false
  }
}

function onDocMouseMove(e: MouseEvent) {
  if (!isDragging) return
  // screenX/Y 为 CSS 像素（左上原点），outerPosition 为物理像素（左上原点）
  pendingPos = {
    x: Math.round(dragOrigWindow.x + (e.screenX - dragOrigScreen.x) * dragScale),
    y: Math.round(dragOrigWindow.y + (e.screenY - dragOrigScreen.y) * dragScale),
  }
  if (!rafId) rafId = requestAnimationFrame(applyPos)
}

function applyPos() {
  rafId = 0
  if (!isDragging || !win || !pendingPos) return
  win.setPosition(new PhysicalPosition(pendingPos.x, pendingPos.y)).catch(() => {})
  pendingPos = null
}

function onDocMouseUp() {
  isDragging = false
  if (rafId) {
    cancelAnimationFrame(rafId)
    rafId = 0
  }
  document.removeEventListener('mousemove', onDocMouseMove)
}

async function onOpacityChange() {
  await invoke('set_pin_window_opacity', {
    opacity: opacity.value / 100,
  }).catch(() => {})
}

async function handleClose() {
  if (win) await win.close()
}
</script>

<style scoped>
.bar-enter-active,
.bar-leave-active {
  transition:
    opacity 0.15s ease,
    transform 0.15s ease;
}
.bar-enter-from,
.bar-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
</style>
