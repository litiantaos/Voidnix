/// Agent View 纯函数（无 DOM / 无 Vue 响应式），便于单测。

import { marked } from 'marked'
import DOMPurify from 'dompurify'
import type { AgentMessage, AgentPart } from '@/types/agent'

marked.setOptions({ gfm: true, breaks: true })

export function renderMarkdown(content: unknown): string {
  if (typeof content !== 'string' || !content) return ''
  const result = marked.parse(content)
  if (typeof result !== 'string') return ''
  const sanitized = DOMPurify.sanitize(result, { ADD_ATTR: ['target', 'rel'] })
  // 所有 a 标签加 target + rel，避免在 webview 内导航
  return sanitized.replace(/<a\s+href/gi, '<a target="_blank" rel="noopener noreferrer" href')
}

export function getMessageText(msg: AgentMessage): string {
  return msg.parts
    .filter((p): p is Extract<AgentPart, { type: 'text' }> => p.type === 'text')
    .map((p) => p.text)
    .join('')
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
