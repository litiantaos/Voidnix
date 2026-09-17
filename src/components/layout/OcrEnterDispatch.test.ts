import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { nextTick } from 'vue'
import { createPinia, setActivePinia } from 'pinia'

// OCR 回车误派发回归：OCR 结果视图（screenshot 扩展 ocr subview）打开时按 Enter，
// 必须执行操作列表首项（复制）；退出 OCR 视图回全局模式（ESC/goHome）后紧接的 Enter
// 不得执行残留的上一会话剪贴板记录——默认列表异步加载前的窗口期内残留结果仍可被
// 回车执行，曾致「无已复制提示 + 直接关窗 + 旧剪贴板记录被粘贴出去」。

const { invokeMock, hideWindowMock } = vi.hoisted(() => ({
  invokeMock: vi.fn().mockImplementation((cmd: string) => {
    if (cmd === 'ocr_image') return Promise.resolve({ text: 'OCR 结果文本', qr: [] })
    if (cmd === 'get_clipboard_history')
      return Promise.resolve([
        {
          id: 'old-record-1',
          content: '绩效复盘要点abc123',
          content_type: 'text',
          source_app: 'Zed',
          created_at: '2026-09-16 10:00:00',
          is_favorite: false,
          score: 0,
          file_size: null,
          image_width: null,
          image_height: null,
        },
      ])
    // 空查询默认列表走 search_apps：真实环境含 IPC + 应用缓存扫描（10-50ms），
    // 残留结果窗口期由此展开——测试用 80ms 延迟还原该窗口
    if (cmd === 'search_apps') return new Promise((r) => setTimeout(() => r([]), 80))
    if (cmd === 'search_files') return Promise.resolve([])
    if (cmd === 'get_app_icons') return Promise.resolve([])
    return Promise.resolve(null)
  }),
  hideWindowMock: vi.fn(),
}))

vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }))
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    label: 'main',
    isVisible: () => Promise.resolve(false),
    startDragging: vi.fn(),
    onFocusChanged: () => Promise.resolve(() => {}),
    onMoved: () => Promise.resolve(() => {}),
    onCloseRequested: () => Promise.resolve(() => {}),
  }),
}))
vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({ setFocus: () => Promise.resolve() }),
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
  emit: vi.fn(),
}))
vi.mock('@tauri-apps/plugin-shell', () => ({ open: vi.fn() }))
vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn().mockRejectedValue(new Error('no store')),
}))
vi.mock('@/utils/tauri', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/utils/tauri')>()),
  isTauri: true,
  hideWindow: hideWindowMock,
}))

import MainView from '@/components/layout/MainView.vue'
import { useAppStore } from '@/stores/app'
import { useSettingsStore } from '@/stores/settings'
import { searchEngine } from '@/runtime/search-engine'
import { getAllExtensions } from '@/runtime/extension-registry'
import '@/locales'

// 全部扩展注册（screenshot/clipboard 等，真实 meta/onExecute；search 供给默认列表 IPC）
await import('@ext/screenshot/index')
await import('@ext/clipboard/index')
await import('@ext/search/index')

function keyEvent(key: string): KeyboardEvent {
  return new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true })
}

function press(key: string) {
  document.dispatchEvent(keyEvent(key))
}

const mounted: ReturnType<typeof mount>[] = []

beforeEach(() => {
  setActivePinia(createPinia())
  // persistActiveExt 落 sessionStorage（同文件跨用例存活）：store 重建会恢复上例残留的
  // 激活扩展，需随用例清空（真实 app 中退出扩展即移除，仅测试环境需要）
  sessionStorage.clear()
  // searchEngine 激活模式是模块级单例（真实 app 中随 store 同步），跨用例需复位
  searchEngine.setActiveExtension(undefined)
  invokeMock.mockClear()
  hideWindowMock.mockClear()
})

afterEach(() => {
  mounted.forEach((w) => w.unmount())
  mounted.length = 0
  document.body.innerHTML = ''
})

