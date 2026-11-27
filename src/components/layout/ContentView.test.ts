import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick, ref, defineComponent, h } from 'vue'
import { createPinia, setActivePinia } from 'pinia'

import ContentView from './ContentView.vue'
import { useAppStore } from '@/stores/app'
import { useRecentsStore } from '@/stores/recents'
import { useScreenSummary } from '@/composables/useScreenSummary'
import type { Extension, SearchResult } from '@/runtime/types'
import '@/locales'

// 探针视图：模块级挂载计数 + 可变本地状态，验证 KeepAlive 缓存往返保留
let probeMounts = 0
let bumpProbe: (() => void) | null = null
const Probe = defineComponent({
  setup() {
    probeMounts++
    const tick = ref(0)
    bumpProbe = () => {
      tick.value++
    }
    return () => h('div', { id: 'probe' }, String(tick.value))
  },
})

const SubProbe = defineComponent({
  setup: () => () => h('div', { id: 'sub-probe' }, 'sub'),
})

const fakeExt = {
  meta: { id: 'probe', name: 'probe' },
  mainView: Probe,
  subviews: {
    detail: { view: SubProbe, title: 'Detail', height: 'auto' },
  },
} as unknown as Extension

const SummaryProbe = defineComponent({
  setup() {
    useScreenSummary(() => 'SUMMARY')
    return () => h('div', { id: 'summary-probe' })
  },
})
const summaryExt = {
  meta: { id: 'summary-probe', name: 'Summary Probe' },
  mainView: SummaryProbe,
  subviews: { detail: { view: SummaryProbe, title: 'Detail' } },
} as unknown as Extension

const mountedWrappers: ReturnType<typeof mount>[] = []

function mountView(extension: Extension | null = fakeExt) {
  const wrapper = mount(ContentView, {
    props: { results: [] as SearchResult[], selectedIndex: 0, extension },
  })
  mountedWrappers.push(wrapper)
  return wrapper
}

describe('ContentView KeepAlive 缓存语义', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    probeMounts = 0
    bumpProbe = null
  })
  afterEach(() => {
    while (mountedWrappers.length) mountedWrappers.pop()!.unmount()
  })

  it('扩展视图经主界面往返保留本地状态（不销毁重建）', async () => {
    expect(useAppStore().activeSubview).toBeNull()
    const wrapper = mountView()
    await nextTick()
    expect(probeMounts).toBe(1)
    expect(wrapper.find('#probe').text()).toBe('0')

    bumpProbe!()
    await nextTick()
    expect(wrapper.find('#probe').text()).toBe('1')

    // 退出到主界面（重看引导流程必经）：视图不渲染但缓存保留
    await wrapper.setProps({ extension: null })
    await nextTick()
    expect(wrapper.find('#probe').exists()).toBe(false)

    // 返回扩展：缓存命中，状态保留且未重挂载（选中态/滚动位随视图实例存活）
    await wrapper.setProps({ extension: fakeExt })
    await nextTick()
    expect(probeMounts).toBe(1)
    expect(wrapper.find('#probe').text()).toBe('1')
  })

  it('ContentView provide 屏身份：摘要注册到主视图 / 子视图正确 key', async () => {
    const appStore = useAppStore()
    const recents = useRecentsStore()
    recents.clear()

    appStore.setActiveExtension('summary-probe')
    mountView(summaryExt)
    await nextTick()
    appStore.setActiveExtension(null)
    expect(recents.views[0]).toMatchObject({
      extId: 'summary-probe',
      subview: null,
      summary: 'SUMMARY',
    })

    // 同一 ContentView 内主视图 → 子视图：上下文切换后新挂载的实例注册到子视图 key
    recents.clear()
    appStore.setActiveExtension('summary-probe')
    appStore.openSubview('detail')
    await nextTick()
    appStore.setActiveExtension(null)
    expect(recents.views[0]).toMatchObject({
      extId: 'summary-probe',
      subview: 'detail',
      summary: 'SUMMARY',
    })
  })

  it('子视图描述符：渲染 view，height 覆盖进入 auto 高度模式', async () => {
    const appStore = useAppStore()
    appStore.openSubview('detail')
    const wrapper = mountView()
    await nextTick()

    expect(wrapper.find('#sub-probe').exists()).toBe(true)
    expect(wrapper.find('#probe').exists()).toBe(false)
    expect(wrapper.find('.view-auto').exists()).toBe(true)
  })

  it('窗口隐藏不卸载视图（无缝继续）：仅 content-visibility 释放 layer，状态与实例冻结', async () => {
    const wrapper = mountView()
    await nextTick()
    expect(probeMounts).toBe(1)
    expect(wrapper.find('#probe').text()).toBe('0')

    bumpProbe!()
    await nextTick()
    expect(wrapper.find('#probe').text()).toBe('1')

    // 隐藏（clearCache）：视图实例保持挂载（滚动/选中/会话状态随 DOM 冻结保留），
    // layer 释放走 contentRef 的 content-visibility:hidden，不卸载 KeepAlive
    window.dispatchEvent(new Event('window-hiding'))
    await nextTick()
    await nextTick()
    await nextTick()
    expect(probeMounts).toBe(1)
    expect(wrapper.find('#probe').exists()).toBe(true)
    expect(wrapper.find('#probe').text()).toBe('1')
  })
})
