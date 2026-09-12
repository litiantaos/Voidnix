import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { KeepAlive, defineComponent, h, nextTick, ref } from 'vue'

// OcrView 状态提升回归：窗口隐藏时 ContentView 卸载 KeepAlive（组件销毁，局部状态丢失），
// 再唤起重挂载应从模块级 ocrSession 恢复现场（预览图 + 识别文本），而非空白。
const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  emit: vi.fn(),
  setActiveExtension: vi.fn(),
}))

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }))
vi.mock('@tauri-apps/api/event', () => ({ emit: mocks.emit }))
vi.mock('@/stores/app', () => ({
  useAppStore: () => ({ setActiveExtension: mocks.setActiveExtension }),
  copyAndHide: vi.fn(),
}))
// isTauri 置真以覆盖 onMounted 的 get_window_max_height 推导路径；
// getCurrentWindow 回 main label：storage.ts 子窗口判定走 main 持久化路径（与
// isTauri=false 时同路径，getStore 失败被内部捕获容忍），不抛 happy-dom 无 internals 异常
vi.mock('@/utils/tauri', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/utils/tauri')>()),
  isTauri: true,
}))
vi.mock('@tauri-apps/api/window', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@tauri-apps/api/window')>()),
  getCurrentWindow: () => ({
    label: 'main',
    onCloseRequested: () => Promise.resolve(() => {}),
  }),
}))

import '@/locales'
import './locales'
import { ocrSession, pendingOcrData } from './index'
import OcrView from './OcrView.vue'
import BaseEmptyState from '@/components/ui/BaseEmptyState.vue'
import BaseTextarea from '@/components/ui/BaseTextarea.vue'

function injectPending(overrides: Partial<NonNullable<typeof pendingOcrData.value>> = {}) {
  pendingOcrData.value = {
    selX: 0,
    selY: 0,
    selW: 100,
    selH: 100,
    scale: 2,
    annotationPng: '',
    previewPng: 'data:image/png;base64,preview',
    ...overrides,
  }
}

/// 模拟 ContentView：v-if 写在 KeepAlive 自身（keepAliveActive 置 false 即卸载 KeepAlive、
/// 组件真销毁——写在 slot 内只会 deactivate 缓存，销毁路径永远走不到）
function mountHost() {
  const alive = ref(true)
  const wrapper = mount(
    defineComponent({
      setup: () => () => (alive.value ? h(KeepAlive, () => h(OcrView)) : null),
    }),
  )
  return { wrapper, alive }
}

async function flush(times = 10) {
  for (let i = 0; i < times; i++) await nextTick()
}

beforeEach(() => {
  mocks.invoke.mockReset()
  mocks.invoke.mockImplementation((cmd: string) => {
    if (cmd === 'ocr_image') return Promise.resolve({ text: '识别文本', qr: [] })
    return Promise.resolve(null)
  })
  mocks.emit.mockReset()
  mocks.setActiveExtension.mockReset()
  pendingOcrData.value = null
  ocrSession.value = { imageUrl: '', ocrText: '', error: '', loading: false }
})

