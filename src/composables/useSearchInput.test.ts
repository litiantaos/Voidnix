import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { defineComponent, h, ref, computed } from 'vue'
import { createPinia, setActivePinia } from 'pinia'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}))
vi.mock('@/utils/tauri', () => ({
  isTauri: true,
  hideWindow: vi.fn(),
  showWindow: vi.fn().mockResolvedValue(undefined),
}))
// 搜索引擎打桩：默认列表内容完全可控，聚焦提示行注入与 openToolList 行为。
// 其余纯函数（getGroupKey 等）保留真实实现（stableMerge 消费）
const searchMock = vi.fn()
vi.mock('@/runtime/search-engine', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/runtime/search-engine')>()
  return {
    ...actual,
    searchEngine: {
      search: (...args: unknown[]) => searchMock(...args),
      setActiveExtension: vi.fn(),
      abort: vi.fn(),
      clearResultCache: vi.fn(),
    },
  }
})
// 扩展表打桩：两个可见扩展（工具列表断言用）
const mockExts = [
  { meta: { id: 'ext-a', name: 'Ext A', icon: 'i-a', order: 1 } },
  { meta: { id: 'ext-b', name: 'Ext B', icon: 'i-b', order: 2 } },
]
vi.mock('@/runtime/extension-registry', () => ({
  getAllExtensions: () => mockExts,
  getExtension: (id: string) => mockExts.find((e) => e.meta.id === id),
}))

import { useSearchInput } from './useSearchInput'
import { useAppStore } from '@/stores/app'
import { SEARCH } from '@/runtime/constants'
import { invoke } from '@tauri-apps/api/core'
import type { SearchResult } from '@/runtime/types'

const appResult: SearchResult = {
  id: 'app-1',
  title: 'App',
  extId: 'search',
  data: { kind: 'application' },
}

function makeWrapper() {
  const searchInput = ref<HTMLInputElement>()
  const results = ref<SearchResult[]>([])
  const selectedIndex = ref(0)

  const TestComp = defineComponent({
    setup() {
      const api = useSearchInput({
        searchInput,
        results,
        selectedIndex,
        activeExtension: computed(() => null),
        reset: () => {},
      })
      return { api, onInput: api.onInput }
    },
    render() {
      return h('input', { ref: searchInput, onInput: this.onInput })
    },
  })
  const wrapper = mount(TestComp)
  // 累积卸载：composable 在 window 上注册 window-invoked 等全局监听，
  // 不卸载会跨用例累积（一次 dispatch 触发多份 maybeFill）
  mountedWrappers.push(wrapper)
  return { wrapper, results, selectedIndex, searchInput }
}

const mountedWrappers: ReturnType<typeof mount>[] = []

