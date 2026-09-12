import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
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

import { hideWindow } from '@/utils/tauri'

// useResultNavigation 通过 getExtension 查找扩展；mock 为最小扩展表
const mockExtensions = new Map<string, { onExecute?: (r: SearchResult) => void }>()
vi.mock('@/runtime/extension-registry', () => ({
  getExtension: (id: string) => mockExtensions.get(id),
  getAllExtensions: () => Array.from(mockExtensions.values()),
}))

import { useResultNavigation } from './useResultNavigation'
import { useAppStore } from '@/stores/app'
import type { SearchResult } from '@/runtime/types'

/// 累积 wrapper：afterEach 统一 unmount，清理 onKeyStroke 注册的 document listener，
/// 避免跨用例累积导致 dispatchEvent 触发多个 listener 干扰断言。
const mountedWrappers: ReturnType<typeof mount>[] = []

/// 构造测试用 component，包裹 useResultNavigation 并暴露内部 refs/方法
function makeWrapper(opts: {
  results?: SearchResult[]
  selectedIndex?: number
  activeExtId?: string | null
}) {
  const results = ref<SearchResult[]>(opts.results ?? [])
  const selectedIndex = ref(opts.selectedIndex ?? 0)
  const activeExtId = ref<string | null>(opts.activeExtId ?? null)
  const searchQuery = ref('')
  const clearSearch = vi.fn()
  const loadDefaultResults = vi.fn().mockResolvedValue(undefined)
  const goHome = vi.fn()
  const exitExtension = vi.fn()
  const activateExtension = vi.fn()

  const TestComp = defineComponent({
    setup() {
      const nav = useResultNavigation({
        results,
        selectedIndex,
        activeExtension: computed(() => null),
        clearSearch,
        loadDefaultResults,
        activateExtension,
        goHome,
        exitExtension,
      })
      return { nav }
    },
    render: () => h('div'),
  })

  const wrapper = mount(TestComp)
  mountedWrappers.push(wrapper)
  return {
    wrapper,
    results,
    selectedIndex,
    activeExtId,
    searchQuery,
    clearSearch,
    loadDefaultResults,
    activateExtension,
    goHome,
    exitExtension,
  }
}

