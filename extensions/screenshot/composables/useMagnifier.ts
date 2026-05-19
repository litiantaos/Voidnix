import { ref, computed, type Ref } from 'vue'
import type { ScreenshotData } from './useTypes'
import { MAGNIFIER_SIZE, MAGNIFIER_ZOOM, MAGNIFIER_OFFSET } from './useTypes'

export function useMagnifier(options: {
  initialScreenshot: ScreenshotData
  screenW: Ref<number>
  screenH: Ref<number>
  dpr: Ref<number>
}) {
  const magnifierCanvas = ref<HTMLCanvasElement>()
  const pickedColor = ref('#000000')
  const crossX = ref(options.initialScreenshot.mouse_x)
  const crossY = ref(options.initialScreenshot.mouse_y)
  const bgImage = ref<HTMLImageElement | null>(null)

  const magnifierStyle = computed(() => {
    const totalH = MAGNIFIER_SIZE + 20
    let left = crossX.value - MAGNIFIER_SIZE - MAGNIFIER_OFFSET
    let top = crossY.value + MAGNIFIER_OFFSET
    if (left < 0) left = crossX.value + MAGNIFIER_OFFSET
    if (top + totalH > options.screenH.value)
      top = crossY.value - totalH - MAGNIFIER_OFFSET
    return { left: `${left}px`, top: `${top}px`, width: `${MAGNIFIER_SIZE}px` }
  })

  async function loadPickerImage() {
    const pickerPath = options.initialScreenshot.data_url
    if (!pickerPath) return
    try {
      const { convertFileSrc } = await import('@tauri-apps/api/core')
      const url = convertFileSrc(pickerPath) + `?t=${Date.now()}`
      for (let i = 0; i < 20; i++) {
        try {
          const resp = await fetch(url)
          if (resp.ok) {
            const blob = await resp.blob()
            const objectUrl = URL.createObjectURL(blob)
            const img = new Image()
            img.onload = () => {
              bgImage.value = img
            }
            img.src = objectUrl
            return
          }
        } catch {
          /* 继续等待 */
        }
        await new Promise((r) => setTimeout(r, 50))
      }
    } catch {
      /* 放大镜不可用 */
    }
  }

  function updateMagnifier(cx: number, cy: number) {
    const canvas = magnifierCanvas.value
    if (!canvas || !bgImage.value) return
    const ctx = canvas.getContext('2d')
    if (!ctx) return
    const sc = options.dpr.value,
      canvasSize = MAGNIFIER_SIZE * sc
    const half = MAGNIFIER_SIZE / MAGNIFIER_ZOOM / 2
    ctx.clearRect(0, 0, canvasSize, canvasSize)
    ctx.imageSmoothingEnabled = false
    ctx.drawImage(
      bgImage.value,
      (cx - half) * sc,
      (cy - half) * sc,
      (MAGNIFIER_SIZE / MAGNIFIER_ZOOM) * sc,
      (MAGNIFIER_SIZE / MAGNIFIER_ZOOM) * sc,
      0,
      0,
      canvasSize,
      canvasSize,
    )
    const px = ctx.getImageData(
      Math.floor(canvasSize / 2),
      Math.floor(canvasSize / 2),
      1,
      1,
    ).data
    pickedColor.value =
      '#' +
      [px[0], px[1], px[2]].map((v) => v.toString(16).padStart(2, '0')).join('')
  }

  return {
    magnifierCanvas,
    pickedColor,
    crossX,
    crossY,
    bgImage,
    magnifierStyle,
    loadPickerImage,
    updateMagnifier,
  }
}
