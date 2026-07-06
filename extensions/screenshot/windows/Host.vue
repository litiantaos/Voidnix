<template>
  <ScreenshotOverlay
    v-if="showScreenshot && screenshotData"
    :key="sessionKey"
    :initial-screenshot="screenshotData"
    @close="onScreenshotClose"
  />
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import ScreenshotOverlay from './Operation.vue'

interface WindowRect {
  x: number
  y: number
  w: number
  h: number
  owner: string
}
interface ScreenshotData {
  width: number
  height: number
  scale: number
  mouse_x: number
  mouse_y: number
  windows: WindowRect[]
}

const showScreenshot = ref(false)
const screenshotData = ref<ScreenshotData | null>(null)
// :key 用：每次新会话自增；上一次 Operation 还在挂时强制 Vue 在同一次 patch 内
// unmount+mount，消灭 false→rAF→true 翻转带来的事件 handler 真空窗口。
const sessionKey = ref(0)
let unmountTimer: ReturnType<typeof setTimeout> | null = null

async function onScreenshotClose(noRestoreFocus = false) {
  await invoke(CMD.exitScreenshotMode, { noRestoreFocus }).catch(() => {})
  // exit_impl 启动 0.15s 的 fade-out 后立即返回；这里延迟卸载 overlay，
  // 让 Vue UI 与 CALayer 背景一起淡出，避免视觉断层。
  if (unmountTimer) clearTimeout(unmountTimer)
  unmountTimer = setTimeout(() => {
    showScreenshot.value = false
    screenshotData.value = null
    unmountTimer = null
  }, 200)
}

function handleReady() {
  const data = (window as unknown as { __screenshotData?: ScreenshotData }).__screenshotData
  if (!data) return
  // 新一次截屏覆盖上一次未完的延迟卸载，避免误清新数据。
  if (unmountTimer) {
    clearTimeout(unmountTimer)
    unmountTimer = null
  }
  screenshotData.value = data
  showScreenshot.value = true
  // 自增 key：若上一个 Operation 还在挂，Vue 会在同一次 patch 内
  // unmount old + mount new，事件监听重绑不留真空帧。
  sessionKey.value += 1
}

onMounted(() => {
  window.addEventListener('__screenshot_ready', handleReady)
})

onUnmounted(() => {
  window.removeEventListener('__screenshot_ready', handleReady)
})
</script>
