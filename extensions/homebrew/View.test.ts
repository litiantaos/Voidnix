import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { KeepAlive, defineComponent, h, nextTick, ref } from 'vue'

// View.vue 的 onActivated 恢复流依赖 Tauri invoke / 事件监听 / app store，全部 mock。
// 场景核心：升级中退出（窗口隐藏 → KeepAlive 整体卸载、组件销毁）后重进，
// brew_run_state 返回 Some 时应渲染列表 + 恢复运行态，而非阻断为加载态等操作结束。
const mocks = vi.hoisted(() => {
  const listeners = new Map<string, (e: { payload: unknown }) => void>()
  return {
    listeners,
    invoke: vi.fn(),
    listen: vi.fn(async (event: string, handler: (e: { payload: unknown }) => void) => {
      listeners.set(event, handler)
      return () => listeners.delete(event)
    }),
    showStatus: vi.fn(),
    openSubview: vi.fn(),
  }
})

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
    showStatus: mocks.showStatus,
    openSubview: mocks.openSubview,
  }),
}))

import '@/locales'
import './locales'
import View from './View.vue'

interface BrewStatusPayload {
  version: string
  packages: {
    name: string
    kind: string
    desc: string
    version: string
    new_version: string
  }[]
  has_update: boolean
  refreshing: boolean
}

let runState: { operation: string; step: string } | null = null
let brewStatus: () => Promise<BrewStatusPayload> = () => Promise.resolve(statusPayload())

function statusPayload(): BrewStatusPayload {
  return {
    version: '4.4.0',
    packages: [
      {
        name: 'git',
        kind: 'formula',
        desc: 'Distributed VCS',
        version: '2.40.0',
        new_version: '2.43.0',
      },
    ],
    has_update: true,
    refreshing: false,
  }
}

function mountHost() {
  const show = ref(true)
  const wrapper = mount(
    defineComponent({
      setup: () => () => h(KeepAlive, () => (show.value ? h(View) : null)),
    }),
  )
  return { wrapper, show }
}

async function flush(times = 10) {
  for (let i = 0; i < times; i++) await nextTick()
}

beforeEach(() => {
  mocks.listeners.clear()
  mocks.invoke.mockReset()
  mocks.invoke.mockImplementation((cmd: string) => {
    if (cmd === 'brew_run_state') return Promise.resolve(runState)
    if (cmd === 'brew_status') return brewStatus()
    if (cmd === 'brew_services') return Promise.resolve([])
    return Promise.resolve(null)
  })
  runState = null
  brewStatus = () => Promise.resolve(statusPayload())
})

describe('homebrew View 运行态恢复', () => {
  it('空闲进入：拉数据渲染列表，无运行态', async () => {
    const { wrapper } = mountHost()
    await flush()

    expect(wrapper.text()).toContain('git')
    expect(wrapper.text()).not.toContain('加载中')
    expect(wrapper.text()).not.toContain('升级中')
  })

  it('升级中退出后重进：渲染列表 + 恢复运行态显示当前步骤，不阻断为加载态', async () => {
    // 第一阶段：空闲进入（列表已拉取）
    const first = mountHost()
    await flush()
    first.wrapper.unmount()

    // 第二阶段：后台升级仍在进行，重进（全新实例，模拟窗口隐藏后 KeepAlive 整体卸载）
    runState = { operation: 'update_upgrade', step: 'upgrade' }
    const second = mountHost()
    await flush()

    const text = second.wrapper.text()
    expect(text).toContain('git')
    expect(text).not.toContain('加载中')
    expect(text).toContain('升级中')
    // 运行中禁用更新按钮，防重复触发
    expect(second.wrapper.html()).toContain('disabled')
    second.wrapper.unmount()
  })

  it('后台操作完成：brew-run-done 清运行态并重拉最新状态', async () => {
    runState = { operation: 'update_upgrade', step: 'upgrade' }
    const { wrapper } = mountHost()
    await flush()
    expect(wrapper.text()).toContain('升级中')

    runState = null
    const fresh = statusPayload()
    fresh.packages[0].new_version = ''
    fresh.has_update = false
    brewStatus = () => Promise.resolve(fresh)
    mocks.listeners.get('brew-run-done')?.({ payload: { operation: 'update_upgrade', step: '' } })
    await flush()

    expect(wrapper.text()).not.toContain('升级中')
    expect(wrapper.text()).not.toContain('2.43.0')
    wrapper.unmount()
  })

  it('查询返回过期 Some（done 事件先于响应到达）：丢弃运行态恢复，不卡死', async () => {
    runState = { operation: 'update_upgrade', step: 'upgrade' }
    // brew_run_state 响应挂起：期间 done 事件先到（事件与 invoke 响应投递通道不同，无顺序保证）
    let resolveState: (v: unknown) => void = () => {}
    mocks.invoke.mockImplementation((cmd: string) => {
      if (cmd === 'brew_run_state') return new Promise((r) => (resolveState = r))
      if (cmd === 'brew_status') return brewStatus()
      if (cmd === 'brew_services') return Promise.resolve([])
      return Promise.resolve(null)
    })
    const { wrapper } = mountHost()
    await flush()

    // 操作结束：done 先于查询响应到达，监听器已清态并重拉
    runState = null
    mocks.listeners.get('brew-run-done')?.({ payload: null })
    await flush()

    // 过期的 Some 响应此时才到达：不得恢复运行态
    resolveState({ operation: 'update_upgrade', step: 'upgrade' })
    await flush()

    expect(wrapper.text()).toContain('git')
    expect(wrapper.text()).not.toContain('升级中')
    wrapper.unmount()
  })

  it('恢复态拉取与完成重拉并发：过期响应不覆盖最新结果', async () => {
    runState = { operation: 'update_upgrade', step: 'upgrade' }
    // 第一轮（onActivated 恢复态拉取）挂起不返回
    let resolveFirst: (v: BrewStatusPayload) => void = () => {}
    const first = new Promise<BrewStatusPayload>((r) => {
      resolveFirst = r
    })
    brewStatus = () => first
    const { wrapper } = mountHost()
    await flush()

    // 完成事件在第一轮在途时触发第二轮（最新数据）
    runState = null
    const fresh = statusPayload()
    fresh.version = '4.5.0'
    brewStatus = () => Promise.resolve(fresh)
    mocks.listeners.get('brew-run-done')?.({ payload: null })
    await flush()
    expect(wrapper.text()).toContain('4.5.0')

    // 过期的第一轮此时才返回：旧数据不得落盘
    resolveFirst(statusPayload())
    await flush()
    expect(wrapper.text()).toContain('4.5.0')
    expect(wrapper.text()).not.toContain('加载中')
    wrapper.unmount()
  })
})
