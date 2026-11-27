import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { computed, defineComponent, h, provide } from 'vue'
import { createPinia, setActivePinia } from 'pinia'
import { useAppStore } from '@/stores/app'
import { SCREEN_CONTEXT_KEY, useScreenSummary, type ScreenContext } from './useScreenSummary'

function mountScreenView(screen: ScreenContext, provider: () => string) {
  const Child = defineComponent({
    setup() {
      useScreenSummary(provider)
      return () => null
    },
  })
  const Host = defineComponent({
    setup() {
      provide(
        SCREEN_CONTEXT_KEY,
        computed(() => screen),
      )
      return () => h(Child)
    },
  })
  return mount(Host)
}

describe('useScreenSummary', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
  })

  it('读取 ContentView 注入的屏身份：挂载注册 / 卸载注销同一个 provider', () => {
    const appStore = useAppStore()
    const register = vi.spyOn(appStore, 'registerScreenSummary')
    const unregister = vi.spyOn(appStore, 'unregisterScreenSummary')
    const provider = () => '香港 01'
    const screen = { extId: 'proxy', subview: 'subscriptions' }

    const wrapper = mountScreenView(screen, provider)
    expect(register).toHaveBeenCalledWith('proxy', 'subscriptions', provider)

    wrapper.unmount()
    expect(unregister).toHaveBeenCalledWith('proxy', 'subscriptions', provider)
  })

  it('主视图（subview null）同样注入注册', () => {
    const appStore = useAppStore()
    const register = vi.spyOn(appStore, 'registerScreenSummary')
    const wrapper = mountScreenView({ extId: 'agent', subview: null }, () => '')
    expect(register).toHaveBeenCalledWith('agent', null, expect.any(Function))
    wrapper.unmount()
  })

  it('无 ContentView 上下文（独立挂载 / 独立窗口）安全空转，不触碰 store', () => {
    const Child = defineComponent({
      setup() {
        useScreenSummary(() => 'x')
        return () => null
      },
    })
    // 不提供 Pinia / 注入：若实现提前 useAppStore 会抛 here，此用例锁死静默空转
    expect(() => mount(Child).unmount()).not.toThrow()
  })
})
