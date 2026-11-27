import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { KeepAlive, defineComponent, h, nextTick } from 'vue'
import { createPinia, setActivePinia } from 'pinia'

// View.vue UI 契约：实时流量面板在顶部（容器同 ai-gateway 拓扑图），开启项副标题不再
// 承载流量；开启开关为主操作、核心更新为次级 ghost 按钮。mock Tauri 最小面按命令分派。
const mocks = vi.hoisted(() => {
  const listeners = new Map<string, (e: { payload: unknown }) => void>()
  return {
    listeners,
    channels: [] as { onmessage: ((payload: unknown) => void) | null }[],
    invoke: vi.fn(),
    listen: vi.fn(async (event: string, handler: (e: { payload: unknown }) => void) => {
      listeners.set(event, handler)
      return () => listeners.delete(event)
    }),
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
vi.mock('@tauri-apps/api/event', () => ({ listen: mocks.listen }))
// isTauri=false 跳过模块级 preload/自动更新，核心状态完全由用例驱动
vi.mock('@/utils/tauri', () => ({ isTauri: false }))
vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn().mockRejectedValue(new Error('no store')),
}))

import { invoke } from '@tauri-apps/api/core'
import { useAppStore } from '@/stores/app'
import '@/locales'
import './locales'
import { config } from './config'
import View from './View.vue'

let coreDownloaded = true
let updateAvailable = false
let proxyEnabled = false

beforeEach(() => {
  setActivePinia(createPinia())
  mocks.listeners.clear()
  mocks.channels.length = 0
  mocks.invoke.mockReset()
  mocks.invoke.mockImplementation((async (cmd: string) => {
    if (cmd === 'is_proxy_enabled') return proxyEnabled
    if (cmd === 'proxy_core_status')
      return { downloaded: coreDownloaded, version: 'v1.19.7', downloading: false }
    if (cmd === 'proxy_get_proxies') return { proxies: {} }
    if (cmd === 'proxy_check_update')
      return { hasUpdate: updateAvailable, current: 'v1.19.7', latest: 'v1.19.9' }
    return undefined
  }) as unknown as typeof invoke)
  coreDownloaded = true
  updateAvailable = false
  proxyEnabled = false
  config.subscriptions = [emptySub('default')]
  config.activeSubscriptionId = 'default'
})

function emptySub(id: string, name = '', url = '', proxyCount = 0) {
  return { id, name, url, updatedAt: '', expiresAt: '', proxyCount }
}

const wrappers: ReturnType<typeof mount>[] = []

function mountView() {
  const wrapper = mount(defineComponent({ setup: () => () => h(KeepAlive, () => h(View)) }))
  wrappers.push(wrapper)
  return wrapper
}

afterEach(() => {
  while (wrappers.length) wrappers.pop()!.unmount()
})

async function flush(times = 10) {
  for (let i = 0; i < times; i++) await nextTick()
  await flushPromises()
}

/// 首屏 onActivated 的 checkUpdate 早于 loadCoreStatus 落定（preload 被跳过），
/// 经就绪事件补一次「状态 + 更新」双查，对齐真实下载完成路径。
async function fireCoreReady() {
  mocks.listeners.get('proxy-core-ready')?.({ payload: null })
  await flush()
}

describe('proxy View 实时流量面板与按钮主次', () => {
  it('代理关闭：无流量面板；开启为主、更新为次级 ghost', async () => {
    updateAvailable = true
    const wrapper = mountView()
    await flush()
    await fireCoreReady()

    expect(wrapper.find('section.soft-card').exists()).toBe(false)

    const row = wrapper.findAll('[role="option"]')[0]!
    expect(row.text()).toContain('核心版本：mihomo v1.19.7')
    expect(row.text()).toContain('有新核心 v1.19.9')

    const buttons = row.findAll('button')
    const toggle = buttons.find((b) => b.text() === '已关闭')!
    const update = buttons.find((b) => b.attributes('title') === '下载更新')!
    expect(toggle.classes()).toContain('soft-chip')
    expect(update.classes()).toContain('soft-chip')
    expect(update.classes()).not.toContain('ui-btn-ghost')
    expect(update.find('i').classes()).toContain('i-ri-download-line')
  })

  it('代理开启：顶部面板显示实时上下行速率，开启项副标题不再含流量', async () => {
    proxyEnabled = true
    const wrapper = mountView()
    await flush()

    const panel = wrapper.find('section.soft-card')
    expect(panel.exists()).toBe(true)
    expect(panel.text()).toContain('下载')
    expect(panel.text()).toContain('上传')
    expect(panel.text()).toContain('0B/s')

    // 流量 Channel 推帧：面板两列各自更新；首帧单点保持 0 横线占位，第二帧起绘制曲线
    mocks.channels[0]!.onmessage?.({ up: 1234, down: 5678 })
    await flush()
    expect(panel.text()).toContain('5.5K/s')
    expect(panel.text()).toContain('1.2K/s')
    const sparks = panel.findAll('.traffic-sparkline')
    expect(sparks).toHaveLength(2)
    // 冷启动/单点：0 横线占位（只有折线，无面积）
    expect(sparks[0]!.findAll('path')).toHaveLength(1)
    expect(sparks[0]!.find('path').attributes('d')).toBe('M0,22 L100,22')

    mocks.channels[0]!.onmessage?.({ up: 2345, down: 6789 })
    await flush()
    expect(sparks[0]!.findAll('path')).toHaveLength(2)
    expect(sparks[1]!.findAll('path')).toHaveLength(2)

    const row = wrapper.findAll('[role="option"]')[0]!
    expect(row.text()).not.toContain('K/s')
  })

  it('代理开启且有新版：更新按钮不出现（副标题提示先关闭），开关保持主操作', async () => {
    proxyEnabled = true
    updateAvailable = true
    const wrapper = mountView()
    await flush()
    await fireCoreReady()

    const row = wrapper.findAll('[role="option"]')[0]!
    expect(row.text()).toContain('请关闭代理后更新')
    const buttons = row.findAll('button')
    expect(buttons.map((b) => b.text())).toContain('已开启')
    expect(buttons.some((b) => b.attributes('title') === '下载更新')).toBe(false)
    expect(buttons.find((b) => b.text() === '已开启')!.classes()).toContain('ui-btn-primary')
  })

  it('订阅列表收束为单个入口：只显示激活订阅摘要，回车进入二级界面', async () => {
    config.subscriptions = [
      emptySub('a', '机场 A', 'https://a.example/sub', 42),
      emptySub('b', '机场 B', 'https://b.example/sub', 88),
    ]
    config.activeSubscriptionId = 'a'
    const wrapper = mountView()
    await flush()

    const rows = wrapper.findAll('[role="option"]')
    const nav = rows.find((r) => r.text().includes('机场 A'))!
    expect(nav.text()).toContain('42 节点')
    expect(nav.text()).not.toContain('机场 B') // 非激活订阅不再平铺

    const appStore = useAppStore()
    const openSubview = vi.spyOn(appStore, 'openSubview')
    const search = vi.spyOn(appStore, 'setSearchQuery')

    // 行尾「切换」按钮：显式入口
    const switchBtn = nav.find('button')
    expect(switchBtn.text()).toBe('切换')
    await switchBtn.trigger('click')
    expect(openSubview).toHaveBeenCalledWith('subscriptions')
    expect(search).toHaveBeenCalledWith('')

    // 整行回车/双击同入口
    openSubview.mockClear()
    search.mockClear()
    await nav.trigger('dblclick')
    expect(openSubview).toHaveBeenCalledWith('subscriptions')
    expect(search).toHaveBeenCalledWith('')
  })
})
