import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import { CMD } from '@/commands'

// 回归：编辑中的文本标注未点外部确认（commitText 未走）直接复制/保存时，
// 导出图缺文字——烧录 beginCanvasText 须先固化编辑态（shape 写入完整 text），
// 再把文本绘制进标注 canvas。修复前占位 shape 的 text 为空串，fillText 永不出现。
const mocks = vi.hoisted(() => ({ invoke: vi.fn() }))

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }))

import '@/locales'
import '../locales'
import Operation from './Operation.vue'
import type { ScreenshotData } from '../composables/useTypes'
import AnnotationPalette from './AnnotationPalette.vue'

/// happy-dom 无 2d context：全量 no-op Proxy，仅记录 fillText（导出烧录的文本证据）
/// 与 measureText/getImageData（宽度自适应 / 取色路径需要返回结构）
const fillTextCalls: string[] = []
function makeCtx(): CanvasRenderingContext2D {
  const target: Record<string, unknown> = {}
  return new Proxy(target, {
    get(t, prop) {
      if (prop === 'measureText')
        return () => ({ width: 12, fontBoundingBoxAscent: 10, actualBoundingBoxAscent: 9 })
      if (prop === 'getImageData') return () => ({ data: new Uint8ClampedArray(4) })
      if (prop === 'fillText')
        return (text: string) => {
          fillTextCalls.push(text)
        }
      if (prop in t) return t[prop as string]
      if (typeof prop === 'symbol') return undefined
      const fn = vi.fn()
      t[prop] = fn
      return fn
    },
    set(t, prop, value) {
      t[prop as string] = value
      return true
    },
  }) as unknown as CanvasRenderingContext2D
}

async function flush(times = 12) {
  for (let i = 0; i < times; i++) await nextTick()
}

/// 等两帧：给 doCancel 等 rAF 收尾逻辑留出确定性执行窗口（不用固定 sleep）
async function nextFrame() {
  await new Promise<void>((resolve) =>
    requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
  )
}

function mountOperation(overrides: Partial<ScreenshotData> = {}, attachTo?: HTMLElement) {
  return mount(Operation, {
    attachTo,
    props: {
      initialScreenshot: {
        width: 800,
        height: 600,
        scale: 2,
        mouse_x: 100,
        mouse_y: 100,
        windows: [],
        last_selection: null,
        ...overrides,
      },
    },
  })
}

/// 走原生指针桥选中窗口（select 阶段 DOM 指针被禁用）：move 更新 hoverWindow，
/// down/up 无拖动即把选区吸附到该窗口并进入 annotate。
async function selectWindow(wrapper: ReturnType<typeof mountOperation>) {
  const bridge = (
    window as unknown as {
      __screenshotPointer?: (type: string, x: number, y: number, shiftKey: boolean) => void
    }
  ).__screenshotPointer
  bridge?.('move', 150, 150, false)
  bridge?.('down', 150, 150, false)
  bridge?.('up', 150, 150, false)
  await flush()
  return wrapper
}

/// 全屏选区（F 键）进入 annotate → 文字工具 → 选区内单击开输入框
async function openTextEditor(wrapper: ReturnType<typeof mountOperation>) {
  await wrapper.find('[tabindex="0"]').trigger('keydown', { key: 'f' })
  const palette = wrapper.findComponent(AnnotationPalette)
  palette.vm.$emit('tool', 'text')
  await flush()
  await wrapper.find('[tabindex="0"]').trigger('mousedown', {
    button: 0,
    clientX: 300,
    clientY: 200,
  })
  await flush()
  return palette
}

let getContextSpy: ReturnType<typeof vi.spyOn> | undefined
let toDataURLSpy: ReturnType<typeof vi.spyOn> | undefined

