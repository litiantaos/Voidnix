import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { KeepAlive, defineComponent, h, nextTick, ref } from 'vue'

// DetailView.vue 的加载/缓存态依赖 Tauri invoke / app store，全部 mock。
// 场景核心：列表↔详情往返的加载态不得互相顶掉已缓存内容（闪屏根因），
// 递归进入依赖时旧包 info 须立即弃用（防「新标题旧列表」错位）。
const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(async () => () => {}),
  showConfirm: vi.fn(),
  closeSubview: vi.fn(),
  setSearchQuery: vi.fn(),
}))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: mocks.invoke,
  Channel: class {
    onmessage: unknown = null
  },
}))
vi.mock('@tauri-apps/api/event', () => ({ listen: mocks.listen }))
vi.mock('@/utils/tauri', () => ({ isTauri: true }))
vi.mock('@/stores/app', () => ({
  useAppStore: () => ({
    searchQuery: '',
    showConfirm: mocks.showConfirm,
    closeSubview: mocks.closeSubview,
    setSearchQuery: mocks.setSearchQuery,
  }),
}))

import '@/locales'
import './locales'
import DetailView from './DetailView.vue'

function setTarget(name: string) {
  sessionStorage.setItem(
    'homebrew:detail',
    JSON.stringify({ name, kind: 'formula', version: '1.0.0', desc: '' }),
  )
}

function mountHost() {
  const show = ref(true)
  const wrapper = mount(
    defineComponent({
      setup: () => () => h(KeepAlive, () => (show.value ? h(DetailView) : null)),
    }),
  )
  return { wrapper, show }
}

async function flush(times = 10) {
  for (let i = 0; i < times; i++) await nextTick()
}

beforeEach(() => {
  mocks.invoke.mockReset()
  mocks.listen.mockClear()
  sessionStorage.removeItem('homebrew:detail')
})

describe('homebrew DetailView 加载态', () => {
  it('初次挂载首帧即 loading 态：不闪「无匹配」空态', async () => {
    setTarget('git')
    // run_state 与 brew_info 均挂起，模拟 onActivated 的 IPC 往返延迟
    mocks.invoke.mockImplementation(() => new Promise(() => {}))
    const { wrapper } = mountHost()
    await flush()

    expect(wrapper.find('i.i-ri-loader-4-line').exists()).toBe(true)
    expect(wrapper.text()).not.toContain('无匹配')
    wrapper.unmount()
  })

  it('重激活重拉在途：保留缓存列表，不闪 spinner', async () => {
    setTarget('git')
    mocks.invoke.mockImplementation((cmd: string) => {
      if (cmd === 'brew_run_state') return Promise.resolve(null)
      if (cmd === 'brew_info')
        return Promise.resolve({
          desc: '',
          deps: [{ name: 'pcre2', version: '10.4', desc: '' }],
          uses: [],
        })
      return Promise.resolve(null)
    })
    const { wrapper, show } = mountHost()
    await flush()
    expect(wrapper.text()).toContain('pcre2')

    show.value = false
    await flush()
    mocks.invoke.mockImplementation((cmd: string) => {
      if (cmd === 'brew_run_state') return Promise.resolve(null)
      if (cmd === 'brew_info') return new Promise(() => {})
      return Promise.resolve(null)
    })
    show.value = true
    await flush()

    expect(wrapper.text()).toContain('pcre2')
    expect(wrapper.find('i.i-ri-loader-4-line').exists()).toBe(false)
    wrapper.unmount()
  })

  it('递归进入依赖：目标变化立即弃旧列表，不残留上一包内容', async () => {
    setTarget('git')
    mocks.invoke.mockImplementation((cmd: string) => {
      if (cmd === 'brew_run_state') return Promise.resolve(null)
      if (cmd === 'brew_info')
        return Promise.resolve({
          desc: '',
          deps: [{ name: 'pcre2', version: '10.4', desc: '' }],
          uses: [],
        })
      return Promise.resolve(null)
    })
    const { wrapper } = mountHost()
    await flush()
    expect(wrapper.text()).toContain('git')

    // 双击依赖项 pcre2：递归进入，新目标 brew_info 挂起
    mocks.invoke.mockImplementation((cmd: string) => {
      if (cmd === 'brew_run_state') return Promise.resolve(null)
      if (cmd === 'brew_info') return new Promise(() => {})
      return Promise.resolve(null)
    })
    const depRow = wrapper.findAll('[role="option"]').find((r) => r.text().includes('pcre2'))
    await depRow!.trigger('dblclick')
    await flush()

    expect(wrapper.find('i.i-ri-loader-4-line').exists()).toBe(true)
    expect(wrapper.text()).not.toContain('git')
    wrapper.unmount()
  })
})
