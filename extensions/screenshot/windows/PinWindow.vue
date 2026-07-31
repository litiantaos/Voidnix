<template>
  <div
    bg="transparent"
    h="screen"
    w="screen"
    select="none"
    relative
    overflow="hidden"
    class="radius-panel"
    @mouseenter="hoverActive = true"
    @mouseleave="hoverActive = false"
    @mousedown="onWindowMouseDown"
    @wheel.prevent="onWheel"
    tabindex="0"
    @keydown.esc.exact="handleClose"
    @focus="onFocus"
  >
    <!-- 图像由 native CALayer 直接贴在 contentView 下渲染，不走 <img>。 -->
    <!-- 工具栏：底部居中。每项独立 Transition——玻璃元素须为动画直接目标，
         opacity 落自身才不破坏 backdrop-filter 采样（落祖先会 group opacity 隔断），
         故共用全局 ui-popup；容器仅定位，flex 宽度变化时 -translate-x-1/2 实时居中 -->
    <div
      bottom="3"
      left="1/2"
      class="-translate-x-1/2"
      flex
      absolute
      gap="2"
      items="center"
      z="10"
      @mousedown.stop
    >
      <!-- 透明度拖动条：窗口太窄时隐藏，关闭按钮自动居中；缩放期间淡出避免跟随底边抖动 -->
      <Transition name="ui-popup">
        <div
          v-if="hoverActive && showOpacitySlider && !isScaling"
          p="x-3"
          flex
          h="7"
          shadow="lg"
          items="center"
          class="mica-bar"
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
      </Transition>

      <!-- 关闭按钮：外层 mica-bar 材质 + ghost 按钮融入（与透明度框同款） -->
      <Transition name="ui-popup">
        <div
          v-if="hoverActive && !isScaling"
          class="mica-bar shadow-lg overflow-hidden"
          flex
          h="7"
          items="center"
        >
          <BaseButton
            variant="ghost"
            class="!rounded-none"
            title="关闭 (Esc)"
            icon="i-ri-close-line"
            @click="handleClose"
          />
        </div>
      </Transition>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { getCurrentWindow, PhysicalPosition, PhysicalSize } from '@tauri-apps/api/window'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import BaseButton from '@/components/ui/BaseButton.vue'
import BaseSlider from '@/components/ui/BaseSlider.vue'

const opacity = ref(100)
const hoverActive = ref(false)
const isScaling = ref(false)
const winWidth = ref(window.innerWidth)
// 整组宽度 ≈ 滑块容器 142（80 滑块 + 32 数值 + gap 6 + p-x-3 24）+ gap 8 + 关闭按钮 28；窗口放不下整组则只留关闭按钮自动居中
const showOpacitySlider = computed(() => winWidth.value >= 180)

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
  window.addEventListener('resize', onResize)
  startHoverPolling()
})

onUnmounted(() => {
  document.removeEventListener('mousemove', onDocMouseMove)
  document.removeEventListener('mouseup', onDocMouseUp)
  window.removeEventListener('resize', onResize)
  stopHoverPolling()
  if (rafId) cancelAnimationFrame(rafId)
  if (scaleRaf) cancelAnimationFrame(scaleRaf)
  clearTimeout(scaleStopTimer)
})

function onResize() {
  winWidth.value = window.innerWidth
}

// 失焦的 NSWindow 下 WKWebView 不派发 mouseenter/leave。
// 轮询全局鼠标位置，与窗口当前 frame 比较，自行更新 hoverActive。
let hoverTimer = 0
function startHoverPolling() {
  if (hoverTimer) return
  hoverTimer = window.setInterval(checkHover, 80)
}
function stopHoverPolling() {
  if (hoverTimer) {
    clearInterval(hoverTimer)
    hoverTimer = 0
  }
}
async function checkHover() {
  if (!win || isDragging) return
  try {
    const [mp, pos, size] = await Promise.all([
      invoke<[number, number]>(CMD.pinGlobalMouse),
      win.outerPosition() as Promise<PhysicalPosition>,
      win.outerSize() as Promise<PhysicalSize>,
    ])
    const scale = window.devicePixelRatio || 1
    // CSS 像素，窗口物理像素 / scale
    const left = pos.x / scale
    const top = pos.y / scale
    const right = left + size.width / scale
    const bottom = top + size.height / scale
    hoverActive.value = mp[0] >= left && mp[0] <= right && mp[1] >= top && mp[1] <= bottom
  } catch {
    /* ignore */
  }
}

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

// 滚轮缩放：维护绝对 scaleLevel（1=原图），每帧的 wheel 增量转 factor 乘进 scaleLevel，
// 发送绝对 scale 给 Rust——Rust 用 orig×scale 一次算出尺寸，比例恒等于原图，
// 不读当前 frame 迭代相乘（NSWindow 可能规整 frame 致比例漂移）。
const ZOOM_SENS = 600 // deltaY 敏感度：100（鼠标一档）→ exp(100/600)≈1.18
const ZOOM_MIN = 0.2 // 最小缩放（原图 20%），防止缩到无法辨识
const ZOOM_MAX = 8 // 最大缩放（原图 800%），再大像素化严重且无意义
let scaleLevel = 1
let deltaAccum = 0
let scaleRaf = 0
let scaleStopTimer = 0
function onWheel(e: WheelEvent) {
  deltaAccum += e.deltaY
  // 缩放期间隐藏控件，停止 220ms 后恢复（控件淡入淡出走 ui-popup）
  isScaling.value = true
  clearTimeout(scaleStopTimer)
  scaleStopTimer = window.setTimeout(() => {
    isScaling.value = false
  }, 220)
  if (!scaleRaf) scaleRaf = requestAnimationFrame(flushScale)
}
function flushScale() {
  scaleRaf = 0
  if (Math.abs(deltaAccum) < 1) return
  // deltaY<0（上滚）→ factor>1 放大；下滚反之
  const factor = Math.exp(-deltaAccum / ZOOM_SENS)
  deltaAccum = 0
  scaleLevel = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, scaleLevel * factor))
  invoke(CMD.scalePinWindow, { scale: scaleLevel }).catch(() => {})
}

async function onOpacityChange() {
  await invoke(CMD.setPinWindowOpacity, {
    opacity: opacity.value / 100,
  }).catch((e: unknown) => console.error('[screenshot] set opacity failed:', e))
}

async function handleClose() {
  if (!win) return
  await invoke(CMD.restorePinFocus, { window: win }).catch(() => {})
  await win.close()
}
</script>
