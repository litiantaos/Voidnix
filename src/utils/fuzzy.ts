import { match } from 'pinyin-pro'
import { SEARCH } from '@/runtime/constants'

const { prefix: SUBSTRING_PREFIX, contains: SUBSTRING_CONTAIN } = SEARCH.WEIGHTS
const PINYIN_BASE = SEARCH.WEIGHTS.pinyinBase
const FIELD_DECAY = SEARCH.WEIGHTS.decay

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

  const indices = match(text, qLower, {
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

function fieldScore(text: string, qLower: string): number {
  return Math.max(substringScore(text, qLower), pinyinScore(text, qLower))
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
 * keyword 双向匹配（keywordSearchAll 专用）。
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

/**
 * 使用频率加权（log 平滑，避免高频应用永远霸榜）。
 * useCount=0→0；1→~50；10→~170；100→~280；上限 cap（constants.WEIGHTS.cap）。
 */
export function frequencyBoost(useCount: number): number {
  if (!useCount || useCount <= 0) return 0
  const { logBase, logMul, cap } = SEARCH.WEIGHTS
  return Math.min(Math.round((Math.log(useCount + 1) / Math.log(logBase)) * logMul), cap)
}
