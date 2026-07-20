import { describe, it, expect, beforeEach, vi } from 'vitest'
import {
  toasts,
  overlayKey,
  showToast,
  dismissToast,
  clearToasts,
  pauseToast,
  resumeToast,
  pauseAllToasts,
  resumeAllToasts,
} from './useToast'

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

  describe('hover 暂停/恢复', () => {
    it('pauseToast 期间不触发自动清除', () => {
      vi.useFakeTimers()
      showToast('错误', { duration: 1000 })
      const id = toasts.value[0].id
      vi.advanceTimersByTime(500)
      pauseToast(id)
      vi.advanceTimersByTime(100000) // 暂停期间时间冻结
      expect(toasts.value).toHaveLength(1)
      vi.useRealTimers()
    })

    it('resumeToast 从剩余时刻继续倒计时', () => {
      vi.useFakeTimers()
      showToast('错误', { duration: 1000 })
      const id = toasts.value[0].id
      vi.advanceTimersByTime(500) // 已过 500，剩 500
      pauseToast(id)
      vi.advanceTimersByTime(100000) // 暂停期间冻结
      resumeToast(id)
      vi.advanceTimersByTime(499)
      expect(toasts.value).toHaveLength(1)
      vi.advanceTimersByTime(2)
      expect(toasts.value).toHaveLength(0)
      vi.useRealTimers()
    })

    it('pauseToast / resumeToast 幂等（未暂停 resume、已暂停 pause 无副作用）', () => {
      vi.useFakeTimers()
      showToast('错误', { duration: 1000 })
      const id = toasts.value[0].id
      resumeToast(id) // 未暂停，无副作用
      vi.advanceTimersByTime(200)
      pauseToast(id)
      pauseToast(id) // 已暂停，无副作用
      vi.advanceTimersByTime(10000)
      expect(toasts.value).toHaveLength(1)
      resumeToast(id)
      vi.advanceTimersByTime(10000)
      expect(toasts.value).toHaveLength(0)
      vi.useRealTimers()
    })
  })

  describe('悬浮区域 pauseAll / resumeAll', () => {
    it('pauseAllToasts 冻结全部 toast', () => {
      vi.useFakeTimers()
      showToast('一', { duration: 1000 })
      showToast('二', { duration: 1000 })
      showToast('三', { duration: 1000 })
      vi.advanceTimersByTime(500)
      pauseAllToasts()
      vi.advanceTimersByTime(100000) // 全部冻结
      expect(toasts.value).toHaveLength(3)
      vi.useRealTimers()
    })

    it('resumeAllToasts 恢复全部并保留各自剩余时长', () => {
      vi.useFakeTimers()
      showToast('一', { duration: 1000 })
      showToast('二', { duration: 2000 })
      vi.advanceTimersByTime(500) // 一剩 500，二剩 1500
      pauseAllToasts()
      vi.advanceTimersByTime(100000)
      resumeAllToasts()
      vi.advanceTimersByTime(500)
      expect(toasts.value).toHaveLength(1) // 一消失，二还在
      expect(toasts.value[0].message).toBe('二')
      vi.advanceTimersByTime(1000)
      expect(toasts.value).toHaveLength(0)
      vi.useRealTimers()
    })
  })
})
