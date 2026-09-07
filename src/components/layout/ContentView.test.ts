import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick, ref, defineComponent, h } from 'vue'
import { createPinia, setActivePinia } from 'pinia'

import ContentView from './ContentView.vue'
import { useAppStore } from '@/stores/app'
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

const fakeExt = {
  meta: { id: 'probe', name: 'probe' },
  mainView: () => Probe,
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

  it('窗口隐藏事件卸载 KeepAlive 缓存（重进扩展重挂载）', async () => {
    const wrapper = mountView()
    await nextTick()
    expect(probeMounts).toBe(1)

    window.dispatchEvent(new Event('window-hiding'))
    await nextTick()
    await nextTick()
    expect(probeMounts).toBe(2)
    expect(wrapper.find('#probe').exists()).toBe(true)
  })
})
