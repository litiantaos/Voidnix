import { ref } from 'vue'

export type ToastKind = 'success' | 'error'

export interface ToastOptions {
  duration?: number
  kind?: ToastKind
}

export interface ToastItem {
  id: number
  message: string
  kind: ToastKind
}

export const toasts = ref<ToastItem[]>([])
/** clearToasts 递增：强制 ToastOverlay 重建，跳过离场动画立即移除 DOM。 */
export const overlayKey = ref(0)
const MAX = 3
let nextId = 0
const timers = new Map<number, ReturnType<typeof setTimeout>>()

/** 推入一条 toast，按 duration 自动清除；超 MAX 则淘汰最早一条。duration=0 不清除（测试用）。 */
export function showToast(message: string, opts?: ToastOptions) {
  const { duration = 2000, kind = 'success' } = opts ?? {}
  const id = nextId++
  toasts.value = [...toasts.value, { id, message, kind }]
  while (toasts.value.length > MAX) {
    const removed = toasts.value[0]
    toasts.value = toasts.value.slice(1)
    clearTimer(removed.id)
  }
  if (duration > 0) {
    timers.set(
      id,
      setTimeout(() => dismissToast(id), duration),
    )
  }
}

export function dismissToast(id: number) {
  toasts.value = toasts.value.filter((t) => t.id !== id)
  clearTimer(id)
}

function clearTimer(id: number) {
  const t = timers.get(id)
  if (t) {
    clearTimeout(t)
    timers.delete(id)
  }
}

/** 测试用：清空全部 toast + 定时器。 */
export function clearToasts() {
  // 递增 overlayKey 强制 ToastOverlay 重建，跳过离场动画立即移除 DOM
  // （规避窗口隐藏时 macOS WKWebView 节流 rAF 致离场动画元素残留到下次显示）
  overlayKey.value++
  for (const t of timers.values()) clearTimeout(t)
  timers.clear()
  toasts.value = []
}
