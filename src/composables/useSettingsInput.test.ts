import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { defineComponent, h } from 'vue'
import { createPinia, setActivePinia } from 'pinia'
import { useSettingsInput } from './useSettingsInput'
import { useAppStore } from '@/stores/app'

describe('useSettingsInput', () => {
  let unmount: (() => void) | undefined

  beforeEach(() => {
    setActivePinia(createPinia())
    unmount = undefined
  })

  afterEach(() => {
    unmount?.()
    unmount = undefined
  })

  function mountSettings() {
    const TestComp = defineComponent({
      setup() {
        useSettingsInput()
        return () => h('div')
      },
    })
    const wrapper = mount(TestComp)
    unmount = () => wrapper.unmount()
  }

  // 派发 Escape 并返回「bubble 阶段 listener 是否收到」
  // useSettingsInput 在 capture 阶段 stopImmediatePropagation 时，bubble listener 不应被调用
  function dispatchEscape(target: HTMLElement) {
    const received = vi.fn()
    document.addEventListener('keydown', received)
    target.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    document.removeEventListener('keydown', received)
    return received
  }

  it('表单控件聚焦 esc → blur + 阻止后续 listener', () => {
    mountSettings()
    const textarea = document.createElement('textarea')
    document.body.appendChild(textarea)
    const blurSpy = vi.spyOn(textarea, 'blur')
    textarea.focus()

    const received = dispatchEscape(textarea)

    expect(blurSpy).toHaveBeenCalledOnce()
    expect(received).not.toHaveBeenCalled()

    textarea.remove()
  })

  it('非表单控件 esc → 不拦截（事件继续传播）', () => {
    mountSettings()
    const div = document.createElement('div')
    document.body.appendChild(div)
    div.focus()

    const received = dispatchEscape(div)

    expect(received).toHaveBeenCalled()

    div.remove()
  })

  it('dialog 打开时 esc → 不拦截（交由 Dialog 处理）', () => {
    mountSettings()
    useAppStore().isDialogOpen = true
    const textarea = document.createElement('textarea')
    document.body.appendChild(textarea)
    const blurSpy = vi.spyOn(textarea, 'blur')
    textarea.focus()

    const received = dispatchEscape(textarea)

    expect(blurSpy).not.toHaveBeenCalled()
    expect(received).toHaveBeenCalled()

    textarea.remove()
  })
})
