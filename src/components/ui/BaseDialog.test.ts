import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import '@/locales'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { ref, defineComponent, KeepAlive, h, nextTick } from 'vue'
import BaseDialog from './BaseDialog.vue'

function mountDialog(props: Record<string, unknown> = {}) {
  return mount(BaseDialog, {
    props: {
      title: '测试标题',
      ...props,
    },
    global: {
      stubs: {
        Teleport: {
          template: '<div><slot /></div>',
        },
        Transition: {
          template: '<div><slot /></div>',
        },
      },
    },
  })
}

describe('BaseDialog', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    setActivePinia(createPinia())
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('渲染标题', () => {
    const wrapper = mountDialog({ title: '确认删除？' })
    expect(wrapper.text()).toContain('确认删除？')
  })

  it('渲染消息', () => {
    const wrapper = mountDialog({ message: '此操作不可撤销' })
    expect(wrapper.text()).toContain('此操作不可撤销')
  })

  it('默认显示取消和确定按钮', () => {
    const wrapper = mountDialog()
    expect(wrapper.text()).toContain('取消')
    expect(wrapper.text()).toContain('确定')
  })

  it('自定义按钮文本', () => {
    const wrapper = mountDialog({ okLabel: '删除', cancelLabel: '返回' })
    expect(wrapper.text()).toContain('删除')
    expect(wrapper.text()).toContain('返回')
  })

  it('showCancel=false 隐藏取消按钮', () => {
    const wrapper = mountDialog({ showCancel: false })
    expect(wrapper.text()).not.toContain('取消')
    expect(wrapper.text()).toContain('确定')
  })

  it('点击确定按钮触发 confirm 事件', async () => {
    const wrapper = mountDialog()
    const confirmBtn = wrapper.findAll('button').find((b) => b.text() === '确定')!
    await confirmBtn.trigger('click')
    vi.advanceTimersByTime(200)
    expect(wrapper.emitted('confirm')).toHaveLength(1)
  })

  it('点击取消按钮触发 cancel 事件', async () => {
    const wrapper = mountDialog()
    const cancelBtn = wrapper.findAll('button').find((b) => b.text() === '取消')!
    await cancelBtn.trigger('click')
    vi.advanceTimersByTime(200)
    expect(wrapper.emitted('cancel')).toHaveLength(1)
    expect(wrapper.emitted('cancel')![0]).toEqual(['cancel'])
  })

  it('Escape 键触发 cancel 事件（reason: escape）', async () => {
    const wrapper = mountDialog()
    await wrapper.find('[role="dialog"]').trigger('keydown', { key: 'Escape' })
    vi.advanceTimersByTime(200)
    expect(wrapper.emitted('cancel')).toHaveLength(1)
    expect(wrapper.emitted('cancel')![0]).toEqual(['escape'])
  })

  it('form 模式下遮罩点击可关闭', async () => {
    const wrapper = mountDialog({ variant: 'form' })
    await wrapper.find('.backdrop-to').trigger('click')
    vi.advanceTimersByTime(200)
    expect(wrapper.emitted('cancel')).toHaveLength(1)
    expect(wrapper.emitted('cancel')![0]).toEqual(['overlay'])
  })

  it('form 模式默认不显示 footer', () => {
    const wrapper = mountDialog({ variant: 'form' })
    expect(wrapper.text()).not.toContain('确定')
    expect(wrapper.text()).not.toContain('取消')
  })

  it('confirm + ArrowLeft 切换焦点到取消', async () => {
    const wrapper = mountDialog()
    await wrapper.find('[role="dialog"]').trigger('keydown', { key: 'ArrowLeft' })
    expect(wrapper.emitted()).not.toHaveProperty('confirm')
    expect(wrapper.emitted()).not.toHaveProperty('cancel')
  })

  it('confirm + Enter 触发确认（focusIndex=1）', async () => {
    const wrapper = mountDialog()
    await wrapper.find('[role="dialog"]').trigger('keydown', { key: 'Enter' })
    vi.advanceTimersByTime(200)
    expect(wrapper.emitted('confirm')).toHaveLength(1)
  })

  it('form + footer：INPUT 上 Enter 提交', async () => {
    const wrapper = mount(BaseDialog, {
      props: { title: '新建', variant: 'form', showFooter: true, okLabel: '创建' },
      slots: { default: '<input data-testid="field" />' },
      global: {
        stubs: {
          Teleport: { template: '<div><slot /></div>' },
          Transition: { template: '<div><slot /></div>' },
        },
      },
    })
    await wrapper.find('[data-testid="field"]').trigger('keydown', { key: 'Enter' })
    vi.advanceTimersByTime(200)
    expect(wrapper.emitted('confirm')).toHaveLength(1)
  })

  it('form + footer：IME composition 中 Enter 不提交', async () => {
    const wrapper = mount(BaseDialog, {
      props: { title: '新建', variant: 'form', showFooter: true, okLabel: '创建' },
      slots: { default: '<input data-testid="field" />' },
      global: {
        stubs: {
          Teleport: { template: '<div><slot /></div>' },
          Transition: { template: '<div><slot /></div>' },
        },
      },
    })
    await wrapper.find('[data-testid="field"]').trigger('keydown', {
      key: 'Enter',
      isComposing: true,
    })
    vi.advanceTimersByTime(200)
    expect(wrapper.emitted('confirm')).toBeUndefined()
  })

  it('closeOnConfirm=false：确定立即 emit 且弹窗仍可见', async () => {
    const wrapper = mountDialog({
      variant: 'form',
      showFooter: true,
      closeOnConfirm: false,
      okLabel: '创建',
    })
    const confirmBtn = wrapper.findAll('button').find((b) => b.text() === '创建')!
    await confirmBtn.trigger('click')
    // 无关窗动画延迟
    expect(wrapper.emitted('confirm')).toHaveLength(1)
    expect(wrapper.find('[role="dialog"]').exists()).toBe(true)
  })

  it('markdown 模式：列表 / 加粗 / 行内代码经 renderMarkdown 渲染', () => {
    const wrapper = mountDialog({
      message: '移除以下内容：\n\n- `/Library/LaunchDaemons` 下的 plist\n- **核心**与运行文件',
      markdown: true,
    })
    const body = wrapper.find('.markdown-body')
    expect(body.exists()).toBe(true)
    // 自定义 renderer 的列表类 + 两项
    expect(body.findAll('ul.md-list li')).toHaveLength(2)
    expect(body.text()).toContain('/Library/LaunchDaemons')
    expect(body.find('ul.md-list li code').exists()).toBe(true)
    expect(body.find('ul.md-list li strong').exists()).toBe(true)
    // 不再走 pre-wrap 纯文本分支
    expect(body.attributes('class')).not.toContain('whitespace-pre-wrap')
  })

  it('非 markdown 模式：消息保持纯文本分支', () => {
    const wrapper = mountDialog({ message: '第一行\n\n- 不是列表' })
    expect(wrapper.find('.markdown-body').exists()).toBe(false)
    expect(wrapper.text()).toContain('- 不是列表')
  })

  it('role="dialog" 和 aria-modal', () => {
    const wrapper = mountDialog()
    const dialog = wrapper.find('[role="dialog"]')
    expect(dialog.exists()).toBe(true)
    expect(dialog.attributes('aria-modal')).toBe('true')
  })

  it('onDeactivated：KeepAlive 切走时 dismiss 关窗', async () => {
    // 动态组件切换触发 KeepAlive deactivated（与扩展快捷键切扩展同路径）
    const page = ref<'dialog' | 'other'>('dialog')
    const reason = ref<string | null>(null)
    const Host = defineComponent({
      setup() {
        return () =>
          h(KeepAlive, null, () =>
            page.value === 'dialog'
              ? h(BaseDialog, {
                  key: 'dialog',
                  title: '测试',
                  onCancel: (r: string) => {
                    reason.value = r
                  },
                })
              : h('div', { key: 'other' }, 'other'),
          )
      },
    })
    mount(Host, {
      global: {
        stubs: {
          Teleport: { template: '<div><slot /></div>' },
          Transition: { template: '<div><slot /></div>' },
        },
      },
    })
    page.value = 'other'
    await nextTick()
    vi.advanceTimersByTime(200)
    expect(reason.value).toBe('dismiss')
  })
})

