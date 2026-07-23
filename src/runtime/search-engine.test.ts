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

  it('框架注入 module = 产出扩展 meta.id（扩展禁填）', async () => {
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

  it('全局模式组间序 = GROUP_ORDER（application→module→file→clipboard→web）', async () => {
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
    expect(out.map(groupOf)).toEqual(['application', 'module', 'file', 'file', 'clipboard', 'web'])
  })

  it('module keyword 入口（boost=500）必在 file 组（低 boost）前——组间序不受 boost 影响', async () => {
    // module 入口 boost=500（KEYWORD_MODULE_BOOST），file 结果 boost=0；module 组仍排前
    registry.push(makeSearchExt('search', () => [result('f1', 'aa file', 'file', 0)], ['aa']))
    const out = await searchEngine.search('aa')
    expect(out.map(groupOf)).toEqual(['module', 'file'])
  })

  it('file 与 folder 同属 file 组', async () => {
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

  it('空 query 默认列表：应用靠 boost 显示（matched=true，非零分过滤）', async () => {
    // 应用 boost>=1（frequencyBoost+recencyScore+1），空 query 时 fuzzy=0 但 finalScore=boost>0 → 显示
    registry.push(
      makeSearchExt('apps-default', () => [
        result('a1', 'Safari', 'application', 50),
        result('a2', 'VSCode', 'application', 100),
      ]),
    )
    const out = await searchEngine.search('')
    expect(out.length).toBe(2)
    // 按 finalScore（=fuzzy(0)+boost）降序
    expect(out.map((r) => r.id)).toEqual(['a2', 'a1'])
  })

  it('空 query 默认列表：module 类 boost=0 的即时答案被过滤（time/uuid 场景）', async () => {
    // time/uuid 空 query 返回即时答案但 boost=0，finalScore=0 → 不进默认列表（避免污染启动屏）
    registry.push(
      makeSearchExt('instant-zero-boost', () => [
        result('t1', '2024-01-01 12:00:00', 'module', 0),
        result('u1', 'abc-123-nano', 'module', 0),
      ]),
    )
    const out = await searchEngine.search('')
    expect(out.length).toBe(0)
  })

  it('非空 query 不匹配：应用被过滤（fuzzy=0 即便 boost>0 也不显示，查找型结果必须命中）', async () => {
    // 搜 'xyz' 不匹配任何应用 title → fuzzy=0 → application 类非 matched → 过滤（避免搜任意词显示所有应用）
    registry.push(
      makeSearchExt('apps-nomatch', () => [
        result('a1', 'Safari', 'application', 500),
        result('a2', 'VSCode', 'application', 1000),
      ]),
    )
    const out = await searchEngine.search('xyz')
    expect(out.length).toBe(0)
  })

  it('module 类即时答案：fuzzy=0 靠 boost 穿透（非 matched 但 finalScore>0 保留）', async () => {
    // 即时答案 title 不含 query（换算结果 '717.00'），但 boost 高应穿透显示
    registry.push(makeSearchExt('instant', () => [result('ans', '717.00', 'module', 1000)]))
    const out = await searchEngine.search('100 usd')
    expect(out.find((r) => r.id === 'ans')).toBeDefined()
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

  it('keyword 合流：全局模式匹配 meta.keywords 产出模块入口', async () => {
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

  it('keyword 反向命中：finalScore 保留 keywordMatch 贡献（非 scoreFields 归零，v3 修正）', async () => {
    // keywords=['usd']，query='100 usd'：keywordMatch 反向命中（'usd' in '100 usd'）返回 > 0，
    // 但 scoreFields(['kwrev2'], '100 usd') = 0（title 与 query 无子串关系）。
    // 旧逻辑 groupAndSort 对 keyword 入口重算 scoreFields → finalScore = 0 + 500 = 500（keywordMatch 贡献丢失）；
    // 新逻辑复用 keywordSearchAll 内部 score（含 keywordMatch）→ finalScore = keywordMatch_score + 500 > 500。
    registry.push(makeSearchExt('kwrev2', () => [], ['usd']))
    const out = await searchEngine.search('100 usd')
    const entry = out.find((r) => r.id === 'module-kwrev2')
    expect(entry).toBeDefined()
    expect(entry?.score).toBeGreaterThan(500)
  })

  it('scoreModuleEntry：无 keywords 仅 name 命中也产出模块入口', async () => {
    registry.push({
      meta: { id: 'nameonly', name: '纯名称模块', icon: 'i-ri-test-line', order: 1 },
      search: { dynamic: () => [] },
    })
    const out = await searchEngine.search('纯名称')
    expect(out.find((r) => r.id === 'module-nameonly')).toBeDefined()
  })

  // ── 流式 emit ──

  it('流式 emit：扩展多次 emit 产出部分结果，onUpdate 增量回调且最终一致', async () => {
    registry.push(
      makeSearchExt('stream', (_q, ctx) => {
        ctx.emit?.([result('a', 'alpha one', 'application', 10)])
        ctx.emit?.([result('b', 'alpha two', 'application', 20)])
        return [result('c', 'alpha three', 'file', 5)]
      }),
    )
    const updates: SearchResult[][] = []
    const final = await searchEngine.search('alpha', (partial) => updates.push([...partial]))
    // emit 2 次 + return 1 次 = 3 次增量回调，结果逐步增长
    expect(updates.length).toBe(3)
    expect(updates[0].length).toBe(1)
    expect(updates[1].length).toBe(2)
    expect(updates[2].length).toBe(3)
    // 最终返回与最后一次 onUpdate 一致
    expect(final.map((r) => r.id).sort()).toEqual(['a', 'b', 'c'])
    expect(updates[2].map((r) => r.id).sort()).toEqual(['a', 'b', 'c'])
  })

  it('流式 emit 与 return 重叠：框架去重不产生重复项', async () => {
    registry.push(
      makeSearchExt('dup-emit', (_q, ctx) => {
        ctx.emit?.([result('x', 'alpha', 'application', 10)])
        return [result('x', 'alpha', 'application', 10)] // 同 id 重叠
      }),
    )
    const final = await searchEngine.search('alpha')
    expect(final.filter((r) => r.id === 'x').length).toBe(1)
  })

  it('流式 emit 在 abort 后静默丢弃（不回调 onUpdate）', async () => {
    let emitFn!: (results: ProviderResult[]) => void
    registry.push(
      makeSearchExt('late-emit', (_q, ctx) => {
        emitFn = ctx.emit!
        return new Promise<ProviderResult[]>((resolve) => {
          ctx.signal.addEventListener('abort', () => resolve([]))
        })
      }),
    )
    const updates: SearchResult[][] = []
    const p = searchEngine.search('x', (partial) => updates.push([...partial]))
    searchEngine.abort()
    emitFn([result('late', 'late result', 'module', 10)])
    await p
    expect(updates.length).toBe(0)
  })

  it('abort() 取消进行中的 search signal', async () => {
    const seen: AbortSignal[] = []
    registry.push(
      makeSearchExt('abort-api', (_q, ctx) => {
        seen.push(ctx.signal)
        return new Promise<ProviderResult[]>((resolve) => {
          ctx.signal.addEventListener('abort', () => resolve([]))
        })
      }),
    )
    const p = searchEngine.search('x')
    expect(seen.length).toBe(1)
    searchEngine.abort()
    expect(seen[0].aborted).toBe(true)
    await p
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
    const p1 = searchEngine.search('a') // 同步执行到 collectAll，dynamic 已被调用、signal 已入 seen
    expect(seen.length).toBe(1)
    const p2 = searchEngine.search('b') // 触发 seen[0] abort
    expect(seen[0].aborted).toBe(true)
    // 收尾：让两次 search 都 resolve
    deferreds[1]?.resolve([])
    await p1
    await p2
  })

  it('单扩展超时 abort 其 child signal，不牵连其它扩展', async () => {
    vi.useFakeTimers()
    const signals: AbortSignal[] = []
    registry.push(
      makeSearchExt('slow', (_q, ctx) => {
        signals.push(ctx.signal)
        return new Promise<ProviderResult[]>((resolve) => {
          ctx.signal.addEventListener('abort', () => resolve([]))
        })
      }),
      makeSearchExt('fast', () => [result('f', 'fast hit', 'module')]),
    )
    const p = searchEngine.search('hit')
    // 推进到 searchTimeoutMs
    await vi.advanceTimersByTimeAsync(3000)
    const out = await p
    expect(signals[0]?.aborted).toBe(true)
    // 快扩展结果仍在（超时只杀慢扩展）
    expect(out.find((r) => r.id === 'f')).toBeDefined()
    vi.useRealTimers()
  })

  it('search 快照 activeModule：await 期间 setActiveModule 不影响本次后处理', async () => {
    let release!: (v: ProviderResult[]) => void
    registry.push(
      makeSearchExt('snap', () => {
        return new Promise<ProviderResult[]>((resolve) => {
          release = resolve
        })
      }),
    )
    // 全局模式启动
    searchEngine.setActiveModule(undefined)
    const p = searchEngine.search('anything')
    // await 中途切到模块模式——不得把本次结果当模块短路
    searchEngine.setActiveModule('snap')
    release([result('g', 'anything global', 'application', 10)])
    const out = await p
    // 全局后处理：application 有 boost 且 title 命中
    expect(out.find((r) => r.id === 'g')).toBeDefined()
    searchEngine.setActiveModule(undefined)
  })
})
