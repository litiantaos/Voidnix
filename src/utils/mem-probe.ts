// dev 内存探针：用 FinalizationRegistry 追踪搜索结果对象的 GC 回收，
// 直接验证「上次搜索结果是否被回收」——不依赖 performance.memory（WKWebView 不暴露）。
// 判读：搜 def 后 probe 显示 abc 的 alive 归零 → 引用已释放，RSS 不降是分配器不还页；
//       abc 的 alive 一直不降 → 有引用持有，真泄漏。
// prod 被 import.meta.env.DEV 编译期常量 tree-shake，registry 不创建，零运行时开销。

interface TagStat {
  total: number // 累计注册数
  finalized: number // 已被 GC 回收数
}

const DEV = import.meta.env.DEV

// tag -> 统计；tag 为 query + 自增序号，保证每次搜索唯一
const stats = new Map<string, TagStat>()

const registry =
  DEV && typeof FinalizationRegistry !== 'undefined'
    ? new FinalizationRegistry<string>((tag) => {
        const s = stats.get(tag)
        if (s) s.finalized++
      })
    : undefined

let seq = 0

/** 为一次搜索的结果对象注册 GC 追踪。返回本次 tag（供 probeMem 引用）。 */
export function trackResults(query: string, items: object[]): string {
  if (!DEV || !registry || items.length === 0) return ''
  const tag = `"${query}"#${++seq}`
  const s: TagStat = { total: items.length, finalized: 0 }
  stats.set(tag, s)
  for (const item of items) registry.register(item, tag)
  return tag
}

/** 打印当前所有已追踪搜索的存活/回收统计。
 *  alive = total - finalized：仍被某处持有的对象数。
 *  尝试主动触发 GC（window.gc 不可用时降级为等待自然 GC）。 */
export function probeMem(label: string): void {
  if (!DEV) return
  if (!registry) {
    console.warn('[mem-probe] FinalizationRegistry unavailable')
    return
  }
  // WebKit 默认不暴露 window.gc；若可用则主动触发一次以提高回收可见性
  const gc = (window as Window & { gc?: () => void }).gc
  gc?.()

  const lines = [...stats.entries()].map(([tag, s]) => {
    const alive = s.total - s.finalized
    return `  ${tag}: alive ${alive}/${s.total} (GC 回收 ${s.finalized})`
  })
  console.warn(`[mem] ${label}\n${lines.join('\n') || '  (无已追踪搜索)'}`)
}

// 控制台全局入口：随时调 __mem() 看当前各搜索结果对象的存活/回收统计
if (DEV) {
  ;(window as Window & { __mem?: () => void }).__mem = () => probeMem('manual')
}
