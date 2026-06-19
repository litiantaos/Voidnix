import { describe, it, expect, vi, beforeEach } from 'vitest'
import { nextTick } from 'vue'

// 模拟 @tauri-apps/plugin-store 的 Store 接口（get/set/save）
const storeGet = vi.fn()
const storeSet = vi.fn()
const storeSave = vi.fn()
const loadMock = vi.fn()

vi.mock('@tauri-apps/plugin-store', () => ({
  load: (path: string) => {
    loadMock(path)
    return Promise.resolve({
      get: storeGet,
      set: storeSet,
      save: storeSave,
    })
  },
}))

const { defineConfig } = await import('./storage')

describe('defineConfig', () => {
  beforeEach(() => {
    storeGet.mockReset()
    storeSet.mockReset()
    storeSave.mockReset()
    loadMock.mockReset()
  })

  it('返回 defaults 响应式对象', () => {
    storeGet.mockResolvedValue(null)
    const config = defineConfig('cfg-defaults', { maxDays: 30, enabled: true })
    expect(config.maxDays).toBe(30)
    expect(config.enabled).toBe(true)
  })

  it('load 完成前返回 defaults（v1.6 N10 竞态）', () => {
    storeGet.mockResolvedValue(null)
    const config = defineConfig('cfg-race', { maxDays: 30 })
    // 同步读取：磁盘值尚未回填，得到 defaults
    expect(config.maxDays).toBe(30)
  })

  it('load 完成后回填磁盘值（仅 backfill 存在的 key）', async () => {
    storeGet.mockImplementation((key: string) => Promise.resolve(key === 'maxDays' ? 99 : null))
    const config = defineConfig('cfg-backfill', { maxDays: 30, enabled: true })
    await vi.waitFor(() => expect(config.maxDays).toBe(99))
    expect(config.enabled).toBe(true) // 磁盘无值保留 default
  })

  it('变更后 300ms 防抖触发 save', async () => {
    storeGet.mockResolvedValue(null)
    const config = defineConfig('cfg-save', { maxDays: 30 })
    await vi.waitFor(() => expect(loadMock).toHaveBeenCalled())
    await nextTick()
    storeSet.mockClear()
    storeSave.mockClear()

    config.maxDays = 60
    // 防抖窗口内不保存
    await nextTick()
    expect(storeSet).not.toHaveBeenCalled()

    await vi.waitFor(() => {
      expect(storeSet).toHaveBeenCalledWith('maxDays', 60)
      expect(storeSave).toHaveBeenCalled()
    })
  })

  it('store 实例缓存：多次保存仅 load 一次（v1.5 B1）', async () => {
    storeGet.mockResolvedValue(null)
    const config = defineConfig('cfg-cache', { maxDays: 30 })
    await vi.waitFor(() => expect(loadMock).toHaveBeenCalledTimes(1))

    config.maxDays = 1
    await vi.waitFor(() => expect(storeSave).toHaveBeenCalled())
    config.maxDays = 2
    await vi.waitFor(() => expect(storeSave).toHaveBeenCalledTimes(2))

    // 两次保存后仍只 load 一次（复用缓存 store）
    expect(loadMock).toHaveBeenCalledTimes(1)
  })
})
