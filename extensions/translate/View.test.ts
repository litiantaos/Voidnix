import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { nextTick } from 'vue'

// View 层聚焦时序测试：快捷键唤起（窗口从隐藏 show）时 window-focused 事件先于
// WebKit 页面焦点状态翻转到达——document.hasFocus() 仍为 false。onWinFocused 若经
// maybeFocusInput 的 hasFocus 守卫会永久错过聚焦（输入框无焦，回车落到 body 被
// 结果列表执行 copyAndHide 误触关窗）。Tauri invoke / 事件 / 扩展状态全部 mock。
const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(async () => () => {}),
  refreshEnvSnapshot: vi.fn(async () => {}),
  appStore: {
    activeExtId: 'translate',
    activeSubview: null,
    isDialogOpen: false,
    showStatus: vi.fn(),
  },
}))

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }))
vi.mock('@tauri-apps/api/event', () => ({ listen: mocks.listen }))
vi.mock('@/runtime/ai-providers', () => ({ refreshEnvSnapshot: mocks.refreshEnvSnapshot }))
vi.mock('./config', () => ({
  config: {
    configs: [{ type: 'youdao', appKey: 'k', appSecret: 's' }],
  },
  resolveAiTargets: () => [],
}))
vi.mock('@/stores/app', () => ({
  useAppStore: () => mocks.appStore,
  // ./index 顶层经 defineExtension 引用（快捷键 toggle 构造），仅注册不执行
  makeToggleHandler: () => () => {},
  copyAndHide: vi.fn(),
}))

import '@/locales'
import './locales'
import { isTranslating } from './index'
import View from './View.vue'

function mountView() {
  // attachTo：detached 树内 focus() 不改变 document.activeElement，需挂到 body
  return mount(View, { attachTo: document.body })
}

describe('translate View 聚焦', () => {
  let hasFocusSpy: ReturnType<typeof vi.spyOn> | null = null

  beforeEach(() => {
    vi.clearAllMocks()
    isTranslating.value = false
  })
  afterEach(() => {
    // 断言失败也恢复，防 mock 泄漏影响后续用例
    hasFocusSpy?.mockRestore()
    hasFocusSpy = null
  })

  it('window-focused 先于页面焦点翻转到达（hasFocus 仍 false）时仍聚焦输入框', async () => {
    hasFocusSpy = vi.spyOn(document, 'hasFocus').mockReturnValue(false)
    const wrapper = mountView()
    // onMounted 异步链（refreshEnvSnapshot → envTouched → isConfigured → textarea 渲染）
    await flushPromises()
    await nextTick()

    window.dispatchEvent(new Event('window-focused'))
    await nextTick()
    expect(document.activeElement?.tagName).toBe('TEXTAREA')

    wrapper.unmount()
  })

  it('翻译进行中（isTranslating）window-focused 不抢焦点（选词翻译让位语义）', async () => {
    isTranslating.value = true
    const wrapper = mountView()
    await flushPromises()
    await nextTick()

    window.dispatchEvent(new Event('window-focused'))
    await nextTick()
    expect(document.activeElement?.tagName).not.toBe('TEXTAREA')

    wrapper.unmount()
  })
})
