/// Markdown 渲染（纯函数，无 DOM / 无 Vue 响应式）。
/// agent / ai-providers 等扩展共用：marked + 自定义 renderer + DOMPurify 净化。

import { marked, type Tokens } from 'marked'
import DOMPurify from 'dompurify'

function escapeHtml(raw: string): string {
  return raw
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
}

/** fence 语言：仅首 token + 白名单字符，供 class / 展示 */
export function sanitizeCodeLang(lang: string | undefined): string {
  if (!lang) return ''
  const token = lang.trim().split(/\s+/)[0] ?? ''
  return /^[a-zA-Z0-9_+#.-]+$/.test(token) ? token : ''
}

/** 代码块外壳：语言标签 + 复制按钮 + pre/code */
export function renderCodeBlock(text: string, lang?: string): string {
  const language = sanitizeCodeLang(lang)
  const langHtml = language
    ? `<span class="md-code-lang">${escapeHtml(language)}</span>`
    : `<span class="md-code-lang md-code-lang--empty"></span>`
  const classAttr = language ? ` class="language-${escapeHtml(language)}"` : ''
  return [
    '<div class="md-code">',
    '<div class="md-code-bar">',
    langHtml,
    '<button type="button" class="md-code-copy" aria-label="复制代码" title="复制">',
    '<i class="i-ri-file-copy-line" aria-hidden="true"></i>',
    '</button>',
    '</div>',
    `<pre class="md-code-pre"><code${classAttr}>${escapeHtml(text)}</code></pre>`,
    '</div>\n',
  ].join('')
}

/**
 * 列表项外壳：固定宽标记节点 + body
 * （不用 li::before/grid：WK 下匿名文本格子不稳，ol/ul 视觉错位）
 */
export function renderListItemHtml(mark: string, bodyHtml: string): string {
  return [
    '<li class="md-li">',
    `<span class="md-li-mark" aria-hidden="true">${escapeHtml(mark)}</span>`,
    `<div class="md-li-body">${bodyHtml}</div>`,
    '</li>\n',
  ].join('')
}

marked.use({
  gfm: true,
  breaks: true,
  renderer: {
    code({ text, lang }: Tokens.Code) {
      return renderCodeBlock(text, lang)
    },
    list(
      this: { parser: { parse: (tokens: Tokens.ListItem['tokens']) => string } },
      token: Tokens.List,
    ) {
      const tag = token.ordered ? 'ol' : 'ul'
      const start =
        typeof token.start === 'number' && Number.isFinite(token.start) ? token.start : 1
      const startAttr = token.ordered && start !== 1 ? ` start="${start}"` : ''
      let items = ''
      for (let i = 0; i < token.items.length; i++) {
        const item = token.items[i]!
        // task list 已有 checkbox 在 body，标记列留空位以保持列宽一致
        const mark = item.task ? '' : token.ordered ? `${start + i}.` : '•'
        const body = this.parser.parse(item.tokens)
        items += renderListItemHtml(mark, body)
      }
      return `<${tag} class="md-list"${startAttr}>\n${items}</${tag}>\n`
    },
  },
})

export function renderMarkdown(content: unknown): string {
  if (typeof content !== 'string' || !content) return ''
  const result = marked.parse(content)
  if (typeof result !== 'string') return ''
  const sanitized = DOMPurify.sanitize(result, {
    ADD_ATTR: ['target', 'rel', 'aria-label', 'aria-hidden', 'title', 'type'],
  })
  // 所有 a 标签加 target + rel，避免在 webview 内导航
  return sanitized.replace(/<a\s+href/gi, '<a target="_blank" rel="noopener noreferrer" href')
}
