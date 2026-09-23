import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { mount, flushPromises, enableAutoUnmount, type VueWrapper } from '@vue/test-utils'
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

  // 用例中途断言失败时内联清理不会执行：泄漏的 rAF/performance stub、body 挂载的
  // scroller 与存活的 wrapper（其 document 级按键监听含 stopImmediatePropagation，
  // 会消费后续用例派发的按键）会把一次失败级联成连锁失败，统一在此兜底恢复
  enableAutoUnmount(afterEach)
  afterEach(() => {
    vi.unstubAllGlobals()
    vi.restoreAllMocks()
    document.body.querySelectorAll('.overflow-y-auto').forEach((el) => el.remove())
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

  it('reset() 归零并同步父级镜像（会话复位的瞬时通道）；空列表安全', async () => {
    const wrapper = mount(BaseList<Item>, {
      props: { items: items(10) },
      slots: { item: ({ item }: { item: Item }) => item.title },
    })
    const setupState = (
      wrapper.vm.$ as unknown as {
        setupState: {
          setSelectedIndex: (i: number) => void
          reset: () => Promise<void>
        }
      }
    ).setupState
    setupState.setSelectedIndex(5)
    expect(wrapper.emitted('select')?.at(-1)).toEqual([5])

    await setupState.reset()
    await nextTick()
    // 归零 + emit 同步镜像（View 的 @select 回写 selectedIndex）
    expect(wrapper.emitted('select')?.at(-1)).toEqual([0])
    expect(wrapper.emitted('update:selectedIndex')?.at(-1)).toEqual([0])
    // 重复 reset 幂等
    await setupState.reset()
    expect(wrapper.emitted('select')?.at(-1)).toEqual([0])

    // 空列表（v-if 卸载前的边界）：行 0 不存在时不滚动不抛错
    await wrapper.setProps({ items: [] })
    await setupState.reset()
    wrapper.unmount()
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

  it('选中高亮滑块：挂载落位首行、随选中移动、items 替换重落位、越界隐藏', async () => {
    const wrapper = mount(BaseList<Item>, {
      props: { items: items(3) },
      slots: { item: ({ item }: { item: Item }) => item.title },
    })

    // happy-dom 无布局引擎：mock 行几何（40px 步进、高 32）
    const mockGeometry = () =>
      wrapper.findAll('[role="option"]').forEach((row, i) => {
        Object.defineProperty(row.element, 'offsetTop', { value: i * 40, configurable: true })
        Object.defineProperty(row.element, 'offsetHeight', { value: 32, configurable: true })
      })
    mockGeometry()
    // 挂载落位发生在 onMounted（先于几何 mock），显式重落位建立确定性基线
    ;(
      wrapper.vm.$ as unknown as { setupState: { placeIndicator: (animated: boolean) => void } }
    ).setupState.placeIndicator(false)

    const indicator = wrapper.get('.selection-indicator')
    // 挂载即瞬时落位聚焦行；行自身 ui-active 保留语义，背景让位（list-focus-row）
    expect(indicator.attributes('style')).toContain('translateY(0px)')
    expect(indicator.attributes('style')).toContain('height: 32px')
    const rows = wrapper.findAll('[role="option"]')
    expect(rows[0].classes()).toContain('list-focus-row')
    expect(rows[1].classes()).not.toContain('list-focus-row')

    // 同列表内移动：滑块跟随新行
    document.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }),
    )
    await flushPromises()
    expect(indicator.attributes('style')).toContain('translateY(40px)')
    expect(wrapper.findAll('[role="option"]')[1].classes()).toContain('list-focus-row')

    // items 替换（引用变化）：滑块按当前几何重落位（key 不变复用元素，几何已变）
    Object.defineProperty(wrapper.findAll('[role="option"]')[1].element, 'offsetTop', {
      value: 99,
      configurable: true,
    })
    await wrapper.setProps({ items: items(2) })
    await flushPromises()
    expect(indicator.attributes('style')).toContain('translateY(99px)')

    // 列表缩短致选中越界：滑块隐藏（无高亮行）
    await wrapper.setProps({ items: items(1) })
    await flushPromises()
    expect(indicator.attributes('style')).toContain('display: none')
    wrapper.unmount()
  })

  it('wrap 回首项正确滚动返回顶部：滑块作 list-body 首子元素不得充当滚动锚', async () => {
    // index 0 的前邻是选中滑块（非分组标题）：盲取 previousElementSibling 作滚动锚
    // 会把矩形取到滑块位置，误判「在视野内」→ wrap 到 0 不滚动
    const frames: FrameRequestCallback[] = []
    vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
      frames.push(cb)
      return frames.length
    })
    vi.stubGlobal('cancelAnimationFrame', () => {})

    const scroller = document.createElement('div')
    scroller.className = 'overflow-y-auto'
    document.body.appendChild(scroller)
    const wrapper = mount(BaseList<Item>, {
      props: { items: items(3) },
      slots: { item: ({ item }: { item: Item }) => item.title },
      attachTo: scroller,
    })

    // 几何：行 0 在视野上方（wrap 目标），行 1/2 在视野内
    const rect = (el: Element, top: number, bottom: number) => {
      Object.defineProperty(el, 'getBoundingClientRect', {
        value: () => ({ top, bottom, height: bottom - top }),
        configurable: true,
      })
    }
    rect(scroller, 0, 200)
    const rows = wrapper.findAll('[role="option"]')
    rect(rows[0].element, -220, -188)
    rect(rows[1].element, 100, 132)
    rect(rows[2].element, 140, 172)
    scroller.scrollTop = 220

    const setSelected = (i: number) =>
      (
        wrapper.vm.$ as unknown as { setupState: { setSelectedIndex: (i: number) => void } }
      ).setupState.setSelectedIndex(i)

    // 可控时钟：连击判定（动画包络 80% 同源推导）读 performance.now，真实毫秒间隔会被误判连击
    let simNow = 0
    vi.spyOn(performance, 'now').mockImplementation(() => simNow)

    // 选中行 2（视野内，无滚动帧），随后 ArrowDown wrap 回行 0（视野上方）
    setSelected(2)
    await flushPromises()
    frames.length = 0
    simNow = 400
    document.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }),
    )
    await flushPromises()
    // wrap 首↔末一律瞬时跳变（锚点回归仍以「滚动确实发生」体现：直写 scrollTop = 0）
    expect(frames.length).toBe(0)
    expect(scroller.scrollTop).toBe(0)
    expect(wrapper.get('.selection-indicator').classes()).not.toContain('follow')

    // 反向 wrap（0 → 末项）：同样瞬时跳变
    frames.length = 0
    simNow = 600
    document.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true, cancelable: true }),
    )
    await flushPromises()
    expect(frames.length).toBe(0)
    expect(wrapper.get('.selection-indicator').classes()).not.toContain('follow')

    wrapper.unmount()
  })

  it('视口跟随滚动与滑块同参：逐帧推进、用户滚动中断、items 替换瞬时跳变', async () => {
    // 手动帧泵：确定性驱动 rAF 时间戳（t0 取首帧时间戳）
    const frames: FrameRequestCallback[] = []
    vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
      frames.push(cb)
      return frames.length
    })
    vi.stubGlobal('cancelAnimationFrame', () => {})

    const scroller = document.createElement('div')
    scroller.className = 'overflow-y-auto'
    document.body.appendChild(scroller)
    const wrapper = mount(BaseList<Item>, {
      props: { items: items(6) },
      slots: { item: ({ item }: { item: Item }) => item.title },
      attachTo: scroller,
    })

    // happy-dom 无布局引擎：mock 滚动容器与行矩形（行均在视野下方，容器高 200）
    const rect = (el: Element, top: number, bottom: number) => {
      Object.defineProperty(el, 'getBoundingClientRect', {
        value: () => ({ top, bottom, height: bottom - top }),
        configurable: true,
      })
    }
    rect(scroller, 0, 200)
    wrapper
      .findAll('[role="option"]')
      .forEach((row, i) => rect(row.element, 220 + i * 40, 252 + i * 40))
    const indicator = wrapper.get('.selection-indicator')

    const arrowDown = () =>
      document.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }),
      )

    // 可控时钟：连击判定（动画包络 80% 同源推导）读 performance.now，真实毫秒间隔会被误判连击
    let simNow = 0
    vi.spyOn(performance, 'now').mockImplementation(() => simNow)

    // 导航至行 1（bottom 292 > 可视底 188）：目标 scrollTop = 104；滚动先行，滑块转
    // follow 模式（滞后同曲线跟随），滚动 rAF 逐帧推进（--duration-normal + ease-inout）
    arrowDown()
    await flushPromises()
    expect(frames.length).toBe(1)
    expect(indicator.classes()).toContain('follow')
    frames.shift()!(0) // t0=0、p=0 → 仍在起点
    expect(scroller.scrollTop).toBe(0)
    frames.shift()!(100) // p=0.5，ease-inout 中程 =0.5
    expect(scroller.scrollTop).toBeGreaterThan(30)
    expect(scroller.scrollTop).toBeLessThan(80)
    frames.shift()!(200) // p=1 → 到达目标，动画结束
    expect(scroller.scrollTop).toBe(104)
    expect(frames.length).toBe(0)

    // 用户手动滚动（滚轮/拖拽）：下一帧检测到外源写入即中断让权
    simNow = 400 // 距上次导航 > follow 档 208ms（(60+200)×0.8），保持动画路径
    arrowDown()
    await flushPromises()
    expect(frames.length).toBe(1)
    frames.shift()!(400) // t0、p=0 → 写起始值 104
    scroller.scrollTop = 37
    frames.shift()!(460)
    expect(scroller.scrollTop).toBe(37)
    expect(frames.length).toBe(0)

    // items 替换 + 归零（同 flush，模拟「结果替换 + 重置选中」）：与滑块一致瞬时跳变
    // （follow 摘除、直写 scrollTop），不排 rAF 帧
    simNow = 700
    const applied = wrapper.setProps({ items: items(3) })
    ;(
      wrapper.vm.$ as unknown as { setupState: { setSelectedIndex: (i: number) => void } }
    ).setupState.setSelectedIndex(0)
    await applied
    await flushPromises()
    // 行 0（bottom 252 > 188）目标 = 37 + 64 = 101
    expect(scroller.scrollTop).toBe(101)
    expect(frames.length).toBe(0)
    expect(indicator.classes()).not.toContain('follow')

    wrapper.unmount()
  })

  it('连击瞬时步进（快于动画包络 80%）：不排 rAF 帧、不挂 follow', async () => {
    const frames: FrameRequestCallback[] = []
    vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
      frames.push(cb)
      return frames.length
    })
    vi.stubGlobal('cancelAnimationFrame', () => {})

    const scroller = document.createElement('div')
    scroller.className = 'overflow-y-auto'
    document.body.appendChild(scroller)
    const wrapper = mount(BaseList<Item>, {
      props: { items: items(4) },
      slots: { item: ({ item }: { item: Item }) => item.title },
      attachTo: scroller,
    })
    const rect = (el: Element, top: number, bottom: number) => {
      Object.defineProperty(el, 'getBoundingClientRect', {
        value: () => ({ top, bottom, height: bottom - top }),
        configurable: true,
      })
    }
    rect(scroller, 0, 200)
    wrapper
      .findAll('[role="option"]')
      .forEach((row, i) => rect(row.element, 220 + i * 40, 252 + i * 40))
    const indicator = wrapper.get('.selection-indicator')
    const arrowDown = () =>
      document.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }),
      )

    let simNow = 0
    vi.spyOn(performance, 'now').mockImplementation(() => simNow)

    // 首按（距上次导航 ∞）：动画路径，排帧
    arrowDown()
    await flushPromises()
    expect(frames.length).toBe(1)
    frames.shift()!(0)
    frames.shift()!(100)
    frames.shift()!(200)
    expect(scroller.scrollTop).toBe(104)

    // 100ms 后连击（跨界 < follow 档 208ms）：瞬时步进，零帧零 follow，scrollTop 直写
    // 目标（行 2 底边 332 − 可视底 188 = 144 → 104 + 144 = 248）
    simNow = 100
    arrowDown()
    await flushPromises()
    expect(frames.length).toBe(0)
    expect(scroller.scrollTop).toBe(248)
    expect(indicator.classes()).not.toContain('follow')

    // 停顿超过窗口后恢复动画
    simNow = 500
    arrowDown()
    await flushPromises()
    expect(frames.length).toBe(1)
    expect(indicator.classes()).toContain('follow')
    frames.length = 0

    wrapper.unmount()
  })

  it('prefers-reduced-motion：滑层与视口跟随退化为瞬时（零 rAF 帧、直写 scrollTop）', async () => {
    vi.spyOn(window, 'matchMedia').mockReturnValue({ matches: true } as MediaQueryList)
    const frames: FrameRequestCallback[] = []
    vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
      frames.push(cb)
      return frames.length
    })
    vi.stubGlobal('cancelAnimationFrame', () => {})

    const scroller = document.createElement('div')
    scroller.className = 'overflow-y-auto'
    document.body.appendChild(scroller)
    const wrapper = mount(BaseList<Item>, {
      props: { items: items(4) },
      slots: { item: ({ item }: { item: Item }) => item.title },
      attachTo: scroller,
    })
    // happy-dom 无布局引擎：mock 滚动容器与行矩形（行均在视野下方，容器高 200；行高 32、40px 步进）
    const rect = (el: Element, top: number, bottom: number) => {
      Object.defineProperty(el, 'getBoundingClientRect', {
        value: () => ({ top, bottom, height: bottom - top }),
        configurable: true,
      })
    }
    rect(scroller, 0, 200)
    wrapper
      .findAll('[role="option"]')
      .forEach((row, i) => rect(row.element, 220 + i * 40, 252 + i * 40))
    wrapper.findAll('[role="option"]').forEach((row, i) => {
      Object.defineProperty(row.element, 'offsetTop', { value: i * 40, configurable: true })
      Object.defineProperty(row.element, 'offsetHeight', { value: 32, configurable: true })
    })
    const indicator = wrapper.get('.selection-indicator')

    document.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }),
    )
    await flushPromises()
    // 跨界导航同连击 snap：不排帧、不挂 follow，scrollTop 直写目标（行 1 底边
    // 292 − 可视底 188 = 104）；滑块直落位行 1
    expect(frames.length).toBe(0)
    expect(scroller.scrollTop).toBe(104)
    expect(indicator.classes()).not.toContain('follow')
    expect(indicator.attributes('style')).toContain('translateY(40px)')

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
