import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { nextTick } from 'vue'
import { createPinia, setActivePinia } from 'pinia'

// 订阅二级界面 UI 契约：顶部「操作」组（添加订阅行，无图标）+ 订阅列表、激活项 accent、
// 行内更新/编辑按钮、搜索过滤与空态。动作细节见 useSubscriptions.test.ts。
const mocks = vi.hoisted(() => ({ invoke: vi.fn() }))

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }))
vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn().mockRejectedValue(new Error('no store')),
}))
vi.mock('@/utils/tauri', () => ({ isTauri: false }))

import '@/locales'
import '../locales'
import { config } from '../config'
import { useAppStore } from '@/stores/app'
import SubscriptionsView from './SubscriptionsView.vue'

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

beforeEach(() => {
  setActivePinia(createPinia())
  mocks.invoke.mockReset()
  mocks.invoke.mockResolvedValue(undefined)
  config.subscriptions = [
    sub({
      id: 'a',
      name: '机场 A',
      url: 'https://a.example/sub',
      proxyCount: 42,
    }),
    sub({
      id: 'b',
      name: '机场 B',
      url: 'https://b.example/sub',
      proxyCount: 88,
    }),
  ]
  config.subscriptions[0]!.expiresAt = '2030-01-01T12:00:00.000Z'
  config.activeSubscriptionId = 'a'
})

const wrappers: ReturnType<typeof mount>[] = []
afterEach(() => {
  while (wrappers.length) wrappers.pop()!.unmount()
})

async function mountView() {
  const wrapper = mount(SubscriptionsView)
  wrappers.push(wrapper)
  await flushPromises()
  return wrapper
}

describe('proxy SubscriptionsView', () => {
  it('顶部「操作」组 + 订阅列表：添加订阅无图标且独占一组', async () => {
    const wrapper = await mountView()

    const headers = wrapper.findAll('.group-header')
    expect(headers).toHaveLength(2)
    expect(headers[0]!.text()).toContain('操作')
    expect(headers[1]!.text()).toContain('订阅')
    expect(headers[1]!.find('button').exists()).toBe(false)

    const rows = wrapper.findAll('[role="option"]')
    expect(rows).toHaveLength(3)
    // 添加订阅：置顶独立动作行，无图标
    expect(rows[0]!.text()).toContain('添加订阅')
    expect(rows[0]!.find('.i-ri-add-line').exists()).toBe(false)
    expect(rows[0]!.find('.fill-mist').exists()).toBe(false)
    // 其下为订阅列表
    expect(rows[1]!.text()).toContain('机场 A')
    expect(rows[1]!.text()).toContain('42 节点')
    expect(rows[1]!.text()).toContain('2030-01-01 到期')
    expect(rows[1]!.find('.text-accent').exists()).toBe(true)
    // 非激活订阅不强调
    expect(rows[2]!.find('.text-accent').exists()).toBe(false)
    // 行内更新 / 编辑次级入口
    expect(rows[2]!.find('button[title="更新订阅"]').exists()).toBe(true)
    expect(rows[2]!.find('button[title="编辑订阅"]').exists()).toBe(true)
  })

  it('搜索过滤命中名称/链接/添加行；无结果空态', async () => {
    const wrapper = await mountView()
    const appStore = useAppStore()

    appStore.setSearchQuery('b.example')
    await nextTick()
    let rows = wrapper.findAll('[role="option"]')
    expect(rows).toHaveLength(1)
    expect(rows[0]!.text()).toContain('机场 B')
    expect(wrapper.findAll('.group-header').map((h) => h.text())).toEqual(['订阅'])

    appStore.setSearchQuery('添加')
    await nextTick()
    rows = wrapper.findAll('[role="option"]')
    expect(rows).toHaveLength(1)
    expect(rows[0]!.text()).toContain('添加订阅')
    expect(wrapper.findAll('.group-header').map((h) => h.text())).toEqual(['操作'])

    appStore.setSearchQuery('nothing-here')
    await nextTick()
    expect(wrapper.findAll('[role="option"]')).toHaveLength(0)
    expect(wrapper.text()).toContain('无结果')
  })

  it('添加订阅行双击打开新建弹窗（不预创建订阅项）', async () => {
    const wrapper = await mountView()
    const before = config.subscriptions.length

    const addRow = wrapper.findAll('[role="option"]').find((r) => r.text().includes('添加订阅'))!
    await addRow.trigger('dblclick')
    await nextTick()

    expect(document.querySelector('[role="dialog"]')?.textContent).toContain('添加订阅')
    expect(config.subscriptions).toHaveLength(before)
  })
})
