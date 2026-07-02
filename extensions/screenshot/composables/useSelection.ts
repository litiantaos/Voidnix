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
  const selResizeStart = ref({ sel: { x: 0, y: 0, w: 0, h: 0 } })

  // 阈值 >0：翻转 resize 瞬间可能产生极小选区，放宽阈值防止边框/控制点 UI 闪烁
  const hasSelection = computed(() => sel.value.w > 0 && sel.value.h > 0)

  function findWindowAt(cx: number, cy: number): WindowRect | null {
    for (const w of options.windows.value) {
      if (cx >= w.x && cx <= w.x + w.w && cy >= w.y && cy <= w.y + w.h) return w
    }
    return null
  }

  function startSelResize(handle: string, e: MouseEvent) {
    selResizeHandle.value = handle
    selResizeStart.value = { sel: { ...sel.value } }
    e.preventDefault()
  }

  // 选区边缘直接跟随鼠标坐标（与首次拉选区同逻辑，边框与鼠标 device-pixel 级重合）。
  // 拖过对边时翻转选区并切换 handle，使调整也支持反向（与首次拉选区一致）。
  function applySelResize(cx: number, cy: number) {
    const s = selResizeStart.value.sel
    let handle = selResizeHandle.value!
    let left = s.x
    let right = s.x + s.w
    let top = s.y
    let bottom = s.y + s.h
    if (handle.includes('e')) right = cx
    if (handle.includes('w')) left = cx
    if (handle.includes('s')) bottom = cy
    if (handle.includes('n')) top = cy
    // 拖过对边：翻转 + 切换 handle（e↔w / n↔s），并把翻转后选区作为新基准，后续基于它继续
    if (right < left) {
      ;[left, right] = [right, left]
      handle = handle.replace('e', 'E').replace('w', 'e').replace('E', 'w')
    }
    if (bottom < top) {
      ;[top, bottom] = [bottom, top]
      handle = handle.replace('n', 'N').replace('s', 'n').replace('N', 's')
    }
    if (handle !== selResizeHandle.value) {
      selResizeHandle.value = handle
      selResizeStart.value = {
        sel: { x: left, y: top, w: right - left, h: bottom - top },
      }
    }
    let x = left
    let y = top
    let w = right - left
    let h = bottom - top
    x = Math.max(0, x)
    y = Math.max(0, y)
    w = Math.min(w, options.screenW.value - x)
    h = Math.min(h, options.screenH.value - y)
    sel.value = { x, y, w, h }
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
    hasSelection,
    findWindowAt,
    startSelResize,
    applySelResize,
    isInsideSel,
  }
}
