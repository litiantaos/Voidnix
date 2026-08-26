import { describe, it, expect, vi, beforeEach } from 'vitest'
import { nextTick } from 'vue'

// 模拟 @tauri-apps/plugin-store 的 Store 接口（get/set/save/onChange）
// onChangeCb 保留 callback 引用：defineConfig 订阅则非 null（可手动模拟 plugin-store emit）
const onChangeCb = { fn: null as ((key: string, value: unknown) => void) | null }
const storeGet = vi.fn()
const storeSet = vi.fn()
const storeSave = vi.fn()
const storeOnChange = vi.fn().mockImplementation((cb: (key: string, value: unknown) => void) => {
  onChangeCb.fn = cb
  return Promise.resolve(() => {})
})
const loadMock = vi.fn()

vi.mock('@tauri-apps/plugin-store', () => ({
  load: (path: string) => {
    loadMock(path)
    return Promise.resolve({
      get: storeGet,
      set: storeSet,
      save: storeSave,
      onChange: storeOnChange,
    })
  },
}))

// 模拟 isTauri 为 false（避免 dynamic import @tauri-apps/api/window）
vi.mock('@/utils/tauri', () => ({ isTauri: false }))

const { defineConfig, whenConfigReady } = await import('./storage')

describe('defineConfig', () => {
  beforeEach(() => {
    onChangeCb.fn = null
    storeGet.mockReset()
    storeSet.mockReset()
    storeSave.mockReset()
    storeOnChange.mockClear()
    loadMock.mockReset()
  })

  it('返回 defaults 响应式对象', () => {
    storeGet.mockResolvedValue(null)
    const config = defineConfig('cfg-defaults', { maxDays: 30, enabled: true })
    expect(config.maxDays).toBe(30)
    expect(config.enabled).toBe(true)
  })

  it('load 完成前返回 defaults（竞态保护）', () => {
    storeGet.mockResolvedValue(null)
    const config = defineConfig('cfg-race', { maxDays: 30 })
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
    // 等 backfill 走完（isLoading 转为 false）
    await new Promise((r) => setTimeout(r, 20))
    storeSet.mockClear()
    storeSave.mockClear()

    config.maxDays = 60
    await nextTick()
    expect(storeSet).not.toHaveBeenCalled() // 防抖窗口内不保存

    await vi.waitFor(() => {
      expect(storeSet).toHaveBeenCalledWith('maxDays', 60)
      expect(storeSave).toHaveBeenCalled()
    })
  })

  it('store 实例缓存：多次保存仅 load 一次', async () => {
    storeGet.mockResolvedValue(null)
    const config = defineConfig('cfg-cache', { maxDays: 30 })
    await vi.waitFor(() => expect(loadMock).toHaveBeenCalledTimes(1))
    await new Promise((r) => setTimeout(r, 20)) // 等 isLoading = false

    config.maxDays = 1
    await vi.waitFor(() => expect(storeSave).toHaveBeenCalled())
    config.maxDays = 2
    await vi.waitFor(() => expect(storeSave).toHaveBeenCalledTimes(2))

    expect(loadMock).toHaveBeenCalledTimes(1)
  })

  it('对象/数组 default 深克隆：mutation 不污染 defaults（C7）', async () => {
    storeGet.mockResolvedValue(null)
    const defaults = { items: ['a'], opts: { flag: false } }
    const config = defineConfig('cfg-clone', defaults)
    await vi.waitFor(() => expect(loadMock).toHaveBeenCalled())

    config.items.push('b')
    config.opts.flag = true
    expect(defaults.items).toEqual(['a'])
    expect(defaults.opts).toEqual({ flag: false })
    expect(config.items).toEqual(['a', 'b'])
    expect(config.opts.flag).toBe(true)
  })

  it('引用型 default 的 race 保护：用户已 mutate 则不回填磁盘值（C7）', async () => {
    storeGet.mockImplementation((key: string) => Promise.resolve(key === 'items' ? ['old'] : null))
    const config = defineConfig('cfg-race-ref', { items: ['default'] })
    config.items.push('user') // 同步 mutate（在 backfill 到达前）
    await vi.waitFor(() => expect(storeGet).toHaveBeenCalled())
    await new Promise((r) => setTimeout(r, 10))
    expect(config.items).toEqual(['default', 'user'])
  })

  // ─── 新增：isLoading 抑制启动期冗余写（P5） ─────────────────

  it('启动回填不触发冗余写（isLoading 抑制）', async () => {
    storeGet.mockImplementation((key: string) => Promise.resolve(key === 'maxDays' ? 99 : null))
    const config = defineConfig('cfg-isloading', { maxDays: 30 })
    await vi.waitFor(() => expect(config.maxDays).toBe(99))
    // 给 microtask 充分时间，确认无写
    await new Promise((r) => setTimeout(r, 50))
    expect(storeSet).not.toHaveBeenCalled()
    expect(storeSave).not.toHaveBeenCalled()
  })

  // ─── 新增：类型守卫（P7） ──────────────────────────────────

  it('类型守卫：磁盘值类型不匹配则丢弃', async () => {
    // maxDays 默认 number，磁盘为 string "abc"
    storeGet.mockImplementation((key: string) => Promise.resolve(key === 'maxDays' ? 'abc' : null))
    const config = defineConfig('cfg-validate', { maxDays: 30 })
    await vi.waitFor(() => expect(storeGet).toHaveBeenCalled())
    await new Promise((r) => setTimeout(r, 10))
    expect(config.maxDays).toBe(30) // 类型不符，保留 default
  })

  it('类型守卫：数组默认值，磁盘为对象则丢弃', async () => {
    storeGet.mockImplementation((key: string) =>
      Promise.resolve(key === 'items' ? { 0: 'x' } : null),
    )
    const config = defineConfig('cfg-validate-arr', { items: ['a'] })
    await vi.waitFor(() => expect(storeGet).toHaveBeenCalled())
    await new Promise((r) => setTimeout(r, 10))
    expect(config.items).toEqual(['a'])
  })

  // ─── 新增：deepEqual 顺序无关（P8） ────────────────────────

  it('deepEqual 顺序无关：{a,b} 与 {b,a} 视为相等', async () => {
    storeGet.mockImplementation((key: string) =>
      Promise.resolve(key === 'opts' ? { b: 2, a: 1 } : null),
    )
    const config = defineConfig('cfg-deepequal', { opts: { a: 1, b: 2 } })
    await vi.waitFor(() => expect(storeGet).toHaveBeenCalled())
    await new Promise((r) => setTimeout(r, 10))
    // race 保护：磁盘值与 default 深度相等（顺序无关）→ 不覆盖（保持 default 引用）
    // 但因为深度相等，结果一致
    expect(config.opts).toEqual({ a: 1, b: 2 })
  })

  // ─── 回归：set 异步自回声不覆盖本地 mutate（曾致 ai.env 写空） ──

  it('竞态：onChange 回灌旧快照不覆盖 emit 到达前已 mutate 的新值', async () => {
    storeGet.mockResolvedValue(null)
    const config = defineConfig('cfg-race-emit', { items: ['init'] as string[] })
    await vi.waitFor(() => expect(loadMock).toHaveBeenCalled())
    await new Promise((r) => setTimeout(r, 20)) // isLoading = false

    // 场景：flushSave 已 set 旧快照 ['a','b']，emit 到达前用户又改成 ['a','b','c']
    config.items = ['a', 'b', 'c']

    // 模拟 plugin-store 异步回放旧快照（订阅了 onChange 时 fn 非 null）
    onChangeCb.fn?.('items', ['a', 'b'])
    await nextTick()

    // 期望：旧快照不得覆盖新值
    expect(config.items).toEqual(['a', 'b', 'c'])
  })

  // ─── 回归：持续变更防饿死（agent 流式增量场景） ──────────────

  it('持续变更防饿死：pending 超 2s 强制落盘', async () => {
    vi.useFakeTimers()
    try {
      storeGet.mockResolvedValue(null)
      const config = defineConfig('cfg-maxwait', { items: ['a'] as string[] })
      await whenConfigReady('cfg-maxwait')
      storeSet.mockClear()
      storeSave.mockClear()

      // 持续变更：每 250ms 一次，一直重置防抖（单凭防抖永不落盘）
      config.items.push('b')
      await nextTick()
      for (let i = 0; i < 8; i++) {
        vi.advanceTimersByTime(250)
        config.items.push(`m${i}`)
        await nextTick()
      }

      // t=2000 的变更触发强制 flush（flushSave 内 await 后 set）
      await Promise.resolve()
      await Promise.resolve()
      expect(storeSet).toHaveBeenCalledWith('items', config.items)
      expect(storeSave).toHaveBeenCalled()
    } finally {
      vi.useRealTimers()
    }
  })
})
