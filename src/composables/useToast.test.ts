import { describe, it, expect, beforeEach, vi } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { showToast } from './useToast'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }))
vi.mock('@/utils/tauri', () => ({ isTauri: true }))

describe('useToast（原生 toast invoke 薄壳）', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockClear()
  })

  it('默认 success / 2000ms', () => {
    showToast('已复制')
    expect(invoke).toHaveBeenCalledWith(CMD.showToast, {
      message: '已复制',
      kind: 'success',
      durationMs: 2000,
    })
  })

  it('透传 error kind 与显式 duration', () => {
    showToast('失败', { kind: 'error', duration: 6000 })
    expect(invoke).toHaveBeenCalledWith(CMD.showToast, {
      message: '失败',
      kind: 'error',
      durationMs: 6000,
    })
  })
})
