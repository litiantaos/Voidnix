import { describe, it, expect, beforeEach } from 'vitest'
import {
  streamView,
  resetStreamViewCache,
  showToolBody,
  toolLabel,
  toolIcon,
  toolDetail,
  isStreamingText,
  getMessageText,
  streamLayoutKey,
  renderMarkdown,
} from './view-logic'
import type { AgentMessage, AgentPart } from '@/types/agent'

function tool(
  partial: Partial<Extract<AgentPart, { type: 'toolCall' }>> & { name: string },
): Extract<AgentPart, { type: 'toolCall' }> {
  return {
    type: 'toolCall',
    id: partial.id ?? 't1',
    name: partial.name,
    state: partial.state ?? 'done',
    args: partial.args,
    output: partial.output,
    parsed: partial.parsed,
  }
}

beforeEach(() => {
  resetStreamViewCache()
})

describe('streamView', () => {
  it('无换行时全部为 tail', () => {
    expect(streamView('hello')).toEqual({
      text: 'hello',
      solid: '',
      tail: 'hello',
      lineKey: 0,
    })
  })

  it('含换行时 solid 含末尾 \\n，tail 为末行', () => {
    expect(streamView('a\nb')).toEqual({
      text: 'a\nb',
      solid: 'a\n',
      tail: 'b',
      lineKey: 1,
    })
  })

  it('同 text 返回同一缓存引用', () => {
    const a = streamView('x\ny')
    const b = streamView('x\ny')
    expect(a).toBe(b)
  })
})

describe('showToolBody', () => {
  it('web_search 成功有 hits 则展示', () => {
    expect(
      showToolBody(
        tool({
          name: 'web_search',
          state: 'done',
          parsed: { hits: [{ title: 'T', url: 'https://x', snippet: '' }] },
        }),
      ),
    ).toBe(true)
  })

  it('web_search 成功仅 answer 也展示', () => {
    expect(
      showToolBody(
        tool({
          name: 'web_search',
          state: 'done',
          parsed: { answer: '摘要', hits: [] },
        }),
      ),
    ).toBe(true)
  })

  it('web_search 成功无 parsed 不展示', () => {
    expect(showToolBody(tool({ name: 'web_search', state: 'done', output: '{"hits":[]}' }))).toBe(
      false,
    )
  })

  it('web_search 失败有 output 则展示', () => {
    expect(showToolBody(tool({ name: 'web_search', state: 'failed', output: 'missing key' }))).toBe(
      true,
    )
  })

  it('run_command 有 output 则展示', () => {
    expect(showToolBody(tool({ name: 'run_command', state: 'done', output: 'ok' }))).toBe(true)
  })

  it('running 不展示', () => {
    expect(showToolBody(tool({ name: 'run_command', state: 'running', output: 'x' }))).toBe(false)
  })
})

describe('tool helpers', () => {
  it('toolLabel / toolIcon', () => {
    expect(toolLabel('web_search')).toBe('搜索')
    expect(toolLabel('run_command')).toBe('命令')
    expect(toolLabel('other')).toBe('other')
    expect(toolIcon('web_search')).toContain('search')
    expect(toolIcon('run_command')).toContain('terminal')
  })

  it('toolDetail 解析 run_command / web_search', () => {
    expect(toolDetail(tool({ name: 'run_command', args: { cmd: 'ls', args: ['-la'] } }))).toBe(
      'ls -la',
    )
    expect(toolDetail(tool({ name: 'web_search', args: { query: '  void  ' } }))).toBe('void')
    expect(toolDetail(tool({ name: 'x', args: {} }))).toBe('')
  })
})

describe('isStreamingText / getMessageText', () => {
  it('仅最后一个 text part 在 streaming 时为 true', () => {
    const msg: AgentMessage = {
      id: 'a',
      role: 'assistant',
      streaming: true,
      parts: [
        { type: 'text', text: 'a' },
        { type: 'toolCall', id: '1', name: 'run_command', state: 'done' },
        { type: 'text', text: 'b' },
      ],
    }
    expect(isStreamingText(msg, 0)).toBe(false)
    expect(isStreamingText(msg, 2)).toBe(true)
    expect(getMessageText(msg)).toBe('ab')
  })

  it('非 streaming 全 false', () => {
    const msg: AgentMessage = {
      id: 'a',
      role: 'assistant',
      parts: [{ type: 'text', text: 'x' }],
    }
    expect(isStreamingText(msg, 0)).toBe(false)
  })
})

describe('streamLayoutKey', () => {
  it('随 streaming 文本长度变化', () => {
    const base: AgentMessage[] = [
      {
        id: 'a',
        role: 'assistant',
        streaming: true,
        parts: [{ type: 'text', text: 'hi' }],
      },
    ]
    const k1 = streamLayoutKey(base, true)
    base[0]!.parts[0] = { type: 'text', text: 'hello' }
    const k2 = streamLayoutKey(base, true)
    expect(k1).not.toBe(k2)
    expect(streamLayoutKey(base, false)).not.toBe(k2)
  })
})

describe('renderMarkdown', () => {
  it('空串返回空', () => {
    expect(renderMarkdown('')).toBe('')
    expect(renderMarkdown(null)).toBe('')
  })

  it('渲染段落并给 a 加 target', () => {
    const html = renderMarkdown('[x](https://example.com)')
    expect(html).toContain('href="https://example.com"')
    expect(html).toContain('target="_blank"')
    expect(html).toContain('noopener')
  })
})
