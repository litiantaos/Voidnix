import { nextTick, onMounted, onUnmounted, watch, type Ref, type ComputedRef } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import type { Phase, Shape, Tool, BlurMode, ScreenshotData, TextRegion } from './useTypes'
import { wrapText } from './wrapText'

/** 覆盖层挂载/卸载 + 标注参数 watch（字号/线宽/模糊/文本检测预热）。 */
export function useOperationLifecycle(options: {
  initialScreenshot: ScreenshotData
  rootEl: Ref<HTMLElement | undefined>
  annotateCanvas: Ref<HTMLCanvasElement | undefined>
  phase: Ref<Phase>
  crossX: Ref<number>
  crossY: Ref<number>
  hoverWindow: Ref<unknown>
  findWindowAt: (x: number, y: number) => unknown
  updateMagnifier: (x: number, y: number) => void
  loadPickerImage: () => void
  onKeyDown: (e: KeyboardEvent) => void
  redraw: (preview?: Shape | null) => void
  selectedShape: ComputedRef<Shape | null>
  shapes: Ref<Shape[]>
  selectedShapeIndex: Ref<number | null>
  annotBlurAmount: Ref<number>
  annotFontSize: Ref<number>
  annotLineWidth: Ref<number>
  annotBlurMode: Ref<BlurMode>
  activeTool: Ref<Tool>
  textInput: Ref<{ visible: boolean; editingIndex: number | null }>
  autoResizeTextInput: () => void
  detectText: () => void
  textRegions: Ref<TextRegion[]>
}) {
  function refocus() {
    options.rootEl.value?.focus()
  }

  function setCrossPosition(cx: number, cy: number) {
    if (options.rootEl.value) {
      options.rootEl.value.style.setProperty('--cross-x', `${cx}px`)
      options.rootEl.value.style.setProperty('--cross-y', `${cy}px`)
    }
    options.crossX.value = cx
    options.crossY.value = cy
    if (options.phase.value === 'select') options.updateMagnifier(cx, cy)
  }

  function onWindowKeyDown(e: KeyboardEvent) {
    if (e.key !== 'Escape') return
    // 根 div 已处理过的不再重复；仅 activeElement 非根时兜底
    if (document.activeElement === options.rootEl.value) return
    options.onKeyDown(e)
  }

  onMounted(() => {
    // 监听已挂 DOM 后解锁窗口鼠标并淡入
    invoke(CMD.screenshotOverlayReady).catch(() => {})

    const { mouse_x, mouse_y } = options.initialScreenshot
    if (options.rootEl.value) {
      options.rootEl.value.style.setProperty('--cross-x', `${mouse_x}px`)
      options.rootEl.value.style.setProperty('--cross-y', `${mouse_y}px`)
    }
    options.hoverWindow.value = options.findWindowAt(mouse_x, mouse_y)
    nextTick(() => {
      options.updateMagnifier(mouse_x, mouse_y)
      refocus()
    })
    options.loadPickerImage()
    window.addEventListener('focus', refocus)
    window.addEventListener('keydown', onWindowKeyDown)
    ;(
      window as unknown as { __setScreenshotCross?: (x: number, y: number) => void }
    ).__setScreenshotCross = setCrossPosition
  })

  onUnmounted(() => {
    window.removeEventListener('focus', refocus)
    window.removeEventListener('keydown', onWindowKeyDown)
    delete (window as unknown as { __setScreenshotCross?: unknown }).__setScreenshotCross
  })

  watch(options.annotateCanvas, () => {
    if (options.annotateCanvas.value) options.redraw()
  })

  watch(options.annotBlurAmount, (v) => {
    const s = options.selectedShape.value
    if (s && s.type === 'blur') {
      s.blurAmount = v
      options.redraw()
    }
  })

  watch(options.annotFontSize, (v) => {
    const s = options.selectedShape.value
    if (s && s.type === 'text') {
      s.fontSize = v
      if (s.text) {
        const font = `${v}px -apple-system, sans-serif`
        s.textLines = wrapText(s.text, s.textWidth ?? 160, font)
      }
      options.redraw()
      const ti = options.textInput.value
      if (ti.visible && ti.editingIndex === options.selectedShapeIndex.value) {
        nextTick(() => options.autoResizeTextInput())
      }
    }
  })

  watch(options.annotLineWidth, (v) => {
    const s = options.selectedShape.value
    if (s && (s.type === 'rect' || s.type === 'line' || s.type === 'arrow')) {
      s.lineWidth = v
      options.redraw()
    }
  })

  watch(options.selectedShapeIndex, (idx) => {
    if (idx === null) return
    const s = options.shapes.value[idx]
    if (!s) return
    if (s.type === 'blur') {
      if (typeof s.blurAmount === 'number') options.annotBlurAmount.value = s.blurAmount
      const mode = s.blurMode ?? 'selection'
      options.annotBlurMode.value = mode
      if (mode === 'text') options.detectText()
    }
    if (s.type === 'text' && typeof s.fontSize === 'number') {
      options.annotFontSize.value = s.fontSize
    }
    if (s.type === 'rect' || s.type === 'line' || s.type === 'arrow') {
      options.annotLineWidth.value = s.lineWidth
    }
  })

  watch(options.annotBlurMode, (mode) => {
    const s = options.selectedShape.value
    if (s && s.type === 'blur') {
      s.blurMode = mode
      options.redraw()
    }
    if (mode === 'text') options.detectText()
  })

  watch(options.activeTool, (tool) => {
    if (tool === 'blur' && options.annotBlurMode.value === 'text') {
      options.detectText()
    }
  })

  // 进入标注即预热文本检测，把 Swift 冷启动放到空闲段
  watch(
    options.phase,
    (p) => {
      if (p === 'annotate') options.detectText()
    },
    { immediate: true },
  )

  watch(options.textRegions, () => options.redraw(), { deep: true })

  return { refocus, setCrossPosition }
}
