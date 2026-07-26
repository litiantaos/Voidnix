import type { TranslateApiConfig } from './config'

const PREAMBLE_PATTERNS = [
  /here\s*(?:is|'s)\s*the\s*translation/i,
  /the\s*translation\s*is/i,
  /translated\s*text/i,
  /translation\s*(?:result|:)/i,
  // 仅匹配行首的中文 preamble，避免 [\s\S]*? 通配误删前导用户内容
  /^(?:以下是翻译|翻译结果|翻译如下|翻译)[:：]?\s*/,
]

/** 清理 LLM 翻译流式结果：去代码块围栏 / 智能引号配对 / 前导文本（"以下是翻译:"等）。 */
export function cleanStreamResult(raw: string): string {
  let s = raw.trim()

  if (s.startsWith('```') && s.endsWith('```')) {
    s = s
      .slice(3, -3)
      .replace(/^[a-z0-9+]+\n?/i, '')
      .trim()
  }

  for (const open of ['"', '\u{201C}', '\u{300C}', '\u{300E}']) {
    const close =
      open === '"'
        ? '"'
        : open === '\u{201C}'
          ? '\u{201D}'
          : open === '\u{300C}'
            ? '\u{300D}'
            : '\u{300F}'
    if (s.startsWith(open) && s.endsWith(close) && s.length > 1) {
      s = s.slice(1, -1).trim()
    }
  }

  for (const pat of PREAMBLE_PATTERNS) {
    const m = s.match(pat)
    if (m && m.index !== undefined) {
      const rest = s.slice(m.index + m[0].length).replace(/^[\s：:]+/, '')
      if (rest) s = rest
      break
    }
  }

  return s
}

/** 翻译引擎展示名（固定两项服务名）。 */
export function engineLabel(cfg: TranslateApiConfig): string {
  return cfg.type === 'youdao' ? '有道翻译' : 'AI 翻译'
}

/**
 * 按译文字符脚本推断朗读语种码（传给 say -v 选语音）。
 * 关键：只要含假名即判日文（中文不含假名），故单遍循环遇假名立即返回；
 * 遇谚文立即返回韩文；汉字仅标记不立即返回（后续可能遇假名改判日文）。
 * 纯汉字的中日文无法按脚本区分，回落 zh（中文语音读日文汉字亦可懂）。
 * 其余非拉丁脚本（西里尔 / 阿拉伯 / 泰文 / 天城文 / 越南文）即时命中对应语种；
 * 纯拉丁（英 / 法 / 德 / 西 等）统一回落 en（say 英文语音可懂度可接受）。
 */
export function detectSpeechLang(text: string): string {
  let hasCJK = false
  for (const ch of text) {
    const cp = ch.codePointAt(0)!
    if (cp >= 0x3040 && cp <= 0x30ff) return 'ja' // 平假名 / 片假名 → 日文
    if (cp >= 0xac00 && cp <= 0xd7af) return 'ko' // 谚文音节 → 韩文
    if (cp >= 0x0400 && cp <= 0x04ff) return 'ru' // 西里尔 → 俄文
    if (cp >= 0x0600 && cp <= 0x06ff) return 'ar' // 阿拉伯
    if (cp >= 0x0e00 && cp <= 0x0e7f) return 'th' // 泰文
    if (cp >= 0x0900 && cp <= 0x097f) return 'hi' // 天城文 → 印地文
    if (cp >= 0x1e00 && cp <= 0x1eff) return 'vi' // 拉丁扩展附加（越南语声调）→ 越南文
    if (
      !hasCJK &&
      ((cp >= 0x4e00 && cp <= 0x9fff) ||
        (cp >= 0x3400 && cp <= 0x4dbf) ||
        (cp >= 0xf900 && cp <= 0xfaff))
    ) {
      hasCJK = true // CJK 统一表意，标记后继续找假名
    }
  }
  return hasCJK ? 'zh' : 'en'
}
