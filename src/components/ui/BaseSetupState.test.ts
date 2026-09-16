import { describe, it, expect } from 'vitest'
import '@/locales'
import { mount } from '@vue/test-utils'
import BaseSetupState from './BaseSetupState.vue'

describe('BaseSetupState', () => {
  it('渲染标题与统一「去配置」主按钮', () => {
    const wrapper = mount(BaseSetupState, { props: { title: '请先配置 AI 提供商' } })
    expect(wrapper.text()).toContain('请先配置 AI 提供商')
    expect(wrapper.text()).toContain('去配置')
    const btn = wrapper.find('button')
    expect(btn.exists()).toBe(true)
  })

  it('点击按钮 emit configure（跳转目标由消费者决定）', async () => {
    const wrapper = mount(BaseSetupState, { props: { title: 'x' } })
    await wrapper.find('button').trigger('click')
    expect(wrapper.emitted('configure')).toHaveLength(1)
  })

  it('actionText 覆盖按钮文案（如中枢空态的「添加提供商」）', () => {
    const wrapper = mount(BaseSetupState, {
      props: { title: '请添加 AI 提供商', actionText: '添加提供商' },
    })
    expect(wrapper.text()).toContain('添加提供商')
    expect(wrapper.text()).not.toContain('去配置')
  })
})
