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
// 搜索引擎打桩：默认列表内容完全可控，聚焦提示行注入与 openToolList 行为
const searchMock = vi.fn()
vi.mock('@/runtime/search-engine', () => ({
  searchEngine: {
    search: (...args: unknown[]) => searchMock(...args),
    setActiveExtension: vi.fn(),
    abort: vi.fn(),
  },
}))
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
      return { api }
    },
    render: () => h('input', { ref: searchInput }),
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
})