describe('useSearchInput 默认列表提示行', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    searchMock.mockReset()
  })
  afterEach(() => {
    while (mountedWrappers.length) mountedWrappers.pop()!.unmount()
  })

  it('空查询默认列表尾部固定追加工具提示行（增量与最终一致）', async () => {
    searchMock.mockImplementation(async (_q: string, onUpdate?: (r: SearchResult[]) => void) => {
      onUpdate?.([appResult])
      return [appResult]
    })
    const { results } = makeWrapper()
    await flushPromises()

    expect(results.value).toHaveLength(2)
    expect(results.value[0].id).toBe('app-1')
    const hint = results.value[1]
    expect(hint.id).toBe(SEARCH.TOOLS_HINT_ID)
    expect(hint.data?.kind).toBe('extension')
    // 合成行无 extId（不触发框架扩展激活分派）
    expect(hint.data?.extId).toBeUndefined()
  })

  it('搜索异常时提示行仍在（不空屏）', async () => {
    searchMock.mockRejectedValue(new Error('boom'))
    const { results } = makeWrapper()
    await flushPromises()

    expect(results.value).toHaveLength(1)
    expect(results.value[0].id).toBe(SEARCH.TOOLS_HINT_ID)
  })

  it('openToolList：query 置 /、结果换为工具列表、输入框同步', async () => {
    searchMock.mockResolvedValue([appResult])
    const appStore = useAppStore()
    const { wrapper, results, searchInput } = makeWrapper()
    await flushPromises()

    wrapper.vm.api.openToolList()

    expect(appStore.searchQuery).toBe('/')
    expect(searchInput.value?.value).toBe('/')
    // 工具列表 = 可见扩展入口（不含提示行）
    expect(results.value.map((r) => r.id)).toEqual(['ext-entry-ext-a', 'ext-entry-ext-b'])
  })

  it('整窗视图接管时 window-invoked 不触发剪贴板填充（防 query 污染）', async () => {
    const appStore = useAppStore()
    makeWrapper()
    await flushPromises()
    vi.mocked(invoke).mockClear()

    // fullscreen 激活：搜索栏仅 v-show 隐藏，元素仍在——须由 fullscreenView 守卫拦截
    appStore.setFullscreenView(defineComponent({ render: () => null }))
    window.dispatchEvent(new CustomEvent('window-invoked'))
    await flushPromises()
    expect(vi.mocked(invoke)).not.toHaveBeenCalled()

    // 退出接管后恢复填充链路（invoke 被调用）
    appStore.setFullscreenView(null)
    window.dispatchEvent(new CustomEvent('window-invoked'))
    await flushPromises()
    expect(vi.mocked(invoke)).toHaveBeenCalled()
  })

  it('扩展激活时 window-invoked 不触发剪贴板填充（query 是扩展过滤参数，显隐不改内容状态）', async () => {
    const appStore = useAppStore()
    makeWrapper()
    await flushPromises()
    vi.mocked(invoke).mockClear()

    // 扩展激活（含 disableSearchInput 与否）：填充会污染扩展 query 过滤浏览上下文
    appStore.setActiveExtension('agent')
    window.dispatchEvent(new CustomEvent('window-invoked'))
    await flushPromises()
    expect(vi.mocked(invoke)).not.toHaveBeenCalled()

    // 回主界面恢复填充链路
    appStore.setActiveExtension(null)
    window.dispatchEvent(new CustomEvent('window-invoked'))
    await flushPromises()
    expect(vi.mocked(invoke)).toHaveBeenCalled()
  })
})

describe('useSearchInput 逐键搜索稳定合并', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    searchMock.mockReset()
  })
  afterEach(() => {
    while (mountedWrappers.length) mountedWrappers.pop()!.unmount()
  })

  it('partial 稳定合并：prev 条目保留、新条目追加同组尾部，不被 partial 整表替换（防闪烁回归）', async () => {
    vi.useFakeTimers()
    const fileA: SearchResult = {
      id: 'f1',
      title: 'alpha.txt',
      extId: 'search',
      data: { kind: 'file' },
    }
    const clipA: SearchResult = {
      id: 'c1',
      title: 'alpha note',
      extId: 'clipboard',
      data: { kind: 'clipboard' },
    }
    const clipB: SearchResult = {
      id: 'c2',
      title: 'alpha beta',
      extId: 'clipboard',
      data: { kind: 'clipboard' },
    }
    let releaseFinal!: () => void
    searchMock.mockImplementation(async (q: string, onUpdate?: (r: SearchResult[]) => void) => {
      if (q === 'al') return [fileA, clipA]
      if (q === 'alpha') {
        // partial：仅同步缓存 clipboard 到达（文件索引后至），final 挂起模拟慢扩展
        onUpdate?.([clipA, clipB])
        await new Promise<void>((r) => (releaseFinal = r))
        return [fileA, clipA, clipB]
      }
      return []
    })
    const { wrapper, results } = makeWrapper()
    await flushPromises()

    const input = wrapper.find('input')
    await input.setValue('al')
    await vi.advanceTimersByTimeAsync(30)
    expect(results.value.map((r) => r.id)).toEqual(['f1', 'c1'])

    await input.setValue('alpha')
    await vi.advanceTimersByTimeAsync(30)
    // partial 阶段（final 挂起）：f1 不因缺席消失（缺席=未到达）、c2 追加 clipboard 组尾；
    // 旧实现此处被整表替换为 [c1,c2]——文件行闪失再在 final 恢复，即逐键闪烁
    expect(results.value.map((r) => r.id)).toEqual(['f1', 'c1', 'c2'])

    releaseFinal()
    await flushPromises()
    expect(results.value.map((r) => r.id)).toEqual(['f1', 'c1', 'c2'])
    vi.useRealTimers()
  })
})