beforeEach(() => {
  fillTextCalls.length = 0
  mocks.invoke.mockReset()
  // picker 轮询自然终止（retry 有上限）；overlay ready 等其余命令直接成功
  mocks.invoke.mockImplementation((cmd: string) =>
    cmd === CMD.readPickerImage ? Promise.reject(new Error('no picker')) : Promise.resolve(null),
  )
  getContextSpy = vi
    .spyOn(HTMLCanvasElement.prototype, 'getContext')
    .mockImplementation(() => makeCtx())
  toDataURLSpy = vi
    .spyOn(HTMLCanvasElement.prototype, 'toDataURL')
    .mockReturnValue('data:image/png;base64,c3R1Yg==')
})

afterEach(() => {
  getContextSpy?.mockRestore()
  toDataURLSpy?.mockRestore()
})

describe('Operation 导出固化编辑态文本', () => {
  it('复制时未确认的编辑中文本进入导出 canvas', async () => {
    const wrapper = mountOperation()
    await flush()
    const palette = await openTextEditor(wrapper)

    const ta = wrapper.find('textarea')
    expect(ta.exists()).toBe(true)
    await ta.setValue('未确认的标注文字')

    // 不点外部确认，直接点工具条复制
    fillTextCalls.length = 0
    palette.vm.$emit('copy')
    await flush()

    expect(fillTextCalls).toContain('未确认的标注文字')
    expect(mocks.invoke).toHaveBeenCalledWith(
      CMD.copyScreenshotToClipboard,
      expect.objectContaining({ annotationPng: 'data:image/png;base64,c3R1Yg==' }),
    )
    wrapper.unmount()
  })

  it('保存与 OCR 路径同样固化（钉图共用 getAnnotationPng）', async () => {
    const wrapper = mountOperation()
    await flush()
    const palette = await openTextEditor(wrapper)
    await wrapper.find('textarea').setValue('未确认的保存文字')

    fillTextCalls.length = 0
    palette.vm.$emit('save')
    await flush()
    expect(fillTextCalls).toContain('未确认的保存文字')

    // OCR 直开子视图：编辑态同样先固化再烧录（annotationPng 参与 OCR 识别）
    await wrapper.find('[tabindex="0"]').trigger('mousedown', {
      button: 0,
      clientX: 320,
      clientY: 220,
    })
    await flush()
    await wrapper.find('textarea').setValue('未确认的 OCR 文字')
    fillTextCalls.length = 0
    palette.vm.$emit('ocr')
    await flush()
    expect(fillTextCalls).toContain('未确认的 OCR 文字')

    wrapper.unmount()
  })

  it('空文本编辑态不产生空标注（导出后占位 shape 被清除）', async () => {
    const wrapper = mountOperation()
    await flush()
    const palette = await openTextEditor(wrapper)
    // 不输入任何内容（空占位）
    fillTextCalls.length = 0
    palette.vm.$emit('copy')
    await flush()
    expect(fillTextCalls).toEqual([])
    expect(mocks.invoke).toHaveBeenCalledWith(
      CMD.copyScreenshotToClipboard,
      expect.objectContaining({ annotationPng: '' }),
    )
    wrapper.unmount()
  })
})

