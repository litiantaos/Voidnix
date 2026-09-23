import { describe, it, expect } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia } from 'pinia'
import ActionMenuHint from './ActionMenuHint.vue'
import BaseList from './BaseList.vue'

function mountList(actionHint?: boolean | ((item: unknown) => boolean)) {
  return mount(BaseList, {
    props: {
      items: [{ id: 'a' }, { id: 'b' }],
      selectedIndex: 0,
      ...(actionHint === undefined ? {} : { actionHint }),
    },
    slots: {
      item: `<template #item="{ item }"><span class="row-content">{{ item.id }}</span></template>`,
    },
    global: { plugins: [createPinia()] },
  })
}

describe('ActionMenuHint', () => {
  it('渲染 ⌘ 与回车图标键帽，装饰性（aria-hidden）', () => {
    const wrapper = mount(ActionMenuHint)
    const el = wrapper.get('span')
    expect(el.attributes('aria-hidden')).toBe('true')
    expect(el.classes()).toContain('action-menu-hint')
    expect(el.find('.action-menu-hint-scrim').exists()).toBe(true)
    const key = el.get('.action-menu-hint-key')
    expect(key.get('span').text()).toBe('⌘')
    expect(key.get('i').classes()).toContain('i-ri-corner-down-left-line')
  })

  it('BaseList actionHint=true → 徽标单实例随焦点行显示（剪贴板/ai-providers 路径）', () => {
    const wrapper = mountList(true)
    const rows = wrapper.findAll('[role="option"]')
    expect(rows).toHaveLength(2)
    // 徽标渲染于选中滑层内（单实例，随滑层移动），行内不再渲染
    for (const row of rows) expect(row.find('.action-menu-hint').exists()).toBe(false)
    expect(wrapper.get('.selection-hint .action-menu-hint').classes()).toContain('show')
  })

  it('BaseList actionHint 谓词 → 按焦点行条件显示（全局结果路径）', async () => {
    const wrapper = mountList((item) => (item as { id: string }).id === 'a')
    expect(wrapper.get('.selection-hint .action-menu-hint').classes()).toContain('show')
    // 导航到不满足谓词的行：徽标淡出（show 摘除），运动仍由滑层承载
    document.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }),
    )
    await flushPromises()
    expect(wrapper.get('.selection-hint .action-menu-hint').classes()).not.toContain('show')
  })

  it('BaseList 缺省（无 actionHint）→ 不渲染（其余列表零开销）', () => {
    const wrapper = mountList()
    expect(wrapper.find('.selection-hint .action-menu-hint').exists()).toBe(false)
  })
})
