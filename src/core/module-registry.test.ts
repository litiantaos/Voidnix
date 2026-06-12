import { describe, it, expect } from 'vitest'
import { registerModule, getModule, getAllModules, initAllModules, searchAll } from './module-registry'
import type { AppModule, SearchResult } from '@/types/module'

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

  describe('searchAll 分组排序', () => {
    it('按类型分组，应用 > 扩展 > 剪贴板 > 文件', async () => {
      const appModule = makeModule({
        id: 'sort-test-apps',
        hidden: true,
        onSearch: async () => [
          { id: 'a1', title: 'Safari', module: 'sort-test-apps', score: 400, data: { kind: 'application' } },
        ],
      })
      const modModule = makeModule({
        id: 'sort-test-mods',
        hidden: true,
        onSearch: async () => [
          { id: 'm1', title: 'Calculator', module: 'sort-test-mods', score: 500, data: { kind: 'module' } },
        ],
      })
      const clipModule = makeModule({
        id: 'sort-test-clip',
        hidden: true,
        onSearch: async () => [
          { id: 'c1', title: 'copied text', module: 'sort-test-clip', score: 600, data: { kind: 'clipboard' } },
        ],
      })
      const fileModule = makeModule({
        id: 'sort-test-files',
        hidden: true,
        onSearch: async () => [
          { id: 'f1', title: 'readme', module: 'sort-test-files', score: 700, data: { kind: 'file' } },
        ],
      })
      registerModule(appModule)
      registerModule(modModule)
      registerModule(clipModule)
      registerModule(fileModule)

      const res = await searchAll('test')
      const kinds = res.map((r) => r.data?.kind as string)

      // 应用排在扩展前面，扩展排在剪贴板前面，剪贴板排在文件前面
      const appIdx = kinds.indexOf('application')
      const modIdx = kinds.indexOf('module')
      const clipIdx = kinds.indexOf('clipboard')
      const fileIdx = kinds.indexOf('file')

      expect(appIdx).toBeLessThan(modIdx)
      expect(modIdx).toBeLessThan(clipIdx)
      expect(clipIdx).toBeLessThan(fileIdx)
    })

    it('组内按 score 降序', async () => {
      const appModule = makeModule({
        id: 'score-test-apps',
        hidden: true,
        onSearch: async () => [
          { id: 'a1', title: 'Finder', module: 'score-test-apps', score: 200, data: { kind: 'application' } },
          { id: 'a2', title: 'Safari', module: 'score-test-apps', score: 400, data: { kind: 'application' } },
        ],
      })
      registerModule(appModule)

      const res = await searchAll('test')
      const appItems = res.filter((r) => r.data?.kind === 'application')

      for (let i = 1; i < appItems.length; i++) {
        expect((appItems[i - 1].score ?? 0)).toBeGreaterThanOrEqual((appItems[i].score ?? 0))
      }
    })

    it('file 和 folder 合并为同一组', async () => {
      const fileModule = makeModule({
        id: 'merge-test-files',
        hidden: true,
        onSearch: async () => [
          { id: 'f1', title: 'readme', module: 'merge-test-files', score: 300, data: { kind: 'file' } },
          { id: 'd1', title: 'project', module: 'merge-test-files', score: 200, data: { kind: 'folder' } },
        ],
      })
      registerModule(fileModule)

      const res = await searchAll('test')
      const fileItems = res.filter((r) => r.data?.kind === 'file' || r.data?.kind === 'folder')

      // file 和 folder 应该相邻，中间不被其他类型隔开
      if (fileItems.length >= 2) {
        const firstIdx = res.indexOf(fileItems[0])
        const lastIdx = res.indexOf(fileItems[fileItems.length - 1])
        const between = res.slice(firstIdx, lastIdx + 1)
        const hasOther = between.some(
          (r) => r.data?.kind !== 'file' && r.data?.kind !== 'folder',
        )
        expect(hasOther).toBe(false)
      }
    })
  })
})
