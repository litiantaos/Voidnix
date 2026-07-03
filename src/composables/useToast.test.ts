import { describe, it, expect, beforeEach, vi } from 'vitest'
import { toasts, overlayKey, showToast, dismissToast, clearToasts } from './useToast'

describe('useToast', () => {
  beforeEach(() => {
    clearToasts()
  })

  it('showToast 推入一条，默认 kind=success', () => {
    showToast('已复制')
    expect(toasts.value).toHaveLength(1)
    expect(toasts.value[0]).toMatchObject({ message: '已复制', kind: 'success' })
  })

  it('kind:error 切换语义', () => {
    showToast('失败', { kind: 'error' })
    expect(toasts.value[0].kind).toBe('error')
  })

  it('duration=0 不自动清除', () => {
    vi.useFakeTimers()
    showToast('常驻', { duration: 0 })
    vi.advanceTimersByTime(10000)
    expect(toasts.value).toHaveLength(1)
    vi.useRealTimers()
  })

  it('duration 后自动 dismiss', () => {
    vi.useFakeTimers()
    showToast('自动消失', { duration: 1000 })
    expect(toasts.value).toHaveLength(1)
    vi.advanceTimersByTime(1000)
    expect(toasts.value).toHaveLength(0)
    vi.useRealTimers()
  })

  it('超 MAX(3) 淘汰最早一条', () => {
    showToast('一', { duration: 0 })
    showToast('二', { duration: 0 })
    showToast('三', { duration: 0 })
    showToast('四', { duration: 0 })
    expect(toasts.value).toHaveLength(3)
    expect(toasts.value.map((t) => t.message)).toEqual(['二', '三', '四'])
  })

  it('dismissToast 移除指定 id', () => {
    showToast('一', { duration: 0 })
    const id = toasts.value[0].id
    dismissToast(id)
    expect(toasts.value).toHaveLength(0)
  })

  it('clearToasts 清空并递增 overlayKey', () => {
    const before = overlayKey.value
    showToast('一', { duration: 0 })
    showToast('二', { duration: 0 })
    clearToasts()
    expect(toasts.value).toHaveLength(0)
    expect(overlayKey.value).toBe(before + 1)
  })

  it('淘汰的 toast 其定时器被清理（不再触发 dismiss）', () => {
    vi.useFakeTimers()
    showToast('一', { duration: 5000 })
    showToast('二', { duration: 5000 })
    showToast('三', { duration: 5000 })
    showToast('四', { duration: 5000 }) // 淘汰「一」
    expect(toasts.value.map((t) => t.message)).toEqual(['二', '三', '四'])
    vi.advanceTimersByTime(5000)
    // 三个剩余 toast 各自定时器到期后正常 dismiss
    expect(toasts.value).toHaveLength(0)
    vi.useRealTimers()
  })
})
