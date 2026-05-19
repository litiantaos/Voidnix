import { ref, computed } from 'vue'
import type { Shape, Tool } from './useTypes'

export function useAnnotation() {
  // ── 标注工具 ──────────────────────────────────────────────
  const activeTool = ref<Tool>(null)
  const annotColor = ref('#ff3b30')
  const annotLineWidth = ref(3)
  const annotBlurAmount = ref(15)

  // ── 形状数据 ──────────────────────────────────────────────
  const shapes = ref<Shape[]>([])
  const currentShape = ref<Shape | null>(null)
  const isDrawing = ref(false)
  const drawStart = ref({ x: 0, y: 0 })

  // 选中形状
  const selectedShapeIndex = ref<number | null>(null)
  const selectedShape = computed(() =>
    selectedShapeIndex.value !== null
      ? (shapes.value[selectedShapeIndex.value] ?? null)
      : null,
  )

  // 鼠标是否在选中元素上
  const isHoveringSelectedShape = ref(false)

  // 拖动整个形状
  const isDraggingShape = ref(false)
  const shapeDragStart = ref({ mx: 0, my: 0, x1: 0, y1: 0, x2: 0, y2: 0 })

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

  function shapeCenter(s: Shape): { cx: number; cy: number } {
    return { cx: (s.x1 + s.x2) / 2, cy: (s.y1 + s.y2) / 2 }
  }

  // 屏幕坐标 (canvasX, canvasY) → 形状本地坐标（反向旋转）
  function toLocal(
    s: Shape,
    canvasX: number,
    canvasY: number,
  ): { lx: number; ly: number } {
    const { cx, cy } = shapeCenter(s)
    const r = s.rotation ?? 0
    const dx = canvasX - cx,
      dy = canvasY - cy
    const cos = Math.cos(-r),
      sin = Math.sin(-r)
    return { lx: cx + dx * cos - dy * sin, ly: cy + dx * sin + dy * cos }
  }

  // 形状本地坐标 → 屏幕坐标（应用 rotation）
  function fromLocal(
    s: Shape,
    localX: number,
    localY: number,
  ): { x: number; y: number } {
    const { cx, cy } = shapeCenter(s)
    const r = s.rotation ?? 0
    const dx = localX - cx,
      dy = localY - cy
    const cos = Math.cos(r),
      sin = Math.sin(r)
    return { x: cx + dx * cos - dy * sin, y: cy + dx * sin + dy * cos }
  }

  // 8个尺寸控制点的鼠标光标：根据 handle id 和当前旋转角度计算
  function handleCursor(id: string, rotation: number): string {
    const baseDeg: Record<string, number> = {
      n: 0,
      ne: 45,
      e: 90,
      se: 135,
      s: 180,
      sw: 225,
      w: 270,
      nw: 315,
    }
    const base = baseDeg[id]
    if (base === undefined) return 'default'
    const total = (base + (rotation * 180) / Math.PI + 360) % 360
    const cursors = [
      'ns-resize',
      'nesw-resize',
      'ew-resize',
      'nwse-resize',
      'ns-resize',
      'nesw-resize',
      'ew-resize',
      'nwse-resize',
    ]
    const idx = Math.round(total / 45) % 8
    return cursors[idx]
  }

  function setTool(tool: Tool) {
    activeTool.value = activeTool.value === tool ? null : tool
  }

  return {
    activeTool,
    annotColor,
    annotLineWidth,
    annotBlurAmount,
    shapes,
    currentShape,
    isDrawing,
    drawStart,
    selectedShapeIndex,
    selectedShape,
    isHoveringSelectedShape,
    isDraggingShape,
    shapeDragStart,
    draggingShapeHandle,
    shapeHandleDragStart,
    rotateStart,
    shapeCenter,
    toLocal,
    fromLocal,
    handleCursor,
    setTool,
  }
}