describe('useSearchInput 窗口重新获焦重跑', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    searchMock.mockReset()
  })
  afterEach(() => {
    while (mountedWrappers.length) mountedWrappers.pop()!.unmount()
  })

  function fileResults(prefix: string, n: number): SearchResult[] {
    return Array.from({ length: n }, (_, i) => ({
      id: `${prefix}-${i}`,
      title: `${prefix} ${i}`,
      extId: 'search',
      data: { kind: 'file' },
    }))
  }

  it('静默重跑：增量 partial 不替换已显示列表，最终结果一次性到达且选中保留', async () => {
    const appStore = useAppStore()
    const { results, selectedIndex } = makeWrapper()
    await flushPromises()

    // 模拟隐藏前的文件搜索状态：8 项结果、选中第 6 项（文件组区域，部分 partial 会越界）
    appStore.setSearchQuery('re')
    results.value = fileResults('old', 8)
    selectedIndex.value = 5

    // 重跑 mock：先吐较短 partial（应用组），最终异步返回完整列表
    let resolveFinal!: (r: SearchResult[]) => void
    const finalList = fileResults('new', 8)
    searchMock.mockImplementation(async (_q: string, onUpdate?: (r: SearchResult[]) => void) => {
      // partial 比旧列表短：若被应用，clampSelected 会把选中归零（回归即失败）
      onUpdate?.([appResult])
      return new Promise<SearchResult[]>((res) => (resolveFinal = res))
    })

    window.dispatchEvent(new CustomEvent('window-focused'))
    await flushPromises()

    // 重跑不携带 onUpdate（无增量替换）：partial 到达时旧列表原样保留，滚动/选中不受扰
    const rerunCall = searchMock.mock.calls.find((c) => c[0] === 're')
    expect(rerunCall?.[1]).toBeUndefined()
    expect(results.value.map((r) => r.id)).toEqual(fileResults('old', 8).map((r) => r.id))
    expect(selectedIndex.value).toBe(5)

    resolveFinal(finalList)
    await flushPromises()
    expect(results.value.map((r) => r.id)).toEqual(finalList.map((r) => r.id))
    expect(selectedIndex.value).toBe(5)
  })

  it('上次结果为空时获焦重跑退回流式（loading 占位链路可用）', async () => {
    const appStore = useAppStore()
    const { results } = makeWrapper()
    await flushPromises()

    appStore.setSearchQuery('re')
    results.value = []
    searchMock.mockImplementation(async (_q: string, onUpdate?: (r: SearchResult[]) => void) => {
      onUpdate?.([appResult])
      return [appResult]
    })

    window.dispatchEvent(new CustomEvent('window-focused'))
    await flushPromises()

    const rerunCall = searchMock.mock.calls.find((c) => c[0] === 're')
    expect(typeof rerunCall?.[1]).toBe('function')
    expect(results.value).toHaveLength(1)
  })

  it('同 query 重跑零重排：usage/recency 升位不改当前会话排序，选中原地不跳位', async () => {
    const appStore = useAppStore()
    const { results, selectedIndex } = makeWrapper()
    await flushPromises()

    // 隐藏前：8 项，选中 f-5
    appStore.setSearchQuery('re')
    results.value = fileResults('f', 8)
    selectedIndex.value = 5

    // 重跑返回重排列表（模拟 increment_use_count 后 f-5 frequency 升位到第 2）
    const reordered = [...fileResults('f', 8)]
    const [picked] = reordered.splice(5, 1)
    reordered.splice(1, 0, picked)
    searchMock.mockResolvedValue(reordered)

    window.dispatchEvent(new CustomEvent('window-focused'))
    await flushPromises()

    // 顺序保持隐藏前列表（内容刷新、顺序稳定），选中原地
    expect(results.value.map((r) => r.id)).toEqual(fileResults('f', 8).map((r) => r.id))
    expect(selectedIndex.value).toBe(5)
    expect(results.value[selectedIndex.value]?.id).toBe('f-5')
  })

  it('重跑新增条目注入同组尾部：不扰动现有序与选中', async () => {
    const appStore = useAppStore()
    const { results, selectedIndex } = makeWrapper()
    await flushPromises()

    appStore.setSearchQuery('re')
    results.value = fileResults('f', 8)
    selectedIndex.value = 5

    // 重跑结果：原有 8 项 + 新文件项 + 新组条目（隐藏期间新增的剪贴板记录）
    const newFile: SearchResult = {
      id: 'f-new',
      title: 'New',
      extId: 'search',
      data: { kind: 'file' },
    }
    const newClip: SearchResult = {
      id: 'clip-1',
      title: 'Clip',
      extId: 'clipboard',
      data: { kind: 'clipboard' },
    }
    searchMock.mockResolvedValue([newFile, ...fileResults('f', 8), newClip])

    window.dispatchEvent(new CustomEvent('window-focused'))
    await flushPromises()

    // 新文件项插到 file 组末尾、新组条目追加尾部；现有序与选中不动
    expect(results.value.map((r) => r.id)).toEqual([
      ...fileResults('f', 8).map((r) => r.id),
      'f-new',
      'clip-1',
    ])
    expect(selectedIndex.value).toBe(5)
  })

  it('选中条目从新列表消失时回退 clamp（索引有效保留，越界归零）', async () => {
    const appStore = useAppStore()
    const { results, selectedIndex } = makeWrapper()
    await flushPromises()

    appStore.setSearchQuery('re')
    results.value = fileResults('f', 8)
    selectedIndex.value = 7

    // 重跑结果不含 f-7 且更短（越界归零）
    searchMock.mockResolvedValue(fileResults('f', 5))

    window.dispatchEvent(new CustomEvent('window-focused'))
    await flushPromises()

    expect(selectedIndex.value).toBe(0)
  })

  it('空 query 获焦刷新默认列表：重排被稳定合并抑制、partial 缺席不收缩列表', async () => {
    const appStore = useAppStore()
    const { results, selectedIndex } = makeWrapper()
    await flushPromises()

    const appA: SearchResult = {
      id: 'app-a',
      title: 'A',
      extId: 'search',
      data: { kind: 'application' },
    }
    const appB: SearchResult = {
      id: 'app-b',
      title: 'B',
      extId: 'search',
      data: { kind: 'application' },
    }
    appStore.setSearchQuery('')
    results.value = [appA, appB]
    selectedIndex.value = 0

    // 刷新：partial 只到 B（A 未到达，不收缩），final 返回 recency 重排（B 升首位）
    let afterPartial: () => void = () => {}
    const partialSeen = new Promise<void>((res) => (afterPartial = res))
    searchMock.mockImplementation(async (_q: string, onUpdate?: (r: SearchResult[]) => void) => {
      onUpdate?.([appB])
      afterPartial()
      return [appB, appA]
    })

    window.dispatchEvent(new CustomEvent('window-focused'))
    await partialSeen
    await flushPromises()

    // partial 阶段：A 未到不视为消失，列表序保持 [app-a, app-b, hint]
    expect(results.value.map((r) => r.id)).toEqual(['app-a', 'app-b', SEARCH.TOOLS_HINT_ID])
    // final 阶段：重排被抑制，顺序仍保持隐藏前
    expect(results.value.map((r) => r.id)).toEqual(['app-a', 'app-b', SEARCH.TOOLS_HINT_ID])
    expect(selectedIndex.value).toBe(0)
    expect(results.value[selectedIndex.value]?.id).toBe('app-a')
  })

  it('后台刷新失败保留现有列表（不闪空态）；转移入口失败落提示行兜底', async () => {
    const appStore = useAppStore()
    const { wrapper, results } = makeWrapper()
    await flushPromises()

    appStore.setSearchQuery('')
    results.value = fileResults('f', 3)
    searchMock.mockRejectedValue(new Error('boom'))

    // 获焦刷新（后台，resetSelection=false）失败：列表原样保留
    window.dispatchEvent(new CustomEvent('window-focused'))
    await flushPromises()
    expect(results.value.map((r) => r.id)).toEqual(fileResults('f', 3).map((r) => r.id))

    // 转移入口（resetSelection=true）失败：清为提示行单行（不残留旧上下文列表）
    await wrapper.vm.api.loadDefaultResults(true)
    await flushPromises()
    expect(results.value.map((r) => r.id)).toEqual([SEARCH.TOOLS_HINT_ID])
  })
})

