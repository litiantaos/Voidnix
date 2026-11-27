import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'

// 诊断子视图最近使用副标题：连接数 / 规则数（注册与离开时读取的接线测试）
const mocks = vi.hoisted(() => {
  const channels: { onmessage: ((payload: unknown) => void) | null }[] = []
  return {
    channels,
    invoke: vi.fn(),
  }
})

vi.mock('@tauri-apps/api/core', () => ({
  invoke: mocks.invoke,
  Channel: class {
    onmessage: ((payload: unknown) => void) | null = null
    constructor() {
      mocks.channels.push(this)
    }
  },
}))
vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn().mockRejectedValue(new Error('no store')),
}))
vi.mock('@/utils/tauri', () => ({ isTauri: false }))

import '@/locales'
import '../locales'
import { CMD } from '@/commands'
import { useAppStore } from '@/stores/app'
import { useRecentsStore } from '@/stores/recents'
import ConnectionsView from './ConnectionsView.vue'
import RulesView from './RulesView.vue'

const wrappers: ReturnType<typeof mount>[] = []
afterEach(() => {
  while (wrappers.length) wrappers.pop()!.unmount()
})

function mountView(component: typeof ConnectionsView | typeof RulesView) {
  const wrapper = mount(component)
  wrappers.push(wrapper)
  return wrapper
}

beforeEach(() => {
  setActivePinia(createPinia())
  useRecentsStore().clear()
  mocks.channels.length = 0
  mocks.invoke.mockReset()
  mocks.invoke.mockImplementation((async (cmd: string) => {
    if (cmd === CMD.proxyGetRules) {
      return {
        rules: [
          { type: 'DOMAIN', payload: 'a.example', proxy: 'DIRECT' },
          { type: 'DOMAIN', payload: 'b.example', proxy: 'DIRECT' },
          { type: 'DOMAIN', payload: 'c.example', proxy: 'DIRECT' },
        ],
      }
    }
    return undefined
  }) as unknown as typeof import('@tauri-apps/api/core').invoke)
})

/// 模拟离开一屏：setActiveExtension(null) 触发 recents 记录 + summary 读取。
function leaveScreen(subview: string) {
  const appStore = useAppStore()
  appStore.setActiveExtension('proxy')
  appStore.openSubview(subview)
  appStore.setActiveExtension(null)
  return useRecentsStore().views[0]
}

describe('proxy 诊断子视图最近使用摘要', () => {
  it('连接界面：活动连接数', async () => {
    mountView(ConnectionsView)
    await flushPromises()

    // 流帧推送两个连接（Channel 由 onMounted 创建）
    mocks.channels[0]!.onmessage?.({
      connections: [
        { id: '1', metadata: {}, upload: 0, download: 0, chains: [] },
        { id: '2', metadata: {}, upload: 0, download: 0, chains: [] },
      ],
    })
    await flushPromises()

    expect(leaveScreen('connections')).toMatchObject({
      extId: 'proxy',
      subview: 'connections',
      summary: '2 个连接',
    })
  })

  it('规则界面：规则总数', async () => {
    mountView(RulesView)
    await flushPromises()

    expect(leaveScreen('rules')).toMatchObject({
      extId: 'proxy',
      subview: 'rules',
      summary: '3 条规则',
    })
  })
})
