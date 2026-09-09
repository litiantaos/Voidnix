import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { nextTick } from 'vue'
import { createPinia, setActivePinia } from 'pinia'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}))
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    label: 'main',
    startDragging: vi.fn(),
    onCloseRequested: () => Promise.resolve(() => {}),
  }),
}))
vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn().mockRejectedValue(new Error('no store')),
}))
vi.mock('@/utils/tauri', () => ({
  isTauri: true,
  hideWindow: vi.fn(),
  showWindow: vi.fn(),
}))

// 隔离重编排 composable：本测试只验证 fullscreen 策略 watch + done 完结路由
const { loadDefaultResults } = vi.hoisted(() => ({ loadDefaultResults: vi.fn() }))
vi.mock('@/composables/useSearchInput', () => ({
  useSearchInput: () => ({
    onInput: vi.fn(),
    handleTagClose: vi.fn(),
    isLoading: false,
    clearSearch: vi.fn(),
    loadDefaultResults,
    activateExtension: vi.fn(),
    goHome: vi.fn(),
    exitExtension: vi.fn(),
    openToolList: vi.fn(),
  }),
}))
vi.mock('@/composables/useResultNavigation', () => ({
  useResultNavigation: () => ({ handleExecute: vi.fn() }),
}))
vi.mock('@/composables/useExtensionHeight', () => ({
  useExtensionHeight: vi.fn(),
}))

import MainView from './MainView.vue'
import WelcomeView from './WelcomeView.vue'
import { useAppStore } from '@/stores/app'
import { useSettingsStore } from '@/stores/settings'
import '@/locales'

const mountedWrappers: ReturnType<typeof mount>[] = []

function mountView() {
  const wrapper = mount(MainView, { attachTo: document.body })
  mountedWrappers.push(wrapper)
  return wrapper
}

describe('MainView 整窗视图完结路由', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    // app store 创建时从 sessionStorage 恢复激活扩展（navigate 重载恢复机制），清掉防跨测试泄漏
    sessionStorage.clear()
    vi.clearAllMocks()
  })
  afterEach(() => {
    while (mountedWrappers.length) mountedWrappers.pop()!.unmount()
  })

  it('配置回填落定前策略静默（默认 onboarded=false 不瞬时激活引导）', () => {
    const appStore = useAppStore()
    mountView()
    // whenConfigReady 的 promise 链尚未走完（同步时刻）：默认值不驱动策略
    expect(appStore.fullscreenView).toBeNull()
  })

  it('未完结 onboarding 且主界面时激活 WelcomeView（Tauri 环境，回填后）', async () => {
    const appStore = useAppStore()
    mountView()
    await flushPromises()
    await nextTick()
    expect(appStore.fullscreenView).toBeTruthy()
  })

  it('设置页重看路径：Escape 完结后回到设置页而非主界面', async () => {
    const appStore = useAppStore()
    mountView()
    await flushPromises()
    await nextTick()
    expect(appStore.fullscreenView).toBeTruthy()

    // 模拟设置页「显示新手引导」写入的完结恢复目标
    appStore.fullscreenReturnExtId = 'settings'

    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await nextTick()

    expect(appStore.activeExtId).toBe('settings')
    expect(appStore.fullscreenReturnExtId).toBeNull()
    expect(appStore.fullscreenView).toBeNull()
    // 回设置页不经主界面：不重载默认列表
    expect(loadDefaultResults).not.toHaveBeenCalled()
    expect(useSettingsStore().onboarded).toBe(true)
  })

  it('首启路径（无恢复目标）：Escape 完结后回主界面并重载默认列表', async () => {
    const appStore = useAppStore()
    mountView()
    await flushPromises()
    await nextTick()
    expect(appStore.fullscreenView).toBeTruthy()

    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await nextTick()

    expect(appStore.activeExtId).toBeNull()
    expect(appStore.fullscreenView).toBeNull()
    expect(loadDefaultResults).toHaveBeenCalledWith(true)
  })

  it('引导期间激活扩展：槽让位且完结恢复目标一并失效', async () => {
    const appStore = useAppStore()
    mountView()
    await flushPromises()
    await nextTick()

    appStore.fullscreenReturnExtId = 'settings'
    appStore.setActiveExtension('agent')
    await nextTick()

    expect(appStore.fullscreenView).toBeNull()
    expect(appStore.fullscreenReturnExtId).toBeNull()
  })

  it('让位后的过期 done：不回主界面收尾', async () => {
    const appStore = useAppStore()
    const wrapper = mountView()
    await flushPromises()
    await nextTick()
    expect(appStore.fullscreenView).toBeTruthy()
    const welcome = wrapper.findComponent(WelcomeView)

    // 扩展快捷键激活让位后到达的 done 已过期（入口守卫防御，不依赖到达途径），
    // 直接模拟该残留 emit
    appStore.setActiveExtension('agent')
    await nextTick()
    expect(appStore.fullscreenView).toBeNull()

    welcome.vm.$emit('done')
    await nextTick()

    // 过期 done 被忽略：留在扩展、不重载默认列表、不抢搜索框焦点
    expect(appStore.activeExtId).toBe('agent')
    expect(loadDefaultResults).not.toHaveBeenCalled()
  })
})