describe('useSearchInput 唤起全选时序', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    searchMock.mockReset()
    searchMock.mockResolvedValue([])
  })
  afterEach(() => {
    while (mountedWrappers.length) mountedWrappers.pop()!.unmount()
    vi.restoreAllMocks()
  })

  it('页面焦点未落定时延迟全选：原生 focus 事件（页面获焦）到达才 select，避免灰→蓝跳变', async () => {
    const appStore = useAppStore()
    const { searchInput } = makeWrapper()
    await flushPromises()
    const el = searchInput.value!
    el.value = 're'
    appStore.setSearchQuery('re')
    vi.spyOn(document, 'hasFocus').mockReturnValue(false)

    window.dispatchEvent(new CustomEvent('window-focused'))
    await flushPromises()
    // 页面未获焦不 select——此刻 select 会以非聚焦选中色（灰）绘制，获焦后翻蓝
    expect(el.selectionStart).toBe(el.selectionEnd)

    // 原生 focus = 页面获得焦点：blur → 全选 → focus 三步同步落位（无动画），一次到位
    const focusSpy = vi.spyOn(el, 'focus')
    window.dispatchEvent(new Event('focus'))
    await flushPromises()
    expect(el.selectionStart).toBe(0)
    expect(el.selectionEnd).toBe(2)
    // 回焦断言：三步落位末位 el.focus()（不丢输入焦点，后续输入直接替换全选内容）
    expect(focusSpy).toHaveBeenCalled()
  })

  it('focus 事件迟发时 rAF 轮询兜底：hasFocus 状态翻转即落位，不等事件', async () => {
    const appStore = useAppStore()
    const { searchInput } = makeWrapper()
    await flushPromises()
    const el = searchInput.value!
    el.value = 're'
    appStore.setSearchQuery('re')
    const hasFocus = vi.spyOn(document, 'hasFocus').mockReturnValue(false)

    window.dispatchEvent(new CustomEvent('window-focused'))
    await flushPromises()
    expect(el.selectionStart).toBe(el.selectionEnd)

    // 键盘输入可达 = 焦点状态已翻转（事件迟发）：轮询通道在下一帧捕获并落位
    hasFocus.mockReturnValue(true)
    for (let i = 0; i < 3 && el.selectionStart === el.selectionEnd; i++) {
      await new Promise<void>((r) => requestAnimationFrame(() => r()))
    }
    expect(el.selectionStart).toBe(0)
    expect(el.selectionEnd).toBe(2)
  })

  it('show 过程的瞬时页面 blur 不取消待定全选（取消只归 window-hiding）', async () => {
    const appStore = useAppStore()
    const { searchInput } = makeWrapper()
    await flushPromises()
    const el = searchInput.value!
    el.value = 're'
    appStore.setSearchQuery('re')
    const hasFocus = vi.spyOn(document, 'hasFocus').mockReturnValue(false)

    window.dispatchEvent(new CustomEvent('window-focused'))
    await flushPromises()
    // 面板 key 迁移产生的瞬时页面 blur（此前版本会误杀待定全选）
    window.dispatchEvent(new Event('blur'))
    await flushPromises()

    hasFocus.mockReturnValue(true)
    for (let i = 0; i < 3 && el.selectionStart === el.selectionEnd; i++) {
      await new Promise<void>((r) => requestAnimationFrame(() => r()))
    }
    expect(el.selectionStart).toBe(0)
    expect(el.selectionEnd).toBe(2)
  })

  it('页面已聚焦走快路径立即全选（用户交互路径无延迟）', async () => {
    const appStore = useAppStore()
    const { searchInput } = makeWrapper()
    await flushPromises()
    const el = searchInput.value!
    el.value = 're'
    appStore.setSearchQuery('re')
    vi.spyOn(document, 'hasFocus').mockReturnValue(true)

    window.dispatchEvent(new CustomEvent('window-focused'))
    await flushPromises()
    expect(el.selectionStart).toBe(0)
    expect(el.selectionEnd).toBe(2)
  })

  it('隐藏折叠残留选中并取消待定全选：隐藏后 focus 不再触发 select', async () => {
    const appStore = useAppStore()
    const { searchInput } = makeWrapper()
    await flushPromises()
    const el = searchInput.value!
    el.value = 're'
    appStore.setSearchQuery('re')
    el.setSelectionRange(0, 2)
    vi.spyOn(document, 'hasFocus').mockReturnValue(false)

    // 待定全选挂起 + 残留全选存在
    window.dispatchEvent(new CustomEvent('window-focused'))
    await flushPromises()

    window.dispatchEvent(new CustomEvent('window-hiding'))
    // 残留选中折叠为光标（防下次唤起首帧灰绘制）
    expect(el.selectionStart).toBe(el.selectionEnd)

    // 待定全选已取消：隐藏后到达的 focus 不触发 select
    window.dispatchEvent(new Event('focus'))
    await flushPromises()
    expect(el.selectionStart).toBe(el.selectionEnd)
  })
})