describe('OCR 回车派发（回归：不得执行全局残留结果）', () => {
  it('全局残留剪贴板结果 + OCR 结果视图打开：Enter 执行复制而非残留记录', async () => {
    const appStore = useAppStore()
    const wrapper = mount(MainView, { attachTo: document.body })
    mounted.push(wrapper)

    // —— 场景前半：全局模式下残留剪贴板结果（模拟剪贴板填充搜索后的隐藏）——
    // 直接经 clipboard 扩展 onExecute 的数据形态构造结果并占据列表（MainView results
    // 为组件局部 ref，经搜索链路填充：这里用真实 dynamic 结果注入）
    const clipboard = getAllExtensions().find((e) => e.meta.id === 'clipboard')!
    const results = await clipboard.search!.dynamic!('绩效复盘要点abc123', {
      signal: new AbortController().signal,
    })
    expect(results.length).toBeGreaterThan(0)
    expect(results[0].data?.kind).toBe('clipboard')

    // 经 onInput 全链路填充（触发 useResultNavigation 消费的同一 results 引用）
    const input = wrapper.find('#main-search-input')
    ;(input.element as HTMLInputElement).value = '绩效复盘要点abc123'
    await input.trigger('input')
    await flushPromises()
    await nextTick()

    // —— 场景后半：OCR 子视图打开（open-extension-subview 同款状态链）——
    appStore.setActiveExtension('screenshot')
    appStore.setSearchQuery('')
    appStore.openSubview('ocr', true)
    // 注入 OCR 数据 → 识别完成 → 操作列表挂载
    const { pendingOcrData } = await import('@ext/screenshot/index')
    pendingOcrData.value = {
      selX: 0,
      selY: 0,
      selW: 100,
      selH: 100,
      scale: 2,
      annotationPng: '',
      previewPng: '',
    }
    await flushPromises()
    await nextTick()
    await nextTick()

    // OCR 完成：操作按钮行（复制为首项）已渲染
    const ocrButtons = wrapper.findAll('button').filter((b) => b.text() === '复制')
    expect(ocrButtons.length).toBeGreaterThan(0)

    // 间谍 clipboard onExecute：若被调用即证明回车落到了全局残留结果（bug）
    const onExecuteSpy = vi.spyOn(clipboard, 'onExecute' as never)

    // —— Enter：应执行 OCR 操作列表首项（复制）——
    press('Enter')
    await flushPromises()

    const pasteCalls = invokeMock.mock.calls.filter(
      ([cmd]) => cmd === 'paste_clipboard_item' || cmd === 'paste_clipboard_items',
    )
    expect(pasteCalls).toHaveLength(0)
    expect(onExecuteSpy).not.toHaveBeenCalled()
    // 复制路径：pasteboard_write_text + toast 反馈（hideWindow 由 toast 定时器延迟 800ms
    // 触发，本断言点不应已隐藏）
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === 'pasteboard_write_text')).toBe(true)
    expect(hideWindowMock).not.toHaveBeenCalled()
  })

  it('OCR 加载中（操作列表未挂载）Enter：不执行任何残留结果', async () => {
    const appStore = useAppStore()
    const wrapper = mount(MainView, { attachTo: document.body })
    mounted.push(wrapper)

    const clipboard = getAllExtensions().find((e) => e.meta.id === 'clipboard')!
    const input = wrapper.find('#main-search-input')
    ;(input.element as HTMLInputElement).value = '绩效复盘要点abc123'
    await input.trigger('input')
    await flushPromises()

    appStore.setActiveExtension('screenshot')
    appStore.setSearchQuery('')
    appStore.openSubview('ocr', true)

    const onExecuteSpy = vi.spyOn(clipboard, 'onExecute' as never)
    press('Enter')
    await flushPromises()
    expect(onExecuteSpy).not.toHaveBeenCalled()
    expect(hideWindowMock).not.toHaveBeenCalled()
  })

  it('ESC 退出 OCR 子视图（goHome）后立即 Enter：残留剪贴板记录不得被执行', async () => {
    const appStore = useAppStore()
    // 已 onboard 的真实用户（配置加载失败时 onboarded=false 会触发首启引导整窗接管，
    // 让 Enter 让位、用例空转）
    useSettingsStore().onboarded = true
    const wrapper = mount(MainView, { attachTo: document.body })
    mounted.push(wrapper)

    // 剪贴板填充搜索 → results 残留剪贴板记录（30ms 防抖后落定）
    const input = wrapper.find('#main-search-input')
    ;(input.element as HTMLInputElement).value = '绩效复盘要点abc123'
    await input.trigger('input')
    await new Promise((r) => setTimeout(r, 120))
    await flushPromises()
    // 防回归前提：残留结果确实含剪贴板记录（非空测试）
    expect(
      wrapper.findAll('[role="option"]').some((n) => n.text().includes('绩效复盘要点abc123')),
    ).toBe(true)

    const clipboard = getAllExtensions().find((e) => e.meta.id === 'clipboard')!
    const onExecuteSpy = vi.spyOn(clipboard, 'onExecute' as never)

    // OCR 子视图打开（含识别完成）→ ESC（external subview → goHome）
    appStore.setActiveExtension('screenshot')
    appStore.setSearchQuery('')
    appStore.openSubview('ocr', true)
    const { pendingOcrData } = await import('@ext/screenshot/index')
    pendingOcrData.value = {
      selX: 0,
      selY: 0,
      selW: 100,
      selH: 100,
      scale: 2,
      annotationPng: '',
      previewPng: '',
    }
    await flushPromises()
    await nextTick()

    press('Escape')
    await nextTick()
    // goHome 后（默认结果异步加载期间）立即回车——残留结果窗口期
    press('Enter')
    await flushPromises()

    const pasteCalls = invokeMock.mock.calls.filter(
      ([cmd]) => cmd === 'paste_clipboard_item' || cmd === 'paste_clipboard_items',
    )
    expect(pasteCalls).toHaveLength(0)
    expect(onExecuteSpy).not.toHaveBeenCalled()
  })
})
