import { nextTick, type Ref } from 'vue'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import type { Sel, Shape, Tool, Phase, WindowRect } from './useTypes'
import { DRAG_THRESHOLD } from './useTypes'

export function useOverlayEvents(options: {
  // 选区相关
  sel: Ref<Sel>
  dragStart: Ref<{ x: number; y: number }>
  isDragging: Ref<boolean>
  pendingDrag: Ref<boolean>
  hoverWindow: Ref<WindowRect | null>
  selResizeHandle: Ref<string | null>
  hasSelection: Ref<boolean>
  findWindowAt: (cx: number, cy: number) => WindowRect | null
  applySelResize: (cx: number, cy: number) => void
  isInsideSel: (cx: number, cy: number) => boolean

  // 标注相关
  phase: Ref<Phase>
  activeTool: Ref<Tool>
  annotColor: Ref<string>
  annotLineWidth: Ref<number>
  annotBlurAmount: Ref<number>
  shapes: Ref<Shape[]>
  currentShape: Ref<Shape | null>
  isDrawing: Ref<boolean>
  drawStart: Ref<{ x: number; y: number }>
  selectedShapeIndex: Ref<number | null>
  isHoveringSelectedShape: Ref<boolean>
  isDraggingShape: Ref<boolean>
  shapeDragStart: Ref<{ mx: number; my: number; x1: number; y1: number; x2: number; y2: number }>

  // 控制点相关
  draggingShapeHandle: Ref<string | null>
  applyShapeHandleDrag: (cx: number, cy: number, shiftKey: boolean) => void

  // 文字输入相关
  textInput: Ref<{
    visible: boolean
    value: string
    x: number
    y: number
    canvasX: number
    canvasY: number
    width: number
    editingIndex: number | null
  }>
  isDraggingTextInput: Ref<boolean>
  textInputDragStart: Ref<{ mx: number; my: number; canvasX: number; canvasY: number }>
  textInputPendingDrag: Ref<boolean>
  openTextInput: (screenX: number, screenY: number, editIndex?: number) => void
  commitText: () => void
  cancelText: () => void

  // 放大镜相关
  crossX: Ref<number>
  crossY: Ref<number>
  updateMagnifier: (cx: number, cy: number) => void
  pickedColor: Ref<string>

  // 屏幕信息
  screenW: Ref<number>
  screenH: Ref<number>
  dpr: Ref<number>

  // 绘制相关
  redraw: (preview?: Shape | null) => void

  // 操作相关
  doCopy: () => Promise<void>
  doSave: () => Promise<void>
  doOcr: () => Promise<void>
  doPin: () => Promise<void>
  doCancel: (forOcr?: boolean) => void

  // DOM
  rootEl: Ref<HTMLElement | undefined>
}) {
  // ── 命中测试 ──────────────────────────────────────────────
function hitTestShape(shape: Shape, px: number, py: number): boolean {
    const HIT = 8
    const { type, x1, y1, x2, y2, textWidth } = shape

    if (type === 'rect') {
      let lpx = px,
        lpy = py
      const r = shape.rotation ?? 0
      if (r !== 0) {
        const ccx = (x1 + x2) / 2,
          ccy = (y1 + y2) / 2
        const dx = px - ccx,
          dy = py - ccy
        const cos = Math.cos(-r),
          sin = Math.sin(-r)
        lpx = ccx + dx * cos - dy * sin
        lpy = ccy + dx * sin + dy * cos
      }
      const lx = Math.min(x1, x2),
        rx = Math.max(x1, x2)
      const ty = Math.min(y1, y2),
        by = Math.max(y1, y2)
      const onH = lpx >= lx - HIT && lpx <= rx + HIT
      const onV = lpy >= ty - HIT && lpy <= by + HIT
      return (
        (onH && (Math.abs(lpy - ty) < HIT || Math.abs(lpy - by) < HIT)) ||
        (onV && (Math.abs(lpx - lx) < HIT || Math.abs(lpx - rx) < HIT))
      )
    }
    if (type === 'blur') {
      const lx = Math.min(x1, x2),
        rx = Math.max(x1, x2),
        ty = Math.min(y1, y2),
        by = Math.max(y1, y2)
      return px >= lx && px <= rx && py >= ty && py <= by
    }
    if (type === 'line' || type === 'arrow') {
      const dx = x2 - x1,
        dy = y2 - y1,
        len2 = dx * dx + dy * dy
      if (len2 === 0) return Math.hypot(px - x1, py - y1) < HIT
      const t = Math.max(0, Math.min(1, ((px - x1) * dx + (py - y1) * dy) / len2))
      return Math.hypot(px - (x1 + t * dx), py - (y1 + t * dy)) < HIT
    }
    if (type === 'text') {
      const w = textWidth ?? 160
      const fontSize = Math.max(14, shape.lineWidth * 6)
      const lines =
        shape.textLines ?? (shape.text ? shape.text.split('\n') : [''])
      const lineH = Math.round(fontSize * 1.3)
      const h = lineH * lines.length
      return (
        px >= x1 - HIT &&
        px <= x1 + w + HIT &&
        py >= y1 - HIT &&
        py <= y1 + h + HIT
      )
    }
    return false
  }

  function findShapeAt(canvasX: number, canvasY: number): number {
    for (let i = options.shapes.value.length - 1; i >= 0; i -= 1) {
      if (hitTestShape(options.shapes.value[i], canvasX, canvasY)) return i
    }
    return -1
  }

  // ── 鼠标事件 ──────────────────────────────────────────────
  function onMouseDown(e: MouseEvent) {
    if (e.button !== 0) return
    const { clientX: cx, clientY: cy } = e

    if (options.textInput.value.visible) {
      options.commitText()
      return
    }

    if (options.phase.value === 'select') {
      options.pendingDrag.value = true
      options.dragStart.value = { x: cx, y: cy }
      return
    }

    if (!options.isInsideSel(cx, cy)) return
    const canvasX = cx - options.sel.value.x
    const canvasY = cy - options.sel.value.y

    const hitIdx = findShapeAt(canvasX, canvasY)
    if (hitIdx >= 0) {
      const s = options.shapes.value[hitIdx]
      options.selectedShapeIndex.value = hitIdx
      options.activeTool.value = s.type
      if (s.type === 'text') {
        options.openTextInput(options.sel.value.x + s.x1, options.sel.value.y + s.y1, hitIdx)
        return
      }
      options.isDraggingShape.value = true
      options.shapeDragStart.value = {
        mx: cx,
        my: cy,
        x1: s.x1,
        y1: s.y1,
        x2: s.x2,
        y2: s.y2,
      }
      return
    }

    options.selectedShapeIndex.value = null
    options.isHoveringSelectedShape.value = false

    if (options.activeTool.value) {
      if (options.activeTool.value === 'text') {
        options.openTextInput(cx, cy)
        return
      }
      options.isDrawing.value = true
      options.drawStart.value = { x: canvasX, y: canvasY }
      options.currentShape.value = {
        type: options.activeTool.value,
        x1: canvasX,
        y1: canvasY,
        x2: canvasX,
        y2: canvasY,
        color: options.annotColor.value,
        lineWidth: options.annotLineWidth.value,
        cornerRadius: options.activeTool.value === 'rect' ? 0 : undefined,
        blurAmount:
          options.activeTool.value === 'blur' ? options.annotBlurAmount.value : undefined,
      }
      return
    }

    options.isDragging.value = true
    options.dragStart.value = { x: cx - options.sel.value.x, y: cy - options.sel.value.y }
  }

  function onMouseMove(e: MouseEvent) {
    const { clientX: cx, clientY: cy } = e

    if (options.rootEl.value) {
      options.rootEl.value.style.setProperty('--cross-x', `${cx}px`)
      options.rootEl.value.style.setProperty('--cross-y', `${cy}px`)
    }
    options.crossX.value = cx
    options.crossY.value = cy

    if (options.phase.value === 'select' && !options.isDragging.value && !options.pendingDrag.value)
      options.updateMagnifier(cx, cy)

    if (options.textInputPendingDrag.value || options.isDraggingTextInput.value) {
      const dx = cx - options.textInputDragStart.value.mx
      const dy = cy - options.textInputDragStart.value.my
      if (options.textInputPendingDrag.value) {
        if (Math.abs(dx) < 3 && Math.abs(dy) < 3) return
        options.textInputPendingDrag.value = false
        options.isDraggingTextInput.value = true
      }
      const idx = options.textInput.value.editingIndex
      if (idx === null) return
      const s = options.shapes.value[idx]
      const tw = s.textWidth ?? 160
      const fontSize = Math.max(14, s.lineWidth * 6)
      const lines = s.textLines ?? (s.text ? s.text.split('\n') : [''])
      const lineH = Math.round(fontSize * 1.3)
      const th = lineH * lines.length
      const newX = options.textInputDragStart.value.canvasX + dx
      const newY = options.textInputDragStart.value.canvasY + dy
      s.x1 = Math.max(0, Math.min(options.sel.value.w - tw, newX))
      s.y1 = Math.max(0, Math.min(options.sel.value.h - th, newY))
      s.x2 = s.x1
      s.y2 = s.y1
      options.textInput.value.canvasX = s.x1
      options.textInput.value.canvasY = s.y1
      options.textInput.value.x = options.sel.value.x + s.x1
      options.textInput.value.y = options.sel.value.y + s.y1
      options.redraw()
      return
    }

    if (options.draggingShapeHandle.value !== null) {
      options.applyShapeHandleDrag(cx, cy, e.shiftKey)
      return
    }

    if (options.selResizeHandle.value) {
      options.applySelResize(cx, cy)
      return
    }

    if (options.isDraggingShape.value && options.selectedShapeIndex.value !== null) {
      const dx = cx - options.shapeDragStart.value.mx,
        dy = cy - options.shapeDragStart.value.my
      const s = options.shapes.value[options.selectedShapeIndex.value]

      if (s.type === 'text') {
        const tw = s.textWidth ?? 160
        const newX1 = options.shapeDragStart.value.x1 + dx
        const newY1 = options.shapeDragStart.value.y1 + dy
        const fontSize = Math.max(14, s.lineWidth * 6)
        const lines = s.textLines ?? (s.text ? s.text.split('\n') : [''])
        const lineH = Math.round(fontSize * 1.3)
        const th = lineH * lines.length

        s.x1 = Math.max(0, Math.min(options.sel.value.w - tw, newX1))
        s.y1 = Math.max(0, Math.min(options.sel.value.h - th, newY1))
        s.x2 = s.x1
        s.y2 = s.y1

        if (
          options.textInput.value.visible &&
          options.textInput.value.editingIndex === options.selectedShapeIndex.value
        ) {
          options.textInput.value.x = options.sel.value.x + s.x1
          options.textInput.value.y = options.sel.value.y + s.y1
          options.textInput.value.canvasX = s.x1
          options.textInput.value.canvasY = s.y1
        }
      } else {
        s.x1 = options.shapeDragStart.value.x1 + dx
        s.y1 = options.shapeDragStart.value.y1 + dy
        s.x2 = options.shapeDragStart.value.x2 + dx
        s.y2 = options.shapeDragStart.value.y2 + dy
      }

      options.redraw()
      return
    }

    if (options.phase.value === 'select' && options.pendingDrag.value) {
      const dx = cx - options.dragStart.value.x,
        dy = cy - options.dragStart.value.y
      if (Math.abs(dx) >= DRAG_THRESHOLD || Math.abs(dy) >= DRAG_THRESHOLD) {
        options.pendingDrag.value = false
        options.isDragging.value = true
        options.hoverWindow.value = null
        options.sel.value = { x: options.dragStart.value.x, y: options.dragStart.value.y, w: 0, h: 0 }
      }
    }

    if (options.phase.value === 'select' && options.isDragging.value) {
      const clampedCx = Math.max(0, Math.min(cx, options.screenW.value))
      const clampedCy = Math.max(0, Math.min(cy, options.screenH.value))
      let w = Math.abs(clampedCx - options.dragStart.value.x),
        h = Math.abs(clampedCy - options.dragStart.value.y)
      if (e.shiftKey) {
        const side = Math.min(w, h)
        w = side
        h = side
      }
      options.sel.value = {
        x:
          clampedCx >= options.dragStart.value.x
            ? options.dragStart.value.x
            : options.dragStart.value.x - w,
        y:
          clampedCy >= options.dragStart.value.y
            ? options.dragStart.value.y
            : options.dragStart.value.y - h,
        w,
        h,
      }
      return
    }

    if (options.phase.value === 'select' && !options.isDragging.value && !options.pendingDrag.value) {
      options.hoverWindow.value = options.findWindowAt(cx, cy)
    }

    if (
      options.phase.value === 'annotate' &&
      options.selectedShapeIndex.value !== null &&
      !options.isDraggingShape.value &&
      !options.isDrawing.value &&
      options.draggingShapeHandle.value === null
    ) {
      if (options.isInsideSel(cx, cy)) {
        const canvasX = cx - options.sel.value.x
        const canvasY = cy - options.sel.value.y
        options.isHoveringSelectedShape.value =
          findShapeAt(canvasX, canvasY) === options.selectedShapeIndex.value
      } else {
        options.isHoveringSelectedShape.value = false
      }
    }

    if (options.phase.value === 'annotate' && options.isDragging.value && !options.activeTool.value) {
      options.sel.value.x = Math.max(
        0,
        Math.min(cx - options.dragStart.value.x, options.screenW.value - options.sel.value.w),
      )
      options.sel.value.y = Math.max(
        0,
        Math.min(cy - options.dragStart.value.y, options.screenH.value - options.sel.value.h),
      )
      return
    }

    if (options.isDrawing.value && options.currentShape.value) {
      let x2 = cx - options.sel.value.x,
        y2 = cy - options.sel.value.y
      if (e.shiftKey) {
        const t = options.currentShape.value.type
        const dx = x2 - options.currentShape.value.x1,
          dy = y2 - options.currentShape.value.y1
        const absDx = Math.abs(dx),
          absDy = Math.abs(dy)
        if (t === 'rect') {
          const side = Math.min(absDx, absDy)
          x2 = options.currentShape.value.x1 + (dx >= 0 ? side : -side)
          y2 = options.currentShape.value.y1 + (dy >= 0 ? side : -side)
        } else if (t === 'line' || t === 'arrow') {
          const ratio = absDy / (absDx || 1)
          if (ratio < 0.414) {
            y2 = options.currentShape.value.y1
          } else if (ratio > 2.414) {
            x2 = options.currentShape.value.x1
          } else {
            const side = Math.min(absDx, absDy)
            x2 = options.currentShape.value.x1 + (dx >= 0 ? side : -side)
            y2 = options.currentShape.value.y1 + (dy >= 0 ? side : -side)
          }
        }
      }
      options.currentShape.value.x2 = x2
      options.currentShape.value.y2 = y2
      options.redraw(options.currentShape.value)
    }
  }

  function onMouseUp() {
    if (options.draggingShapeHandle.value !== null) {
      options.draggingShapeHandle.value = null
      return
    }
    if (options.selResizeHandle.value) {
      options.selResizeHandle.value = null
      return
    }
    if (options.isDraggingShape.value) {
      options.isDraggingShape.value = false
      if (options.selectedShapeIndex.value !== null) {
        const { clientX: cx, clientY: cy } = {
          clientX: options.crossX.value,
          clientY: options.crossY.value,
        }
        if (options.isInsideSel(cx, cy)) {
          const canvasX = cx - options.sel.value.x
          const canvasY = cy - options.sel.value.y
          options.isHoveringSelectedShape.value =
            findShapeAt(canvasX, canvasY) === options.selectedShapeIndex.value
        } else {
          options.isHoveringSelectedShape.value = false
        }
      }
      return
    }
    if (options.isDraggingTextInput.value) {
      options.isDraggingTextInput.value = false
      options.textInputPendingDrag.value = false
      return
    }
    if (options.textInputPendingDrag.value) {
      options.textInputPendingDrag.value = false
      return
    }

    if (options.phase.value === 'select' && options.pendingDrag.value) {
      options.pendingDrag.value = false
      if (options.hoverWindow.value) {
        options.sel.value = {
          x: options.hoverWindow.value.x,
          y: options.hoverWindow.value.y,
          w: options.hoverWindow.value.w,
          h: options.hoverWindow.value.h,
        }
        options.hoverWindow.value = null
        options.phase.value = 'annotate'
        nextTick(() => options.rootEl.value?.focus())
      }
      return
    }

    if (options.phase.value === 'select' && options.isDragging.value) {
      options.isDragging.value = false
      if (options.hasSelection.value) {
        options.phase.value = 'annotate'
        nextTick(() => options.rootEl.value?.focus())
      }
      return
    }

    options.isDragging.value = false

    if (options.isDrawing.value && options.currentShape.value) {
      const shape = { ...options.currentShape.value }
      options.currentShape.value = null
      options.isDrawing.value = false
      const hasSize =
        Math.abs(shape.x2 - shape.x1) > 2 || Math.abs(shape.y2 - shape.y1) > 2
      if (hasSize) {
        options.shapes.value.push(shape)
        options.selectedShapeIndex.value = options.shapes.value.length - 1
      }
      options.redraw()
    }
  }

  function onDoubleClick(e: MouseEvent) {
    if (options.phase.value !== 'annotate') return
    const { clientX: cx, clientY: cy } = e
    if (!options.isInsideSel(cx, cy)) return

    const canvasX = cx - options.sel.value.x
    const canvasY = cy - options.sel.value.y
    const hitIdx = findShapeAt(canvasX, canvasY)

    if (hitIdx >= 0 && options.shapes.value[hitIdx].type === 'text') {
      const s = options.shapes.value[hitIdx]
      options.selectedShapeIndex.value = hitIdx
      options.openTextInput(options.sel.value.x + s.x1, options.sel.value.y + s.y1, hitIdx)
    }
  }

  function onKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      if (options.textInput.value.visible) {
        options.cancelText()
        return
      }
      if (options.selectedShapeIndex.value !== null) {
        options.selectedShapeIndex.value = null
        options.isHoveringSelectedShape.value = false
        return
      }
      options.doCancel()
    }
    if (e.key === 'Enter' && options.hasSelection.value && !options.textInput.value.visible)
      options.doCopy()
    if ((e.metaKey || e.ctrlKey) && e.key === 'z') {
      if (options.selectedShapeIndex.value !== null) {
        options.shapes.value.splice(options.selectedShapeIndex.value, 1)
        options.selectedShapeIndex.value = null
        options.isHoveringSelectedShape.value = false
      } else {
        options.shapes.value.pop()
      }
      options.redraw()
    }
    if (
      (e.key === 'Delete' || e.key === 'Backspace') &&
      options.selectedShapeIndex.value !== null &&
      !options.textInput.value.visible
    ) {
      options.shapes.value.splice(options.selectedShapeIndex.value, 1)
      options.selectedShapeIndex.value = null
      options.isHoveringSelectedShape.value = false
      options.redraw()
    }
    if (e.key === 'f' || e.key === 'F') {
      options.sel.value = { x: 0, y: 0, w: options.screenW.value, h: options.screenH.value }
      options.phase.value = 'annotate'
      options.hoverWindow.value = null
      options.isDragging.value = false
      options.pendingDrag.value = false
      nextTick(() => options.rootEl.value?.focus())
    }
    if (
      (e.key === 'c' || e.key === 'C') &&
      options.phase.value === 'select' &&
      !e.metaKey &&
      !e.ctrlKey
    ) {
      writeText(options.pickedColor.value).catch(() => {})
    }
  }

  return {
    hitTestShape,
    findShapeAt,
    onMouseDown,
    onMouseMove,
    onMouseUp,
    onDoubleClick,
    onKeyDown,
  }
}
