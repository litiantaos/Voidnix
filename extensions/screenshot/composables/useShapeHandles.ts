import { ref, computed, type Ref } from 'vue'
import type { Shape, Sel, ShapeHandle } from './useTypes'
import { TEXT_MIN_WIDTH, TEXT_DRAG_PAD } from './useTypes'
import { wrapText } from './wrapText'

export function useShapeHandles(options: {
  sel: Ref<Sel>
  selectedShape: Ref<Shape | null>
  selectedShapeIndex: Ref<number | null>
  textInput: Ref<{
    visible: boolean
    editingIndex: number | null
    width: number
  }>
  fromLocal: (s: Shape, localX: number, localY: number) => { x: number; y: number }
  redraw: () => void
}) {
  // 拖动形状控制点
  const draggingShapeHandle = ref<string | null>(null)
  const shapeHandleDragStart = ref({
    mx: 0,
    my: 0,
    x1: 0,
    y1: 0,
    x2: 0,
    y2: 0,
    cr: 0,
    tw: 0,
    rotation: 0,
  })

  // 旋转：仅矩形支持，通过显式 'rot' 控制点触发
  const rotateStart = ref({ angleStart: 0, rotationStart: 0 })

  function screenPos(canvasX: number, canvasY: number) {
    return { x: options.sel.value.x + canvasX, y: options.sel.value.y + canvasY }
  }

  function hpStyle(canvasX: number, canvasY: number): Record<string, string> {
    const p = screenPos(canvasX, canvasY)
    return { left: `${p.x}px`, top: `${p.y}px` }
  }

  const shapeHandles = computed((): ShapeHandle[] => {
    const s = options.selectedShape.value
    if (!s) return []
    const { x1, y1, x2, y2 } = s

    if (s.type === 'rect' || s.type === 'blur') {
      const lx = Math.min(x1, x2),
        rx = Math.max(x1, x2)
      const ty = Math.min(y1, y2),
        by = Math.max(y1, y2)
      const mx = (lx + rx) / 2,
        my = (ty + by) / 2
      const hp = (lpx: number, lpy: number) => {
        if (s.type === 'rect') {
          const p = options.fromLocal(s, lpx, lpy)
          return hpStyle(p.x, p.y)
        }
        return hpStyle(lpx, lpy)
      }
      const handles: ShapeHandle[] = [
        { id: 'nw', style: hp(lx, ty) },
        { id: 'n', style: hp(mx, ty) },
        { id: 'ne', style: hp(rx, ty) },
        { id: 'w', style: hp(lx, my) },
        { id: 'e', style: hp(rx, my) },
        { id: 'sw', style: hp(lx, by) },
        { id: 's', style: hp(mx, by) },
        { id: 'se', style: hp(rx, by) },
      ]
      if (s.type === 'rect') {
        const aboveOffset = 18
        handles.push({ id: 'cr', style: hp(mx - 10, ty - aboveOffset) })
        handles.push({ id: 'rot', style: hp(mx + 10, ty - aboveOffset) })
      }
      return handles
    }

    if (s.type === 'line' || s.type === 'arrow') {
      return [
        { id: 'p1', style: hpStyle(x1, y1) },
        { id: 'p2', style: hpStyle(x2, y2) },
      ]
    }

    if (s.type === 'text') {
      const fontSize = s.fontSize ?? Math.max(14, s.lineWidth * 6)
      const lines = s.textLines ?? (s.text ? s.text.split('\n') : [''])
      const lineH = Math.round(fontSize * 1.3)
      const th = lineH * Math.max(1, lines.length)

      const tempCanvas = document.createElement('canvas')
      const tempCtx = tempCanvas.getContext('2d')!
      tempCtx.font = `${fontSize}px -apple-system, sans-serif`
      let maxTextWidth = 0
      lines.forEach((line) => {
        maxTextWidth = Math.max(maxTextWidth, tempCtx.measureText(line).width)
      })
      const tw = Math.max(s.textWidth ?? 160, maxTextWidth)

      const rx = x1 + tw + TEXT_DRAG_PAD
      const my = y1 + th / 2

      return [
        { id: 'e', style: hpStyle(rx, my) },
      ]
    }

    return []
  })

  function startShapeHandleDrag(handleId: string, e: MouseEvent) {
    const s = options.selectedShape.value
    if (!s) return
    draggingShapeHandle.value = handleId
    shapeHandleDragStart.value = {
      mx: e.clientX,
      my: e.clientY,
      x1: s.x1,
      y1: s.y1,
      x2: s.x2,
      y2: s.y2,
      cr: s.cornerRadius ?? 0,
      tw: s.textWidth ?? 160,
      rotation: s.rotation ?? 0,
    }
    if (handleId === 'rot') {
      const ccx = options.sel.value.x + (s.x1 + s.x2) / 2
      const ccy = options.sel.value.y + (s.y1 + s.y2) / 2
      rotateStart.value = {
        angleStart: Math.atan2(e.clientY - ccy, e.clientX - ccx),
        rotationStart: s.rotation ?? 0,
      }
    }
    e.preventDefault()
  }

  function applyShapeHandleDrag(
    cx: number,
    cy: number,
    shiftKey: boolean,
  ) {
    const s = options.selectedShape.value
    if (!s || draggingShapeHandle.value === null) return
    const hid = draggingShapeHandle.value
    const { mx, my, x1, y1, x2, y2, cr, tw, rotation } =
      shapeHandleDragStart.value
    const dxScreen = cx - mx,
      dyScreen = cy - my

    if (hid === 'rot') {
      const sxCenter = options.sel.value.x + (x1 + x2) / 2
      const syCenter = options.sel.value.y + (y1 + y2) / 2
      const angleNow = Math.atan2(cy - syCenter, cx - sxCenter)
      let next =
        rotateStart.value.rotationStart +
        (angleNow - rotateStart.value.angleStart)
      if (shiftKey) {
        const step = (15 * Math.PI) / 180
        next = Math.round(next / step) * step
      }
      s.rotation = next
      options.redraw()
      return
    }

    const effectiveRotation = s.type === 'rect' ? rotation : 0
    const cos = Math.cos(-effectiveRotation),
      sin = Math.sin(-effectiveRotation)
    const dx = dxScreen * cos - dyScreen * sin
    const dy = dxScreen * sin + dyScreen * cos

    if (s.type === 'rect' || s.type === 'blur') {
      const lx0 = Math.min(x1, x2),
        rx0 = Math.max(x1, x2)
      const ty0 = Math.min(y1, y2),
        by0 = Math.max(y1, y2)
      let lx = lx0,
        rx = rx0,
        ty = ty0,
        by = by0

      if (hid === 'nw') {
        lx = lx0 + dx
        ty = ty0 + dy
      } else if (hid === 'n') {
        ty = ty0 + dy
      } else if (hid === 'ne') {
        rx = rx0 + dx
        ty = ty0 + dy
      } else if (hid === 'w') {
        lx = lx0 + dx
      } else if (hid === 'e') {
        rx = rx0 + dx
      } else if (hid === 'sw') {
        lx = lx0 + dx
        by = by0 + dy
      } else if (hid === 's') {
        by = by0 + dy
      } else if (hid === 'se') {
        rx = rx0 + dx
        by = by0 + dy
      } else if (hid === 'cr' && s.type === 'rect') {
        const maxCr = Math.min((rx0 - lx0) / 2, (by0 - ty0) / 2)
        const projected = (-dx + dy) / Math.SQRT2
        s.cornerRadius = Math.max(0, Math.min(maxCr, Math.round(cr + projected)))
        options.redraw()
        return
      }

      if (rx - lx < 4) {
        if (hid.includes('w')) lx = rx - 4
        else rx = lx + 4
      }
      if (by - ty < 4) {
        if (hid.includes('n')) ty = by - 4
        else by = ty + 4
      }

      if (s.type === 'rect' && s.cornerRadius) {
        s.cornerRadius = Math.min(
          s.cornerRadius,
          Math.floor(Math.min((rx - lx) / 2, (by - ty) / 2)),
        )
      }

      s.x1 = x1 <= x2 ? lx : rx
      s.x2 = x1 <= x2 ? rx : lx
      s.y1 = y1 <= y2 ? ty : by
      s.y2 = y1 <= y2 ? by : ty
    } else if (s.type === 'line' || s.type === 'arrow') {
      if (hid === 'p1') {
        let nx = x1 + dxScreen
        let ny = y1 + dyScreen
        if (shiftKey) {
          const dx = nx - x2
          const dy = ny - y2
          const absDx = Math.abs(dx)
          const absDy = Math.abs(dy)
          const ratio = absDy / (absDx || 1)
          if (ratio < 0.414) {
            ny = y2
          } else if (ratio > 2.414) {
            nx = x2
          } else {
            const side = Math.min(absDx, absDy)
            nx = x2 + (dx >= 0 ? side : -side)
            ny = y2 + (dy >= 0 ? side : -side)
          }
        }
        s.x1 = nx
        s.y1 = ny
      } else if (hid === 'p2') {
        let nx = x2 + dxScreen
        let ny = y2 + dyScreen
        if (shiftKey) {
          const dx = nx - x1
          const dy = ny - y1
          const absDx = Math.abs(dx)
          const absDy = Math.abs(dy)
          const ratio = absDy / (absDx || 1)
          if (ratio < 0.414) {
            ny = y1
          } else if (ratio > 2.414) {
            nx = x1
          } else {
            const side = Math.min(absDx, absDy)
            nx = x1 + (dx >= 0 ? side : -side)
            ny = y1 + (dy >= 0 ? side : -side)
          }
        }
        s.x2 = nx
        s.y2 = ny
      }
    } else if (s.type === 'text') {
      if (hid === 'e') {
        const handleStartX = options.sel.value.x + x1 + tw
        const handleNewX = handleStartX + dxScreen
        const minHandleX = options.sel.value.x + x1 + TEXT_MIN_WIDTH
        const maxHandleX = options.sel.value.x + options.sel.value.w - 10
        const clampedHandleX = Math.max(
          minHandleX,
          Math.min(maxHandleX, handleNewX),
        )
        const newWidth = clampedHandleX - (options.sel.value.x + x1)

        s.textWidth = newWidth
        if (options.textInput.value.visible &&
          options.textInput.value.editingIndex === options.selectedShapeIndex.value) {
          options.textInput.value.width = newWidth
        }

        const fontSize = s.fontSize ?? Math.max(14, s.lineWidth * 6)
        const font = `${fontSize}px -apple-system, sans-serif`
        s.textLines = wrapText(s.text ?? '', newWidth, font)
      }
    }

    options.redraw()
  }

  return {
    draggingShapeHandle,
    shapeHandleDragStart,
    rotateStart,
    shapeHandles,
    startShapeHandleDrag,
    applyShapeHandleDrag,
  }
}
