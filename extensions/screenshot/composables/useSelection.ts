import { ref, computed, type Ref } from 'vue'
import type { Sel, WindowRect } from './useTypes'

export function useSelection(options: {
  screenW: Ref<number>
  screenH: Ref<number>
  windows: Ref<WindowRect[]>
}) {
  const sel = ref<Sel>({ x: 0, y: 0, w: 0, h: 0 })
  const dragStart = ref({ x: 0, y: 0 })
  const isDragging = ref(false)
  const pendingDrag = ref(false)
  const hoverWindow = ref<WindowRect | null>(null)
  const selResizeHandle = ref<string | null>(null)
  const selResizeStart = ref({ x: 0, y: 0, sel: { x: 0, y: 0, w: 0, h: 0 } })

  const hasSelection = computed(() => sel.value.w > 4 && sel.value.h > 4)

  function findWindowAt(cx: number, cy: number): WindowRect | null {
    for (const w of options.windows.value) {
      if (cx >= w.x && cx <= w.x + w.w && cy >= w.y && cy <= w.y + w.h) return w
    }
    return null
  }

  function startSelResize(handle: string, e: MouseEvent) {
    selResizeHandle.value = handle
    selResizeStart.value = { x: e.clientX, y: e.clientY, sel: { ...sel.value } }
    e.preventDefault()
  }

  function applySelResize(cx: number, cy: number) {
    const dx = cx - selResizeStart.value.x
    const dy = cy - selResizeStart.value.y
    const s = { ...selResizeStart.value.sel }
    const h = selResizeHandle.value!
    let { x, y, w, h: ht } = s
    if (h.includes('e')) w = Math.max(10, s.w + dx)
    if (h.includes('s')) ht = Math.max(10, s.h + dy)
    if (h.includes('w')) {
      x = s.x + dx
      w = Math.max(10, s.w - dx)
    }
    if (h.includes('n')) {
      y = s.y + dy
      ht = Math.max(10, s.h - dy)
    }
    x = Math.max(0, x)
    y = Math.max(0, y)
    w = Math.min(w, options.screenW.value - x)
    ht = Math.min(ht, options.screenH.value - y)
    sel.value = { x, y, w, h: ht }
  }

  function isInsideSel(cx: number, cy: number) {
    const { x, y, w, h } = sel.value
    return cx >= x && cx <= x + w && cy >= y && cy <= y + h
  }

  return {
    sel,
    dragStart,
    isDragging,
    pendingDrag,
    hoverWindow,
    selResizeHandle,
    selResizeStart,
    hasSelection,
    findWindowAt,
    startSelResize,
    applySelResize,
    isInsideSel,
  }
}
