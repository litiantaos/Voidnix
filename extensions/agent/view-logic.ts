/// Agent View 纯函数（无 DOM / 无 Vue 响应式），便于单测。

import { marked, type Tokens } from 'marked'
import DOMPurify from 'dompurify'
import type { AgentMessage, AgentPart } from '@/types/agent'

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

export function getMessageText(msg: AgentMessage): string {
  return msg.parts
    .filter((p): p is Extract<AgentPart, { type: 'text' }> => p.type === 'text')
    .map((p) => p.text)
    .join('')
}

/**
 * 历史浮层单行 label：折叠空白 + 截断到 maxLen，空文本回退序号占位。
 * 入参 ordinal 从 1 起（仅用于空消息兜底，不在正常 label 前加序号，避免噪音）。
 */
export function buildHistoryLabel(text: string, ordinal: number, maxLen = 60): string {
  const flat = text.replace(/\s+/g, ' ').trim()
  if (!flat) return `#${ordinal}`
  return flat.length > maxLen ? flat.slice(0, maxLen) + '…' : flat
}

/** 是否为该消息中最后一个 text part 且仍在流式输出 */
export function isStreamingText(msg: AgentMessage, partIndex: number): boolean {
  if (!msg.streaming) return false
  for (let j = msg.parts.length - 1; j >= 0; j--) {
    if (msg.parts[j]?.type === 'text') return j === partIndex
  }
  return false
}

export type StreamView = { text: string; solid: string; tail: string; lineKey: number }

let streamViewCache: StreamView | null = null

/** 一次拆 solid/tail/lineKey；同 text 同 tick 多次调用命中缓存 */
export function streamView(text: string): StreamView {
  if (streamViewCache?.text === text) return streamViewCache
  const idx = text.lastIndexOf('\n')
  const solid = idx === -1 ? '' : text.slice(0, idx + 1)
  const tail = idx === -1 ? text : text.slice(idx + 1)
  let lineKey = 0
  for (let i = 0; i < text.length; i++) {
    if (text[i] === '\n') lineKey++
  }
  streamViewCache = { text, solid, tail, lineKey }
  return streamViewCache
}

/** 测试用：清空 streamView 缓存 */
export function resetStreamViewCache(): void {
  streamViewCache = null
}

let solidMdCache: { solid: string; html: string } | null = null

/** solid 段 markdown 缓存：solid 前缀稳定时不重复 parse+sanitize */
export function renderSolidMarkdown(solid: string): string {
  if (solidMdCache?.solid === solid) return solidMdCache.html
  const html = renderMarkdown(solid)
  solidMdCache = { solid, html }
  return html
}

/** 测试用：清空 solid markdown 缓存 */
export function resetSolidMdCache(): void {
  solidMdCache = null
}

/** 工具结果区：web_search 成功 hits / answer；失败或其它工具有 output */
export function showToolBody(part: Extract<AgentPart, { type: 'toolCall' }>): boolean {
  if (part.state !== 'done' && part.state !== 'failed') return false
  if (part.name === 'web_search') {
    if (part.state === 'done' && part.parsed && (part.parsed.hits.length > 0 || part.parsed.answer))
      return true
    return part.state === 'failed' && !!part.output
  }
  return !!part.output
}

export function toolLabel(name: string): string {
  switch (name) {
    case 'web_search':
      return '搜索'
    case 'run_command':
      return '命令'
    default:
      return name
  }
}

export function toolIcon(name: string): string {
  switch (name) {
    case 'web_search':
      return 'i-ri-search-line'
    case 'run_command':
      return 'i-ri-terminal-box-line'
    default:
      return 'i-ri-tools-line'
  }
}

/** 工具参数明细：run_command → cmd args…，web_search → query，其余空串。 */
export function toolDetail(part: Extract<AgentPart, { type: 'toolCall' }>): string {
  if (!part.args || typeof part.args !== 'object') return ''
  const obj = part.args as Record<string, unknown>
  if (part.name === 'run_command') {
    const cmd = typeof obj.cmd === 'string' ? obj.cmd : ''
    const argsArr = Array.isArray(obj.args)
      ? obj.args.filter((a): a is string => typeof a === 'string')
      : []
    return [cmd, ...argsArr].filter(Boolean).join(' ')
  }
  if (part.name === 'web_search') {
    return typeof obj.query === 'string' ? obj.query.trim() : ''
  }
  return ''
}

/**
 * 轻量签名：消息条数 + 当前 streaming 文本/工具输出长度 + 生成态。
 * 供 View watch 用，避免 deep watch 每个 textDelta。
 */
export function streamLayoutKey(messages: AgentMessage[], isGenerating: boolean): string {
  let streamLen = 0
  let toolN = 0
  for (const m of messages) {
    if (!m.streaming) continue
    for (const p of m.parts) {
      if (p.type === 'text') streamLen += p.text.length
      else if (p.type === 'toolCall') {
        toolN++
        streamLen += p.output?.length ?? 0
      }
    }
  }
  return `${messages.length}:${streamLen}:${toolN}:${isGenerating}`
}

/** part 列表稳定 key：tool 用 id，其余 type+index */
export function partKey(part: AgentPart, index: number): string {
  if (part.type === 'toolCall') return part.id
  if (part.type === 'notice') return `notice-${part.kind}-${index}`
  return `text-${index}`
}
