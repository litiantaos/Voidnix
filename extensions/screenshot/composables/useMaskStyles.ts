import { computed, type Ref } from 'vue'
import type { Sel, WindowRect, Tool } from './useTypes'
import { PALETTE_H, PALETTE_GAP } from './useTypes'

export function useMaskStyles(options: {
  sel: Ref<Sel>
  hoverWindow: Ref<WindowRect | null>
  screenW: Ref<number>
  screenH: Ref<number>
  selResizeHandle: Ref<string | null>
  draggingShapeHandle: Ref<string | null>
  selectedShape: Ref<{ type: Tool; rotation?: number } | null>
  isDraggingShape: Ref<boolean>
  isDraggingTextInput: Ref<boolean>
  isHoveringSelectedShape: Ref<boolean>
  isDragging: Ref<boolean>
  isHoveringAnyShape: Ref<boolean>
  activeTool: Ref<Tool>
  crossX: Ref<number>
  crossY: Ref<number>
}) {
  const selectionStyle = computed(() => ({
    left: `${options.sel.value.x}px`,
    top: `${options.sel.value.y}px`,
    width: `${options.sel.value.w}px`,
    height: `${options.sel.value.h}px`,
  }))

  const hoverWindowStyle = computed(() => {
    const w = options.hoverWindow.value
    if (!w) return {}
    return {
      left: `${w.x}px`,
      top: `${w.y}px`,
      width: `${w.w}px`,
      height: `${w.h}px`,
    }
  })

  const hoverMaskTop = computed(() => {
    const w = options.hoverWindow.value
    if (!w) return {}
    return { left: 0, top: 0, right: 0, height: `${w.y}px` }
  })

  const hoverMaskBottom = computed(() => {
    const w = options.hoverWindow.value
    if (!w) return {}
    return { left: 0, bottom: 0, right: 0, top: `${w.y + w.h}px` }
  })

  const hoverMaskLeft = computed(() => {
    const w = options.hoverWindow.value
    if (!w) return {}
    return { left: 0, top: `${w.y}px`, width: `${w.x}px`, height: `${w.h}px` }
  })

  const hoverMaskRight = computed(() => {
    const w = options.hoverWindow.value
    if (!w) return {}
    return {
      right: 0,
      top: `${w.y}px`,
      left: `${w.x + w.w}px`,
      height: `${w.h}px`,
    }
  })

  const maskTop = computed(() => ({
    left: 0,
    top: 0,
    right: 0,
    height: `${options.sel.value.y}px`,
  }))

  const maskBottom = computed(() => ({
    left: 0,
    bottom: 0,
    right: 0,
    top: `${options.sel.value.y + options.sel.value.h}px`,
  }))

  const maskLeft = computed(() => ({
    left: 0,
    top: `${options.sel.value.y}px`,
    width: `${options.sel.value.x}px`,
    height: `${options.sel.value.h}px`,
  }))

  const maskRight = computed(() => ({
    right: 0,
    top: `${options.sel.value.y}px`,
    left: `${options.sel.value.x + options.sel.value.w}px`,
    height: `${options.sel.value.h}px`,
  }))

  // ── 调色板位置 ──────────────────────────────────────────────
  const palettePos = computed<'below' | 'above' | 'inside'>(() => {
    const { y, h } = options.sel.value
    if (y + h + PALETTE_GAP + PALETTE_H <= options.screenH.value) return 'below'
    if (y - PALETTE_H - PALETTE_GAP >= 0) return 'above'
    return 'inside'
  })

  const LABEL_H = 20
  const LABEL_GAP = 8
  const EDGE_PAD = 8

  const selSizeStyle = computed(() => {
    const { x, y } = options.sel.value
    const left = x < EDGE_PAD ? `${EDGE_PAD - x}px` : '0px'
    if (y >= LABEL_H + LABEL_GAP && palettePos.value !== 'above')
      return { top: `-${LABEL_H + LABEL_GAP}px`, left }
    return { top: `${LABEL_GAP}px`, left }
  })

  const hoverSizeStyle = computed(() => {
    const w = options.hoverWindow.value
    if (!w) return {}
    const left = w.x < EDGE_PAD ? `${EDGE_PAD - w.x}px` : '0px'
    return w.y >= LABEL_H + LABEL_GAP
      ? { top: `-${LABEL_H + LABEL_GAP}px`, left }
      : { top: `${LABEL_GAP}px`, left }
  })

  // ── 选区控制点 ──────────────────────────────────────────────
  const handles = computed(() => {
    const { w, h } = options.sel.value
    const half = -4
    return [
      {
        id: 'nw',
        style: { left: `${half}px`, top: `${half}px`, cursor: 'nw-resize' },
      },
      {
        id: 'n',
        style: {
          left: `${w / 2 + half}px`,
          top: `${half}px`,
          cursor: 'n-resize',
        },
      },
      {
        id: 'ne',
        style: { right: `${half}px`, top: `${half}px`, cursor: 'ne-resize' },
      },
      {
        id: 'w',
        style: {
          left: `${half}px`,
          top: `${h / 2 + half}px`,
          cursor: 'w-resize',
        },
      },
      {
        id: 'e',
        style: {
          right: `${half}px`,
          top: `${h / 2 + half}px`,
          cursor: 'e-resize',
        },
      },
      {
        id: 'sw',
        style: { left: `${half}px`, bottom: `${half}px`, cursor: 'sw-resize' },
      },
      {
        id: 's',
        style: {
          left: `${w / 2 + half}px`,
          bottom: `${half}px`,
          cursor: 's-resize',
        },
      },
      {
        id: 'se',
        style: { right: `${half}px`, bottom: `${half}px`, cursor: 'se-resize' },
      },
    ]
  })

  // ── 光标样式 ──────────────────────────────────────────────
  function getCursorForHandle(h: string) {
    const map: Record<string, string> = {
      nw: 'nw-resize',
      n: 'n-resize',
      ne: 'ne-resize',
      w: 'w-resize',
      e: 'e-resize',
      sw: 'sw-resize',
      s: 's-resize',
      se: 'se-resize',
    }
    return map[h] || 'default'
  }

  const cursorStyle = computed(() => {
    if (options.selResizeHandle.value) return getCursorForHandle(options.selResizeHandle.value)
    if (options.draggingShapeHandle.value === 'e' && options.selectedShape.value?.type === 'text')
      return 'ew-resize'
    if (options.draggingShapeHandle.value === 'rot') return 'grabbing'
    if (options.draggingShapeHandle.value === 'cr') return 'ns-resize'
    if (options.isDraggingShape.value) return 'move'
    if (options.isDraggingTextInput.value) return 'move'
    if (options.isHoveringSelectedShape.value) return 'move'
    if (options.isDragging.value) return 'grabbing'
    if (!options.activeTool.value) {
      if (options.isHoveringAnyShape.value) return 'pointer'
      const { x, y, w, h } = options.sel.value
      const cx = options.crossX.value,
        cy = options.crossY.value
      if (cx >= x && cx <= x + w && cy >= y && cy <= y + h) return 'grab'
    }
    return 'default'
  })

  return {
    selectionStyle,
    hoverWindowStyle,
    hoverMaskTop,
    hoverMaskBottom,
    hoverMaskLeft,
    hoverMaskRight,
    maskTop,
    maskBottom,
    maskLeft,
    maskRight,
    palettePos,
    selSizeStyle,
    hoverSizeStyle,
    handles,
    cursorStyle,
    getCursorForHandle,
  }
}
