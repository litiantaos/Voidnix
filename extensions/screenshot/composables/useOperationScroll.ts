import { nextTick, type Ref } from 'vue'
import type { Phase, Sel, Tool, Shape } from './useTypes'
import { useScrollCapture } from './useScrollCapture'

/** 滚动截屏编排：phase 切换、标注互斥清空、完成/保存/取消。 */
export function useOperationScroll(options: {
  phase: Ref<Phase>
  sel: Ref<Sel>
  hasSelection: Ref<boolean>
  shapes: Ref<Shape[]>
  selectedShapeIndex: Ref<number | null>
  activeTool: Ref<Tool>
  isDrawing: Ref<boolean>
  currentShape: Ref<Shape | null>
  rootEl: Ref<HTMLElement | undefined>
  reportToolbarRect: () => void
  doScrollCopy: (dataUrl: string) => Promise<void>
  doScrollSave: (dataUrl: string) => Promise<void>
  doCancel: () => void
}) {
  const scrollCapture = useScrollCapture()

  async function onScrollStart() {
    if (!options.hasSelection.value) return
    // 进入滚动截屏：清空已有标注（互斥）；选中/绘制态复位
    options.shapes.value = []
    options.selectedShapeIndex.value = null
    options.activeTool.value = null
    options.isDrawing.value = false
    options.currentShape.value = null
    options.phase.value = 'scroll'
    await scrollCapture.start(options.sel.value)
    if (scrollCapture.error.value) {
      options.phase.value = 'annotate'
      return
    }
    await nextTick()
    options.reportToolbarRect()
    nextTick(() => options.rootEl.value?.focus())
  }

  async function onScrollFinish() {
    try {
      const dataUrl = await scrollCapture.finish()
      if (dataUrl) {
        await options.doScrollCopy(dataUrl)
      } else {
        options.doCancel()
      }
    } catch (err) {
      console.error('[scroll] finish failed:', err)
      options.doCancel()
    }
  }

  async function onScrollSave() {
    try {
      const dataUrl = await scrollCapture.finish()
      if (dataUrl) {
        await options.doScrollSave(dataUrl)
      } else {
        options.doCancel()
      }
    } catch (err) {
      console.error('[scroll] save failed:', err)
      options.doCancel()
    }
  }

  async function onScrollCancel() {
    await scrollCapture.cancel()
    options.doCancel()
  }

  return {
    scrollCapture,
    onScrollStart,
    onScrollFinish,
    onScrollSave,
    onScrollCancel,
  }
}
