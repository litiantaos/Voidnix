import { describe, it, expect, beforeEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
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
    setActivePinia(createPinia())
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
    const buttons = wrapper.findAll('button')
    const confirmBtn = buttons.find((b) => b.text() === '确定')
    expect(confirmBtn).toBeDefined()
    await confirmBtn!.trigger('click')
    expect(wrapper.emitted('confirm')).toHaveLength(1)
  })

  it('点击取消按钮触发 cancel 事件', async () => {
    const wrapper = mountDialog()
    const buttons = wrapper.findAll('button')
    const cancelBtn = buttons.find((b) => b.text() === '取消')
    expect(cancelBtn).toBeDefined()
    await cancelBtn!.trigger('click')
    expect(wrapper.emitted('cancel')).toHaveLength(1)
    expect(wrapper.emitted('cancel')![0]).toEqual(['cancel'])
  })

  it('Escape 键触发 cancel 事件（reason: escape）', async () => {
    const wrapper = mountDialog()
    await wrapper.find('[role="dialog"]').trigger('keydown', { key: 'Escape' })
    expect(wrapper.emitted('cancel')).toHaveLength(1)
    expect(wrapper.emitted('cancel')![0]).toEqual(['escape'])
  })

  it('confirm + warning 模式下遮罩点击不关闭', async () => {
    const wrapper = mountDialog({ variant: 'confirm', kind: 'warning' })
    await wrapper.find('.backdrop-to').trigger('click')
    expect(wrapper.emitted('cancel')).toBeUndefined()
  })

  it('form 模式下遮罩点击可关闭', async () => {
    const wrapper = mountDialog({ variant: 'form' })
    await wrapper.find('.backdrop-to').trigger('click')
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
    const dialog = wrapper.find('[role="dialog"]')
    await dialog.trigger('keydown', { key: 'ArrowLeft' })
    expect(wrapper.emitted()).not.toHaveProperty('confirm')
    expect(wrapper.emitted()).not.toHaveProperty('cancel')
  })

  it('confirm + Enter 触发确认（focusIndex=1）', async () => {
    const wrapper = mountDialog()
    const dialog = wrapper.find('[role="dialog"]')
    await dialog.trigger('keydown', { key: 'Enter' })
    expect(wrapper.emitted('confirm')).toHaveLength(1)
  })

  it('role="dialog" 和 aria-modal', () => {
    const wrapper = mountDialog()
    const dialog = wrapper.find('[role="dialog"]')
    expect(dialog.exists()).toBe(true)
    expect(dialog.attributes('aria-modal')).toBe('true')
  })
})