describe('screenshot OcrView 会话状态跨 KeepAlive 卸载保留', () => {
  it('正常流：注入数据 → 识别 → 渲染预览与文本', async () => {
    injectPending()
    const { wrapper } = mountHost()
    await flush()

    expect(wrapper.find('img').attributes('src')).toBe('data:image/png;base64,preview')
    expect(wrapper.findComponent(BaseTextarea).props('modelValue')).toBe('识别文本')
    wrapper.unmount()
  })

  it('回归：KeepAlive 卸载（窗口隐藏）后重挂载，现场从模块级会话恢复且不重新识别', async () => {
    injectPending()
    const { wrapper, alive } = mountHost()
    await flush()
    // 只数 ocr_image：挂载还会经 get_window_max_height 查高度天花板（isTauri 置真）
    const ocrCalls = () => mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'ocr_image').length
    expect(ocrCalls()).toBe(1)

    // 窗口隐藏：keepAliveActive=false 卸载 KeepAlive，组件销毁
    alive.value = false
    await flush()
    expect(wrapper.findComponent(OcrView).exists()).toBe(false)

    // 再唤起：重挂载，pendingOcrData 已消费（null），现场应从 ocrSession 恢复
    alive.value = true
    await flush()

    expect(wrapper.find('img').attributes('src')).toBe('data:image/png;base64,preview')
    expect(wrapper.findComponent(BaseTextarea).props('modelValue')).toBe('识别文本')
    expect(ocrCalls()).toBe(1)
    wrapper.unmount()
  })

  it('编辑后的文本与会话同步：卸载重挂载保留最新编辑值', async () => {
    injectPending()
    const { wrapper, alive } = mountHost()
    await flush()

    // 用户编辑识别结果（textarea v-model 直写模块级会话）
    ocrSession.value.ocrText = '编辑后的文本'
    alive.value = false
    await flush()
    alive.value = true
    await flush()

    expect(wrapper.findComponent(BaseTextarea).props('modelValue')).toBe('编辑后的文本')
    wrapper.unmount()
  })

  it('窗口高度封顶：输入框上限从 get_window_max_height 同源推导（防回归到无上限撑高窗口 + 窗口级滚动）', async () => {
    mocks.invoke.mockImplementation((cmd: string) => {
      if (cmd === 'ocr_image') return Promise.resolve({ text: '识别文本', qr: [] })
      if (cmd === 'get_window_max_height') return Promise.resolve(900)
      return Promise.resolve(null)
    })
    injectPending()
    const { wrapper } = mountHost()
    await flush()

    // 900 − 568 固定部分 − 12 余量 = 320；天花板与 set_main_frame 的 Rust clamp
    // 同源（visibleFrame × 0.9），内容恒不超窗——按整屏高推导会撑过 clamp 产生滚动
    const textarea = wrapper.findComponent(BaseTextarea)
    expect(textarea.props('maxHeight')).toBe(320)
    expect(wrapper.find('textarea').attributes('style')).toContain('max-height: 320px')
    wrapper.unmount()
  })

  it('回退：天花板查询失败（None）时输入框保持安全上限（封顶模式不退化）', async () => {
    injectPending()
    const { wrapper } = mountHost()
    await flush()

    expect(wrapper.findComponent(BaseTextarea).props('maxHeight')).toBe(224)
    wrapper.unmount()
  })

  it('加载遮罩：识别期间覆于预览区，完成/出错即消失', async () => {
    let resolveOcr!: (v: { text: string; qr: string[] }) => void
    mocks.invoke.mockImplementation((cmd: string) => {
      if (cmd === 'ocr_image') return new Promise((r) => (resolveOcr = r))
      return Promise.resolve(null)
    })
    injectPending()
    const { wrapper } = mountHost()
    await flush()

    // 识别挂起：磨砂遮罩在位（BaseEmptyState loading）
    expect(wrapper.findComponent(BaseEmptyState).props('loading')).toBe(true)
    expect(wrapper.findComponent(BaseTextarea).exists()).toBe(false)

    resolveOcr({ text: '识别文本', qr: [] })
    await flush()
    expect(wrapper.findComponent(BaseEmptyState).exists()).toBe(false)
    wrapper.unmount()
  })

  it('新 OCR 注入：会话重置为新预览图，重新识别', async () => {
    injectPending()
    const first = mountHost()
    await flush()
    first.wrapper.unmount()

    // 新一轮截图 OCR：注入新数据，预览图与文本都应翻新
    mocks.invoke.mockImplementation((cmd: string) => {
      if (cmd === 'ocr_image') return Promise.resolve({ text: '新文本', qr: [] })
      return Promise.resolve(null)
    })
    injectPending({ previewPng: 'data:image/png;base64,new' })
    const second = mountHost()
    await flush()

    expect(second.wrapper.find('img').attributes('src')).toBe('data:image/png;base64,new')
    expect(second.wrapper.findComponent(BaseTextarea).props('modelValue')).toBe('新文本')
    second.wrapper.unmount()
  })
})
