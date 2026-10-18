import { describe, it, expect, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import ShortcutInput from './ShortcutInput.vue'

// 录制态走 DOM keydown（未注册的组合不会经 Rust 转发），事件来源无关
vi.mock('@/composables/useTauriListener', () => ({ useTauriListener: () => {} }))

describe('ShortcutInput 录制', () => {
  function mountInput() {
    setActivePinia(createPinia())
    return mount(ShortcutInput, {
      props: { modelValue: '' },
    })
  }

  it('Option+字母按物理键位录入，不落 macOS 合成字符', async () => {
    const wrapper = mountInput()
    await wrapper.trigger('click')
    // macOS：⌥C 的 key 是 ç，Rust 侧无法解析注册；code 仍是 KeyC
    await wrapper.trigger('keydown', { key: 'ç', code: 'KeyC', altKey: true })
    expect(wrapper.emitted('update:modelValue')?.[0]).toEqual(['Alt+C'])
  })

  it('Option+死键同样还原为物理键位', async () => {
    const wrapper = mountInput()
    await wrapper.trigger('click')
    // ⌥E 是死键（´），key 为 Dead
    await wrapper.trigger('keydown', { key: 'Dead', code: 'KeyE', altKey: true })
    expect(wrapper.emitted('update:modelValue')?.[0]).toEqual(['Alt+E'])
  })

  it('Shift 合成符号按物理键位落位（避免 + 破坏快捷键串）', async () => {
    const wrapper = mountInput()
    await wrapper.trigger('click')
    await wrapper.trigger('keydown', {
      key: '+',
      code: 'Equal',
      shiftKey: true,
    })
    expect(wrapper.emitted('update:modelValue')?.[0]).toEqual(['Shift+='])
  })
})
