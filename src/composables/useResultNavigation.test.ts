import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { defineComponent, h, ref, computed } from 'vue'
import { createPinia, setActivePinia } from 'pinia'

// Mock Tauri APIs（composables 内部调用 invoke/hideWindow/open 等）
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}))
vi.mock('@tauri-apps/plugin-shell', () => ({
  open: vi.fn().mockResolvedValue(undefined),
}))
vi.mock('@/utils/tauri', () => ({
  isTauri: false,
  hideWindow: vi.fn(),
}))

// useResultNavigation 通过 getExtension 查找扩展；mock 为最小扩展表
const mockExtensions = new Map<string, { onExecute?: (r: SearchResult) => void }>()
vi.mock('@/runtime/extension-registry', () => ({
  getExtension: (id: string) => mockExtensions.get(id),
  getAllExtensions: () => Array.from(mockExtensions.values()),
}))

import { useResultNavigation } from './useResultNavigation'
import type { SearchResult } from '@/runtime/types'

/// 构造测试用 component，包裹 useResultNavigation 并暴露内部 refs/方法
function makeWrapper(opts: {
  results?: SearchResult[]
  selectedIndex?: number
  activeModuleId?: string | null
}) {
  const results = ref<SearchResult[]>(opts.results ?? [])
  const selectedIndex = ref(opts.selectedIndex ?? 0)
  const activeModuleId = ref<string | null>(opts.activeModuleId ?? null)
  const searchQuery = ref('')
  const clearSearch = vi.fn()
  const loadDefaultResults = vi.fn().mockResolvedValue(undefined)
  const goHome = vi.fn()

  const TestComp = defineComponent({
    setup() {
      const nav = useResultNavigation({
        searchInput: ref(undefined),
        results,
        selectedIndex,
        activeModule: computed(() => null),
        clearSearch,
        loadDefaultResults,
        goHome,
      })
      return { nav }
    },
    render: () => h('div'),
  })

  const wrapper = mount(TestComp)
  return {
    wrapper,
    results,
    selectedIndex,
    activeModuleId,
    searchQuery,
    clearSearch,
    loadDefaultResults,
    goHome,
  }
}

describe('useResultNavigation', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockExtensions.clear()
  })

  describe('handleExecute 分派', () => {
    it('kind=module 触发 setActiveModule + clearSearch', async () => {
      const { wrapper, results, clearSearch } = makeWrapper({
        results: [
          {
            id: 'm1',
            title: 'Calculator',
            module: 'calculator',
            data: { kind: 'module', moduleId: 'calculator' },
          },
        ],
        selectedIndex: 0,
      })
      await wrapper.vm.nav.handleExecute(results.value[0])
      await flushPromises()
      expect(clearSearch).toHaveBeenCalled()
    })

    it('kind=file 调用扩展 onExecute', async () => {
      const onExecute = vi.fn()
      mockExtensions.set('clipboard', { onExecute })
      const result: SearchResult = {
        id: 'c1',
        title: 'foo.txt',
        module: 'clipboard',
        data: { kind: 'file', path: '/tmp/foo.txt' },
      }
      const { wrapper, results } = makeWrapper({ results: [result], selectedIndex: 0 })
      await wrapper.vm.nav.handleExecute(results.value[0])
      await flushPromises()
      expect(onExecute).toHaveBeenCalledWith(result)
    })

    it('未注册扩展的 result 不抛错（onExecute 可选）', async () => {
      const result: SearchResult = {
        id: 'x',
        title: 'x',
        module: 'nonexistent',
        data: { kind: 'file' },
      }
      const { wrapper, results } = makeWrapper({ results: [result] })
      // 不应 throw
      await expect(wrapper.vm.nav.handleExecute(results.value[0])).resolves.toBeUndefined()
    })
  })

  describe('H7：Enter 越界防护', () => {
    it('selectedIndex 越界时 Enter 不崩溃（onKeystroke 内 null 检查）', () => {
      // 模拟竞态：results 缩短但 selectedIndex 未重置
      makeWrapper({
        results: [{ id: 'a', title: 'A', module: 'x', data: { kind: 'file' } }],
        selectedIndex: 5, // 越界
      })
      // onKeyStroke 注册在 document 上；派发 Enter 应被 onKeydown 处理
      // H7 修复：results[5] === undefined → if (!result) return → 不调 handleExecute
      expect(() => {
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
      }).not.toThrow()
    })
  })
})
