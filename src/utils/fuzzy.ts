import { SEARCH } from '@/runtime/constants'

const { prefix: SUBSTRING_PREFIX, contains: SUBSTRING_CONTAIN } = SEARCH.WEIGHTS
const PINYIN_BASE = SEARCH.WEIGHTS.pinyinBase
const FIELD_DECAY = SEARCH.WEIGHTS.decay

/// pinyin-pro 延迟加载：携带完整汉字拼音字典（体积大头），首次 CJK 查询时才异步拉取。
/// 加载完成前 pinyinScore 返回 0（降级为纯子串匹配），完成后恢复正常。
type PinyinMatchFn = typeof import('pinyin-pro').match
let pinyinMatch: PinyinMatchFn | null = null
let pinyinPromise: Promise<void> | null = null

function ensurePinyin() {
  if (pinyinPromise) return
  pinyinPromise = import('pinyin-pro')
    .then((m) => {
      pinyinMatch = m.match
      // 拼音加载完成时清空 score 缓存：加载期间缓存的 CJK 文本得分不含拼音分（偏低）
      fieldScoreCache.clear()
    })
    .catch(() => {})
}

/// 预热：在 app mount 后调用，让拼音模块在空闲时加载完成（用户首次搜索前就绪）
export function prewarmPinyin() {
  ensurePinyin()
}

/// 等待拼音模块加载完成（测试用；运行时 fire-and-forget 无需 await）
export async function pinyinReady() {
  ensurePinyin()
  await pinyinPromise
}

/** 子串打分。haystack 由内部 toLowerCase（各 field 不同，无法跨结果缓存）；
 *  needle 约定已 toLowerCase（由 scoreFields/keywordMatch 入口预算，避免热路径重复 lower）。 */
function substringScore(haystack: string, needle: string): number {
  if (!haystack || !needle) return 0
  const t = haystack.toLowerCase()
  const idx = t.indexOf(needle)
  if (idx < 0) return 0
  const ratio = needle.length / Math.max(t.length, 1)
  const base = idx === 0 ? SUBSTRING_PREFIX : SUBSTRING_CONTAIN
  return Math.round(base + ratio * 200 - idx * 4)
}

/** 拼音打分。text 中文原值（lower 对中文幂等）；qLower 约定已 toLowerCase。 */
function pinyinScore(text: string, qLower: string): number {
  if (!text || !qLower) return 0
  if (!/[㐀-鿿]/.test(text)) return 0
  if (!/^[a-zA-Z\s]+$/.test(qLower)) return 0
  // 拼音模块尚未加载：触发异步加载（下次调用时就绪），本轮降级为 0
  if (!pinyinMatch) {
    ensurePinyin()
    return 0
  }

  const indices = pinyinMatch(text, qLower, {
    precision: 'start',
    continuous: true,
    space: 'ignore',
    // 输入法约定 ü = v（绿 lv / 女 nv / 略 lve）
    v: true,
  })
  if (!indices || indices.length === 0) return 0

  const coverage = indices.length / text.length
  const startBonus = indices[0] === 0 ? 80 : 0
  const gap = indices[indices.length - 1] - indices[0] - indices.length + 1
  const penalty = gap * 12
  return Math.max(0, Math.round(PINYIN_BASE + coverage * 200 + startBonus - penalty))
}

// Query-scoped fieldScore 缓存：同一 (text, qLower) 对的结果在单次搜索内不变。
// qLower 变化时自动清空——无需显式生命周期管理，零内存泄漏。
// 消除 flush 增量重排时 keywordSearchAll / scoreResults 的重复 toLowerCase + match() 调用。
let cachedQ = ''
const fieldScoreCache = new Map<string, number>()

/** 单字段打分（substring + pinyin 取 max），带 query-scoped 缓存。
 *  搜索引擎每次 flush 都重跑 buildGlobal → keywordSearchAll，同批 text 被重算 2-4 遍；
 *  缓存命中时跳过 toLowerCase 分配 + pinyin-pro match() 拼音索引查找。 */
function fieldScore(text: string, qLower: string): number {
  if (cachedQ !== qLower) {
    cachedQ = qLower
    fieldScoreCache.clear()
  }
  const hit = fieldScoreCache.get(text)
  if (hit !== undefined) return hit
  const result = Math.max(substringScore(text, qLower), pinyinScore(text, qLower))
  fieldScoreCache.set(text, result)
  return result
}

/**
 * 对一组字段（标题、描述、关键词）按权重打分取 max。
 * 第一个字段视为主字段权重 1.0，后续字段每增加一个衰减 FIELD_DECAY。
 * query 在入口预算 qLower（trim + toLowerCase）一次，内部函数复用，避免每字段重复 lower。
 */
export function scoreFields(fields: (string | undefined | null)[], query: string): number {
  const qLower = query.trim().toLowerCase()
  if (!qLower) return 0
  let best = 0
  let weight = 1.0
  for (const f of fields) {
    if (!f) continue
    const s = fieldScore(f, qLower) * weight
    if (s > best) best = s
    weight *= FIELD_DECAY
  }
  return Math.round(best)
}

/**
 * keyword 双向匹配（scoreModuleEntry / keyword 入口专用）。
 * scoreFields 的 substringScore 只查「query 是 field 子串」，对 keyword 场景有缺陷：
 * keyword 通常很短（"usd"/"汇率"），多词 query（"100 usd"/"美元汇率"）比 keyword 长 → 永远 0 分。
 * 此处补全反向：keyword 是 query 子串时也命中（降权 0.5，弱信号），覆盖「query 包含关键词」语义。
 */
export function keywordMatch(keywords: (string | undefined | null)[], query: string): number {
  const qLower = query.trim().toLowerCase()
  if (!qLower) return 0
  let best = 0
  for (const k of keywords) {
    if (!k) continue
    const kLower = k.toLowerCase()
    const forward = substringScore(kLower, qLower)
    const reverse = substringScore(qLower, kLower) * 0.5
    const py = pinyinScore(kLower, qLower)
    if (forward > best) best = forward
    if (reverse > best) best = reverse
    if (py > best) best = py
  }
  return Math.round(best)
}

/** 模块入口打分单一源：name/id/description 正向 + keywords 双向。全局 keyword 与 `/` 工具列表共用。 */
export function scoreModuleEntry(
  meta: { name: string; id?: string; description?: string; keywords?: string[] },
  query: string,
): number {
  return Math.max(
    scoreFields([meta.name, meta.id, meta.description], query),
    keywordMatch(meta.keywords ?? [], query),
  )
}

/**
 * 使用频率加权（log 平滑，避免高频应用永远霸榜）。
 * useCount=0→0；1→~50；10→~170；100→~280；上限 cap（constants.WEIGHTS.cap）。
 */
export function frequencyBoost(useCount: number): number {
  if (!useCount || useCount <= 0) return 0
  const { logBase, logMul, cap } = SEARCH.WEIGHTS
  return Math.min(Math.round((Math.log(useCount + 1) / Math.log(logBase)) * logMul), cap)
}
