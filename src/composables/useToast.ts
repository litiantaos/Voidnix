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

interface TimerState {
  handle: ReturnType<typeof setTimeout>
  expiresAt: number // 预计到期时刻
  pausedAt: number | null // 非 null = 暂停中
}

const timers = new Map<number, TimerState>()

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
  if (duration > 0) arm(id, duration)
}

function arm(id: number, duration: number) {
  const handle = setTimeout(() => dismissToast(id), duration)
  timers.set(id, { handle, expiresAt: Date.now() + duration, pausedAt: null })
}

export function dismissToast(id: number) {
  toasts.value = toasts.value.filter((t) => t.id !== id)
  clearTimer(id)
}

/** 鼠标悬浮暂停自动清除：用于阅读长内容/错误消息。幂等（已暂停时无副作用）。 */
export function pauseToast(id: number) {
  const s = timers.get(id)
  if (!s || s.pausedAt !== null) return
  clearTimeout(s.handle)
  s.pausedAt = Date.now()
}

/** 鼠标离开恢复倒计时：从暂停时剩余时刻继续。剩余 ≤0 立即 dismiss。 */
export function resumeToast(id: number) {
  const s = timers.get(id)
  if (!s || s.pausedAt === null) return
  // 暂停期间时间冻结：把 expiresAt 推后暂停时长
  s.expiresAt += Date.now() - s.pausedAt
  s.pausedAt = null
  const remaining = s.expiresAt - Date.now()
  if (remaining <= 0) {
    dismissToast(id)
    return
  }
  s.handle = setTimeout(() => dismissToast(id), remaining)
}

/** 悬浮 toast 区域：冻结全部倒计时。任一 toast 悬浮即暂停全部，避免悬浮中其他消失导致重排 → mouseleave 失焦。 */
export function pauseAllToasts() {
  for (const id of [...timers.keys()]) pauseToast(id)
}

/** 离开 toast 区域：恢复全部倒计时。快照 keys（resume 可能 dismiss 致 map 变更）。 */
export function resumeAllToasts() {
  for (const id of [...timers.keys()]) resumeToast(id)
}

function clearTimer(id: number) {
  const s = timers.get(id)
  if (s) {
    clearTimeout(s.handle)
    timers.delete(id)
  }
}

/** 测试用：清空全部 toast + 定时器。 */
export function clearToasts() {
  // 递增 overlayKey 强制 ToastOverlay 重建，跳过离场动画立即移除 DOM
  // （规避窗口隐藏时 macOS WKWebView 节流 rAF 致离场动画元素残留到下次显示）
  overlayKey.value++
  for (const s of timers.values()) clearTimeout(s.handle)
  timers.clear()
  toasts.value = []
}
