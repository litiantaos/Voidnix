import { ref, computed, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import type { ScreenshotData, Sel } from './useTypes'
import { MAGNIFIER_SIZE, MAGNIFIER_ZOOM, MAGNIFIER_OFFSET, handleAbsolutePos } from './useTypes'

export function useMagnifier(options: {
  initialScreenshot: ScreenshotData
  screenW: Ref<number>
  screenH: Ref<number>
  dpr: Ref<number>
  sel: Ref<Sel>
  hoveredHandle: Ref<string | null>
}) {
  const magnifierCanvas = ref<HTMLCanvasElement>()
  const pickedColor = ref('#000000')
  const crossX = ref(options.initialScreenshot.mouse_x)
  const crossY = ref(options.initialScreenshot.mouse_y)
  const bgImage = ref<HTMLImageElement | null>(null)

  const magnifierStyle = computed(() => {
    const totalH = MAGNIFIER_SIZE + 20
    // hover 控制点时锚定控制点几何位置（放大窗固定不随鼠标微动）；否则跟随鼠标
    const hid = options.hoveredHandle.value
    const { x: ax, y: ay } = hid
      ? handleAbsolutePos(hid, options.sel.value)
      : { x: crossX.value, y: crossY.value }
    // 默认左下角方位（与 select 一致），仅边界 clamp 时翻转
    let left = ax - MAGNIFIER_SIZE - MAGNIFIER_OFFSET
    let top = ay + MAGNIFIER_OFFSET
    if (left < 0) left = ax + MAGNIFIER_OFFSET
    if (top + totalH > options.screenH.value) top = ay - totalH - MAGNIFIER_OFFSET
    return { left: `${left}px`, top: `${top}px`, width: `${MAGNIFIER_SIZE}px` }
  })

  async function loadPickerImage() {
    try {
      const dataUrl = await invoke<string>(CMD.readPickerImage)
      if (!dataUrl) return
      const img = new Image()
      img.onload = () => {
        bgImage.value = img
        // 加载完成后立即绘制一次，不等鼠标移动（否则首帧空白直到 mousemove）
        updateMagnifier(crossX.value, crossY.value)
      }
      img.src = dataUrl
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
    const px = ctx.getImageData(Math.floor(canvasSize / 2), Math.floor(canvasSize / 2), 1, 1).data
    pickedColor.value =
      '#' + [px[0], px[1], px[2]].map((v) => v.toString(16).padStart(2, '0')).join('')
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
