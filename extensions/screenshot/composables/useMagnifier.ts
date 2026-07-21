import { ref, computed, watch, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import type { ScreenshotData, Sel } from './useTypes'
import { MAGNIFIER_SIZE, MAGNIFIER_ZOOM, MAGNIFIER_OFFSET, handleAbsolutePos } from './useTypes'

/** picker JPEG 与 enter 并行编码；主屏 Retina 常晚于 Vue mount，需轮询就绪。 */
const PICKER_RETRY_MS = 40
const PICKER_RETRY_MAX = 40 // ~1.6s

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
  let loadGen = 0
  let retryTimer: ReturnType<typeof setTimeout> | null = null

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

  function clearRetry() {
    if (retryTimer != null) {
      clearTimeout(retryTimer)
      retryTimer = null
    }
  }

  function paintMagnifier(cx: number, cy: number) {
    const canvas = magnifierCanvas.value
    if (!canvas || !bgImage.value) return
    const ctx = canvas.getContext('2d')
    if (!ctx) return
    const sc = options.dpr.value
    const canvasSize = MAGNIFIER_SIZE * sc
    const half = MAGNIFIER_SIZE / MAGNIFIER_ZOOM / 2
    // hover 控制点时锚定控制点位置（放大窗内容固定，不跟随指针微动）；
    // 与 magnifierStyle 的位置锚定同源，确保框与画面都锁在控制点上。
    const hid = options.hoveredHandle.value
    let ax = cx
    let ay = cy
    if (hid) {
      const p = handleAbsolutePos(hid, options.sel.value)
      ax = p.x
      ay = p.y
    }
    ctx.clearRect(0, 0, canvasSize, canvasSize)
    ctx.imageSmoothingEnabled = false
    ctx.drawImage(
      bgImage.value,
      (ax - half) * sc,
      (ay - half) * sc,
      (MAGNIFIER_SIZE / MAGNIFIER_ZOOM) * sc,
      (MAGNIFIER_SIZE / MAGNIFIER_ZOOM) * sc,
      0,
      0,
      canvasSize,
      canvasSize,
    )
    const px = ctx.getImageData(Math.floor(canvasSize / 2), Math.floor(canvasSize / 2), 1, 1).data
    pickedColor.value =
      '#' + [px[0], px[1], px[2]].map((v) => v.toString(16).padStart(2, '0').toUpperCase()).join('')
  }

  function bindPickerDataUrl(dataUrl: string, gen: number) {
    const img = new Image()
    img.onload = () => {
      if (gen !== loadGen) return
      bgImage.value = img
      // canvas 可能尚未挂上（v-if showMagnifier）；挂上后 setMagnifierCanvas 会再 paint
      paintMagnifier(crossX.value, crossY.value)
    }
    img.onerror = () => {
      if (gen !== loadGen) return
      scheduleRetry(gen, 0)
    }
    img.src = dataUrl
  }

  function scheduleRetry(gen: number, attempt: number) {
    if (gen !== loadGen) return
    if (attempt >= PICKER_RETRY_MAX) return
    clearRetry()
    retryTimer = setTimeout(() => {
      retryTimer = null
      if (gen !== loadGen) return
      void tryLoadOnce(gen, attempt + 1)
    }, PICKER_RETRY_MS)
  }

  async function tryLoadOnce(gen: number, attempt: number) {
    if (gen !== loadGen) return
    try {
      const dataUrl = await invoke<string>(CMD.readPickerImage)
      if (gen !== loadGen) return
      if (!dataUrl) {
        scheduleRetry(gen, attempt)
        return
      }
      bindPickerDataUrl(dataUrl, gen)
    } catch {
      scheduleRetry(gen, attempt)
    }
  }

  async function loadPickerImage() {
    clearRetry()
    const gen = ++loadGen
    bgImage.value = null
    await tryLoadOnce(gen, 0)
  }

  function updateMagnifier(cx: number, cy: number) {
    paintMagnifier(cx, cy)
  }

  // canvas 已挂载时控制点间切换（A→B）需重绘：annotate 阶段 mousemove 不调
  // updateMagnifier（仅 select/resize 调），没有这层 watch 放大窗会停在上一个控制点画面。
  const stopHoverWatch = watch(
    () => options.hoveredHandle.value,
    () => {
      if (options.hoveredHandle.value && magnifierCanvas.value && bgImage.value) {
        paintMagnifier(crossX.value, crossY.value)
      }
    },
  )

  /** canvas ref 回调：bg 已就绪但 canvas 后挂时补绘一帧。 */
  function setMagnifierCanvas(el: HTMLCanvasElement | null | undefined) {
    magnifierCanvas.value = el ?? undefined
    if (el && bgImage.value) {
      paintMagnifier(crossX.value, crossY.value)
    }
  }

  function dispose() {
    clearRetry()
    loadGen++
    stopHoverWatch()
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
    setMagnifierCanvas,
    dispose,
  }
}
