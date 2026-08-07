import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import BaseList from './BaseList.vue'

interface Item {
  id: string
  title: string
}

function items(count: number): Item[] {
  return Array.from({ length: count }, (_, i) => ({ id: String(i), title: `item-${i}` }))
}

describe('BaseList', () => {
  it('结果缩短时释放已卸载的 DOM 引用并裁掉尾部空槽', async () => {
    const wrapper = mount(BaseList<Item>, {
      props: { items: items(100) },
      slots: { item: ({ item }: { item: Item }) => item.title },
    })

    expect(wrapper.findAll('[role="option"]')).toHaveLength(100)

    await wrapper.setProps({ items: items(3) })
    await nextTick()

    expect(wrapper.findAll('[role="option"]')).toHaveLength(3)
    const refs = (
      wrapper.vm.$ as unknown as { setupState: { itemRefs: Array<HTMLElement | null> } }
    ).setupState.itemRefs
    expect(refs.filter(Boolean)).toHaveLength(3)
    expect(refs).toHaveLength(3)
  })

  it('卸载时清空全部 DOM 引用', () => {
    const wrapper = mount(BaseList<Item>, {
      props: { items: items(20) },
      slots: { item: ({ item }: { item: Item }) => item.title },
    })
    const refs = (
      wrapper.vm.$ as unknown as { setupState: { itemRefs: Array<HTMLElement | null> } }
    ).setupState.itemRefs

    expect(refs.filter(Boolean)).toHaveLength(20)
    wrapper.unmount()
    expect(refs.filter(Boolean)).toHaveLength(0)
  })
})
