import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { KeepAlive, defineComponent, h, ref } from 'vue'
import { createPinia, setActivePinia } from 'pinia'

// 用量是实时数据：每次进入扩展（KeepAlive 首挂载/重进均触发 activated）拉最新，
// 窗口唤起获焦（window-focused，回调自带 activeExtId 激活判断）补刷，失活期跳过；
// 配置指纹变化（改 Key / 端点）同样重拉。回归起点：断网进入失败后联网重进，
// 错误驻留缓存永不刷新。
const mocks = vi.hoisted(() => {
  const memStore = new Map<string, unknown>()
  return {
    storeGet: vi.fn((k: string) => Promise.resolve(memStore.get(k))),
    storeSet: vi.fn((k: string, v: unknown) => {
      memStore.set(k, v)
      return Promise.resolve()
    }),
    storeSave: vi.fn(() => Promise.resolve()),
    storeOnChange: vi.fn(() => Promise.resolve(() => {})),
    invoke: vi.fn(),
  }
})

vi.mock('@tauri-apps/plugin-store', () => ({
  load: () =>
    Promise.resolve({
      get: mocks.storeGet,
      set: mocks.storeSet,
      save: mocks.storeSave,
      onChange: mocks.storeOnChange,
    }),
}))
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }))

import '@/locales'
import './locales'
import { useAppStore } from '@/stores/app'
import { config, type AiProvider } from '@/runtime/ai-providers'
import View from './View.vue'

/** 可切换的配额拉取结果：reject 模拟断网，resolve 模拟联网成功 */
let quota: () => Promise<unknown> = () => Promise.resolve(quotaPayload())

function quotaPayload() {
  const reset = Date.now() + 2 * 3_600_000
  return {
    level: 'MAX',
    expired: false,
    fiveHour: { percentage: 61, nextResetTime: reset },
    weekly: { percentage: 24, nextResetTime: reset + 5 * 86_400_000 },
    totalCalls: 10,
    totalTokens: 1_200_000,
    tokensSeries: [1, 2, 3],
    error: null,
  }
}

const zhipuProvider: AiProvider = {
  id: 'zp',
  name: '',
  endpoint: 'https://open.bigmodel.cn/api/coding/paas/v4',
  responsesEndpoint: '',
  models: ['glm-5.3'],
  keys: [{ id: 'k1', label: '默认', apiKey: 'sk-zhipu-key' }],
  usageKind: '',
  envKey: '',
}

/** KeepAlive 宿主 + activeExtId 同步（真实 app 中 MainView 激活扩展时两者一体变化） */
const mountedHosts: { wrapper: ReturnType<typeof mount> }[] = []

function mountHost() {
  const appStore = useAppStore()
  const show = ref(true)
  appStore.activeExtId = 'ai-providers'
  const wrapper = mount(
    defineComponent({
      setup: () => () => h(KeepAlive, () => (show.value ? h(View) : null)),
    }),
  )
  const leave = () => {
    show.value = false
    appStore.activeExtId = null
  }
  const reenter = () => {
    appStore.activeExtId = 'ai-providers'
    show.value = true
  }
  const host = { wrapper, leave, reenter }
  mountedHosts.push(host)
  return host
}

beforeEach(() => {
  setActivePinia(createPinia())
  mocks.invoke.mockReset()
  mocks.invoke.mockImplementation((cmd: string) => {
    if (cmd === 'ai_providers_zhipu_quota') return quota()
    return Promise.resolve(null)
  })
  config.providers.splice(0, config.providers.length, structuredClone(zhipuProvider))
  quota = () => Promise.resolve(quotaPayload())
})

afterEach(() => {
  // 断言失败也卸载，防泄漏组件的指纹 watch 看到共享 config 变更虚增后续用例的 invoke 计数
  while (mountedHosts.length) mountedHosts.pop()!.wrapper.unmount()
})

describe('ai-providers View 每次进入拉最新', () => {
  it('断网拉取失败后联网重进（KeepAlive 激活，不重挂载）重拉自愈', async () => {
    quota = () => Promise.reject('网络不可达')
    const { wrapper, leave, reenter } = mountHost()
    await flushPromises()
    expect(wrapper.text()).toContain('网络不可达')
    expect(mocks.invoke).toHaveBeenCalledTimes(1)

    quota = () => Promise.resolve(quotaPayload())
    leave()
    await flushPromises()
    expect(mocks.invoke).toHaveBeenCalledTimes(1)
    reenter()
    await flushPromises()

    expect(mocks.invoke).toHaveBeenCalledTimes(2)
    expect(wrapper.text()).not.toContain('网络不可达')
    expect(wrapper.text()).toContain('MAX')
    expect(wrapper.text()).toContain('5h 61%')
  })

  it('窗口唤起获焦（视图保持激活）补刷拉最新；失活期获焦跳过', async () => {
    const { wrapper, leave } = mountHost()
    await flushPromises()
    expect(wrapper.text()).toContain('MAX')
    expect(mocks.invoke).toHaveBeenCalledTimes(1)

    window.dispatchEvent(new Event('window-focused'))
    await flushPromises()
    expect(mocks.invoke).toHaveBeenCalledTimes(2)

    // 失活期（其它界面获焦）：监听常驻但激活判断跳过
    leave()
    await flushPromises()
    window.dispatchEvent(new Event('window-focused'))
    await flushPromises()
    expect(mocks.invoke).toHaveBeenCalledTimes(2)
  })

  it('配置指纹变化（改 Key）重拉', async () => {
    const { wrapper } = mountHost()
    await flushPromises()
    expect(mocks.invoke).toHaveBeenCalledTimes(1)

    config.providers[0]!.keys[0]!.apiKey = 'sk-2'
    await flushPromises()
    expect(mocks.invoke).toHaveBeenCalledTimes(2)
    expect(wrapper.text()).toContain('MAX')
  })
})
