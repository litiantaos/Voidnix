// 记事本动效核心纯逻辑:code point 拆分 + 前后缀 diff + code unit/point 索引映射。
// 渲染与测量在 View.vue,此处保持零 DOM 依赖(co-location 测试)。

/** 字符级动画(逐字 pop/ghost/FLIP)的启用上限;超过后降级为纯文本直更,光标动效保留。 */
export const ANIM_MAX_CHARS = 1200

/** 批量新增(粘贴/IME 提交)时逐字 stagger 步长与总延迟上限(ms)。 */
export const STAGGER_STEP = 16
export const STAGGER_CAP = 160

/// 按 Unicode code point 拆分(代理对不拆半,emoji 完整显示)。
export function toChars(text: string): string[] {
  return Array.from(text)
}

export interface TextDiff {
  /** 公共前缀 code point 数(此段 cell 原样保留)。 */
  prefix: number
  /** 公共后缀 code point 数。 */
  suffix: number
  /** old 中被替换段长度(prefix..oldLen-suffix);0 = 纯插入。 */
  removed: number
  /** new 替换段长度;0 = 纯删除。 */
  added: number
}

/// 单点编辑 diff:求公共前缀/后缀,中间视为「删 removed + 增 added」。
/// 记事本场景(键入/删除/粘贴/替换选中)均为单点变化,O(n) 前后缀扫描即最小编辑位置。
export function diffChars(oldChars: string[], newChars: string[]): TextDiff {
  const oldLen = oldChars.length
  const newLen = newChars.length
  const maxPrefix = Math.min(oldLen, newLen)
  let prefix = 0
  while (prefix < maxPrefix && oldChars[prefix] === newChars[prefix]) prefix++
  // 整体相同(含都为空)时 suffix 从 prefix 起算,removed/added 归零
  if (prefix === oldLen && prefix === newLen) return { prefix, suffix: 0, removed: 0, added: 0 }
  const maxSuffix = Math.min(oldLen - prefix, newLen - prefix)
  let suffix = 0
  while (suffix < maxSuffix && oldChars[oldLen - 1 - suffix] === newChars[newLen - 1 - suffix]) {
    suffix++
  }
  return {
    prefix,
    suffix,
    removed: oldLen - prefix - suffix,
    added: newLen - prefix - suffix,
  }
}

export interface IndexMap {
  /** code unit 索引 → code point 索引(长 len+1;selectionStart/End 转 cp 偏移用)。 */
  cu2cp: number[]
  /** code point 索引 → 起始 code unit 索引(长 n+1;cp 偏移转 setSelectionRange 用)。 */
  cpStart: number[]
}

/// 构建 code unit 与 code point 索引的双向映射。
/// textarea selectionStart/End 是 code unit 索引;渲染层 cell 是 code point 粒度。
export function buildIndexMap(text: string): IndexMap {
  const cu2cp: number[] = new Array(text.length + 1)
  const cpStart: number[] = []
  let cu = 0
  let cp = 0
  cu2cp[0] = 0
  while (cu < text.length) {
    cpStart[cp] = cu
    // 代理对:[uD800-uDBFF][uDC00-uDFFF] 合一个 code point
    const isPair =
      text.charCodeAt(cu) >= 0xd800 &&
      text.charCodeAt(cu) <= 0xdbff &&
      cu + 1 < text.length &&
      text.charCodeAt(cu + 1) >= 0xdc00 &&
      text.charCodeAt(cu + 1) <= 0xdfff
    const step = isPair ? 2 : 1
    if (isPair) cu2cp[cu + 1] = cp + 1 // 代理对中间 code unit 填后继 cp(防稀疏洞;正常 selection 不落此处)
    cu += step
    cp += 1
    cu2cp[cu] = cp
  }
  cpStart[cp] = text.length
  return { cu2cp, cpStart }
}
