import { describe, it, expect, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useRecentsStore } from './recents'
import { LIMITS } from '@/runtime/constants'

const KEY = 'voidnix.recent-views'

describe('recents store', () => {
  beforeEach(() => {
    sessionStorage.clear()
    setActivePinia(createPinia())
  })

  it('按屏记录：后写置顶，同屏去重', () => {
    const store = useRecentsStore()
    store.record('agent', null, '')
    store.record('archive', 'entries', 'zip')
    store.record('agent', null, '')

    expect(store.views.map((v) => [v.extId, v.subview])).toEqual([
      ['agent', null],
      ['archive', 'entries'],
    ])
  })

  it('同一扩展的不同屏互不覆盖（主视图 / 子视图是两条记录）', () => {
    const store = useRecentsStore()
    store.record('screenshot', null, '')
    store.record('screenshot', 'ocr', '')

    expect(store.views.map((v) => v.subview)).toEqual(['ocr', null])
  })

  it('超出上限淘汰最旧', () => {
    const store = useRecentsStore()
    for (let i = 0; i < LIMITS.maxRecentViews + 2; i++) store.record(`ext-${i}`, null, '')

    expect(store.views).toHaveLength(LIMITS.maxRecentViews)
    expect(store.views[0].extId).toBe(`ext-${LIMITS.maxRecentViews + 1}`)
    expect(store.views.at(-1)?.extId).toBe('ext-2')
  })

  it('sessionStorage 持久化，新 store 恢复（navigate 重载路径，含过滤词）', () => {
    useRecentsStore().record('clipboard', null, 'foo')
    useRecentsStore().record('screenshot', 'ocr', 'bar')

    setActivePinia(createPinia())
    const restored = useRecentsStore()
    expect(restored.views.map((v) => [v.extId, v.subview, v.query])).toEqual([
      ['screenshot', 'ocr', 'bar'],
      ['clipboard', null, 'foo'],
    ])
  })

  it('内容摘要：随记录持久化，折叠空白并截断上限', () => {
    const store = useRecentsStore()
    const long = 'x'.repeat(200)
    store.record('archive', null, '', `  report.zip\n· 12 项  ${long} `)
    const summary = store.views[0].summary
    expect(summary.startsWith('report.zip · 12 项 x')).toBe(true)
    expect(summary.length).toBeLessThanOrEqual(80)

    setActivePinia(createPinia())
    expect(useRecentsStore().views[0].summary).toBe(summary)
  })

  it('同屏重进更新摘要（旧摘要不残留）', () => {
    const store = useRecentsStore()
    store.record('archive', null, '', 'a.zip')
    store.record('archive', null, '', 'b.zip')

    expect(store.views).toHaveLength(1)
    expect(store.views[0].summary).toBe('b.zip')
  })

  it('旧记录无 summary 字段 → 空串（向后兼容）', () => {
    sessionStorage.setItem(KEY, JSON.stringify([{ extId: 'agent', subview: null, query: 'q' }]))
    expect(useRecentsStore().views[0].summary).toBe('')
  })

  it('clear 同时清空内存与 sessionStorage（自测等干净起点场景）', () => {
    const store = useRecentsStore()
    store.record('agent', null, '')
    store.clear()

    expect(store.views).toEqual([])
    expect(sessionStorage.getItem(KEY)).toBeNull()
  })

  it('损坏 / 非法结构静默降级为空', () => {
    sessionStorage.setItem(KEY, '{"bad"')
    setActivePinia(createPinia())
    expect(useRecentsStore().views).toEqual([])
  })

  it('跳过非法记录（非对象 / 缺 extId），query 非法回退空串', () => {
    sessionStorage.setItem(
      KEY,
      JSON.stringify([
        null,
        { subview: 'ocr' },
        { extId: 'agent' },
        { extId: 'notes', subview: 3, query: 42 },
      ]),
    )
    const store = useRecentsStore()
    expect(store.views.map((v) => [v.extId, v.subview, v.query])).toEqual([
      ['agent', null, ''],
      ['notes', null, ''],
    ])
  })
})
