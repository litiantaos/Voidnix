import { describe, it, expect } from 'vitest'
import { toLlmMessages } from './logic'
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

  it('assistant toolCall → toolCalls（args 序列化为 JSON 字符串）', () => {
    const out = toLlmMessages([
      assistantMsg([
        {
          type: 'toolCall',
          id: 'call-1',
          name: 'run_command',
          args: { cmd: 'ls' },
          state: 'done',
        },
      ]),
    ])
    expect(out[0].role).toBe('assistant')
    expect(out[0].content).toBeUndefined()
    expect(out[0].toolCalls).toEqual([
      {
        id: 'call-1',
        type: 'function',
        function: { name: 'run_command', arguments: JSON.stringify({ cmd: 'ls' }) },
      },
    ])
  })

  it('assistant text + toolCall 同时保留', () => {
    const out = toLlmMessages([
      assistantMsg([
        { type: 'text', text: 'running' },
        {
          type: 'toolCall',
          id: 'c1',
          name: 'run_command',
          args: {},
          state: 'streaming',
        },
      ]),
    ])
    expect(out[0].content).toBe('running')
    expect(out[0].toolCalls).toHaveLength(1)
  })

  it('toolCall 无 args → arguments 为 "{}"', () => {
    const out = toLlmMessages([
      assistantMsg([{ type: 'toolCall', id: 'c', name: 'n', state: 'done' }]),
    ])
    expect(out[0].toolCalls?.[0].function.arguments).toBe('{}')
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
