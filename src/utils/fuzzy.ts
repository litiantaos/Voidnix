import { match } from 'pinyin-pro'

/**
 * 统一前端模糊匹配。
 *
 * 同时覆盖：
 * - 中文原文 / 拼音全拼 / 拼音首字母（pinyin-pro，多音字 + ü→v 自动处理）
 * - 英文/数字子串（lowercase includes）
 *
 * 不同字段独立打分取最大；返回 0 表示不匹配。
 */

const SUBSTRING_PREFIX = 1000
const SUBSTRING_CONTAIN = 600
const PINYIN_BASE = 400
const FIELD_DECAY = 0.85

function substringScore(text: string, query: string): number {
  if (!text || !query) return 0
  const t = text.toLowerCase()
  const q = query.toLowerCase()
  const idx = t.indexOf(q)
  if (idx < 0) return 0
  const ratio = q.length / Math.max(t.length, 1)
  const base = idx === 0 ? SUBSTRING_PREFIX : SUBSTRING_CONTAIN
  return Math.round(base + ratio * 200 - idx * 4)
}

function pinyinScore(text: string, query: string): number {
  if (!text || !query) return 0
  if (!/[㐀-鿿]/.test(text)) return 0
  if (!/^[a-zA-Z\s]+$/.test(query)) return 0

  const indices = match(text, query, {
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

function fieldScore(text: string, query: string): number {
  return Math.max(substringScore(text, query), pinyinScore(text, query))
}

/**
 * 对一组字段（标题、描述、关键词）按权重打分取 max。
 * 第一个字段视为主字段权重 1.0，后续字段每增加一个衰减 FIELD_DECAY。
 */
export function scoreFields(fields: (string | undefined | null)[], query: string): number {
  const q = query.trim()
  if (!q) return 0
  let best = 0
  let weight = 1.0
  for (const f of fields) {
    if (!f) continue
    const s = fieldScore(f, q) * weight
    if (s > best) best = s
    weight *= FIELD_DECAY
  }
  return Math.round(best)
}

/**
 * 使用频率加权（log 平滑，避免高频应用永远霸榜）。
 * useCount=0→0；1→~50；10→~170；100→~280；上限 320。
 */
export function frequencyBoost(useCount: number): number {
  if (!useCount || useCount <= 0) return 0
  return Math.min(Math.round(Math.log2(useCount + 1) * 50), 320)
}