describe('useResultNavigation', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockExtensions.clear()
    vi.clearAllMocks()
  })
  afterEach(() => {
    while (mountedWrappers.length) mountedWrappers.pop()!.unmount()
  })

  describe('handleExecute 分派', () => {
    it('kind=extension 触发 activateExtension', async () => {
      const { wrapper, results, activateExtension } = makeWrapper({
        results: [
          {
            id: 'm1',
            title: 'Calculator',
            extId: 'calculator',
            data: { kind: 'extension', extId: 'calculator' },
          },
        ],
        selectedIndex: 0,
      })
      await wrapper.vm.nav.handleExecute(results.value[0])
      await flushPromises()
      expect(activateExtension).toHaveBeenCalledWith('calculator')
    })

    it('工具提示行（TOOLS_HINT_ID）触发 openToolList，不走扩展激活', async () => {
      const openToolList = vi.fn()
      const hint: SearchResult = {
        id: 'voidnix:tools-hint',
        title: '输入 / 浏览全部工具',
        extId: 'voidnix',
        data: { kind: 'extension' },
      }
      const results = ref<SearchResult[]>([hint])
      const selectedIndex = ref(0)

      const TestComp = defineComponent({
        setup() {
          const nav = useResultNavigation({
            results,
            selectedIndex,
            activeExtension: computed(() => null),
            clearSearch: vi.fn(),
            loadDefaultResults: vi.fn().mockResolvedValue(undefined),
            activateExtension: vi.fn(),
            goHome: vi.fn(),
            exitExtension: vi.fn(),
            openToolList,
          })
          return { nav }
        },
        render: () => h('div'),
      })
      const wrapper = mount(TestComp)
      mountedWrappers.push(wrapper)

      await wrapper.vm.nav.handleExecute(hint)
      expect(openToolList).toHaveBeenCalledOnce()
    })

    it('kind=file 调用扩展 onExecute', async () => {
      const onExecute = vi.fn()
      mockExtensions.set('clipboard', { onExecute })
      const result: SearchResult = {
        id: 'c1',
        title: 'foo.txt',
        extId: 'clipboard',
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
        extId: 'nonexistent',
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
        results: [{ id: 'a', title: 'A', extId: 'x', data: { kind: 'file' } }],
        selectedIndex: 5, // 越界
      })
      // onKeyStroke 注册在 document 上；派发 Enter 应被 onKeydown 处理
      // H7 修复：results[5] === undefined → if (!result) return → 不调 handleExecute
      expect(() => {
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
      }).not.toThrow()
    })
  })

  describe('整窗视图接管：正常层键盘整体让位', () => {
    function makeFullscreenWrapper() {
      const results = ref<SearchResult[]>([
        { id: 'app', title: 'App', extId: 'search', data: { kind: 'application' } },
      ])
      const selectedIndex = ref(0)

      const TestComp = defineComponent({
        setup() {
          const nav = useResultNavigation({
            results,
            selectedIndex,
            activeExtension: computed(() => null),
            clearSearch: vi.fn(),
            loadDefaultResults: vi.fn().mockResolvedValue(undefined),
            activateExtension: vi.fn(),
            goHome: vi.fn(),
            exitExtension: vi.fn(),
          })
          return { nav }
        },
        render: () => h('div'),
      })
      const wrapper = mount(TestComp)
      mountedWrappers.push(wrapper)
      const store = useAppStore()
      return { store }
    }

    it('fullscreenView 激活时 Enter/Esc 不触发导航与藏窗', () => {
      const { store } = makeFullscreenWrapper()
      store.setFullscreenView(defineComponent({ render: () => h('div') }))

      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))

      expect(hideWindow).not.toHaveBeenCalled()
    })

    it('fullscreenView 清空后键盘恢复响应（Esc 藏窗）', () => {
      const { store } = makeFullscreenWrapper()
      store.setFullscreenView(defineComponent({ render: () => h('div') }))
      store.setFullscreenView(null)

      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
      expect(hideWindow).toHaveBeenCalledOnce()
    })
  })

  describe('模态弹窗打开：正常层键盘导航让位', () => {
    // 场景：弹窗打开但焦点不在弹窗内（如菜单栏「检查更新」唤起时被搜索框聚焦链路抢占），
    // 回车不得执行列表选中项——让位给弹窗（焦点在弹窗内时 BaseDialog 自身 stopPropagation）
    function mountModal() {
      const el = document.createElement('div')
      el.setAttribute('role', 'dialog')
      el.setAttribute('aria-modal', 'true')
      document.body.appendChild(el)
      return () => el.remove()
    }

    it('模态期间 Enter 不执行列表项、ArrowDown 不动选中、Escape 不藏窗；弹窗移除后恢复', () => {
      const ctx = makeWrapper({
        results: [
          {
            id: 'm1',
            title: 'M',
            extId: 'calculator',
            data: { kind: 'extension', extId: 'calculator' },
          },
        ],
        selectedIndex: 0,
      })
      const removeModal = mountModal()

      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))

      expect(ctx.activateExtension).not.toHaveBeenCalled()
      expect(ctx.selectedIndex.value).toBe(0)
      expect(hideWindow).not.toHaveBeenCalled()

      removeModal()
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
      expect(hideWindow).toHaveBeenCalledOnce()
    })
  })

  describe('Escape：统一退出当前层', () => {
    // esc 统一退出当前层：事件到达即退出（输入框聚焦也直接退出，不先失焦）。
    function makeEscapeDispatcher() {
      const ctx = makeWrapper({})
      const appStore = useAppStore()
      const dispatch = (key: string) =>
        document.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true }))
      return { exitExtension: ctx.exitExtension, appStore, dispatch }
    }

    it('扩展 mainView esc → exitExtension', () => {
      const { exitExtension, appStore, dispatch } = makeEscapeDispatcher()
      appStore.activeExtId = 'translate'
      appStore.activeSubview = null

      dispatch('Escape')

      expect(exitExtension).toHaveBeenCalledOnce()
    })

    it('功能/设置子视图 esc → closeSubview（不区分控件类型）', () => {
      const { exitExtension, appStore, dispatch } = makeEscapeDispatcher()
      appStore.activeExtId = 'screenshot'
      appStore.activeSubview = 'config'

      dispatch('Escape')

      expect(exitExtension).not.toHaveBeenCalled()
      expect(appStore.activeSubview).toBeNull()
    })

    it('全局模式 esc → hideWindow', () => {
      const { exitExtension, appStore, dispatch } = makeEscapeDispatcher()
      appStore.activeExtId = null
      appStore.activeSubview = null

      dispatch('Escape')

      expect(exitExtension).not.toHaveBeenCalled()
      expect(hideWindow).toHaveBeenCalledOnce()
    })
  })
})
