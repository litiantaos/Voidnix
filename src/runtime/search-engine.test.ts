import { describe, it, expect, vi, beforeEach } from 'vitest'
import type {
  Extension,
  SearchResult,
  SearchContext,
  ProviderResult,
  SearchResultKind,
} from './types'

// 用可控数组替代真实注册中心，按测试用例注入 fake 扩展
const registry: Extension[] = []
vi.mock('./extension-registry', () => ({
  getAllExtensions: () => registry,
}))

const { searchEngine } = await import('./search-engine')

/** 构造 fake 扩展（dynamic 接收 ctx 便于 abort 测试）。 */
function makeSearchExt(
  id: string,
  dynamic: (q: string, ctx: SearchContext) => ProviderResult[] | Promise<ProviderResult[]>,
  keywords?: string[],
): Extension {
  return {
    meta: { id, name: id, icon: 'i-ri-test-line', order: 1, keywords },
    search: { dynamic },
  }
}

function result(id: string, title: string, kind: SearchResultKind, boost = 0): ProviderResult {
  return { id, title, boost, data: { kind } }
}

function groupOf(r: SearchResult): string {
  const k = r.data?.kind
  return k === 'file' || k === 'folder' ? 'file' : (k ?? 'other')
}

describe('SearchEngine', () => {
  beforeEach(() => {
    registry.length = 0
    searchEngine.setActiveModule(undefined)
  })

  it('框架注入 module = 产出扩展 meta.id（v1.6 N4，扩展禁填）', async () => {
    registry.push(makeSearchExt('inj', () => [result('a', 'alpha', 'module')]))
    const out = await searchEngine.search('alpha')
    expect(out[0].module).toBe('inj')
  })

  it('全局模式 kind=module 结果注入 source（扩展显示名）；应用/文件等不注入', async () => {
    registry.push(
      makeSearchExt('src', () => [
        result('tool', 'aa tool', 'module'), // 工具型 → 注入 source
        result('app', 'aa app', 'application'), // 应用 → 不注入
        result('file', 'aa file', 'file'), // 文件 → 不注入
      ]),
    )
    // makeSearchExt 的 name 默认 = id；此处验证 source 值
    const out = await searchEngine.search('aa')
    const tool = out.find((r) => r.id === 'tool')
    const app = out.find((r) => r.id === 'app')
    expect(tool?.source).toBe('src')
    expect(app?.source).toBeUndefined()
  })

  it('模块模式不注入 source（结果都来自当前模块，无需标注）', async () => {
    searchEngine.setActiveModule('modsrc')
    registry.push(makeSearchExt('modsrc', () => [result('a', 'alpha', 'module')]))
    const out = await searchEngine.search('alpha')
    expect(out[0].source).toBeUndefined()
  })

  it('去重：同 <module>:<id> 组合键保留首个', async () => {
    searchEngine.setActiveModule('dedup')
    registry.push(
      makeSearchExt('dedup', () => [
        result('dup', 'alpha', 'module'),
        result('dup', 'beta', 'module'), // 同 id，应被去重
        result('uniq', 'gamma', 'module'),
      ]),
    )
    const out = await searchEngine.search('a')
    expect(out.map((r) => r.id)).toEqual(['dup', 'uniq'])
  })

  it('全局模式组间序 = GROUP_ORDER（application→file→module→clipboard→web）', async () => {
    registry.push(
      makeSearchExt('order', () => [
        result('web', 'aa web', 'web'),
        result('clip', 'aa clip', 'clipboard'),
        result('mod', 'aa mod', 'module'),
        result('folder', 'aa folder', 'folder'),
        result('file', 'aa file', 'file'),
        result('app', 'aa app', 'application'),
      ]),
    )
    const out = await searchEngine.search('aa')
    expect(out.map(groupOf)).toEqual(['application', 'file', 'file', 'module', 'clipboard', 'web'])
  })

  it('file 与 folder 同属 file 组（v1.5 合并）', async () => {
    registry.push(
      makeSearchExt('ff', () => [
        result('f1', 'aa file', 'file'),
        result('d1', 'aa folder', 'folder'),
      ]),
    )
    const out = await searchEngine.search('aa')
    expect(out.every((r) => groupOf(r) === 'file')).toBe(true)
  })

  it('boost 叠加决定组内序（finalScore = fuzzy + boost）', async () => {
    registry.push(
      makeSearchExt('boost', () => [
        result('low', 'aa', 'application', 0),
        result('high', 'aa', 'application', 1000),
      ]),
    )
    const out = await searchEngine.search('aa')
    expect(out.map((r) => r.id)).toEqual(['high', 'low'])
  })

  it('全局模式过滤零分项（title 不含 query）', async () => {
    registry.push(
      makeSearchExt('filter', () => [
        result('hit', 'match', 'module'),
        result('miss', 'zzzzz', 'module'),
      ]),
    )
    const out = await searchEngine.search('match')
    expect(out.map((r) => r.id)).toEqual(['hit'])
  })

  it('模块模式 bypass groupAndSort：保留扩展返回序 + 不过滤零分', async () => {
    searchEngine.setActiveModule('modmode')
    registry.push(
      makeSearchExt('modmode', () => [
        result('web', 'zzz', 'web'),
        result('app', 'zzz', 'application'),
        result('clip', 'zzz', 'clipboard'),
      ]),
    )
    const out = await searchEngine.search('zzz')
    // 保留扩展返回序，未按 GROUP_ORDER 重排；query 'zzz' 全命中故不过滤
    expect(out.map((r) => r.id)).toEqual(['web', 'app', 'clip'])
  })

  it('keyword 合流：全局模式匹配 meta.keywords 产出模块入口（v1.6 N2）', async () => {
    registry.push(makeSearchExt('kw', () => [], ['encode', '解码']))
    const out = await searchEngine.search('encode')
    const entry = out.find((r) => r.data?.kind === 'module')
    expect(entry).toBeDefined()
    expect(entry?.data?.moduleId).toBe('kw')
    expect(entry?.module).toBe('kw')
    expect(entry?.boost).toBe(500) // KEYWORD_MODULE_BOOST
  })

  it('keyword 合流：模块模式禁用（已在某模块内）', async () => {
    searchEngine.setActiveModule('kwmod')
    registry.push(makeSearchExt('kwmod', () => [], ['encode']))
    const out = await searchEngine.search('encode')
    expect(out.find((r) => r.data?.kind === 'module')).toBeUndefined()
  })

  it('keyword 反向匹配：多词 query 含 keyword 时产出模块入口', async () => {
    // scoreFields 单向子串对此返回 0（query「100 usd」比 keyword「usd」长）
    registry.push(makeSearchExt('kwrev', () => [], ['usd', '汇率']))
    const out = await searchEngine.search('100 usd')
    const entry = out.find((r) => r.data?.kind === 'module' && r.data.moduleId === 'kwrev')
    expect(entry).toBeDefined()
  })

  it('keyword 入口抑制：dynamic 已产出结果的扩展不再显示模块入口（即时答案优先）', async () => {
    // 「100 usd」同时命中 dynamic（返回换算值）与 keyword（usd 反向命中）；
    // 预期仅保留 dynamic 即时答案，抑制该扩展的 module-kwsup 入口
    registry.push(
      makeSearchExt('kwsup', () => [result('ans', '717.00', 'module', 1000)], ['usd', '汇率']),
    )
    const out = await searchEngine.search('100 usd')
    expect(out.find((r) => r.id === 'ans')).toBeDefined()
    expect(out.find((r) => r.id === 'module-kwsup')).toBeUndefined()
  })

  it('keyword 入口保留：dynamic 结果与 query 无关（finalScore=0）时不抑制', async () => {
    // calculator history 标题「= 42」与 query「计算器」无关（fuzzy=0, boost=0 → 会被 groupAndSort 过滤）；
    // 扩展有 keyword「计算器」→ keyword 入口应保留，不被不相关 dynamic 结果误杀
    registry.push(makeSearchExt('calc', () => [result('h0', '= 42', 'module')], ['计算器', 'calc']))
    const out = await searchEngine.search('计算器')
    expect(out.find((r) => r.data?.kind === 'module' && r.data.moduleId === 'calc')).toBeDefined()
  })

  it('keyword 入口保留：dynamic 数据型结果（kind≠module）不抑制入口', async () => {
    // clipboard 记录 kind=clipboard（数据型，非即时答案），即便标题命中 query 也不抑制模块入口：
    // 用户搜「剪贴板」应先看到模块入口，其次才是剪贴板记录
    registry.push(
      makeSearchExt('clip', () => [result('c1', '剪贴板内容', 'clipboard')], [
        'clipboard',
        '剪贴板',
      ]),
    )
    const out = await searchEngine.search('剪贴板')
    const entry = out.find((r) => r.data?.kind === 'module' && r.data.moduleId === 'clip')
    expect(entry).toBeDefined()
    // module 组排在 clipboard 组之前
    const entryIdx = out.indexOf(entry!)
    const recordIdx = out.findIndex((r) => r.id === 'c1')
    expect(entryIdx).toBeLessThanOrEqual(recordIdx)
  })

  it('新查询 abort 旧查询的 signal', async () => {
    const seen: AbortSignal[] = []
    const deferreds: Array<{ resolve: (v: ProviderResult[]) => void }> = []
    registry.push(
      makeSearchExt('abort-target', (_q, ctx) => {
        seen.push(ctx.signal)
        return new Promise<ProviderResult[]>((resolve) => {
          deferreds.push({ resolve })
          ctx.signal.addEventListener('abort', () => resolve([]))
        })
      }),
    )
    const p1 = searchEngine.search('a') // 同步执行到 await searchDynamic，dynamic 已被调用
    expect(seen.length).toBe(1)
    const p2 = searchEngine.search('b') // 触发 seen[0] abort
    expect(seen[0].aborted).toBe(true)
    // 收尾：让两次 search 都 resolve
    deferreds[1]?.resolve([])
    await p1
    await p2
  })
})
