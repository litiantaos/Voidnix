import { describe, it, expect, beforeEach } from 'vitest'
import { mount, type VueWrapper } from '@vue/test-utils'
import { nextTick, h, KeepAlive, defineComponent, ref } from 'vue'
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

  it('已停用 KeepAlive 子树内重挂载的列表不响应键盘（后台列表不得劫持导航与回车）', async () => {
    // 复现路径：停用视图的响应式 watcher 仍活跃，驱动其内部 v-if 翻转使列表卸载后
    // 在停用树内重挂载（如剪贴板 history 随全局 query 过滤清空再回填）——重挂载不
    // 触发 activated/deactivated 钩子对，初始态若按 active 处理会成为后台仍消费
    // ↑↓ 与 Enter 的僵尸列表（用户在其它扩展里回车触发剪贴板粘贴的根因）
    const showA = ref(true)
    const showList = ref(true)
    const selects: number[] = []
    let executes = 0
    const ChildA = defineComponent({
      setup: () => () =>
        showList.value
          ? h(BaseList, {
              items: items(3),
              onSelect: (i: number) => selects.push(i),
              onExecute: () => executes++,
            })
          : h('div'),
    })
    const ChildB = defineComponent({ setup: () => () => h('div') })
    const wrapper = mount({
      setup: () => () => h(KeepAlive, () => (showA.value ? h(ChildA) : h(ChildB))),
    })

    // 切到 B 停用 A，随后 A 停用期间其内部 v-if 翻转一轮（卸载 → 停用树内重挂载）
    showA.value = false
    await nextTick()
    showList.value = false
    await nextTick()
    showList.value = true
    await nextTick()

    document.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }),
    )
    await nextTick()
    expect(selects).toEqual([])

    document.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }),
    )
    await nextTick()
    expect(executes).toBe(0)

    // 重新激活后恢复正常响应（激活转换经根注入钩子/实时判定恢复）
    showA.value = true
    await nextTick()
    document.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }),
    )
    await nextTick()
    expect(selects).toEqual([1])
    document.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }),
    )
    await nextTick()
    expect(executes).toBe(1)

    wrapper.unmount()
  })

  it('列表缩短致选中越界：Enter 不派发 undefined，方向键 wrap 自愈（H7 同族）', async () => {
    const wrapper = mount(BaseList<Item>, {
      props: { items: items(6) },
      slots: { item: ({ item }: { item: Item }) => item.title },
    })
    for (let i = 0; i < 5; i++) {
      document.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }),
      )
    }
    await nextTick()
    expect(wrapper.emitted('select')?.at(-1)).toEqual([5])

    // 过滤使列表缩短为 2 项：localIndex=5 越界（无高亮），Enter 不派发 undefined
    await wrapper.setProps({ items: items(2) })
    await nextTick()
    document.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }),
    )
    await nextTick()
    expect(wrapper.emitted('execute')).toBeUndefined()

    // 方向键自愈：越界索引经 wrapIndex 回到 0
    document.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }),
    )
    await nextTick()
    expect(wrapper.emitted('select')?.at(-1)).toEqual([0])
    wrapper.unmount()
  })
})
