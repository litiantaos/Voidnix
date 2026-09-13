import { describe, it, expect, beforeEach } from 'vitest'
import { mount, type VueWrapper } from '@vue/test-utils'
import { nextTick, h, KeepAlive } from 'vue'
import { createPinia, setActivePinia } from 'pinia'
import { useAppStore } from '@/stores/app'
import BaseList from './BaseList.vue'

interface Item {
  id: string
  title: string
}

function items(count: number): Item[] {
  return Array.from({ length: count }, (_, i) => ({ id: String(i), title: `item-${i}` }))
}

describe('BaseList', () => {
  beforeEach(() => {
    // BaseList 读 app store（整窗视图让位守卫）：挂载需 active pinia
    setActivePinia(createPinia())
  })

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
    wrapper.unmount()
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

  it('跨会话转移（activeExtId 变化）统一归首项并同步父级镜像；受控列表（KeepAlive 外）不参与', async () => {
    const appStore = useAppStore()
    // KeepAlive 包裹：自管列表场景（挂载即 activated，inKeepAliveTree 置位）
    const selfManaged = mount({
      setup: () => () => h(KeepAlive, () => h(BaseList, { items: items(10) })),
    })
    // 泛型组件 findComponent 类型推断退化为 DOMWrapper，断言回 VueWrapper
    const list = selfManaged.findComponent(BaseList) as unknown as VueWrapper
    ;(
      list.vm.$ as unknown as { setupState: { setSelectedIndex: (i: number) => void } }
    ).setupState.setSelectedIndex(5)
    expect(list.emitted('select')?.at(-1)).toEqual([5])

    // 进入扩展（null → ext）：归零 + emit 同步镜像
    appStore.setActiveExtension('clipboard')
    await nextTick()
    expect(list.emitted('select')?.at(-1)).toEqual([0])

    // 退出扩展（ext → null）：再次归零
    appStore.setActiveExtension(null)
    await nextTick()
    expect(list.emitted('select')?.at(-1)).toEqual([0])

    // 受控列表（KeepAlive 外，标准列表场景）：activeExtId 变化不归零不回写——
    // exitExtension 同步恢复的 savedToolIndex 不被覆盖
    const controlled = mount(BaseList<Item>, {
      props: { items: items(10), selectedIndex: 5 },
      slots: { item: ({ item }: { item: Item }) => item.title },
    })
    appStore.setActiveExtension('clipboard')
    await nextTick()
    expect(controlled.emitted('select')).toBeUndefined()
    expect(controlled.emitted('update:selectedIndex')).toBeUndefined()
    // 卸载清 document 键盘监听：残留监听会消费后续用例派发的按键（Enter 执行即消费）
    selfManaged.unmount()
    controlled.unmount()
  })

  it('Enter auto-repeat 不执行行项：跨扩展跳转按住回车的 repeat 不得落到目标视图首行', async () => {
    const wrapper = mount(BaseList<Item>, {
      props: { items: items(3) },
      slots: { item: ({ item }: { item: Item }) => item.title },
    })
    document.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true, repeat: true }),
    )
    await nextTick()
    expect(wrapper.emitted('execute')).toBeUndefined()
    wrapper.unmount()
  })

  it('Enter 执行即消费：同一按键不再触发后续 document 监听', () => {
    const wrapper = mount(BaseList<Item>, {
      props: { items: items(3) },
      slots: { item: ({ item }: { item: Item }) => item.title },
    })
    // 注册在 BaseList（先 mount）之后的 document 监听，模拟 useResultNavigation 等后续消费者
    let laterSawEnter = false
    const later = (e: KeyboardEvent) => {
      if (e.key === 'Enter') laterSawEnter = true
    }
    document.addEventListener('keydown', later)
    document.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }),
    )
    document.removeEventListener('keydown', later)
    expect(wrapper.emitted('execute')).toHaveLength(1)
    expect(laterSawEnter).toBe(false)
    wrapper.unmount()
  })
})
