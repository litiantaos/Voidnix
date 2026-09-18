import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
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

  it('BaseList actionHint=true → 每行渲染（剪贴板/ai-providers 路径）', () => {
    const wrapper = mountList(true)
    const rows = wrapper.findAll('[role="option"]')
    expect(rows).toHaveLength(2)
    for (const row of rows) expect(row.find('.action-menu-hint').exists()).toBe(true)
  })

  it('BaseList actionHint 谓词 → 按 item 条件渲染（全局结果路径）', () => {
    const wrapper = mountList((item) => (item as { id: string }).id === 'a')
    const rows = wrapper.findAll('[role="option"]')
    expect(rows[0].find('.action-menu-hint').exists()).toBe(true)
    expect(rows[1].find('.action-menu-hint').exists()).toBe(false)
  })

  it('BaseList 缺省（无 actionHint）→ 不渲染（其余列表零开销）', () => {
    const wrapper = mountList()
    for (const row of wrapper.findAll('[role="option"]')) {
      expect(row.find('.action-menu-hint').exists()).toBe(false)
    }
  })
})
