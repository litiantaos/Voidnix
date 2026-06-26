import { describe, it, expect } from 'vitest'
import { toLlmMessages, tryParseSearch } from './logic'
import type { AgentMessage } from '@/types/agent'

function userMsg(text: string): AgentMessage {
  return { id: 'u-' + text, role: 'user', parts: [{ type: 'text', text }] }
}

function assistantMsg(parts: AgentMessage['parts'], streaming = false): AgentMessage {
  return { id: 'a-' + Math.random(), role: 'assistant', parts, streaming }
}

describe('toLlmMessages', () => {
  it('user 消息 → {role:user, content}', () => {
    const out = toLlmMessages([userMsg('hello')])
    expect(out).toEqual([{ role: 'user', content: 'hello' }])
  })

  it('合并 user 的多个 text part', () => {
    const out = toLlmMessages([
      {
        id: 'u',
        role: 'user',
        parts: [
          { type: 'text', text: 'a' },
          { type: 'text', text: 'b' },
        ],
      },
    ])
    expect(out).toEqual([{ role: 'user', content: 'ab' }])
  })

  it('assistant 纯文本 → {role:assistant, content}', () => {
    const out = toLlmMessages([assistantMsg([{ type: 'text', text: 'hi' }])])
    expect(out).toEqual([{ role: 'assistant', content: 'hi' }])
  })

  it('assistant toolCall → toolCalls + 对应 tool result', () => {
    const out = toLlmMessages([
      assistantMsg([
        {
          type: 'toolCall',
          id: 'call-1',
          name: 'run_command',
          args: { cmd: 'ls' },
          output: 'file1\nfile2',
          state: 'done',
        },
      ]),
    ])
    expect(out).toHaveLength(2)
    expect(out[0].role).toBe('assistant')
    expect(out[0].content).toBeUndefined()
    expect(out[0].toolCalls).toEqual([
      {
        id: 'call-1',
        type: 'function',
        function: { name: 'run_command', arguments: JSON.stringify({ cmd: 'ls' }) },
      },
    ])
    expect(out[1]).toEqual({ role: 'tool', toolCallId: 'call-1', content: 'file1\nfile2' })
  })

  it('assistant text + toolCall 同时保留（text 进 content，toolCall 进 toolCalls + result）', () => {
    const out = toLlmMessages([
      assistantMsg([
        { type: 'text', text: 'running' },
        {
          type: 'toolCall',
          id: 'c1',
          name: 'run_command',
          args: {},
          output: 'ok',
          state: 'done',
        },
      ]),
    ])
    expect(out).toHaveLength(2)
    expect(out[0].content).toBe('running')
    expect(out[0].toolCalls).toHaveLength(1)
    expect(out[1]).toEqual({ role: 'tool', toolCallId: 'c1', content: 'ok' })
  })

  it('toolCall 无 args → arguments 为 "{}"，result content 占位', () => {
    const out = toLlmMessages([
      assistantMsg([{ type: 'toolCall', id: 'c', name: 'n', state: 'done' }]),
    ])
    expect(out[0].toolCalls?.[0].function.arguments).toBe('{}')
    expect(out[1]).toEqual({ role: 'tool', toolCallId: 'c', content: '(无输出)' })
  })

  it('多轮历史：user → assistant(toolCall) → tool(result) → user 第二条', () => {
    const out = toLlmMessages([
      userMsg('列出文件'),
      assistantMsg([
        {
          type: 'toolCall',
          id: 'c1',
          name: 'run_command',
          args: { cmd: 'ls' },
          output: 'a.txt',
          state: 'done',
        },
        { type: 'text', text: '目录里有 a.txt' },
      ]),
      userMsg('删除它'),
    ])
    expect(out.map((m) => m.role)).toEqual(['user', 'assistant', 'tool', 'user'])
    expect(out[1].toolCalls).toHaveLength(1)
    expect(out[2]).toEqual({ role: 'tool', toolCallId: 'c1', content: 'a.txt' })
    expect(out[3]).toEqual({ role: 'user', content: '删除它' })
  })

  it('streaming 消息被跳过', () => {
    const out = toLlmMessages([
      userMsg('keep'),
      assistantMsg([{ type: 'text', text: 'partial' }], true),
    ])
    expect(out).toHaveLength(1)
    expect(out[0].role).toBe('user')
  })

  it('空 parts 的 assistant 消息被跳过', () => {
    const out = toLlmMessages([assistantMsg([])])
    expect(out).toEqual([])
  })

  it('空输入 → []', () => {
    expect(toLlmMessages([])).toEqual([])
  })

  it('保留消息顺序', () => {
    const out = toLlmMessages([
      userMsg('a'),
      assistantMsg([{ type: 'text', text: 'b' }]),
      userMsg('c'),
    ])
    expect(out.map((m) => m.role)).toEqual(['user', 'assistant', 'user'])
  })
})

describe('tryParseSearch', () => {
  it('解析 answer + hits', () => {
    const result = tryParseSearch(
      JSON.stringify({
        answer: '摘要',
        hits: [{ title: 'T', url: 'https://x', snippet: 'S' }],
      }),
    )
    expect(result?.answer).toBe('摘要')
    expect(result?.hits).toEqual([{ title: 'T', url: 'https://x', snippet: 'S' }])
  })

  it('无 answer 仅 hits', () => {
    const result = tryParseSearch(JSON.stringify({ hits: [{ title: 'T', url: 'U' }] }))
    expect(result?.answer).toBeUndefined()
    expect(result?.hits).toHaveLength(1)
  })

  it('过滤无 title 且无 url 的 hit', () => {
    const result = tryParseSearch(
      JSON.stringify({ hits: [{ snippet: 'orphan' }, { title: 'T', url: 'U' }] }),
    )
    expect(result?.hits).toHaveLength(1)
  })

  it('answer 空白且无有效 hits → undefined', () => {
    expect(tryParseSearch(JSON.stringify({ answer: '   ', hits: [] }))).toBeUndefined()
  })

  it('非 JSON → undefined', () => {
    expect(tryParseSearch('not json')).toBeUndefined()
  })
})
