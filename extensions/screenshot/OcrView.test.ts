import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { KeepAlive, defineComponent, h, nextTick, ref } from 'vue'

// OcrView 状态提升回归：KeepAlive 卸载（LRU 驱逐 / navigate 重载）时组件销毁、局部状态
// 丢失，重挂载应从模块级 ocrSession 恢复现场（预览图 + 识别文本），而非空白。
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

/// 模拟卸载宿主：v-if 写在 KeepAlive 自身（组件真销毁——写在 slot 内只会 deactivate
/// 缓存，销毁路径永远走不到；对应 LRU 驱逐 / navigate 重载）
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

  it('回归：KeepAlive 卸载（LRU 驱逐）后重挂载，现场从模块级会话恢复且不重新识别', async () => {
    injectPending()
    const { wrapper, alive } = mountHost()
    await flush()
    // 只数 ocr_image：挂载还会经 get_window_max_height 查高度天花板（isTauri 置真）
    const ocrCalls = () => mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'ocr_image').length
    expect(ocrCalls()).toBe(1)

    // 卸载 KeepAlive（对应 LRU 驱逐 / navigate 重载），组件销毁
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

    // 900 − 318 固定部分 − 12 余量 = 570；天花板与 set_main_frame 的 Rust clamp
    // 同源（visibleFrame × 0.9），内容恒不超窗——按整屏高推导会撑过 clamp 产生滚动
    const textarea = wrapper.findComponent(BaseTextarea)
    expect(textarea.props('maxHeight')).toBe(570)
    expect(wrapper.find('textarea').attributes('style')).toContain('max-height: 570px')
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

    // 遮罩在滚动层外（兄弟层）：滚动容器内的 absolute 遮罩只覆盖初始视口，
    // 且作为滚动内容随长图滚走（钉住视口回归锁）。用包含关系而非 parentElement——
    // 测试环境 Transition 被 stub，遮罩的直接父节点是 transition-stub
    const outer = wrapper.find('.fill-ctrl')
    const scrollLayer = wrapper.find('.hide-scrollbar')
    const mask = wrapper.find('.backdrop-blur-xs')
    expect(mask.exists()).toBe(true)
    expect(outer.element.contains(scrollLayer.element)).toBe(true)
    expect(outer.element.contains(mask.element)).toBe(true)
    expect(scrollLayer.element.contains(mask.element)).toBe(false)
    // 圆角裁剪回归锁：外层 overflow-hidden（图片/遮罩四角不溢出 radius-panel 边框）
    expect(outer.attributes('overflow')).toBe('hidden')

    resolveOcr({ text: '识别文本', qr: [] })
    await flush()
    expect(wrapper.findComponent(BaseEmptyState).exists()).toBe(false)
    wrapper.unmount()
  })

  it('操作按钮行：左右键环形切换，回车触发当前项；textarea 聚焦时让出', async () => {
    injectPending()
    const { wrapper } = mountHost()
    await flush()

    const buttons = wrapper.findAll('button')
    expect(buttons).toHaveLength(5)
    // 默认选中首项（复制）
    expect(buttons[0].classes()).toContain('ui-active')

    const press = (key: string, opts?: KeyboardEventInit) => {
      document.dispatchEvent(
        new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...opts }),
      )
    }

    // 右移 0→1→2；选中「去空格」后写入带空格文本回车触发
    press('ArrowRight')
    press('ArrowRight')
    await flush(1)
    expect(buttons[2].classes()).toContain('ui-active')
    expect(buttons[0].classes()).not.toContain('ui-active')
    ocrSession.value.ocrText = 'a b  c'
    press('Enter')
    await flush(1)
    expect(ocrSession.value.ocrText).toBe('abc')

    // 左移：2→1→0，再左移环形到尾项（去空行）
    press('ArrowLeft')
    press('ArrowLeft')
    press('ArrowLeft')
    await flush(1)
    expect(buttons[4].classes()).toContain('ui-active')

    // Enter 执行即消费：后续 document 监听（全局导航等）不再收到同一按键
    let laterSawEnter = false
    const later = (e: KeyboardEvent) => {
      if (e.key === 'Enter') laterSawEnter = true
    }
    document.addEventListener('keydown', later)
    press('Enter')
    document.removeEventListener('keydown', later)
    expect(laterSawEnter).toBe(false)

    // textarea 聚焦（编辑识别结果）时让出：左右键移动光标、回车换行，不切不触发。
    // happy-dom 的 focus() 不更新 document.activeElement（仍为 BODY），手动打桩
    // 到 textarea 模拟聚焦；真实聚焦行为由 e2e 真键盘覆盖
    const textarea = wrapper.find('textarea')
    Object.defineProperty(document, 'activeElement', {
      configurable: true,
      get: () => textarea.element,
    })
    try {
      press('ArrowRight')
      press('Enter')
      await flush(1)
      expect(buttons[4].classes()).toContain('ui-active')
      expect(ocrSession.value.ocrText).toBe('abc')
    } finally {
      // 断言失败也恢复,防桩泄漏污染同文件后续用例
      Reflect.deleteProperty(document, 'activeElement')
    }
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
