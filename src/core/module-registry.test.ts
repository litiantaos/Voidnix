import { describe, it, expect } from 'vitest'
import { registerModule, getModule, getAllModules, initAllModules } from './module-registry'
import type { AppModule } from '@/types/module'

function makeModule(overrides: Partial<AppModule> & { id: string }): AppModule {
  return {
    name: overrides.id,
    description: '',
    icon: 'i-ri-test',
    keywords: [],
    ...overrides,
  }
}

describe('module-registry', () => {
  it('registerModule + getModule', () => {
    registerModule(makeModule({ id: 'reg-test-1' }))
    expect(getModule('reg-test-1')).toBeDefined()
    expect(getModule('reg-test-1')?.id).toBe('reg-test-1')
  })

  it('getModule 不存在返回 undefined', () => {
    expect(getModule('nonexistent')).toBeUndefined()
  })

  it('getAllModules 返回全部注册模块', () => {
    const before = getAllModules().length
    registerModule(makeModule({ id: 'reg-test-2' }))
    registerModule(makeModule({ id: 'reg-test-3' }))
    expect(getAllModules().length).toBeGreaterThanOrEqual(before + 2)
  })

  it('重复注册覆盖旧模块', () => {
    registerModule(makeModule({ id: 'reg-test-dup', name: 'V1' }))
    registerModule(makeModule({ id: 'reg-test-dup', name: 'V2' }))
    expect(getModule('reg-test-dup')?.name).toBe('V2')
  })

  describe('initAllModules', () => {
    it('调用 onInit 并只初始化一次', async () => {
      let initCount = 0
      registerModule(
        makeModule({
          id: 'reg-test-init',
          onInit: async () => {
            initCount++
          },
        }),
      )
      await initAllModules()
      await initAllModules()
      expect(initCount).toBe(1)
    })

    it('onInit 失败不阻塞其他模块', async () => {
      let otherInit = false
      registerModule(
        makeModule({
          id: 'reg-test-fail',
          onInit: async () => {
            throw new Error('fail')
          },
        }),
      )
      registerModule(
        makeModule({
          id: 'reg-test-ok',
          onInit: async () => {
            otherInit = true
          },
        }),
      )
      await initAllModules()
      expect(otherInit).toBe(true)
    })
  })
})
