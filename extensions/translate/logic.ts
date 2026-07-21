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
