import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { defineComponent, h } from 'vue'
import { createPinia, setActivePinia } from 'pinia'

// Mock Tauri APIs：invoke 按命令分派最小返回；Channel/事件流仅需可构造
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: unknown = null
  },
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}))
// plugin-store 不可达：defineConfig 纯内存（backfill/持久化静默失败），config 单例可控
vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn().mockRejectedValue(new Error('no store')),
}))
// isTauri=false 跳过模块级 preload（preloaded 状态不受跨用例 invoke 影响）
vi.mock('@/utils/tauri', () => ({
  isTauri: false,
}))

import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { useAppStore } from '@/stores/app'
import { config } from './config'
import { useProxyPanel, isSubscriptionStale } from './useProxyPanel'

/// 累积 wrapper：afterEach 统一 unmount，清理 onMounted/onUnmounted 注册的 listener
const mountedWrappers: ReturnType<typeof mount>[] = []

/// mount 后等 onMounted 异步链（listen 注册 + is_proxy_enabled 权威复核）走完再返回，
/// 消除复核回写与 toggle 赋值的微任务交错（真实 app 中预加载启动早期已完成，无此交错）
async function mountPanel() {
  let panel!: ReturnType<typeof useProxyPanel>
  const TestComp = defineComponent({
    setup() {
      panel = useProxyPanel()
      return () => h('div')
    },
  })
  mountedWrappers.push(mount(TestComp))
  await flushPromises()
  return panel
}

function setProxyInvocations() {
  return vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === CMD.setProxyEnabled)
}

describe('useProxyPanel toggleEnabled 首启确认（只弹一次）', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    config.tunConfirmed = false
    config.secret = 'test-secret'
    vi.mocked(invoke).mockImplementation((async (cmd: string) => {
      if (cmd === CMD.proxyGetProxies) return { proxies: {} }
      if (cmd === CMD.isProxyEnabled) return false
      return undefined
    }) as unknown as typeof invoke)
  })
  afterEach(() => {
    while (mountedWrappers.length) mountedWrappers.pop()!.unmount()
  })

  it('未确认时取消：不开启不置位，下次开启仍会弹', async () => {
    const appStore = useAppStore()
    const confirmSpy = vi.spyOn(appStore, 'showConfirm').mockResolvedValue(false)
    const panel = await mountPanel()

    await panel.toggleEnabled()
    await flushPromises()

    expect(confirmSpy).toHaveBeenCalledTimes(1)
    expect(setProxyInvocations()).toHaveLength(0)
    expect(config.tunConfirmed).toBe(false)
  })

  it('未确认时同意：弹一次确认，置位并调 setProxyEnabled', async () => {
    const appStore = useAppStore()
    const confirmSpy = vi.spyOn(appStore, 'showConfirm').mockResolvedValue(true)
    const panel = await mountPanel()

    await panel.toggleEnabled()
    await flushPromises()

    expect(confirmSpy).toHaveBeenCalledTimes(1)
    expect(config.tunConfirmed).toBe(true)
    expect(setProxyInvocations()).toHaveLength(1)
    expect(setProxyInvocations()[0]![1]).toMatchObject({ enabled: true })
  })

  it('已确认后开关：不再弹确认，直接走 setProxyEnabled', async () => {
    const appStore = useAppStore()
    const confirmSpy = vi.spyOn(appStore, 'showConfirm').mockResolvedValue(true)
    config.tunConfirmed = true
    const panel = await mountPanel()

    // 两次 toggle 必然一开一关（初始方向不依赖前序用例的 preloaded 残留）：
    // 已确认后全程零首启弹窗
    await panel.toggleEnabled()
    await flushPromises()
    await panel.toggleEnabled()
    await flushPromises()

    expect(confirmSpy).not.toHaveBeenCalled()
    expect(config.tunConfirmed).toBe(true)
    const dirs = setProxyInvocations().map(([, a]) => (a as { enabled: boolean }).enabled)
    expect(dirs).toHaveLength(2)
    expect(dirs).toContain(true)
    expect(dirs).toContain(false)
  })
})

describe('isSubscriptionStale 自动更新过期判定', () => {
  const HOUR = 3600_000
  const ago = (h: number) => new Date(Date.now() - h * HOUR).toISOString()

  beforeEach(() => {
    setActivePinia(createPinia())
    config.autoUpdateEnabled = true
    config.autoUpdateIntervalHours = 24
  })
  afterEach(() => {
    config.autoUpdateIntervalHours = 24
  })

  it('无 url / 间隔内拉取过 → 不过期；从未拉取（updatedAt 空）→ 过期', () => {
    expect(
      isSubscriptionStale({
        id: 'a',
        name: '',
        url: '',
        updatedAt: ago(48),
        expiresAt: '',
        proxyCount: 0,
      }),
    ).toBe(false)
    expect(
      isSubscriptionStale({
        id: 'a',
        name: '',
        url: 'https://s.example/sub',
        updatedAt: ago(23),
        expiresAt: '',
        proxyCount: 1,
      }),
    ).toBe(false)
    expect(
      isSubscriptionStale({
        id: 'a',
        name: '',
        url: 'https://s.example/sub',
        updatedAt: '',
        expiresAt: '',
        proxyCount: 0,
      }),
    ).toBe(true)
  })

  it('超过间隔 → 过期；间隔设置即时生效', () => {
    const sub = {
      id: 'a',
      name: '',
      url: 'https://s.example/sub',
      updatedAt: ago(3),
      expiresAt: '',
      proxyCount: 1,
    }
    expect(isSubscriptionStale(sub)).toBe(false) // 3h < 24h
    config.autoUpdateIntervalHours = 1
    expect(isSubscriptionStale(sub)).toBe(true) // 3h > 1h
  })

  it('间隔非法值防御：0/负数钳到 1h，NaN 落 24h 档', () => {
    const sub = {
      id: 'a',
      name: '',
      url: 'https://s.example/sub',
      updatedAt: ago(2),
      expiresAt: '',
      proxyCount: 1,
    }
    config.autoUpdateIntervalHours = 0
    expect(isSubscriptionStale(sub)).toBe(true) // 2h > 钳后 1h
    const fresh = { ...sub, updatedAt: ago(0.5) }
    expect(isSubscriptionStale(fresh)).toBe(false) // 0.5h < 1h
    config.autoUpdateIntervalHours = Number.NaN
    expect(isSubscriptionStale({ ...sub, updatedAt: ago(25) })).toBe(true) // NaN → 24h 档
    expect(isSubscriptionStale({ ...sub, updatedAt: ago(23) })).toBe(false)
  })
})
