import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { defineComponent, h } from 'vue'
import { createPinia, setActivePinia } from 'pinia'

// 订阅二级界面动作：激活切换 / 行内更新 / 保存（含新建自动激活）/ 删除回退。
// Tauri 只需 invoke 最小分派；isTauri=false 跳过模块级自动更新。
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: unknown = null
  },
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}))
vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn().mockRejectedValue(new Error('no store')),
}))
vi.mock('@/utils/tauri', () => ({ isTauri: false }))

import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { config } from './config'
import { useSubscriptions } from './useSubscriptions'

function sub(partial: Partial<{ id: string; name: string; url: string; proxyCount: number }> = {}) {
  return {
    id: partial.id ?? 'a',
    name: partial.name ?? '',
    url: partial.url ?? 'https://s.example/sub',
    updatedAt: '',
    expiresAt: '',
    proxyCount: partial.proxyCount ?? 1,
  }
}

const mountedWrappers: ReturnType<typeof mount>[] = []

async function mountPanel() {
  let panel!: ReturnType<typeof useSubscriptions>
  const TestComp = defineComponent({
    setup() {
      panel = useSubscriptions()
      return () => h('div')
    },
  })
  mountedWrappers.push(mount(TestComp))
  await flushPromises()
  return panel
}

function callsOf(cmd: string) {
  return vi.mocked(invoke).mock.calls.filter(([c]) => c === cmd)
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.clearAllMocks()
  vi.mocked(invoke).mockImplementation((async (cmd: string) => {
    if (cmd === CMD.proxyUpdateSubscription) return { count: 12, expire: null, skipped: 0 }
    return undefined
  }) as unknown as typeof invoke)
  config.subscriptions = [
    sub({ id: 'a', name: '机场 A', url: 'https://a.example/sub', proxyCount: 42 }),
    sub({ id: 'b', name: '', url: '', proxyCount: 0 }),
  ]
  config.activeSubscriptionId = 'a'
})

afterEach(() => {
  while (mountedWrappers.length) mountedWrappers.pop()!.unmount()
})

describe('订阅二级界面动作', () => {
  it('回车分派：有节点切换激活；空订阅打开编辑弹窗', async () => {
    config.activeSubscriptionId = 'b' // 当前激活为空订阅，首行 A 处于非激活态
    const panel = await mountPanel()

    panel.onExecute(panel.items.value[0]!)
    await flushPromises()
    expect(callsOf(CMD.proxySetActiveSubscription)).toHaveLength(1)
    expect(callsOf(CMD.proxySetActiveSubscription)[0]![1]).toMatchObject({ id: 'a' })

    panel.onExecute(panel.items.value[1]!)
    await flushPromises()
    expect(panel.showEditModal.value).toBe(true)
    expect(panel.isCreating.value).toBe(false)
    expect(panel.editForm.value).toEqual({ name: '', url: '' })
  })

  it('新建并保存：拉取成功后落库并自动激活', async () => {
    const panel = await mountPanel()
    panel.openCreateModal()
    panel.editForm.value = { name: '新机场', url: 'https://new.example/sub' }

    await panel.saveSub()
    await flushPromises()

    expect(panel.showEditModal.value).toBe(false)
    const created = config.subscriptions.find((s) => s.name === '新机场')!
    expect(created.url).toBe('https://new.example/sub')
    expect(created.proxyCount).toBe(12)
    expect(created.updatedAt).not.toBe('')
    expect(callsOf(CMD.proxyUpdateSubscription)[0]![1]).toMatchObject({
      id: created.id,
      url: 'https://new.example/sub',
    })
    expect(callsOf(CMD.proxySetActiveSubscription)[0]![1]).toMatchObject({ id: created.id })
    expect(config.activeSubscriptionId).toBe(created.id)
  })

  it('行内更新：回填节点数/到期并清除失败标记', async () => {
    config.subscriptions[0]!.lastError = 'timeout'
    const panel = await mountPanel()

    await panel.refreshSub(panel.items.value[0]!)
    await flushPromises()

    expect(callsOf(CMD.proxyUpdateSubscription)[0]![1]).toMatchObject({
      id: 'a',
      url: 'https://a.example/sub',
    })
    const updated = config.subscriptions[0]!
    expect(updated.proxyCount).toBe(12)
    expect(updated.lastError).toBe('')
    expect(panel.updatingSubId.value).toBe('')
  })

  it('删除激活订阅：回退剩余首项并通知 Rust 新激活 id', async () => {
    const panel = await mountPanel()
    panel.deletingSub.value = config.subscriptions[0]!

    await panel.doRemoveSub()
    await flushPromises()

    expect(config.subscriptions.map((s) => s.id)).toEqual(['b'])
    expect(config.activeSubscriptionId).toBe('b')
    expect(callsOf(CMD.proxyRemoveSubscription)[0]![1]).toMatchObject({
      id: 'a',
      newActiveSubId: 'b',
    })
  })
})
