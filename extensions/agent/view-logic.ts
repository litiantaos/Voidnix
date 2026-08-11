/// Agent View 纯函数（无 DOM / 无 Vue 响应式），便于单测。

import type { AgentMessage, AgentPart } from '@/types/agent'
import { renderMarkdown } from '@/utils/markdown'
import { t } from '@/runtime/i18n'

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

/** 工具结果区：web_search 成功展示 answer 摘要 / 失败展示 output；其它工具有 output */
export function showToolBody(part: Extract<AgentPart, { type: 'toolCall' }>): boolean {
  if (part.state !== 'done' && part.state !== 'failed') return false
  if (part.name === 'web_search') {
    if (part.state === 'done') return !!part.parsed
    return !!part.output
  }
  return !!part.output
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

/** 工具语义 label（active 时与「思考」同款 shimmer） */
export function toolLabel(name: string): string {
  switch (name) {
    case 'web_search':
      return t('agent.tool.search')
    case 'run_command':
      return t('agent.tool.command')
    default:
      return t('agent.tool.default')
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
      if (p.type === 'text' || p.type === 'reasoning') streamLen += p.text.length
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
  if (part.type === 'reasoning') return `reasoning-${index}`
  return `text-${index}`
}