describe('useSearchInput 唤起聚焦让位（模态弹窗打开）', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    searchMock.mockReset()
  })
  afterEach(() => {
    while (mountedWrappers.length) mountedWrappers.pop()!.unmount()
    document.querySelectorAll('[role="dialog"]').forEach((el) => el.remove())
  })

  it('模态弹窗打开时 window-focused 不抢搜索框焦点（弹窗按钮持有焦点），弹窗移除后恢复', async () => {
    // 场景：菜单栏「检查更新」唤起窗口的同时弹 UpdateDialog——弹窗 mount 时聚焦按钮，
    // 唤起链路（window-focused → focusHandler）不得把焦点抢回搜索框
    const searchInput = ref<HTMLInputElement>()
    const TestComp = defineComponent({
      setup() {
        useSearchInput({
          searchInput,
          results: ref<SearchResult[]>([]),
          selectedIndex: ref(0),
          activeExtension: computed(() => null),
          reset: () => {},
        })
        return {}
      },
      render: () => h('input', { ref: searchInput }),
    })
    mountedWrappers.push(mount(TestComp, { attachTo: document.body }))
    await flushPromises()

    // 模拟 BaseDialog：Teleport body + role=dialog + aria-modal，mount 时已聚焦确认按钮
    const modal = document.createElement('div')
    modal.setAttribute('role', 'dialog')
    modal.setAttribute('aria-modal', 'true')
    const btn = document.createElement('button')
    modal.appendChild(btn)
    document.body.appendChild(modal)
    btn.focus()
    expect(document.activeElement).toBe(btn)

    window.dispatchEvent(new CustomEvent('window-focused'))
    await flushPromises()
    expect(document.activeElement).toBe(btn)

    // 弹窗移除后唤起链路恢复聚焦搜索框
    modal.remove()
    window.dispatchEvent(new CustomEvent('window-focused'))
    await flushPromises()
    expect(document.activeElement).toBe(searchInput.value)
  })
})