describe('BaseDialog 内容驱动高度动画', () => {
  // happy-dom 无布局引擎（getBoundingClientRect 恒 0）：stub ResizeObserver + mock 测量值，
  // 手动派发回调驱动 FLIP 流程（锁旧高 → 写目标高 → settle 清回 auto 并重新观察）
  const roInstances: Array<{
    callback: ResizeObserverCallback
    observed: Element[]
  }> = []
  const FakeRO = class {
    callback: ResizeObserverCallback
    observed: Element[] = []
    constructor(cb: ResizeObserverCallback) {
      this.callback = cb
      roInstances.push(this as unknown as { callback: ResizeObserverCallback; observed: Element[] })
    }
    observe(el: Element) {
      this.observed.push(el)
    }
    unobserve(el: Element) {
      this.observed = this.observed.filter((e) => e !== el)
    }
    disconnect() {
      this.observed = []
    }
  }

  it('自然高度变化：写显式目标高 → settle 后清回 auto 并重新观察', async () => {
    vi.useFakeTimers()
    vi.stubGlobal('ResizeObserver', FakeRO)
    const wrapper = mountDialog()
    const el = wrapper.find('.dialog-to').element as HTMLElement
    expect(roInstances).toHaveLength(1)
    expect(roInstances[0]!.observed).toContain(el)

    // 内容撑高（挂载基线 0 → 120）：mock 测量后派发 RO 回调
    const spy = vi.spyOn(el, 'getBoundingClientRect').mockReturnValue({ height: 120 } as DOMRect)
    roInstances[0]!.callback([], undefined as unknown as ResizeObserver)
    await nextTick()

    // 目标高显式写入（transition 端点），动画期间 unobserve 防自触发
    expect(el.style.height).toBe('120px')
    expect(roInstances[0]!.observed).not.toContain(el)

    vi.advanceTimersByTime(230)
    expect(el.style.height).toBe('')
    expect(roInstances[0]!.observed).toContain(el)

    spy.mockRestore()
    vi.unstubAllGlobals()
    vi.useRealTimers()
    roInstances.length = 0
    wrapper.unmount()
  })
})
