<template>
  <ScreenshotOverlay
    v-if="showScreenshot && screenshotData"
    :initial-screenshot="screenshotData"
    @close="onScreenshotClose"
  />
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import ScreenshotOverlay from './Operation.vue'

interface WindowRect {
  x: number
  y: number
  w: number
  h: number
  owner: string
}
interface ScreenshotData {
  data_url: string
  width: number
  height: number
  scale: number
  mouse_x: number
  mouse_y: number
  windows: WindowRect[]
}

const showScreenshot = ref(false)
const screenshotData = ref<ScreenshotData | null>(null)

async function onScreenshotClose(forOcr = false) {
  await invoke('exit_screenshot_mode', { noRestoreFocus: forOcr }).catch(
    () => {},
  )
  showScreenshot.value = false
  screenshotData.value = null
}

function handleReady() {
  const data = (window as unknown as { __screenshotData?: ScreenshotData })
    .__screenshotData
  if (!data) return
  showScreenshot.value = false
  screenshotData.value = null
  requestAnimationFrame(() => {
    screenshotData.value = data
    showScreenshot.value = true
  })
}

onMounted(() => {
  window.addEventListener('__screenshot_ready', handleReady)
})

onUnmounted(() => {
  window.removeEventListener('__screenshot_ready', handleReady)
})
</script>