describe('Operation 全局快捷键不侵入文本编辑', () => {
  const win = { x: 100, y: 100, w: 200, h: 200, owner: 'Test' }

  it('编辑文字时按 F 不重写选区、不抢焦点', async () => {
    const wrapper = mountOperation({ windows: [win] }, document.body)
    await flush()
    await selectWindow(wrapper)
    // 已吸附到 200×200 窗口：标注 canvas 宽 = 200 × dpr(2)
    expect(wrapper.find('canvas').attributes('width')).toBe('400')

    const palette = wrapper.findComponent(AnnotationPalette)
    palette.vm.$emit('tool', 'text')
    await flush()
    await wrapper
      .find('[tabindex="0"]')
      .trigger('mousedown', { button: 0, clientX: 150, clientY: 150 })
    await flush()

    const textarea = wrapper.find('textarea')
    expect(textarea.exists()).toBe(true)
    const focused = textarea.element as HTMLTextAreaElement
    focused.focus()
    await flush()
    expect(document.activeElement).toBe(focused)
    await textarea.trigger('keydown', { key: 'f' })
    await flush()

    // F 仍冒泡到根节点，但编辑态应整体让位：选区保持窗口尺寸、编辑框仍在
    expect(wrapper.find('canvas').attributes('width')).toBe('400')
    expect(wrapper.find('textarea').exists()).toBe(true)
    expect(document.activeElement).toBe(focused)
    wrapper.unmount()
  })

  it('编辑文字时 Esc 只取消编辑，不退出截屏', async () => {
    const wrapper = mountOperation({ windows: [win] })
    await flush()
    await selectWindow(wrapper)

    const palette = wrapper.findComponent(AnnotationPalette)
    palette.vm.$emit('tool', 'text')
    await flush()
    await wrapper
      .find('[tabindex="0"]')
      .trigger('mousedown', { button: 0, clientX: 150, clientY: 150 })
    await flush()
    expect(wrapper.find('textarea').exists()).toBe(true)

    await wrapper.find('textarea').trigger('keydown', { key: 'Escape' })
    await flush()
    await nextFrame()

    expect(wrapper.find('textarea').exists()).toBe(false)
    expect(wrapper.emitted('close')).toBeFalsy()
    wrapper.unmount()
  })

  it('编辑文字时 Cmd+Enter 只提交，不触发复制', async () => {
    const wrapper = mountOperation({ windows: [win] })
    await flush()
    await selectWindow(wrapper)

    const palette = wrapper.findComponent(AnnotationPalette)
    palette.vm.$emit('tool', 'text')
    await flush()
    await wrapper
      .find('[tabindex="0"]')
      .trigger('mousedown', { button: 0, clientX: 150, clientY: 150 })
    await flush()
    await wrapper.find('textarea').setValue('提交文字')

    await wrapper.find('textarea').trigger('keydown', { key: 'Enter', metaKey: true })
    await flush()
    await nextFrame()

    // 编辑框消费 Cmd+Enter 并 stopPropagation：只落文本，不再冒泡成「复制截屏」
    expect(wrapper.find('textarea').exists()).toBe(false)
    expect(mocks.invoke).not.toHaveBeenCalledWith(CMD.copyScreenshotToClipboard, expect.anything())
    wrapper.unmount()
  })

  it('焦点在色板时 Esc 只取消编辑，不二次退出截屏', async () => {
    const wrapper = mountOperation({ windows: [win] })
    await flush()
    await selectWindow(wrapper)

    const palette = wrapper.findComponent(AnnotationPalette)
    palette.vm.$emit('tool', 'text')
    await flush()
    await wrapper
      .find('[tabindex="0"]')
      .trigger('mousedown', { button: 0, clientX: 150, clientY: 150 })
    await flush()
    expect(wrapper.find('textarea').exists()).toBe(true)

    // 编辑中点了色板（palette 阻止冒泡，不会 commit）：焦点/事件目标在按钮上。
    // 根节点冒泡处理 Esc 后，window 级兜底不得再处理一次。
    await palette.find('button').trigger('keydown', { key: 'Escape' })
    await flush()
    await nextFrame()

    expect(wrapper.find('textarea').exists()).toBe(false)
    expect(wrapper.emitted('close')).toBeFalsy()
    wrapper.unmount()
  })

  it('标注阶段（无编辑框）按 F 不重写选区', async () => {
    const wrapper = mountOperation({ windows: [win] })
    await flush()
    await selectWindow(wrapper)
    expect(wrapper.find('canvas').attributes('width')).toBe('400')

    await wrapper.find('[tabindex="0"]').trigger('keydown', { key: 'f' })
    await flush()
    expect(wrapper.find('canvas').attributes('width')).toBe('400')
    wrapper.unmount()
  })
})
